//! The application settings: the single settings row's flags, default account names, Git workflow
//! and clone directory. The settings of each provider are `provider_settings`.

use super::Result;
use rusqlite::types::FromSql;
use rusqlite::{params, Connection};
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::accounts::read_accounts;
use super::agents::{read_account_order, read_added_agents};
use super::provider_settings::read_provider_settings;
use super::rows::{enum_from_row, enum_to_text};
use super::{Store, SETTINGS_ID};
use crate::protocol::{
    AgentId, GitWorkflowSettings, GitWorkflowSettingsChanges, KeepAwakeWhen, Settings,
};

/// Where a clone goes while the user has not chosen a directory.
const BUILT_IN_CLONE_DIR: &str = "~/Projects";

impl Store {
    /// The settings row, at its defaults, for a freshly created database.
    pub(super) fn ensure_settings_row(&self) -> Result<()> {
        self.lock().execute(
            "INSERT OR IGNORE INTO settings (id, auto_pull_repositories) VALUES (?1, 0)",
            params![SETTINGS_ID],
        )?;
        Ok(())
    }

    /// Every setting, read under one take of the lock, so a write on another task cannot land
    /// between two of the reads and leave one record mixing old and new values.
    pub fn get_settings(&self) -> Result<Settings> {
        let conn = self.lock();
        Ok(Settings {
            auto_close_idle_shell_terminals: read_setting(
                &conn,
                "auto_close_idle_shell_terminals",
            )?,
            auto_pull_repositories: read_setting(&conn, "auto_pull_repositories")?,
            auto_delete_unresumable_archives: read_setting(
                &conn,
                "auto_delete_unresumable_archives",
            )?,
            auto_approve_session_requests: read_setting(&conn, "auto_approve_session_requests")?,
            auto_approve_resume_requests: read_setting(&conn, "auto_approve_resume_requests")?,
            auto_approve_account_switches: read_setting(&conn, "auto_approve_account_switches")?,
            auto_approve_handoffs: read_setting(&conn, "auto_approve_handoffs")?,
            ask_questions_as_choices: read_setting(&conn, "ask_questions_as_choices")?,
            interface_language: read_setting(&conn, "interface_language")?,
            keep_awake: read_setting(&conn, "keep_awake")?,
            keep_awake_when: read_keep_awake_when(&conn)?,
            auto_select_account: read_setting(&conn, "auto_select_account")?,
            auto_switch_on_usage_limit: read_setting(&conn, "auto_switch_on_usage_limit")?,
            auto_start_quota_windows: read_setting(&conn, "auto_start_quota_windows")?,
            resume_sessions_on_start: read_setting(&conn, "resume_sessions_on_start")?,
            default_clone_dir: clone_dir_or_built_in(read_setting(&conn, "default_clone_dir")?),
            accounts: read_accounts(&conn)?,
            account_order: read_account_order(&conn)?,
            added_agents: read_added_agents(&conn)?,
            provider_settings: read_provider_settings(&conn)?,
            agent_tags: read_agent_tags(&conn)?,
            default_account_names: read_default_account_names(&conn)?,
            disabled_default_accounts: read_disabled_default_accounts(&conn)?,
            git_workflow: read_git_workflow_settings(&conn)?,
            versions: Default::default(),
        })
    }

    /// The names the user gave the agents' default accounts, by the agent's wire name. A value that
    /// is not a JSON object of strings reads as no names.
    pub fn default_account_names(&self) -> Result<std::collections::BTreeMap<String, String>> {
        read_default_account_names(&self.lock())
    }

    /// The tags the user put on each agent, by wire name, for the agents that have any. A value
    /// that is not a JSON object of string lists reads as none.
    pub fn agent_tags(&self) -> Result<std::collections::BTreeMap<String, Vec<String>>> {
        read_agent_tags(&self.lock())
    }

