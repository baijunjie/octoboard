//! The version of each field of the records a form edits (the settings, a console, a project, an
//! account), which lets a write say which version of a field it was based on and be refused when
//! another client changed the field since (`write_conflict`).
//!
//! Versions are held in memory and never stored. Every version is drawn from one process-wide
//! counter that starts at the wall clock's microseconds since the epoch (about 1.8e15, below the
//! 2^53 a JavaScript number holds exactly) and only grows, and a field never changed since the
//! daemon started has that starting value as its version. While the wall clock does not move back
//! between runs, a restarted daemon starts above every version an earlier run handed out, so a
//! client holding one of them is refused rather than believed, and nothing has to be persisted.
//! The check compares for equality, so if the clock did move back, a version of an earlier run
//! matches a current one only by coincidence. The entries of a deleted record are kept, so a
//! writer holding its old versions cannot recreate it unnoticed.
//!
//! Versions advance where a record is published. The form-edited records are published only
//! through [`AppState::publish_settings`], [`AppState::publish_console`] and
//! [`AppState::publish_project`] (`broadcast_settings` included), which read the record under the
//! publication lock: each field whose value differs from the one last published moves to a fresh
//! version, so every writer advances them, single-action changes (a rename, a pin) included,
//! without a check of its own. A record with nothing published yet is new to every client, so all
//! its fields advance. The `snapshot` only reads: it stamps the current versions and records the
//! value of a field it has never seen as the one published.

mod paths;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use anyhow::Result;

use super::AppState;
use crate::protocol::{error_code, CodedError, Console, Event, Project, Settings};
use crate::time::now_micros;

pub(crate) use self::paths::SETTINGS_FIELDS;
use self::paths::{account_paths, console_paths, project_paths, settings_paths, Paths};

/// The record a version belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RecordKey {
    Settings,
    Console(String),
    Project(String),
    Account(String),
}

/// Whether a record is being published after a change or only read for a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stamp {
    Publish,
    Snapshot,
}

/// What is tracked of one record.
#[derive(Default)]
struct Tracked {
    /// Each field's value as last published (or first seen by a snapshot).
    baseline: Paths,
    /// The version of each field that has moved since the daemon started.
    versions: BTreeMap<String, u64>,
}

struct Inner {
    /// The version last handed out; also what a field that never moved has.
    counter: u64,
    seed: u64,
    records: HashMap<RecordKey, Tracked>,
}

pub struct FieldVersions {
    inner: Mutex<Inner>,
}

impl Default for FieldVersions {
    fn default() -> Self {
        Self::starting_at(now_micros())
    }
}

impl FieldVersions {
    /// A counter that starts at `seed` (the clock, for the daemon).
    pub fn starting_at(seed: u64) -> Self {
        Self {
            inner: Mutex::new(Inner {
                counter: seed,
                seed,
                records: HashMap::new(),
            }),
        }
    }

    /// The complete versions of the record `key` whose fields hold `values`, which become the
    /// published ones where `stamp` is [`Stamp::Publish`].
    fn stamp(&self, key: RecordKey, values: Paths, stamp: Stamp) -> BTreeMap<String, u64> {
        let mut inner = self.inner.lock().expect("field versions lock poisoned");
        let Inner {
            counter,
            seed,
            records,
        } = &mut *inner;
        let tracked = records.entry(key).or_default();
        for (path, value) in values {
            match (stamp, tracked.baseline.get(&path)) {
                (_, Some(published)) if *published == value => {}
                (Stamp::Snapshot, Some(_)) => {}
                (Stamp::Snapshot, None) => {
                    tracked.baseline.insert(path, value);
                }
                (Stamp::Publish, _) => {
                    *counter += 1;
                    tracked.versions.insert(path.clone(), *counter);
                    tracked.baseline.insert(path, value);
                }
            }
        }
        tracked
            .baseline
            .keys()
            .map(|path| {
                (
                    path.clone(),
                    tracked.versions.get(path).copied().unwrap_or(*seed),
                )
            })
            .collect()
    }

    /// The current version of each of `paths` of the record `key`.
    fn current(
        &self,
        key: &RecordKey,
        paths: impl Iterator<Item = String>,
    ) -> BTreeMap<String, u64> {
        let inner = self.inner.lock().expect("field versions lock poisoned");
        let tracked = inner.records.get(key);
        paths
            .map(|path| {
                let version = tracked
                    .and_then(|tracked| tracked.versions.get(&path))
                    .copied()
                    .unwrap_or(inner.seed);
                (path, version)
            })
            .collect()
    }

    /// Stamps `settings` and each of its accounts with their versions.
    pub fn stamp_settings(&self, settings: &mut Settings, stamp: Stamp) {
        settings.versions = self.stamp(RecordKey::Settings, settings_paths(settings), stamp);
        for account in &mut settings.accounts {
            account.versions = self.stamp(
                RecordKey::Account(account.id.clone()),
                account_paths(account),
                stamp,
            );
        }
    }

    pub fn stamp_console(&self, console: &mut Console, stamp: Stamp) {
        console.versions = self.stamp(
            RecordKey::Console(console.id.clone()),
            console_paths(console),
            stamp,
        );
    }

