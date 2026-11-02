//! The application settings: the single settings row's flags, default account names, approval
//! modes, Git workflow and clone directory.

use anyhow::Result;
use rusqlite::{params, Connection};

use super::agents::{read_account_order, read_added_agents};
use super::rows::{enum_from_row, enum_from_row_opt, enum_to_text};
use super::{Store, SETTINGS_ID};
use crate::git_workflow::{GitWorkflowSettings, GitWorkflowSettingsChanges};
use crate::protocol::{Agent, ApprovalReviewer, ClaudeApprovalMode, Settings};

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

    pub fn get_settings(&self) -> Result<Settings> {
        let account_order = read_account_order(&self.lock())?;
        Ok(Settings {
            auto_pull_repositories: self.auto_pull_repositories()?,
            auto_delete_unresumable_archives: self.auto_delete_unresumable_archives()?,
            auto_approve_session_requests: self.auto_approve_session_requests()?,
            auto_approve_resume_requests: self.auto_approve_resume_requests()?,
            auto_approve_account_switches: self.auto_approve_account_switches()?,
            auto_approve_handoffs: self.auto_approve_handoffs()?,
            ask_questions_as_choices: self.ask_questions_as_choices()?,
            interface_language: self.interface_language()?,
            auto_select_account: self.auto_select_account()?,
            auto_switch_on_usage_limit: self.auto_switch_on_usage_limit()?,
            sidebar_order_by_status: self.sidebar_order_by_status()?,
            auto_start_quota_windows: self.auto_start_quota_windows()?,
            default_clone_dir: self.default_clone_dir()?,
            accounts: self.list_accounts()?,
            account_order,
            added_agents: self.added_agents()?,
            codex_approval_reviewer: self.codex_approval_reviewer()?,
            claude_approval_mode: self.claude_approval_mode()?,
            claude_compatibility: self.claude_compatibility()?,
            agent_tags: self.agent_tags()?,
            default_account_names: self.default_account_names()?,
            disabled_default_accounts: self.disabled_default_accounts()?,
            git_workflow: self.git_workflow_settings()?,
        })
    }

    /// The names the user gave the agents' default accounts, by the agent's wire name. A value that
    /// is not a JSON object of strings reads as no names.
    pub fn default_account_names(&self) -> Result<std::collections::BTreeMap<String, String>> {
        read_default_account_names(&self.lock())
    }

    /// The choices the user made for the Claude compatibility setting, by the agent's wire name.
    /// An agent they never chose for has no entry. A value that is not a JSON object of booleans
    /// reads as no choices.
    pub fn claude_compatibility_choices(&self) -> Result<std::collections::BTreeMap<String, bool>> {
        read_claude_compatibility_choices(&self.lock())
    }

    /// Whether `agent`'s sessions are to load Claude Code's instructions, skills and the rest: the
    /// user's choice when they made one. Until then Grok Build follows its own default, which is
    /// on, and Codex is on while Claude Code is among the added agents, so adding or removing it
    /// moves the value. Claude Code itself has no such setting and reads as off.
    pub fn claude_compatibility_of(&self, agent: Agent) -> Result<bool> {
        let conn = self.lock();
        let choices = read_claude_compatibility_choices(&conn)?;
        Ok(effective_claude_compatibility(
            agent,
            &choices,
            read_added_agents(&conn)?.as_deref(),
        ))
    }

    /// The effective setting for every agent that has one, by wire name.
    pub fn claude_compatibility(&self) -> Result<std::collections::BTreeMap<String, bool>> {
        let conn = self.lock();
        let choices = read_claude_compatibility_choices(&conn)?;
        let added = read_added_agents(&conn)?;
        Ok(Agent::ALL
            .into_iter()
            .filter(|agent| agent.has_claude_compatibility())
            .map(|agent| {
                (
                    agent.wire_name().to_string(),
                    effective_claude_compatibility(agent, &choices, added.as_deref()),
                )
            })
            .collect())
    }

    /// Records the user's choice of `enabled` for `agent`'s Claude compatibility. The read and the
    /// write are one step under the store's lock. Whether the stored choice changed.
    pub fn set_claude_compatibility(&self, agent: Agent, enabled: bool) -> Result<bool> {
        let conn = self.lock();
        let mut choices = read_claude_compatibility_choices(&conn)?;
        if choices.insert(agent.wire_name().to_string(), enabled) == Some(enabled) {
            return Ok(false);
        }
        conn.execute(
            "UPDATE settings SET claude_compatibility = ?2 WHERE id = ?1",
            params![
                SETTINGS_ID,
                serde_json::to_string(&choices).expect("a map of booleans serializes to JSON")
            ],
        )?;
        Ok(true)
    }

    /// The tags the user put on each agent, by wire name, for the agents that have any. A value
    /// that is not a JSON object of string lists reads as none.
    pub fn agent_tags(&self) -> Result<std::collections::BTreeMap<String, Vec<String>>> {
        read_agent_tags(&self.lock())
    }

    /// Replaces `agent`'s tags with `tags`, which the caller has normalised; an empty list clears
    /// them. The read and the write are one step under the store's lock. Whether the stored tags
    /// changed.
    pub fn set_agent_tags(&self, agent: Agent, tags: Vec<String>) -> Result<bool> {
        let conn = self.lock();
        let mut all = read_agent_tags(&conn)?;
        let changed = if tags.is_empty() {
            all.remove(agent.wire_name()).is_some()
        } else {
            all.insert(agent.wire_name().to_string(), tags.clone()) != Some(tags)
        };
        if !changed {
            return Ok(false);
        }
        let stored = (!all.is_empty()).then(|| {
            serde_json::to_string(&all).expect("a map of string lists serializes to JSON")
        });
        conn.execute(
            "UPDATE settings SET agent_tags = ?2 WHERE id = ?1",
            params![SETTINGS_ID, stored],
        )?;
        Ok(true)
    }

    /// The agents, by wire name, whose default account is turned off for new work. A value that is
    /// not a JSON array of strings reads as none.
    pub fn disabled_default_accounts(&self) -> Result<std::collections::BTreeSet<String>> {
        read_disabled_default_accounts(&self.lock())
    }

    /// Turns `agent`'s default account off for new work, or back on. The read and the write are
    /// one step under the store's lock. Whether the stored set changed.
    pub fn set_default_account_disabled(&self, agent: Agent, disabled: bool) -> Result<bool> {
        let conn = self.lock();
        let mut agents = read_disabled_default_accounts(&conn)?;
        let changed = if disabled {
            agents.insert(agent.wire_name().to_string())
        } else {
            agents.remove(agent.wire_name())
        };
        if !changed {
            return Ok(false);
        }
        let stored = (!agents.is_empty())
            .then(|| serde_json::to_string(&agents).expect("a set of strings serializes to JSON"));
        conn.execute(
            "UPDATE settings SET disabled_default_accounts = ?2 WHERE id = ?1",
            params![SETTINGS_ID, stored],
        )?;
        Ok(true)
    }

    /// Whether the user turned `account_id` (`None` the default account) of `agent` off for new
    /// work. An id that is no account of `agent`, or no longer on record, is not disabled.
    pub fn account_disabled(&self, agent: Agent, account_id: Option<&str>) -> Result<bool> {
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
    pub fn default_account_name(&self, agent: Agent) -> Result<String> {
        Ok(
            crate::protocol::default_account_name_in(&self.default_account_names()?, agent)
                .to_string(),
        )
    }

    /// What an account is called: the stored account's name (its id when it is gone), or for
    /// `None`, the default account's name.
    pub fn account_display_name(&self, agent: Agent, account_id: Option<&str>) -> Result<String> {
        match account_id {
            Some(id) => Ok(self
                .get_account(id)?
                .map_or_else(|| id.to_string(), |account| account.name)),
            None => self.default_account_name(agent),
        }
    }

    /// Names the agent's default account, or clears the name with `None`. The read and the write
    /// are one step under the store's lock. Whether the stored names changed.
    pub fn set_default_account_name(&self, agent: Agent, name: Option<&str>) -> Result<bool> {
        let conn = self.lock();
        let mut names = read_default_account_names(&conn)?;
        let before = names.clone();
        match name {
            Some(name) => names.insert(agent.wire_name().to_string(), name.to_string()),
            None => names.remove(agent.wire_name()),
        };
        if names == before {
            return Ok(false);
        }
        let stored = (!names.is_empty())
            .then(|| serde_json::to_string(&names).expect("a map of strings serializes to JSON"));
        conn.execute(
            "UPDATE settings SET default_account_names = ?2 WHERE id = ?1",
            params![SETTINGS_ID, stored],
        )?;
        Ok(true)
    }

    pub fn codex_approval_reviewer(&self) -> Result<Option<ApprovalReviewer>> {
        Ok(self.lock().query_row(
            "SELECT codex_approval_reviewer FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| enum_from_row_opt(row, 0),
        )?)
    }

    pub fn set_codex_approval_reviewer(&self, reviewer: Option<ApprovalReviewer>) -> Result<()> {
        self.lock().execute(
            "UPDATE settings SET codex_approval_reviewer = ?2 WHERE id = ?1",
            params![SETTINGS_ID, reviewer.as_ref().map(enum_to_text)],
        )?;
        Ok(())
    }

    pub fn claude_approval_mode(&self) -> Result<Option<ClaudeApprovalMode>> {
        Ok(self.lock().query_row(
            "SELECT claude_approval_mode FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| enum_from_row_opt(row, 0),
        )?)
    }

    pub fn set_claude_approval_mode(&self, mode: Option<ClaudeApprovalMode>) -> Result<()> {
        self.lock().execute(
            "UPDATE settings SET claude_approval_mode = ?2 WHERE id = ?1",
            params![SETTINGS_ID, mode.as_ref().map(enum_to_text)],
        )?;
        Ok(())
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
            "UPDATE settings SET development = ?2, rebase_before_merge = ?3, delivery = ?4,
                                 revert_gate = ?5, squash_before_delivery = ?6
             WHERE id = ?1",
            params![
                SETTINGS_ID,
                enum_to_text(&next.development),
                next.rebase_before_merge,
                enum_to_text(&next.delivery),
                next.revert_gate,
                next.squash_before_delivery,
            ],
        )?;
        Ok(true)
    }

    /// The tag of the language the user last picked for the interface, as a client reported it;
    /// `None` until one has.
    pub fn interface_language(&self) -> Result<Option<String>> {
        Ok(self.lock().query_row(
            "SELECT interface_language FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?)
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

    /// The directory a clone lands in when none is named: the stored one, else `~/Projects`
    /// expanded, which is as absolute and lexically normalised as the home it expands from.
    pub fn default_clone_dir(&self) -> Result<String> {
        let stored: Option<String> = self.lock().query_row(
            "SELECT default_clone_dir FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?;
        Ok(stored.unwrap_or_else(|| {
            crate::hostfs::expand(BUILT_IN_CLONE_DIR)
                .to_string_lossy()
                .into_owned()
        }))
    }

    /// Stores the already-normalised directory, or `None` to go back to the built-in default.
    /// Whether the setting actually changed.
    pub fn set_default_clone_dir(&self, value: Option<&str>) -> Result<bool> {
        let conn = self.lock();
        let current: Option<String> = conn.query_row(
            "SELECT default_clone_dir FROM settings WHERE id = ?1",
            params![SETTINGS_ID],
            |row| row.get(0),
        )?;
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
                    self.read_bool_setting(concat!(
                        "SELECT ", stringify!($get), " FROM settings WHERE id = ?1"
                    ))
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
    auto_pull_repositories, set_auto_pull_repositories;
    auto_delete_unresumable_archives, set_auto_delete_unresumable_archives;
    auto_approve_session_requests, set_auto_approve_session_requests;
    auto_approve_resume_requests, set_auto_approve_resume_requests;
    auto_approve_account_switches, set_auto_approve_account_switches;
    auto_approve_handoffs, set_auto_approve_handoffs;
    ask_questions_as_choices, set_ask_questions_as_choices;
    auto_select_account, set_auto_select_account;
    auto_switch_on_usage_limit, set_auto_switch_on_usage_limit;
    sidebar_order_by_status, set_sidebar_order_by_status;
    auto_start_quota_windows, set_auto_start_quota_windows;
}

impl Store {
    /// The switch `sql` selects from the settings row, the statement `bool_settings!` builds for
    /// its column.
    fn read_bool_setting(&self, sql: &'static str) -> Result<bool> {
        Ok(self
            .lock()
            .query_row(sql, params![SETTINGS_ID], |row| row.get(0))?)
    }

    /// Runs `sql`, the statement `bool_settings!` builds to write a column only when it differs
    /// from `value`. Whether it did.
    fn write_bool_setting(&self, sql: &'static str, value: bool) -> Result<bool> {
        let changed = self.lock().execute(sql, params![SETTINGS_ID, value])?;
        Ok(changed > 0)
    }
}

fn read_git_workflow_settings(conn: &Connection) -> Result<GitWorkflowSettings> {
    Ok(conn.query_row(
        "SELECT development, rebase_before_merge, delivery, revert_gate, squash_before_delivery
         FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| {
            Ok(GitWorkflowSettings {
                development: enum_from_row(row, 0)?,
                squash_before_delivery: row.get(4)?,
                rebase_before_merge: row.get(1)?,
                delivery: enum_from_row(row, 2)?,
                revert_gate: row.get(3)?,
            })
        },
    )?)
}

/// The stored default account names; a value that is not a JSON object of strings reads as none.
fn read_default_account_names(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, String>> {
    let text: Option<String> = conn.query_row(
        "SELECT default_account_names FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| row.get(0),
    )?;
    Ok(text
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

/// The stored Claude compatibility choices; a value that is not a JSON object of booleans reads as
/// none.
fn read_claude_compatibility_choices(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, bool>> {
    let text: Option<String> = conn.query_row(
        "SELECT claude_compatibility FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| row.get(0),
    )?;
    Ok(text
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

/// The stored agent tags; a value that is not a JSON object of string lists reads as none.
fn read_agent_tags(conn: &Connection) -> Result<std::collections::BTreeMap<String, Vec<String>>> {
    let text: Option<String> = conn.query_row(
        "SELECT agent_tags FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| row.get(0),
    )?;
    Ok(text
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

/// The Claude compatibility setting in effect for `agent` given the user's `choices` and the
/// added agents (`None` while undecided, which counts as Claude Code not being among them).
fn effective_claude_compatibility(
    agent: Agent,
    choices: &std::collections::BTreeMap<String, bool>,
    added: Option<&[Agent]>,
) -> bool {
    if !agent.has_claude_compatibility() {
        return false;
    }
    choices
        .get(agent.wire_name())
        .copied()
        .unwrap_or_else(|| match agent {
            Agent::Codex => added.is_some_and(|added| added.contains(&Agent::Claude)),
            _ => true,
        })
}

/// The stored agents whose default account is disabled; a value that is not a JSON array of strings
/// reads as none.
fn read_disabled_default_accounts(conn: &Connection) -> Result<std::collections::BTreeSet<String>> {
    let text: Option<String> = conn.query_row(
        "SELECT disabled_default_accounts FROM settings WHERE id = ?1",
        params![SETTINGS_ID],
        |row| row.get(0),
    )?;
    Ok(text
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_workflow::{self, Delivery, Development, GitWorkflowOverride};

    use crate::store::test_support::*;

    /// Until the user chooses, Grok Build's Claude compatibility is on and Codex's follows whether
    /// Claude Code is an added agent; a choice is stored per agent and wins from then on; Claude
    /// Code never has the setting.
    #[test]
    fn the_claude_compatibility_follows_added_agents_until_chosen_and_round_trips_per_agent() {
        let path = temp_db("claude-compatibility");
        let store = Store::open(&path).expect("store");
        let stored = |store: &Store| -> Option<String> {
            store
                .lock()
                .query_row(
                    "SELECT claude_compatibility FROM settings WHERE id = ?1",
                    params![SETTINGS_ID],
                    |row| row.get(0),
                )
                .unwrap()
        };
        let on = |store: &Store, agent| store.claude_compatibility_of(agent).unwrap();
        assert!(!on(&store, Agent::Codex));
        assert!(on(&store, Agent::Grok));
        assert!(!on(&store, Agent::Claude));

        store
            .set_added_agents_if_undecided(&[Agent::Codex])
            .unwrap();
        assert!(!on(&store, Agent::Codex));
        store.add_added_agent(Agent::Claude).unwrap();
        assert!(on(&store, Agent::Codex));
        assert_eq!(stored(&store), None);
        assert!(store.claude_compatibility_choices().unwrap().is_empty());

        assert!(store.set_claude_compatibility(Agent::Codex, false).unwrap());
        assert!(!store.set_claude_compatibility(Agent::Codex, false).unwrap());
        assert_eq!(stored(&store).as_deref(), Some(r#"{"codex":false}"#));
        assert!(!on(&store, Agent::Codex));
        assert!(on(&store, Agent::Grok));
        assert_eq!(
            store.get_settings().unwrap().claude_compatibility,
            [("codex".to_string(), false), ("grok".to_string(), true)].into()
        );

        // A choice outlives the added agents, whichever way it went.
        assert!(store.set_claude_compatibility(Agent::Codex, true).unwrap());
        store.remove_added_agent(Agent::Claude).unwrap();
        assert!(on(&store, Agent::Codex));
        assert!(store.set_claude_compatibility(Agent::Grok, false).unwrap());
        assert!(!on(&store, Agent::Grok));
    }

    /// The tags of an agent round-trip per agent, an empty list clears them and a repeat write
    /// changes nothing, and they outlive the agent's removal from the added agents.
    #[test]
    fn agent_tags_round_trip_per_agent_and_outlive_the_agents_removal() {
        let path = temp_db("agent-tags");
        let store = Store::open(&path).expect("store");
        let tags = |names: &[&str]| names.iter().map(|tag| tag.to_string()).collect::<Vec<_>>();
        assert!(store.agent_tags().unwrap().is_empty());

        assert!(store
            .set_agent_tags(Agent::Codex, tags(&["review", "tests"]))
            .unwrap());
        assert!(!store
            .set_agent_tags(Agent::Codex, tags(&["review", "tests"]))
            .unwrap());
        assert!(store
            .set_agent_tags(Agent::Codex, tags(&["tests", "review"]))
            .unwrap());
        assert!(store.set_agent_tags(Agent::Claude, tags(&["ui"])).unwrap());
        assert_eq!(
            store.get_settings().unwrap().agent_tags,
            [
                ("claude".to_string(), tags(&["ui"])),
                ("codex".to_string(), tags(&["tests", "review"])),
            ]
            .into()
        );

        store
            .set_added_agents_if_undecided(&[Agent::Claude, Agent::Codex])
            .unwrap();
        store.remove_added_agent(Agent::Codex).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.agent_tags().unwrap()["codex"],
            tags(&["tests", "review"])
        );
        store.add_added_agent(Agent::Codex).unwrap();
        assert_eq!(
            store.agent_tags().unwrap()["codex"],
            tags(&["tests", "review"])
        );

        assert!(store.set_agent_tags(Agent::Codex, Vec::new()).unwrap());
        assert!(!store.set_agent_tags(Agent::Codex, Vec::new()).unwrap());
        assert!(store.set_agent_tags(Agent::Claude, Vec::new()).unwrap());
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

    /// The approval settings read as unset until chosen, and round-trip.
    #[test]
    fn approval_settings_round_trip() {
        let path = temp_db("approval-columns");
        let store = Store::open(&path).unwrap();
        assert_eq!(store.codex_approval_reviewer().unwrap(), None);
        assert_eq!(store.claude_approval_mode().unwrap(), None);

        store
            .set_codex_approval_reviewer(Some(ApprovalReviewer::AutoReview))
            .unwrap();
        store
            .set_claude_approval_mode(Some(ClaudeApprovalMode::Bypass))
            .unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        let settings = store.get_settings().unwrap();
        assert_eq!(
            settings.codex_approval_reviewer,
            Some(ApprovalReviewer::AutoReview)
        );
        assert_eq!(
            settings.claude_approval_mode,
            Some(ClaudeApprovalMode::Bypass)
        );
        store.set_codex_approval_reviewer(None).unwrap();
        assert_eq!(store.codex_approval_reviewer().unwrap(), None);
        store.set_claude_approval_mode(None).unwrap();
        assert_eq!(store.claude_approval_mode().unwrap(), None);
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
                column: "sidebar_order_by_status",
                default: true,
                read: |settings| settings.sidebar_order_by_status,
                get: Store::sidebar_order_by_status,
                set: Store::set_sidebar_order_by_status,
            },
            Setting {
                column: "auto_start_quota_windows",
                default: false,
                read: |settings| settings.auto_start_quota_windows,
                get: Store::auto_start_quota_windows,
                set: Store::set_auto_start_quota_windows,
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
            delivery: Some(Delivery::PullRequest),
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
                development: Development::Worktree,
                squash_before_delivery: false,
                rebase_before_merge: true,
                delivery: Delivery::PullRequest,
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
            development: Some(Development::InPlace),
            squash_before_delivery: Some(false),
            delivery: Some(Delivery::Merge),
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
                development: Some(Development::Branch),
                squash_before_delivery: Some(true),
                rebase_before_merge: Some(false),
                delivery: Some(Delivery::PullRequest),
                revert_gate: Some(true),
            })
            .unwrap();
        assert_eq!(
            git_workflow::effective(&store, &stored()).unwrap(),
            GitWorkflowSettings {
                development: Development::InPlace,
                squash_before_delivery: false,
                rebase_before_merge: false,
                delivery: Delivery::Merge,
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
        assert!(!store.account_disabled(Agent::Codex, None).unwrap());
        assert!(!store
            .set_default_account_disabled(Agent::Codex, false)
            .unwrap());

        assert!(store
            .set_default_account_disabled(Agent::Codex, true)
            .unwrap());
        assert!(!store
            .set_default_account_disabled(Agent::Codex, true)
            .unwrap());
        assert!(store
            .set_default_account_disabled(Agent::Claude, true)
            .unwrap());
        assert_eq!(stored(&store).as_deref(), Some(r#"["claude","codex"]"#));
        assert!(store.account_disabled(Agent::Codex, None).unwrap());
        assert!(!store.account_disabled(Agent::Grok, None).unwrap());
        assert_eq!(
            store.get_settings().unwrap().disabled_default_accounts,
            ["claude".to_string(), "codex".to_string()].into()
        );

        assert!(store
            .set_default_account_disabled(Agent::Codex, false)
            .unwrap());
        assert!(store
            .set_default_account_disabled(Agent::Claude, false)
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
}