    /// Replaces `agent`'s tags with `tags`, which the caller has normalised; an empty list clears
    /// them. The read and the write are one step under the store's lock. Whether the stored tags
    /// changed.
    pub fn set_agent_tags(&self, agent: AgentId, tags: Vec<String>) -> Result<bool> {
        let conn = self.lock();
        let mut all = read_stored_agent_tags(&conn)?;
        let changed = if tags.is_empty() {
            all.remove(agent.wire_name()).is_some()
        } else {
            all.insert(agent.wire_name().to_string(), tags.clone()) != Some(tags)
        };
        if !changed {
            return Ok(false);
        }
        write_json_setting(&conn, "agent_tags", (!all.is_empty()).then_some(&all))?;
        Ok(true)
    }

    /// The agents, by wire name, whose default account is turned off for new work. A value that is
    /// not a JSON array of strings reads as none.
    pub fn disabled_default_accounts(&self) -> Result<std::collections::BTreeSet<String>> {
        read_disabled_default_accounts(&self.lock())
    }

    /// Turns `agent`'s default account off for new work, or back on. The read and the write are
    /// one step under the store's lock. Whether the stored set changed.
    pub fn set_default_account_disabled(&self, agent: AgentId, disabled: bool) -> Result<bool> {
        let conn = self.lock();
        let mut agents = read_stored_disabled_default_accounts(&conn)?;
        let changed = if disabled {
            agents.insert(agent.wire_name().to_string())
        } else {
            agents.remove(agent.wire_name())
        };
        if !changed {
            return Ok(false);
        }
        write_json_setting(
            &conn,
            "disabled_default_accounts",
            (!agents.is_empty()).then_some(&agents),
        )?;
        Ok(true)
    }

    /// Whether the user turned `account_id` (`None` the default account) of `agent` off for new
    /// work. An id that is no account of `agent`, or no longer on record, is not disabled.
    pub fn account_disabled(&self, agent: AgentId, account_id: Option<&str>) -> Result<bool> {
        match account_id {
            None => Ok(self
                .disabled_default_accounts()?
                .contains(agent.wire_name())),
            Some(id) => Ok(self
                .get_account(id)?
                .is_some_and(|account| account.agent == agent && account.disabled)),
        }
    }

    /// The name of `agent`'s default account: the user's own, else the daemon's word for it.
    pub fn default_account_name(&self, agent: AgentId) -> Result<String> {
        Ok(
            crate::protocol::default_account_name_in(&self.default_account_names()?, agent)
                .to_string(),
        )
    }

    /// What an account is called: the stored account's name (its id when it is gone), or for
    /// `None`, the default account's name.
    pub fn account_display_name(&self, agent: AgentId, account_id: Option<&str>) -> Result<String> {
        match account_id {
            Some(id) => Ok(self
                .get_account(id)?
                .map_or_else(|| id.to_string(), |account| account.name)),
            None => self.default_account_name(agent),
        }
    }

    /// Names the agent's default account, or clears the name with `None`. The read and the write
    /// are one step under the store's lock. Whether the stored names changed.
    pub fn set_default_account_name(&self, agent: AgentId, name: Option<&str>) -> Result<bool> {
        let conn = self.lock();
        let mut names = read_stored_default_account_names(&conn)?;
        let before = names.clone();
        match name {
            Some(name) => names.insert(agent.wire_name().to_string(), name.to_string()),
            None => names.remove(agent.wire_name()),
        };
        if names == before {
            return Ok(false);
        }
        write_json_setting(
            &conn,
            "default_account_names",
            (!names.is_empty()).then_some(&names),
        )?;
        Ok(true)
    }

    /// The application-wide git workflow settings. A project's override is resolved against them by
    /// `git_workflow::effective`, never by reading this directly.
    pub fn git_workflow_settings(&self) -> Result<GitWorkflowSettings> {
        let conn = self.lock();
        read_git_workflow_settings(&conn)
    }

    /// Applies `changes` to the application-wide git workflow settings in one read-modify-write.
    /// Whether anything actually changed.
    pub fn change_git_workflow_settings(
        &self,
        changes: &GitWorkflowSettingsChanges,
    ) -> Result<bool> {
        let conn = self.lock();
        let current = read_git_workflow_settings(&conn)?;
        let next = changes.applied_to(current);
        if next == current {
            return Ok(false);
        }
        conn.execute(
            "UPDATE settings SET development = ?2, rebase_before_delivery = ?3, delivery = ?4,
                                 revert_gate = ?5, squash_before_delivery = ?6
             WHERE id = ?1",
            params![
                SETTINGS_ID,
                enum_to_text(&next.development),
                next.rebase_before_delivery,
                enum_to_text(&next.delivery),
                next.revert_gate,
                next.squash_before_delivery,
            ],
        )?;
        Ok(true)
    }