    pub fn stamp_project(&self, project: &mut Project, stamp: Stamp) {
        project.versions = self.stamp(
            RecordKey::Project(project.id.clone()),
            project_paths(project),
            stamp,
        );
    }

    /// Stamps the record `event` publishes, if it is one a form edits.
    pub fn publish(&self, event: &mut Event) {
        match event {
            Event::SettingsUpdated { settings } => self.stamp_settings(settings, Stamp::Publish),
            Event::ConsoleUpserted { console } => self.stamp_console(console, Stamp::Publish),
            Event::ProjectUpserted { project } => self.stamp_project(project, Stamp::Publish),
            _ => {}
        }
    }

    /// Refuses `base`, the versions a write was based on, when it names a path the record does not
    /// have (`unreadable_request`) or a version another write has moved on from
    /// (`write_conflict`, naming every such path and its current version). `record` holds the
    /// record's paths.
    fn check(&self, key: &RecordKey, record: &Paths, base: &BTreeMap<String, u64>) -> Result<()> {
        if let Some(path) = base.keys().find(|path| !record.contains_key(*path)) {
            let detail = format!("`{path}` is not a field that has a version");
            return Err(CodedError::raised(
                error_code::UNREADABLE_REQUEST,
                format!("unreadable request: {detail}"),
                &[("detail", &detail)],
            ));
        }
        let current = self.current(key, base.keys().cloned());
        let stale: BTreeMap<&String, u64> = base
            .iter()
            .filter(|(path, version)| current[*path] != **version)
            .map(|(path, _)| (path, current[path]))
            .collect();
        if stale.is_empty() {
            return Ok(());
        }
        let fields = stale
            .keys()
            .map(|path| path.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let versions = serde_json::to_string(&stale)?;
        Err(CodedError::raised(
            error_code::WRITE_CONFLICT,
            format!("{fields} changed since it was read"),
            &[("fields", &fields), ("versions", &versions)],
        ))
    }
}

impl AppState {
    /// Checks `base` against the versions of the record `key`, as [`FieldVersions::check`] does.
    pub fn check_versions(&self, key: &RecordKey, base: &BTreeMap<String, u64>) -> Result<()> {
        let record = match key {
            RecordKey::Settings => settings_paths(&Settings::default()),
            RecordKey::Console(id) => console_paths(&self.store.require_console(id)?),
            RecordKey::Project(id) => project_paths(&self.store.require_project(id)?),
            RecordKey::Account(id) => account_paths(&self.store.require_account(id)?),
        };
        self.field_versions.check(key, &record, base)
    }

    /// Begins a write of the record `key` that was based on the versions `base`: with none, the
    /// write is not checked and nothing is held. Otherwise the check is made under the lock all
    /// checked writes share, which the guard returned holds, so the caller keeps it until the
    /// change is applied and published: two checked writes cannot both pass the check and then
    /// both write.
    pub async fn checked_write(
        &self,
        key: &RecordKey,
        base: &BTreeMap<String, u64>,
    ) -> Result<Option<tokio::sync::MutexGuard<'_, ()>>> {
        if base.is_empty() {
            return Ok(None);
        }
        let guard = self.checked_writes.lock().await;
        self.check_versions(key, base)?;
        Ok(Some(guard))
    }

    /// Publishes the settings the store holds now. The record is read under the publication lock
    /// and stamped there, so a publish made from an older read cannot land after a newer one and
    /// give the stale values fresh versions.
    pub fn publish_settings(&self) -> Result<()> {
        let _publication = self
            .session_publication
            .lock()
            .expect("session publication lock poisoned");
        let settings = self.store.get_settings()?;
        self.publish_locked(Event::SettingsUpdated { settings });
        Ok(())
    }

    /// Publishes the console `id` as the store holds it now, as [`Self::publish_settings`] does;
    /// one deleted in the meantime is not published.
    pub fn publish_console(&self, id: &str) -> Result<()> {
        let _publication = self
            .session_publication
            .lock()
            .expect("session publication lock poisoned");
        if let Some(console) = self.store.get_console(id)? {
            self.publish_locked(Event::ConsoleUpserted { console });
        }
        Ok(())
    }

    /// Publishes the project `id` as the store holds it now, as [`Self::publish_settings`] does.
    pub fn publish_project(&self, id: &str) -> Result<()> {
        let _publication = self
            .session_publication
            .lock()
            .expect("session publication lock poisoned");
        if let Some(project) = self.store.get_project(id)? {
            self.publish_locked(Event::ProjectUpserted { project });
        }
        Ok(())
    }

    /// Stamps and sends `event`; the caller holds the publication lock.
    pub(super) fn publish_locked(&self, mut event: Event) {
        self.field_versions.publish(&mut event);
        self.broadcast_locked(event);
    }

    /// The versions of every field of the settings, as a client holds them.
    pub fn settings_versions(&self) -> Result<BTreeMap<String, u64>> {
        let paths = settings_paths(&Settings::default());
        Ok(self
            .field_versions
            .current(&RecordKey::Settings, paths.into_keys()))
    }
}
