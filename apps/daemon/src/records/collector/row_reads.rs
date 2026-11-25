//! Scheduling the collector's blocking row queries across passes.

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use tokio::sync::oneshot;

use super::{fold_rows, Cursor};
use crate::blocking_pass::BLOCKING_WIDTH;
use crate::protocol::{AgentId, Session};
use crate::records::{rows, Hints, Locator};

#[cfg(test)]
type BeforeRead = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Default)]
pub(super) struct RowReads {
    #[cfg(test)]
    background: bool,
    running: HashMap<String, Running>,
    finished: HashMap<String, Finished>,
    requested: Vec<Request>,
    last_started: Option<String>,
    #[cfg(test)]
    pub(super) before_read: Option<BeforeRead>,
}

pub(super) struct Version {
    launch: Option<String>,
    stirred: u64,
}

struct Source {
    identity: (AgentId, Option<String>, Option<String>),
    boundary: Option<String>,
    version: Version,
}

struct Request {
    session: Session,
    source: Source,
}

struct Running {
    source: Source,
    answer: oneshot::Receiver<Option<Vec<Value>>>,
}

struct Finished {
    source: Source,
    rows: Option<Vec<Value>>,
}

impl RowReads {
    pub(super) fn with_background() -> Self {
        Self {
            #[cfg(test)]
            background: true,
            ..Self::default()
        }
    }

    pub(super) fn begin_pass(&mut self, ids: &HashSet<&str>) {
        self.finished.clear();
        for (id, mut running) in std::mem::take(&mut self.running) {
            let rows = match running.answer.try_recv() {
                Ok(rows) => rows,
                Err(oneshot::error::TryRecvError::Closed) => None,
                Err(oneshot::error::TryRecvError::Empty) => {
                    self.running.insert(id, running);
                    continue;
                }
            };
            if ids.contains(id.as_str()) {
                self.finished.insert(
                    id,
                    Finished {
                        source: running.source,
                        rows,
                    },
                );
            }
        }
    }

    /// Returns whether this cursor still needs a read, so it cannot go to sleep before the
    /// final query has finished.
    pub(super) fn scan(
        &mut self,
        hints: &Hints,
        _locator: &Locator,
        session: &Session,
        launch: Option<&str>,
        cursor: &mut Cursor,
    ) -> bool {
        cursor.rows_seen_live |= launch.is_some();
        if let Some(done) = self.finished.remove(&session.id) {
            if cursor.identity.as_ref() == Some(&done.source.identity)
                && cursor.stored_boundary == done.source.boundary
                && launch == done.source.version.launch.as_deref()
            {
                cursor.rows_read = Some(done.source.version);
                if let Some(rows) = done.rows {
                    fold_rows(session, cursor, rows);
                }
            }
        }
        let stirred = hints.stirred(&session.id);
        let due = match launch {
            Some(_) => cursor
                .rows_read
                .as_ref()
                .is_none_or(|read| read.launch.as_deref() != launch || read.stirred != stirred),
            None => {
                cursor.rows_seen_live
                    && cursor
                        .rows_read
                        .as_ref()
                        .is_none_or(|read| read.launch.is_some())
            }
        };
        if !due {
            return false;
        }
        let version = Version {
            launch: launch.map(str::to_owned),
            stirred,
        };
        #[cfg(test)]
        if !self.background {
            cursor.rows_read = Some(version);
            if let Some(Ok(rows)) = rows::read(_locator, session) {
                fold_rows(session, cursor, rows);
            }
            return false;
        }
        if !self.running.contains_key(&session.id) {
            self.requested.push(Request {
                session: session.clone(),
                source: Source {
                    identity: cursor.identity.clone().expect("collector cursor identity"),
                    boundary: cursor.stored_boundary.clone(),
                    version,
                },
            });
        }
        true
    }

    pub(super) fn start_pending(&mut self, locator: &Locator) {
        // Start after the previous batch's last id, so busy sessions cannot repeatedly take all
        // the slots ahead of another session. Requests are rebuilt from current data next pass.
        let mut requested = std::mem::take(&mut self.requested);
        requested.sort_unstable_by(|a, b| a.session.id.cmp(&b.session.id));
        let after_last = requested.partition_point(|request| {
            self.last_started
                .as_ref()
                .is_some_and(|last| &request.session.id <= last)
        });
        let len = requested.len();
        if len == 0 {
            return;
        }
        requested.rotate_left(after_last % len);
        let free = BLOCKING_WIDTH - self.running.len();
        let mut env = None;
        for Request { session, source } in requested.into_iter().take(free) {
            let (report, answer) = oneshot::channel();
            if let (Some((store, native_id)), Some(shell_env)) = (
                rows::of(&session),
                env.get_or_insert_with(|| locator.shell_env().cloned().map(std::sync::Arc::new)),
            ) {
                let shell_env = shell_env.clone();
                let native_id = native_id.to_owned();
                let config_dir = session.config_dir.map(std::path::PathBuf::from);
                #[cfg(test)]
                let before_read = self
                    .before_read
                    .clone()
                    .map(|before| (before, session.id.clone()));
                tokio::task::spawn_blocking(move || {
                    #[cfg(test)]
                    if let Some((before, id)) = before_read {
                        before(&id);
                    }
                    let account = rows::RowAccount {
                        config_dir: config_dir.as_deref(),
                        shell_env: &shell_env,
                    };
                    let _ = report.send(store.lines(&account, &native_id).ok());
                });
            } else {
                let _ = report.send(None);
            }
            self.last_started = Some(session.id.clone());
            self.running.insert(session.id, Running { source, answer });
        }
    }
}