    /// Makes every later read or write of the settings fail.
    #[cfg(test)]
    pub fn drop_settings(&self) -> Result<()> {
        self.lock().execute_batch("DROP TABLE settings")?;
        Ok(())
    }

    /// The tag of the language the user last picked for the interface, as a client reported it;
    /// `None` until one has.
    pub fn interface_language(&self) -> Result<Option<String>> {
        read_setting(&self.lock(), "interface_language")
    }

    /// Stores an already-validated tag. Whether the setting actually changed.
    pub fn set_interface_language(&self, tag: &str) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET interface_language = ?2 \
             WHERE id = ?1 AND interface_language IS NOT ?2",
            params![SETTINGS_ID, tag],
        )?;
        Ok(changed > 0)
    }

    /// When Prevent sleep holds (`keep_awake`).
    pub fn keep_awake_when(&self) -> Result<KeepAwakeWhen> {
        read_keep_awake_when(&self.lock())
    }

    /// Whether the setting actually changed.
    pub fn set_keep_awake_when(&self, value: KeepAwakeWhen) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE settings SET keep_awake_when = ?2 WHERE id = ?1 AND keep_awake_when != ?2",
            params![SETTINGS_ID, enum_to_text(&value)],
        )?;
        Ok(changed > 0)
    }

    /// The directory a clone lands in when none is named: the stored one, else `~/Projects`
    /// expanded, which is as absolute and lexically normalised as the home it expands from.
    pub fn default_clone_dir(&self) -> Result<String> {
        Ok(clone_dir_or_built_in(read_setting(
            &self.lock(),
            "default_clone_dir",
        )?))
    }

    /// Stores the already-normalised directory, or `None` to go back to the built-in default.
    /// Whether the setting actually changed.
    pub fn set_default_clone_dir(&self, value: Option<&str>) -> Result<bool> {
        let conn = self.lock();
        let current: Option<String> = read_setting(&conn, "default_clone_dir")?;
        if current.as_deref() == value {
            return Ok(false);
        }
        conn.execute(
            "UPDATE settings SET default_clone_dir = ?2 WHERE id = ?1",
            params![SETTINGS_ID, value],
        )?;
        Ok(true)
    }
}

/// The settings that are a plain switch, each a column of the settings row named as its getter is:
/// `$get` reads it, and `$set` writes it, answering whether the stored value changed. A switch
/// added to the settings row is one line here, with its column in the schema.
macro_rules! bool_settings {
    ($($get:ident, $set:ident;)*) => {
        impl Store {
            $(
                pub fn $get(&self) -> Result<bool> {
                    read_setting(&self.lock(), stringify!($get))
                }

                /// Whether the setting actually changed.
                pub fn $set(&self, value: bool) -> Result<bool> {
                    self.write_bool_setting(
                        concat!(
                            "UPDATE settings SET ", stringify!($get), " = ?2 WHERE id = ?1 AND ",
                            stringify!($get), " != ?2"
                        ),
                        value,
                    )
                }
            )*
        }
    };
}

bool_settings! {
    auto_close_idle_shell_terminals, set_auto_close_idle_shell_terminals;
    auto_pull_repositories, set_auto_pull_repositories;
    auto_delete_unresumable_archives, set_auto_delete_unresumable_archives;
    auto_approve_session_requests, set_auto_approve_session_requests;
    auto_approve_resume_requests, set_auto_approve_resume_requests;
    auto_approve_account_switches, set_auto_approve_account_switches;
    auto_approve_handoffs, set_auto_approve_handoffs;
    ask_questions_as_choices, set_ask_questions_as_choices;
    auto_select_account, set_auto_select_account;
    auto_switch_on_usage_limit, set_auto_switch_on_usage_limit;
    auto_start_quota_windows, set_auto_start_quota_windows;
    resume_sessions_on_start, set_resume_sessions_on_start;
    keep_awake, set_keep_awake;
}

