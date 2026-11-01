//! Writing a session's shared-config changes (theme, plugin switches, chosen model) back to the
//! account's shared `config.toml`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::adapter::files::{read_regular_text, SMALL_FILE_LIMIT};
use crate::agents::layout::GROK_CONFIG_FILE;
use crate::hostfs::replace::{replace_beside, Permissions};

/// Keys a sharing account's session may copy from its `config.toml` onto the shared file.
///
/// The session file stays a real copy, so the MCP block appended to it is never this write. The
/// bytes go to the canonical target the account's link resolved to at launch, and the link is not
/// replaced; if by the time the process exits the entry is no longer a link resolving there,
/// nothing is written. A session that did not change a key does not write it, so it does not put
/// back a value another session has already published.
///
/// A fullscreen launch writes `--model` into `[models] default` and `--reasoning-effort` into
/// `[models] default_reasoning_effort` with no further action. The effort is never copied. The
/// default is copied only when it differs from the model this launch passed, which is what a model
/// chosen in the session looks like; choosing the launch's own model again cannot be told from the
/// launch write, and is left in the session copy.
#[derive(Debug)]
pub struct SharedConfigWriteback {
    session_config: PathBuf,
    account_config: PathBuf,
    shared_target: PathBuf,
    baseline: ConfigSlice,
    launch_model: Option<String>,
}

/// The keys [`SharedConfigWriteback`] compares. `None` means the key is absent. An empty plugin
/// list is present and is copied: clearing the list is how a plugin is switched off.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfigSlice {
    theme: Option<String>,
    enabled: Option<Vec<String>>,
    disabled: Option<Vec<String>>,
    model: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct ConfigChanges {
    theme: Option<String>,
    enabled: Option<Vec<String>>,
    disabled: Option<Vec<String>>,
    model: Option<String>,
}

impl ConfigChanges {
    fn is_empty(&self) -> bool {
        self.theme.is_none()
            && self.enabled.is_none()
            && self.disabled.is_none()
            && self.model.is_none()
    }
}

/// Serializes this daemon's exits so they merge into the latest shared file rather than dropping
/// each other's keys. External Grok processes and editors do not take this lock.
fn writeback_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `Some` when `account_config`'s source entry is a symlink to a file that exists. A plain file is
/// the account's own, and a dangling link has no shared file to write. A session copy above its
/// size bound has no baseline to compare with, so it is not written back either.
pub(super) fn shared_config_writeback(
    source_home: &Path,
    grok_home: &Path,
    launch_model: Option<&str>,
) -> Option<SharedConfigWriteback> {
    let account_config = source_home.join(GROK_CONFIG_FILE);
    let meta = std::fs::symlink_metadata(&account_config).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    let shared_target = std::fs::canonicalize(&account_config).ok()?;
    let session_config = grok_home.join(GROK_CONFIG_FILE);
    let text = read_regular_text(&session_config, SMALL_FILE_LIMIT).ok()?;
    let table: toml::Table = text.parse().ok()?;
    Some(SharedConfigWriteback {
        session_config,
        account_config,
        shared_target,
        baseline: ConfigSlice::from_table(&table),
        launch_model: launch_model.map(str::to_owned),
    })
}

impl SharedConfigWriteback {
    /// Copies the keys the session changed onto the shared file. A failure is logged and leaves the
    /// shared file as it was; the session has already exited, and removing its home must still happen.
    pub fn publish(&self) {
        let _guard = writeback_lock();
        if let Err(err) = self.publish_locked() {
            tracing::warn!(
                path = %self.account_config.display(),
                %err,
                "writing Grok settings back to the shared configuration failed"
            );
        }
    }

    pub(super) fn publish_locked(&self) -> Result<()> {
        // A session's changes belong to the configuration it started from. If the account now
        // shares a different file, publishing into that new file would carry unrelated settings.
        if !self.still_shares() {
            return Ok(());
        }
        let target = &self.shared_target;
        if *target == self.session_config {
            anyhow::bail!("the shared configuration resolves to the session's own copy");
        }
        if std::fs::symlink_metadata(target).is_ok_and(|meta| meta.file_type().is_symlink()) {
            anyhow::bail!("the shared configuration did not resolve to a file");
        }

        let session: toml::Table = read_regular_text(&self.session_config, SMALL_FILE_LIMIT)
            .with_context(|| format!("reading {}", self.session_config.display()))?
            .parse()
            .context("the session's configuration is not valid TOML")?;
        let changes =
            ConfigSlice::from_table(&session).changes(&self.baseline, self.launch_model.as_deref());
        if changes.is_empty() {
            return Ok(());
        }

        let current = read_regular_text(target, SMALL_FILE_LIMIT)
            .with_context(|| format!("reading {}", target.display()))?;
        let merged = apply_changes(&current, &changes)?;
        if merged == current {
            return Ok(());
        }
        let before: toml::Table = current
            .parse()
            .context("the shared configuration is not valid TOML")?;
        let after: toml::Table = merged
            .parse()
            .context("the shared configuration would be left unreadable")?;
        let mut expected = before.clone();
        apply_to_table(&mut expected, &changes)
            .context("the shared configuration is not shaped so these keys can be set")?;
        if after != expected {
            anyhow::bail!(
                "writing the keys would change more of the shared configuration than those keys"
            );
        }
        // Checked again just before the replace, to narrow the window for a relink meanwhile.
        if !self.still_shares() {
            return Ok(());
        }
        replace_beside(target, merged.as_bytes(), Permissions::KeptOrPrivate)
            .with_context(|| format!("writing {}", target.display()))
    }

