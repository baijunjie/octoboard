//! The accounts table: the agent accounts the user added, their enabled and sharing state.

use anyhow::Result;
use rusqlite::{params, OptionalExtension, Row};

use super::rows::{enum_from_row, enum_to_text};
use super::Store;
use crate::protocol::{error_code, now_millis, Account, CodedError};

/// What turning an account's sharing on recorded when it started; see
/// [`Store::account_sharing_start`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharingStart {
    pub stamp: String,
    pub default_dir: String,
    /// Turning sharing on has completed once since it started: what it carried over from the
    /// account's own Codex database is not carried again, since it may have been taken back since.
    pub completed: bool,
}

impl Store {
    pub fn insert_account(&self, account: &Account) -> Result<()> {
        self.lock()
            .execute(
                "INSERT INTO accounts (id, agent, name, config_dir, created_at, share_default_config, share_default_config_incomplete, disabled) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    account.id,
                    enum_to_text(&account.agent),
                    account.name,
                    account.config_dir,
                    now_millis(),
                    account.share_default_config,
                    account.share_default_config_incomplete,
                    account.disabled,
                ],
            )
            .map_err(|err| account_write_error(err, account))?;
        Ok(())
    }

    /// Renames the account, repoints it, or both — whatever the caller already decided to write;
    /// this does not itself check the name for a collision, which is `coordinator`'s business. The
    /// table's own `accounts_agent_name_unique` index is the backstop for that check, not a
    /// replacement for it.
    ///
    /// The coordinator turns an account's sharing off before repointing it, so a new directory
    /// starts out not sharing: the choice is recorded off and complete, and what turning sharing
    /// on recorded of the old directory is forgotten. A directory spelled differently that
    /// compares as the same one (`account_dir_key`, folding case on macOS) is no repoint: what
    /// sharing recorded stays, so its backups are still restored.
    pub fn update_account(&self, account: &Account) -> Result<()> {
        let conn = self.lock();
        let current: Option<String> = conn
            .query_row(
                "SELECT config_dir FROM accounts WHERE id = ?1",
                params![account.id],
                |row| row.get(0),
            )
            .optional()?;
        let same_dir = current.is_some_and(|current| {
            crate::account_dirs::account_dir_key(&current)
                == crate::account_dirs::account_dir_key(&account.config_dir)
        });
        conn.execute(
            "UPDATE accounts SET name = ?2, config_dir = ?3,
                 share_default_config = CASE WHEN ?4 THEN share_default_config ELSE 0 END,
                 share_default_config_incomplete = CASE WHEN ?4
                     THEN share_default_config_incomplete ELSE 0 END,
                 share_default_config_backup = CASE WHEN ?4
                     THEN share_default_config_backup ELSE NULL END,
                 share_default_config_dir = CASE WHEN ?4
                     THEN share_default_config_dir ELSE NULL END,
                 share_default_config_completed = CASE WHEN ?4
                     THEN share_default_config_completed ELSE 0 END
             WHERE id = ?1",
            params![account.id, account.name, account.config_dir, same_dir],
        )
        .map_err(|err| account_write_error(err, account))?;
        Ok(())
    }

    /// Turns the account off for new work, or back on. Whether the stored flag changed.
    pub fn set_account_disabled(&self, id: &str, disabled: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE accounts SET disabled = ?2 WHERE id = ?1 AND disabled != ?2",
            params![id, disabled],
        )?;
        Ok(changed > 0)
    }

    /// Records the account's sharing choice and whether the last change toward it is incomplete.
    pub fn set_account_sharing(&self, id: &str, share: bool, incomplete: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE accounts SET share_default_config = ?2, share_default_config_incomplete = ?3
             WHERE id = ?1",
            params![id, share, incomplete],
        )?;
        Ok(())
    }

    /// What turning sharing on recorded when it started on the account's directory: the stamp
    /// its backups are named with (`<item>.bak-<stamp>`), the default directory, resolved, its
    /// links lead into, and whether it has completed since. `None` when sharing was never started
    /// there, or was undone since.
    pub fn account_sharing_start(&self, id: &str) -> Result<Option<SharingStart>> {
        let start = self
            .lock()
            .query_row(
                "SELECT share_default_config_backup, share_default_config_dir,
                     share_default_config_completed
                 FROM accounts WHERE id = ?1",
                params![id],
                |row| {
                    let (stamp, default_dir) = (
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    );
                    Ok(stamp
                        .zip(default_dir)
                        .map(|(stamp, default_dir)| SharingStart {
                            stamp,
                            default_dir,
                            completed: row.get(2).unwrap_or(false),
                        }))
                },
            )
            .optional()?;
        Ok(start.flatten())
    }

    /// Records that turning sharing on starts on `config_dir`, unless the account has been
    /// repointed meanwhile; answers whether it was recorded.
    pub fn start_account_sharing(
        &self,
        id: &str,
        config_dir: &str,
        start: &SharingStart,
    ) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE accounts SET share_default_config_backup = ?3, share_default_config_dir = ?4,
                 share_default_config_completed = 0
             WHERE id = ?1 AND config_dir = ?2",
            params![id, config_dir, start.stamp, start.default_dir],
        )?;
        Ok(changed > 0)
    }

    /// Records the account's directory `config_dir` brought in line with the choice `share`: the
    /// start is marked completed, or forgotten once sharing is undone, and the account is no
    /// longer incomplete. Nothing is written, and `false` answered, when the account has been
    /// repointed or its choice changed meanwhile.
    pub fn finish_account_sharing(&self, id: &str, config_dir: &str, share: bool) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE accounts SET share_default_config_incomplete = 0,
                 share_default_config_backup = CASE WHEN ?3 THEN share_default_config_backup END,
                 share_default_config_dir = CASE WHEN ?3 THEN share_default_config_dir END,
                 share_default_config_completed = ?3
             WHERE id = ?1 AND config_dir = ?2 AND share_default_config = ?3",
            params![id, config_dir, share],
        )?;
        Ok(changed > 0)
    }

    pub fn get_account(&self, id: &str) -> Result<Option<Account>> {
        let account = self
            .lock()
            .query_row(
                "SELECT id, agent, name, config_dir, share_default_config, share_default_config_incomplete, disabled FROM accounts WHERE id = ?1",
                params![id],
                read_account,
            )
            .optional()?;
        Ok(account)
    }

    /// The directory of the account `account_id` names, `None` for the default account, which pins
    /// nothing, and for an id that is no longer on record.
    pub fn account_dir(&self, account_id: &Option<String>) -> Result<Option<String>> {
        Ok(match account_id {
            None => None,
            Some(id) => self.get_account(id)?.map(|account| account.config_dir),
        })
    }

    /// Every account of every agent, oldest first.
    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare("SELECT id, agent, name, config_dir, share_default_config, share_default_config_incomplete, disabled FROM accounts ORDER BY created_at")?;
        let rows = stmt
            .query_map([], read_account)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Removes the account, after clearing it from every console and project that refers to it —
    /// which puts each console back on its agent's default account and each project back to
    /// inheriting its console's — in the same locked step, so no reader ever sees one still
    /// pointing at an id the `accounts` table no longer has. A project that pins the default
    /// account refers to no id, so it is left alone.
    pub fn delete_account(&self, id: &str) -> Result<()> {
        let conn = self.lock();
        for column in ["claude_account_id", "codex_account_id", "grok_account_id"] {
            for table in ["consoles", "projects"] {
                conn.execute(
                    &format!("UPDATE {table} SET {column} = NULL WHERE {column} = ?1"),
                    params![id],
                )?;
            }
        }
        conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        Ok(())
    }
}