impl Store {
    /// Runs `sql`, the statement `bool_settings!` builds to write a column only when it differs
    /// from `value`. Whether it did.
    fn write_bool_setting(&self, sql: &'static str, value: bool) -> Result<bool> {
        let changed = self.lock().execute(sql, params![SETTINGS_ID, value])?;
        Ok(changed > 0)
    }
}

fn read_keep_awake_when(conn: &Connection) -> Result<KeepAwakeWhen> {
    Ok(conn.query_row(
        "SELECT keep_awake_when FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| enum_from_row(row, 0),
    )?)
}

fn read_git_workflow_settings(conn: &Connection) -> Result<GitWorkflowSettings> {
    Ok(conn.query_row(
        "SELECT development, rebase_before_delivery, delivery, revert_gate, squash_before_delivery
         FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| {
            Ok(GitWorkflowSettings {
                development: enum_from_row(row, 0)?,
                squash_before_delivery: row.get(4)?,
                rebase_before_delivery: row.get(1)?,
                delivery: enum_from_row(row, 2)?,
                revert_gate: row.get(3)?,
            })
        },
    )?)
}

/// The value of the settings row's `column`.
fn read_setting<T: FromSql>(conn: &Connection, column: &str) -> Result<T> {
    Ok(conn.query_row(
        &format!("SELECT {column} FROM settings WHERE id = ?1"),
        params![SETTINGS_ID],
        |row| row.get(0),
    )?)
}

/// The value of a settings column that holds JSON, `None` while it is NULL. A value that does not
/// parse as `T` reads as `None` too, with a warning: the daemon is the only writer, so this can
/// only mean a hand-edited database, or one written by a later version.
pub(super) fn read_json_setting_opt<T: DeserializeOwned>(
    conn: &Connection,
    column: &str,
) -> Result<Option<T>> {
    let text: Option<String> = read_setting(conn, column)?;
    Ok(text.and_then(|text| match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(err) => {
            tracing::warn!(%err, column, "a settings value is not the JSON it should be; reading it as unset");
            None
        }
    }))
}

/// [`read_json_setting_opt`], with `T`'s default for a value that is NULL or malformed.
pub(super) fn read_json_setting<T: DeserializeOwned + Default>(
    conn: &Connection,
    column: &str,
) -> Result<T> {
    Ok(read_json_setting_opt(conn, column)?.unwrap_or_default())
}

/// Stores `value` as JSON in the settings column `column`; `None` stores NULL.
pub(super) fn write_json_setting<T: Serialize>(
    conn: &Connection,
    column: &str,
    value: Option<&T>,
) -> Result<()> {
    let text = value.map(serde_json::to_string).transpose()?;
    conn.execute(
        &format!("UPDATE settings SET {column} = ?2 WHERE id = ?1"),
        params![SETTINGS_ID, text],
    )?;
    Ok(())
}

/// The directory a clone lands in: `stored`, else `~/Projects` expanded.
fn clone_dir_or_built_in(stored: Option<String>) -> String {
    stored.unwrap_or_else(|| {
        crate::hostfs::expand(BUILT_IN_CLONE_DIR)
            .to_string_lossy()
            .into_owned()
    })
}