    /// The account entry is still a link that resolves to the file it resolved to at launch. A
    /// plain file, a broken link or a link to another file does not.
    fn still_shares(&self) -> bool {
        std::fs::symlink_metadata(&self.account_config)
            .is_ok_and(|meta| meta.file_type().is_symlink())
            && std::fs::canonicalize(&self.account_config).ok().as_ref()
                == Some(&self.shared_target)
    }
}

impl ConfigSlice {
    fn from_table(table: &toml::Table) -> Self {
        Self {
            theme: string_key(table, "ui", "theme"),
            enabled: string_array(table, "plugins", "enabled"),
            disabled: string_array(table, "plugins", "disabled"),
            model: string_key(table, "models", "default"),
        }
    }

    /// What of `self` (the session file at exit) differs from `baseline` (the copy taken before
    /// Grok ran) and is safe to publish. A `[models] default` equal to `launch_model` is the launch
    /// flag's own write.
    fn changes(&self, baseline: &Self, launch_model: Option<&str>) -> ConfigChanges {
        ConfigChanges {
            theme: self.theme.clone().filter(|_| self.theme != baseline.theme),
            enabled: self
                .enabled
                .clone()
                .filter(|_| self.enabled != baseline.enabled),
            disabled: self
                .disabled
                .clone()
                .filter(|_| self.disabled != baseline.disabled),
            model: self.model.clone().filter(|value| {
                self.model != baseline.model && launch_model.is_none_or(|launch| value != launch)
            }),
        }
    }
}

fn string_key(table: &toml::Table, section: &str, key: &str) -> Option<String> {
    table
        .get(section)?
        .as_table()?
        .get(key)?
        .as_str()
        .map(str::to_owned)
}

/// `None` when the key is absent or any element is not a string, so a shape this does not know is
/// left untouched rather than rewritten as a string list.
fn string_array(table: &toml::Table, section: &str, key: &str) -> Option<Vec<String>> {
    let array = table.get(section)?.as_table()?.get(key)?.as_array()?;
    let mut items = Vec::with_capacity(array.len());
    for value in array {
        items.push(value.as_str()?.to_owned());
    }
    Some(items)
}

fn apply_changes(current: &str, changes: &ConfigChanges) -> Result<String> {
    let mut doc = current
        .parse::<toml_edit::DocumentMut>()
        .context("the shared configuration is not valid TOML")?;
    if let Some(theme) = &changes.theme {
        set_value(&mut doc, "ui", "theme", theme.as_str().into())?;
    }
    if let Some(enabled) = &changes.enabled {
        set_value(&mut doc, "plugins", "enabled", string_array_value(enabled))?;
    }
    if let Some(disabled) = &changes.disabled {
        set_value(
            &mut doc,
            "plugins",
            "disabled",
            string_array_value(disabled),
        )?;
    }
    if let Some(model) = &changes.model {
        set_value(&mut doc, "models", "default", model.as_str().into())?;
    }
    Ok(doc.to_string())
}

fn string_array_value(items: &[String]) -> toml_edit::Value {
    let mut array = toml_edit::Array::new();
    for item in items {
        array.push(item.as_str());
    }
    toml_edit::Value::Array(array)
}

/// Sets `section.key` in a standard table or an inline table. Refuses when `section` is some other
/// kind of value, rather than replacing it.
fn set_value(
    doc: &mut toml_edit::DocumentMut,
    section: &str,
    key: &str,
    value: toml_edit::Value,
) -> Result<()> {
    let item = doc
        .as_table_mut()
        .entry(section)
        .or_insert(toml_edit::Item::None);
    if item.is_none() {
        *item = toml_edit::table();
    }
    if let Some(table) = item.as_table_mut() {
        table.insert(key, toml_edit::value(value));
        return Ok(());
    }
    if let Some(inline) = item
        .as_value_mut()
        .and_then(|value| value.as_inline_table_mut())
    {
        inline.insert(key, value);
        return Ok(());
    }
    anyhow::bail!("`[{section}]` is not a table, so `{key}` was not written")
}

fn apply_to_table(table: &mut toml::Table, changes: &ConfigChanges) -> Result<()> {
    if let Some(theme) = &changes.theme {
        set_table_string(table, "ui", "theme", theme.clone())?;
    }
    if let Some(enabled) = &changes.enabled {
        set_table_array(table, "plugins", "enabled", enabled)?;
    }
    if let Some(disabled) = &changes.disabled {
        set_table_array(table, "plugins", "disabled", disabled)?;
    }
    if let Some(model) = &changes.model {
        set_table_string(table, "models", "default", model.clone())?;
    }
    Ok(())
}

fn section_table<'a>(table: &'a mut toml::Table, section: &str) -> Result<&'a mut toml::Table> {
    let value = table
        .entry(section)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    value
        .as_table_mut()
        .with_context(|| format!("`[{section}]` is not a table"))
}

fn set_table_string(
    table: &mut toml::Table,
    section: &str,
    key: &str,
    value: String,
) -> Result<()> {
    section_table(table, section)?.insert(key.to_owned(), toml::Value::String(value));
    Ok(())
}

fn set_table_array(
    table: &mut toml::Table,
    section: &str,
    key: &str,
    items: &[String],
) -> Result<()> {
    let array = items.iter().cloned().map(toml::Value::String).collect();
    section_table(table, section)?.insert(key.to_owned(), toml::Value::Array(array));
    Ok(())
}