/// Turns the `accounts_agent_name_unique` index tripping into the protocol's own
/// `account_name_taken` refusal, so an insert or update that reaches this far surfaces the same
/// way as the collision check `coordinator` already runs beforehand — this only fires when the
/// two disagree, which is two requests racing past that check. Any other error is passed through
/// unchanged.
fn account_write_error(err: rusqlite::Error, account: &Account) -> anyhow::Error {
    let is_name_collision = matches!(
        &err,
        rusqlite::Error::SqliteFailure(sqlite_err, _)
            if sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation
    );
    if is_name_collision {
        return CodedError::raised(
            error_code::ACCOUNT_NAME_TAKEN,
            format!(
                "the name `{}` is already used for {}",
                account.name,
                account.agent.label()
            ),
            &[("agent", account.agent.label()), ("name", &account.name)],
        );
    }
    err.into()
}

fn read_account(row: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get(0)?,
        agent: enum_from_row(row, 1)?,
        name: row.get(2)?,
        config_dir: row.get(3)?,
        share_default_config: row.get(4)?,
        share_default_config_incomplete: row.get(5)?,
        disabled: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Agent, ProjectAccount};

    use crate::store::test_support::*;

    /// Removing an account clears every console's and project's reference to it, in the same step,
    /// so no reader is ever left pointing at an id the `accounts` table no longer has.
    #[test]
    fn deleting_an_account_clears_every_consoles_and_projects_reference_to_it() {
        let path = temp_db("delete-account");
        let store = Store::open(&path).expect("store");
        store
            .insert_account(&account(
                "claude-acct",
                Agent::Claude,
                "/home/u/.claude-alt",
            ))
            .expect("account");
        store
            .insert_console(&console(Some("claude-acct"), None, None))
            .expect("insert");
        let mut other = console(Some("claude-acct"), None, None);
        other.id = "console-2".to_string();
        store.insert_console(&other).expect("insert");
        store
            .insert_account(&account(
                "claude-other",
                Agent::Claude,
                "/home/u/.claude-other",
            ))
            .expect("account");
        let mut untouched = console(Some("claude-other"), None, None);
        untouched.id = "console-3".to_string();
        store.insert_console(&untouched).expect("insert");
        let mut referring = project(false);
        referring.accounts.claude = named("claude-acct");
        // Another agent's setting of the same project is not touched by a Claude account going.
        referring.accounts.codex = ProjectAccount::Default;
        store.insert_project(&referring).expect("insert");
        // A project that pins the default account refers to no id, so it keeps saying so.
        let mut defaulting = project(false);
        defaulting.id = "project-2".to_string();
        defaulting.path = "/tmp/project-2".to_string();
        defaulting.accounts.claude = ProjectAccount::Default;
        store.insert_project(&defaulting).expect("insert");
        let mut inheriting = project(false);
        inheriting.id = "project-3".to_string();
        inheriting.path = "/tmp/project-3".to_string();
        store.insert_project(&inheriting).expect("insert");

        store.delete_account("claude-acct").expect("deleted");
        let claude_account = |id: &str| store.get_project(id).unwrap().unwrap().accounts.claude;
        assert_eq!(claude_account("project-1"), ProjectAccount::Inherit);
        assert_eq!(claude_account("project-2"), ProjectAccount::Default);
        assert_eq!(claude_account("project-3"), ProjectAccount::Inherit);
        assert_eq!(
            store
                .get_project("project-1")
                .unwrap()
                .unwrap()
                .accounts
                .codex,
            ProjectAccount::Default
        );
        assert!(store.get_account("claude-acct").unwrap().is_none());
        for id in ["console-1", "console-2"] {
            let console = store.get_console(id).unwrap().unwrap();
            assert_eq!(console.accounts.claude, None);
        }
        let untouched = store.get_console("console-3").unwrap().unwrap();
        assert_eq!(untouched.accounts.claude.as_deref(), Some("claude-other"));
    }

    /// An account's name and directory round-trip, and `update_account` can change either
    /// independently of the other.
    #[test]
    fn an_accounts_name_and_directory_round_trip_and_update_independently() {
        let path = temp_db("account-round-trip");
        let store = Store::open(&path).expect("store");
        store
            .insert_account(&account("acct-1", Agent::Codex, "/home/u/.codex-alt"))
            .expect("account");

        let mut renamed = store.get_account("acct-1").unwrap().unwrap();
        renamed.name = "Work".to_string();
        store.update_account(&renamed).expect("renamed");
        let after_rename = store.get_account("acct-1").unwrap().unwrap();
        assert_eq!(after_rename.name, "Work");
        assert_eq!(after_rename.config_dir, "/home/u/.codex-alt");

        let mut repointed = after_rename;
        repointed.config_dir = "/home/u/.codex-work".to_string();
        store.update_account(&repointed).expect("repointed");
        let after_repoint = store.get_account("acct-1").unwrap().unwrap();
        assert_eq!(after_repoint.name, "Work");
        assert_eq!(after_repoint.config_dir, "/home/u/.codex-work");

        assert_eq!(store.list_accounts().unwrap(), [after_repoint]);
    }

    /// Repointing an account starts the new directory out not sharing and forgets what turning
    /// sharing on recorded of the old one, and an outcome carried out on the old directory is not
    /// recorded for the new one; a rename changes none of it.
    #[test]
    fn repointing_an_account_starts_its_new_directory_out_not_sharing() {
        let path = temp_db("account-sharing-repoint");
        let store = Store::open(&path).expect("store");
        store
            .insert_account(&account("acct-1", Agent::Codex, "/home/u/.codex-alt"))
            .expect("account");
        let start = SharingStart {
            stamp: "20261020T000000Z".to_string(),
            default_dir: "/home/u/.codex".to_string(),
            completed: false,
        };
        store.set_account_sharing("acct-1", true, true).unwrap();
        assert!(store
            .start_account_sharing("acct-1", "/home/u/.codex-alt", &start)
            .unwrap());
        assert!(store
            .finish_account_sharing("acct-1", "/home/u/.codex-alt", true)
            .unwrap());

        let mut renamed = store.get_account("acct-1").unwrap().unwrap();
        renamed.name = "Work".to_string();
        store.update_account(&renamed).unwrap();
        let after_rename = store.get_account("acct-1").unwrap().unwrap();
        assert!(after_rename.share_default_config && !after_rename.share_default_config_incomplete);
        assert_eq!(
            store.account_sharing_start("acct-1").unwrap(),
            Some(SharingStart {
                completed: true,
                ..start.clone()
            })
        );

        let mut repointed = after_rename;
        repointed.config_dir = "/home/u/.codex-work".to_string();
        store.update_account(&repointed).unwrap();
        let after_repoint = store.get_account("acct-1").unwrap().unwrap();
        assert!(
            !after_repoint.share_default_config && !after_repoint.share_default_config_incomplete
        );
        assert_eq!(store.account_sharing_start("acct-1").unwrap(), None);
        store.set_account_sharing("acct-1", true, true).unwrap();
        assert!(!store
            .start_account_sharing("acct-1", "/home/u/.codex-alt", &start)
            .unwrap());
        assert!(!store
            .finish_account_sharing("acct-1", "/home/u/.codex-alt", true)
            .unwrap());
        assert!(
            store
                .get_account("acct-1")
                .unwrap()
                .unwrap()
                .share_default_config_incomplete
        );

        // Respelled only: the same directory where the platform compares it so, keeping what
        // sharing recorded of it.
        let start = SharingStart {
            stamp: "20261021T000000Z".to_string(),
            ..start
        };
        assert!(store
            .start_account_sharing("acct-1", "/home/u/.codex-work", &start)
            .unwrap());
        let mut respelled = store.get_account("acct-1").unwrap().unwrap();
        respelled.config_dir = "/home/u/.Codex-Work".to_string();
        store.update_account(&respelled).unwrap();
        let same = crate::account_dirs::account_dir_key("/home/u/.codex-work")
            == crate::account_dirs::account_dir_key("/home/u/.Codex-Work");
        assert_eq!(
            store.account_sharing_start("acct-1").unwrap().is_some(),
            same
        );
    }

    /// An account row's flag is written alone, a repeat write reports no change, and an id of
    /// another agent is not disabled for the agent asked about.
    #[test]
    fn an_accounts_disabled_flag_is_written_alone() {
        let path = temp_db("account-disabled-flag");
        let store = Store::open(&path).expect("store");
        store
            .insert_account(&Account {
                id: "work".to_string(),
                agent: Agent::Claude,
                name: "Work".to_string(),
                config_dir: "/home/u/.claude-work".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        assert!(store.set_account_disabled("work", true).unwrap());
        assert!(!store.set_account_disabled("work", true).unwrap());
        assert!(!store.set_account_disabled("nope", true).unwrap());
        assert!(store.get_account("work").unwrap().unwrap().disabled);
        assert!(store.account_disabled(Agent::Claude, Some("work")).unwrap());
        assert!(!store.account_disabled(Agent::Codex, Some("work")).unwrap());
        assert!(store.set_account_disabled("work", false).unwrap());
        assert!(!store.account_disabled(Agent::Claude, Some("work")).unwrap());
    }
}