/// The stored default account names, with those of providers this build does not register; a value
/// that is not a JSON object of strings reads as none. What is rewritten starts from this, so an
/// unregistered provider's entry stays.
fn read_stored_default_account_names(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, String>> {
    read_json_setting(conn, "default_account_names")
}

/// The default account names of the registered providers.
fn read_default_account_names(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, String>> {
    Ok(registered_keys(read_stored_default_account_names(conn)?))
}

/// The stored agent tags, with those of providers this build does not register; a value that is not
/// a JSON object of string lists reads as none.
fn read_stored_agent_tags(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, Vec<String>>> {
    read_json_setting(conn, "agent_tags")
}

/// The agent tags of the registered providers.
fn read_agent_tags(conn: &Connection) -> Result<std::collections::BTreeMap<String, Vec<String>>> {
    Ok(registered_keys(read_stored_agent_tags(conn)?))
}

/// The stored agents whose default account is disabled, with providers this build does not
/// register; a value that is not a JSON array of strings reads as none.
fn read_stored_disabled_default_accounts(
    conn: &Connection,
) -> Result<std::collections::BTreeSet<String>> {
    read_json_setting(conn, "disabled_default_accounts")
}

/// The registered providers whose default account is disabled.
fn read_disabled_default_accounts(conn: &Connection) -> Result<std::collections::BTreeSet<String>> {
    let mut agents = read_stored_disabled_default_accounts(conn)?;
    agents.retain(|id| AgentId::from_wire_name(id).is_some());
    Ok(agents)
}

/// `map` without the entries keyed by a provider this build does not register: a setting kept for
/// such a provider stays in the stored value, and is not loaded.
pub(super) fn registered_keys<V>(
    mut map: std::collections::BTreeMap<String, V>,
) -> std::collections::BTreeMap<String, V> {
    map.retain(|id, _| AgentId::from_wire_name(id).is_some());
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_workflow;
    use crate::protocol::{GitDelivery, GitDevelopment, GitWorkflowOverride};

    use crate::store::test_support::*;

    /// The tags of an agent round-trip per agent, an empty list clears them and a repeat write
    /// changes nothing, and they outlive the agent's removal from the added agents.
    #[test]
    fn agent_tags_round_trip_per_agent_and_outlive_the_agents_removal() {
        let path = temp_db("agent-tags");
        let store = Store::open(&path).expect("store");
        let tags = |names: &[&str]| names.iter().map(|tag| tag.to_string()).collect::<Vec<_>>();
        assert!(store.agent_tags().unwrap().is_empty());

        assert!(store
            .set_agent_tags(AgentId::CODEX, tags(&["review", "tests"]))
            .unwrap());
        assert!(!store
            .set_agent_tags(AgentId::CODEX, tags(&["review", "tests"]))
            .unwrap());
        assert!(store
            .set_agent_tags(AgentId::CODEX, tags(&["tests", "review"]))
            .unwrap());
        assert!(store
            .set_agent_tags(AgentId::CLAUDE, tags(&["ui"]))
            .unwrap());
        assert_eq!(
            store.get_settings().unwrap().agent_tags,
            [
                ("claude".to_string(), tags(&["ui"])),
                ("codex".to_string(), tags(&["tests", "review"])),
            ]
            .into()
        );

        store
            .set_added_agents_if_undecided(&[AgentId::CLAUDE, AgentId::CODEX])
            .unwrap();
        store.remove_added_agent(AgentId::CODEX).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.agent_tags().unwrap()["codex"],
            tags(&["tests", "review"])
        );
        store.add_added_agent(AgentId::CODEX).unwrap();
        assert_eq!(
            store.agent_tags().unwrap()["codex"],
            tags(&["tests", "review"])
        );

        assert!(store.set_agent_tags(AgentId::CODEX, Vec::new()).unwrap());
        assert!(!store.set_agent_tags(AgentId::CODEX, Vec::new()).unwrap());
        assert!(store.set_agent_tags(AgentId::CLAUDE, Vec::new()).unwrap());
        assert!(store.agent_tags().unwrap().is_empty());
        let stored: Option<String> = store
            .lock()
            .query_row(
                "SELECT agent_tags FROM settings WHERE id = ?1",
                params![SETTINGS_ID],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, None);
    }

    /// Each boolean setting starts at its default, round-trips, and a repeat write reports no
    /// change; the settings are independent of one another.
    #[test]
    fn the_boolean_settings_default_round_trip_and_a_repeat_write_changes_nothing() {
        struct Setting {
            column: &'static str,
            default: bool,
            read: fn(&Settings) -> bool,
            get: fn(&Store) -> Result<bool>,
            set: fn(&Store, bool) -> Result<bool>,
        }
        let settings = [
            Setting {
                column: "auto_close_idle_shell_terminals",
                default: false,
                read: |settings| settings.auto_close_idle_shell_terminals,
                get: Store::auto_close_idle_shell_terminals,
                set: Store::set_auto_close_idle_shell_terminals,
            },
            Setting {
                column: "auto_pull_repositories",
                default: false,
                read: |settings| settings.auto_pull_repositories,
                get: Store::auto_pull_repositories,
                set: Store::set_auto_pull_repositories,
            },
            Setting {
                column: "auto_delete_unresumable_archives",
                default: true,
                read: |settings| settings.auto_delete_unresumable_archives,
                get: Store::auto_delete_unresumable_archives,
                set: Store::set_auto_delete_unresumable_archives,
            },
            Setting {
                column: "auto_approve_session_requests",
                default: false,
                read: |settings| settings.auto_approve_session_requests,
                get: Store::auto_approve_session_requests,
                set: Store::set_auto_approve_session_requests,
            },
            Setting {
                column: "auto_approve_resume_requests",
                default: false,
                read: |settings| settings.auto_approve_resume_requests,
                get: Store::auto_approve_resume_requests,
                set: Store::set_auto_approve_resume_requests,
            },
            Setting {
                column: "auto_approve_account_switches",
                default: false,
                read: |settings| settings.auto_approve_account_switches,
                get: Store::auto_approve_account_switches,
                set: Store::set_auto_approve_account_switches,
            },
            Setting {
                column: "auto_approve_handoffs",
                default: false,
                read: |settings| settings.auto_approve_handoffs,
                get: Store::auto_approve_handoffs,
                set: Store::set_auto_approve_handoffs,
            },
            Setting {
                column: "ask_questions_as_choices",
                default: true,
                read: |settings| settings.ask_questions_as_choices,
                get: Store::ask_questions_as_choices,
                set: Store::set_ask_questions_as_choices,
            },
            Setting {
                column: "auto_select_account",
                default: false,
                read: |settings| settings.auto_select_account,
                get: Store::auto_select_account,
                set: Store::set_auto_select_account,
            },
            Setting {
                column: "auto_switch_on_usage_limit",
                default: false,
                read: |settings| settings.auto_switch_on_usage_limit,
                get: Store::auto_switch_on_usage_limit,
                set: Store::set_auto_switch_on_usage_limit,
            },
            Setting {
                column: "auto_start_quota_windows",
                default: false,
                read: |settings| settings.auto_start_quota_windows,
                get: Store::auto_start_quota_windows,
                set: Store::set_auto_start_quota_windows,
            },
            Setting {
                column: "keep_awake",
                default: false,
                read: |settings| settings.keep_awake,
                get: Store::keep_awake,
                set: Store::set_keep_awake,
            },
            Setting {
                column: "resume_sessions_on_start",
                default: false,
                read: |settings| settings.resume_sessions_on_start,
                get: Store::resume_sessions_on_start,
                set: Store::set_resume_sessions_on_start,
            },
        ];

        for setting in &settings {
            let column = setting.column;
            let changed = !setting.default;
            let path = temp_db(&format!("setting-{column}"));
            let store = Store::open(&path).expect("store");
            assert_eq!(
                (setting.read)(&store.get_settings().unwrap()),
                setting.default,
                "{column} starts at its default"
            );

            assert!((setting.set)(&store, changed).unwrap(), "{column}");
            assert!(
                !(setting.set)(&store, changed).unwrap(),
                "{column}: a repeat write changes nothing"
            );
            assert_eq!((setting.get)(&store).unwrap(), changed, "{column}");
            assert_eq!(
                (setting.read)(&store.get_settings().unwrap()),
                changed,
                "{column}"
            );
            for other in settings.iter().filter(|other| other.column != column) {
                assert_eq!(
                    (other.get)(&store).unwrap(),
                    other.default,
                    "writing {column} left {} alone",
                    other.column
                );
            }
            assert!((setting.set)(&store, setting.default).unwrap(), "{column}");
        }
    }

    /// The application-wide git workflow settings start at worktree, on, ask, off, change item by
    /// item with a repeat write reporting no change.
    #[test]
    fn the_git_workflow_settings_default_and_change_per_item() {
        let path = temp_db("git-workflow-settings");
        let store = Store::open(&path).expect("store");
        assert_eq!(
            store.git_workflow_settings().unwrap(),
            GitWorkflowSettings::default()
        );
        assert_eq!(
            store.get_settings().unwrap().git_workflow,
            GitWorkflowSettings::default()
        );

        let changes = GitWorkflowSettingsChanges {
            delivery: Some(GitDelivery::PullRequest),
            revert_gate: Some(true),
            ..GitWorkflowSettingsChanges::default()
        };
        assert!(store.change_git_workflow_settings(&changes).unwrap());
        assert!(!store.change_git_workflow_settings(&changes).unwrap());
        assert!(!store
            .change_git_workflow_settings(&GitWorkflowSettingsChanges::default())
            .unwrap());
        assert_eq!(
            store.git_workflow_settings().unwrap(),
            GitWorkflowSettings {
                development: GitDevelopment::Worktree,
                squash_before_delivery: false,
                rebase_before_delivery: true,
                delivery: GitDelivery::PullRequest,
                revert_gate: true,
            }
        );
    }

    /// A project's override is stored per item with NULL for following the application-wide
    /// value, and `effective` tracks a later change of the application-wide value for the items
    /// the project does not hold.
    #[test]
    fn a_project_override_falls_back_to_the_global_value_and_follows_its_changes() {
        let path = temp_db("git-workflow-override");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        let stored = || store.get_project("project-1").unwrap().unwrap();
        assert_eq!(stored().git_workflow, GitWorkflowOverride::default());
        assert_eq!(
            git_workflow::effective(&store, &stored()).unwrap(),
            GitWorkflowSettings::default()
        );

        let mut edited = stored();
        edited.git_workflow = GitWorkflowOverride {
            development: Some(GitDevelopment::InPlace),
            squash_before_delivery: Some(false),
            delivery: Some(GitDelivery::Merge),
            ..GitWorkflowOverride::default()
        };
        store.update_project(&edited).expect("update");
        assert_eq!(stored().git_workflow, edited.git_workflow);
        assert_eq!(
            store.list_projects().unwrap()[0].git_workflow,
            edited.git_workflow
        );

        store
            .change_git_workflow_settings(&GitWorkflowSettingsChanges {
                development: Some(GitDevelopment::Branch),
                squash_before_delivery: Some(true),
                rebase_before_delivery: Some(false),
                delivery: Some(GitDelivery::PullRequest),
                revert_gate: Some(true),
            })
            .unwrap();
        assert_eq!(
            git_workflow::effective(&store, &stored()).unwrap(),
            GitWorkflowSettings {
                development: GitDevelopment::InPlace,
                squash_before_delivery: false,
                rebase_before_delivery: false,
                delivery: GitDelivery::Merge,
                revert_gate: true,
            }
        );

        let mut cleared = stored();
        cleared.git_workflow = GitWorkflowOverride::default();
        store.update_project(&cleared).expect("update");
        assert_eq!(stored().git_workflow, GitWorkflowOverride::default());
        assert!(
            git_workflow::effective(&store, &stored())
                .unwrap()
                .squash_before_delivery
        );
    }

    /// Turning a default account off and on records it per agent, a repeat write reports no change,
    /// and the last one back on clears the column to NULL, as a database that never had one reads.
    #[test]
    fn the_disabled_default_accounts_round_trip_and_clear_to_null() {
        let path = temp_db("disabled-default-accounts");
        let store = Store::open(&path).expect("store");
        let stored = |store: &Store| -> Option<String> {
            store
                .lock()
                .query_row(
                    "SELECT disabled_default_accounts FROM settings WHERE id = ?1",
                    params![SETTINGS_ID],
                    |row| row.get(0),
                )
                .unwrap()
        };
        assert!(store.disabled_default_accounts().unwrap().is_empty());
        assert!(!store.account_disabled(AgentId::CODEX, None).unwrap());
        assert!(!store
            .set_default_account_disabled(AgentId::CODEX, false)
            .unwrap());

        assert!(store
            .set_default_account_disabled(AgentId::CODEX, true)
            .unwrap());
        assert!(!store
            .set_default_account_disabled(AgentId::CODEX, true)
            .unwrap());
        assert!(store
            .set_default_account_disabled(AgentId::CLAUDE, true)
            .unwrap());
        assert_eq!(stored(&store).as_deref(), Some(r#"["claude","codex"]"#));
        assert!(store.account_disabled(AgentId::CODEX, None).unwrap());
        assert!(!store.account_disabled(AgentId::GROK, None).unwrap());
        assert_eq!(
            store.get_settings().unwrap().disabled_default_accounts,
            ["claude".to_string(), "codex".to_string()].into()
        );

        assert!(store
            .set_default_account_disabled(AgentId::CODEX, false)
            .unwrap());
        assert!(store
            .set_default_account_disabled(AgentId::CLAUDE, false)
            .unwrap());
        assert_eq!(stored(&store), None);
        assert!(store.disabled_default_accounts().unwrap().is_empty());
    }

    /// The default clone directory is the built-in one until set, and a blank value goes back to
    /// it.
    #[test]
    fn the_default_clone_dir_falls_back_to_the_built_in_one() {
        let path = temp_db("clone-dir");
        let store = Store::open(&path).expect("store");
        let built_in = crate::hostfs::expand("~/Projects")
            .to_string_lossy()
            .into_owned();
        assert_eq!(store.default_clone_dir().unwrap(), built_in);

        assert!(store.set_default_clone_dir(Some("/work/repos")).unwrap());
        assert!(!store.set_default_clone_dir(Some("/work/repos")).unwrap());
        assert_eq!(
            store.get_settings().unwrap().default_clone_dir,
            "/work/repos"
        );
        assert!(store.set_default_clone_dir(None).unwrap());
        assert_eq!(store.default_clone_dir().unwrap(), built_in);
    }

    /// The interface language is unset until a client reports one, a repeat of the same tag
    /// changes nothing, and a later one replaces it.
    #[test]
    fn the_interface_language_is_unset_until_reported() {
        let path = temp_db("interface-language");
        let store = Store::open(&path).expect("store");
        assert_eq!(store.interface_language().unwrap(), None);

        assert!(store.set_interface_language("ja").unwrap());
        assert!(!store.set_interface_language("ja").unwrap());
        assert_eq!(
            store.get_settings().unwrap().interface_language.as_deref(),
            Some("ja")
        );
        assert!(store.set_interface_language("de").unwrap());
        assert_eq!(store.interface_language().unwrap().as_deref(), Some("de"));
    }

    /// The JSON settings that name providers load only the registered ones, and a write keeps the
    /// entries of a provider this build does not register.
    #[test]
    fn settings_naming_an_unregistered_provider_are_left_out_of_a_load_and_kept_through_writes() {
        let path = temp_db("settings-unregistered-provider");
        let store = Store::open(&path).expect("store");
        store
            .lock()
            .execute_batch(
                r#"UPDATE settings SET
                       added_agents = '["retired","claude"]',
                       account_order = '{"retired":["x",null]}',
                       default_account_names = '{"retired":"Old"}',
                       disabled_default_accounts = '["retired"]',
                       agent_tags = '{"retired":["old"]}'"#,
            )
            .unwrap();
        let settings = store.get_settings().unwrap();
        assert_eq!(settings.added_agents, Some(vec![AgentId::CLAUDE]));
        assert!(settings.account_order.is_empty());
        assert!(settings.default_account_names.is_empty());
        assert!(settings.disabled_default_accounts.is_empty());
        assert!(settings.agent_tags.is_empty());

        assert!(store.add_added_agent(AgentId::CODEX).unwrap());
        assert!(store
            .set_agent_tags(AgentId::CODEX, vec!["new".to_string()])
            .unwrap());
        assert!(store
            .set_default_account_name(AgentId::CODEX, Some("Mine"))
            .unwrap());
        assert!(store
            .set_default_account_disabled(AgentId::CODEX, true)
            .unwrap());
        assert!(store.reorder_accounts(AgentId::CODEX, &[None]).unwrap());
        let raw = |column: &str| -> String {
            store
                .lock()
                .query_row(&format!("SELECT {column} FROM settings"), [], |row| {
                    row.get(0)
                })
                .unwrap()
        };
        assert_eq!(raw("added_agents"), r#"["claude","codex","retired"]"#);
        for column in [
            "account_order",
            "default_account_names",
            "disabled_default_accounts",
            "agent_tags",
        ] {
            assert!(raw(column).contains("retired"), "{column}");
        }
        assert_eq!(
            store
                .get_settings()
                .unwrap()
                .agent_tags
                .keys()
                .collect::<Vec<_>>(),
            ["codex"]
        );
    }
}
