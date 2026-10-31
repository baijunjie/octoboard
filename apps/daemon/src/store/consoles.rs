//! The consoles table: a console's record, its per-agent account references and launch defaults.

use anyhow::Result;
use rusqlite::{params, OptionalExtension, Row};

use super::rows::{column_slot, enum_from_row, enum_from_row_opt, enum_to_text, launch_from_row};
use super::Store;
use crate::protocol::{Agent, Console, PerAgent};

const CONSOLE_COLUMNS: &str = "id, name, workdir, default_agent,
                               claude_account_id, codex_account_id, grok_account_id,
                               claude_console_session_model, claude_console_session_effort,
                               codex_console_session_model, codex_console_session_effort,
                               grok_console_session_model, grok_console_session_effort, icon,
                               console_session_prompt, created_at, project_session_agent,
                               claude_project_session_model, claude_project_session_effort,
                               codex_project_session_model, codex_project_session_effort,
                               grok_project_session_model, grok_project_session_effort";

impl Store {
    pub fn insert_console(&self, console: &Console) -> Result<()> {
        self.lock().execute(
            "INSERT INTO consoles (id, name, workdir, default_agent, claude_account_id,
                                   codex_account_id, grok_account_id, claude_console_session_model,
                                   claude_console_session_effort, codex_console_session_model,
                                   codex_console_session_effort, grok_console_session_model,
                                   grok_console_session_effort, icon, console_session_prompt,
                                   created_at, project_session_agent,
                                   claude_project_session_model, claude_project_session_effort,
                                   codex_project_session_model, codex_project_session_effort,
                                   grok_project_session_model, grok_project_session_effort)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19, ?20, ?21, ?22, ?23)",
            params![
                console.id,
                console.name,
                console.workdir,
                enum_to_text(&console.default_agent),
                console.accounts.claude,
                console.accounts.codex,
                console.accounts.grok,
                console.launch.claude.model,
                console.launch.claude.effort,
                console.launch.codex.model,
                console.launch.codex.effort,
                console.launch.grok.model,
                console.launch.grok.effort,
                console.icon,
                console.console_session_prompt,
                console.created_at,
                console.project_session_agent.as_ref().map(enum_to_text),
                console.project_session_launch.claude.model,
                console.project_session_launch.claude.effort,
                console.project_session_launch.codex.model,
                console.project_session_launch.codex.effort,
                console.project_session_launch.grok.model,
                console.project_session_launch.grok.effort,
            ],
        )?;
        Ok(())
    }

    pub fn update_console(&self, console: &Console) -> Result<()> {
        self.lock().execute(
            "UPDATE consoles SET name = ?2, default_agent = ?3,
                                 claude_account_id = ?4, codex_account_id = ?5,
                                 grok_account_id = ?6, claude_console_session_model = ?7,
                                 claude_console_session_effort = ?8,
                                 codex_console_session_model = ?9,
                                 codex_console_session_effort = ?10,
                                 grok_console_session_model = ?11,
                                 grok_console_session_effort = ?12, icon = ?13,
                                 console_session_prompt = ?14, project_session_agent = ?15,
                                 claude_project_session_model = ?16,
                                 claude_project_session_effort = ?17,
                                 codex_project_session_model = ?18,
                                 codex_project_session_effort = ?19,
                                 grok_project_session_model = ?20,
                                 grok_project_session_effort = ?21
             WHERE id = ?1",
            params![
                console.id,
                console.name,
                enum_to_text(&console.default_agent),
                console.accounts.claude,
                console.accounts.codex,
                console.accounts.grok,
                console.launch.claude.model,
                console.launch.claude.effort,
                console.launch.codex.model,
                console.launch.codex.effort,
                console.launch.grok.model,
                console.launch.grok.effort,
                console.icon,
                console.console_session_prompt,
                console.project_session_agent.as_ref().map(enum_to_text),
                console.project_session_launch.claude.model,
                console.project_session_launch.claude.effort,
                console.project_session_launch.codex.model,
                console.project_session_launch.codex.effort,
                console.project_session_launch.grok.model,
                console.project_session_launch.grok.effort,
            ],
        )?;
        Ok(())
    }

    pub fn delete_console(&self, id: &str) -> Result<()> {
        self.lock()
            .execute("DELETE FROM consoles WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_console(&self, id: &str) -> Result<Option<Console>> {
        let console = self
            .lock()
            .query_row(
                &format!("SELECT {CONSOLE_COLUMNS} FROM consoles WHERE id = ?1"),
                params![id],
                read_console,
            )
            .optional()?;
        Ok(console)
    }

    pub fn list_consoles(&self) -> Result<Vec<Console>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {CONSOLE_COLUMNS} FROM consoles ORDER BY created_at"
        ))?;
        let consoles = stmt
            .query_map([], read_console)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(consoles)
    }
}

fn read_console(row: &Row<'_>) -> rusqlite::Result<Console> {
    Ok(Console {
        id: row.get(0)?,
        name: row.get(1)?,
        workdir: row.get(2)?,
        default_agent: enum_from_row(row, 3)?,
        accounts: PerAgent::try_from_fn(|agent| row.get(4 + column_slot(agent)))?,
        launch: PerAgent::try_from_fn(|agent| launch_from_row(row, 7 + 2 * column_slot(agent)))?,
        icon: row.get(13)?,
        console_session_prompt: row.get(14)?,
        created_at: row.get(15)?,
        project_session_agent: enum_from_row_opt::<Agent>(row, 16)?,
        project_session_launch: PerAgent::try_from_fn(|agent| {
            launch_from_row(row, 17 + 2 * column_slot(agent))
        })?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::store::test_support::*;

    /// A console's console session model and effort round-trip through update, insert and a
    /// reopening, each of the six independent.
    #[test]
    fn the_console_session_launch_columns_round_trip() {
        let path = temp_db("console-session-launch-columns");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
        }

        let store = Store::open(&path).expect("reopened");
        let mut kept = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(kept.launch.claude.model, None);
        // Six distinct values, so a pair of columns swapped between the SELECT list and the
        // positional reads fails here.
        kept.launch.claude.model = Some("claude-model".to_string());
        kept.launch.claude.effort = Some("claude-effort".to_string());
        kept.launch.codex.model = Some("codex-model".to_string());
        kept.launch.codex.effort = Some("codex-effort".to_string());
        kept.launch.grok.model = Some("grok-model".to_string());
        kept.launch.grok.effort = Some("grok-effort".to_string());
        store.update_console(&kept).expect("writes the columns");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(reread.launch.claude.model.as_deref(), Some("claude-model"));
        assert_eq!(
            reread.launch.claude.effort.as_deref(),
            Some("claude-effort")
        );
        assert_eq!(reread.launch.codex.model.as_deref(), Some("codex-model"));
        assert_eq!(reread.launch.codex.effort.as_deref(), Some("codex-effort"));
        assert_eq!(reread.launch.grok.model.as_deref(), Some("grok-model"));
        assert_eq!(reread.launch.grok.effort.as_deref(), Some("grok-effort"));

        // A console inserted with all six set reads them back the same way.
        let mut inserted = console(None, None, None);
        inserted.id = "console-2".to_string();
        inserted.workdir = "/tmp/console-2".to_string();
        inserted.launch.claude.model = kept.launch.claude.model.clone();
        inserted.launch.claude.effort = kept.launch.claude.effort.clone();
        inserted.launch.codex.model = kept.launch.codex.model.clone();
        inserted.launch.codex.effort = kept.launch.codex.effort.clone();
        inserted.launch.grok.model = kept.launch.grok.model.clone();
        inserted.launch.grok.effort = kept.launch.grok.effort.clone();
        store.insert_console(&inserted).expect("insert");
        let listed = store.get_console("console-2").unwrap().expect("listed");
        assert_eq!(
            [
                listed.launch.claude.model,
                listed.launch.claude.effort,
                listed.launch.codex.model,
                listed.launch.codex.effort,
                listed.launch.grok.model,
                listed.launch.grok.effort,
            ]
            .map(|field| field.expect("set")),
            [
                "claude-model",
                "claude-effort",
                "codex-model",
                "codex-effort",
                "grok-model",
                "grok-effort",
            ]
        );

        // Reopened once more, the same values are read back from the stored columns.
        drop(store);
        let store = Store::open(&path).expect("reopened again");
        let again = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(again.launch.codex.effort.as_deref(), Some("codex-effort"));
    }

    /// A console follows `default_agent` until it names a project session agent, which then
    /// round-trips.
    #[test]
    fn the_project_session_agent_round_trips() {
        let path = temp_db("project-session-agent-column");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut kept = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(kept.project_session_agent, None);
        kept.project_session_agent = Some(Agent::Grok);
        store.update_console(&kept).expect("writes the column");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(reread.project_session_agent, Some(Agent::Grok));
    }

    /// A console's project session model and effort start unset and round-trip, each of the six
    /// independent.
    #[test]
    fn the_console_project_session_launch_columns_round_trip() {
        let path = temp_db("console-project-session-launch-columns");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut kept = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(kept.project_session_launch.claude.model, None);
        assert_eq!(kept.project_session_launch.grok.effort, None);
        // Six distinct values, so a pair of columns swapped between the SELECT list and the
        // positional reads fails here.
        kept.project_session_launch.claude.model = Some("claude-model".to_string());
        kept.project_session_launch.claude.effort = Some("claude-effort".to_string());
        kept.project_session_launch.codex.model = Some("codex-model".to_string());
        kept.project_session_launch.codex.effort = Some("codex-effort".to_string());
        kept.project_session_launch.grok.model = Some("grok-model".to_string());
        kept.project_session_launch.grok.effort = Some("grok-effort".to_string());
        store.update_console(&kept).expect("writes the columns");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(
            reread.project_session_launch.claude.model.as_deref(),
            Some("claude-model")
        );
        assert_eq!(
            reread.project_session_launch.claude.effort.as_deref(),
            Some("claude-effort")
        );
        assert_eq!(
            reread.project_session_launch.codex.model.as_deref(),
            Some("codex-model")
        );
        assert_eq!(
            reread.project_session_launch.codex.effort.as_deref(),
            Some("codex-effort")
        );
        assert_eq!(
            reread.project_session_launch.grok.model.as_deref(),
            Some("grok-model")
        );
        assert_eq!(
            reread.project_session_launch.grok.effort.as_deref(),
            Some("grok-effort")
        );
    }

    /// A console carries no console session instructions of the user's own until it is given
    /// some, which then round-trip.
    #[test]
    fn the_console_session_prompt_round_trips() {
        let path = temp_db("console-session-prompt-column");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut kept = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(kept.console_session_prompt, None);
        kept.console_session_prompt = Some("Be brief.".to_string());
        store.update_console(&kept).expect("writes the column");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(reread.console_session_prompt.as_deref(), Some("Be brief."));
    }

    /// A console's references round-trip as written, and a session keeps its own copy of its
    /// account's directory regardless of what the console's reference is afterwards: only
    /// `set_session_account` changes it, and always the two fields together.
    #[test]
    fn a_consoles_account_references_round_trip_and_a_session_keeps_its_own_directory() {
        let path = temp_db("round-trip");
        let store = Store::open(&path).expect("store");

        store
            .insert_account(&account(
                "claude-acct",
                Agent::Claude,
                "/home/u/.claude-alt",
            ))
            .expect("account");
        store
            .insert_account(&account("codex-acct", Agent::Codex, "/home/u/.codex-alt"))
            .expect("account");
        store
            .insert_account(&account("grok-acct", Agent::Grok, "/home/u/.grok-alt"))
            .expect("account");
        store
            .insert_console(&console(
                Some("claude-acct"),
                Some("codex-acct"),
                Some("grok-acct"),
            ))
            .expect("insert");
        let stored = store.get_console("console-1").unwrap().unwrap();
        assert_eq!(stored.accounts.claude.as_deref(), Some("claude-acct"));
        assert_eq!(stored.accounts.codex.as_deref(), Some("codex-acct"));
        assert_eq!(stored.accounts.grok.as_deref(), Some("grok-acct"));

        let mut opened = session(Some("/home/u/.claude-alt"));
        opened.account_id = Some("claude-acct".to_string());
        store.insert_session(&opened).expect("insert");

        // Updating the session's mutable fields leaves the account and directory it has alone —
        // neither is in `update_session`'s write set.
        let mut live = store.get_session("session-1").unwrap().unwrap();
        live.account_id = Some("elsewhere".to_string());
        live.config_dir = Some("/elsewhere".to_string());
        live.title = "Renamed".to_string();
        store.update_session(&live).expect("update");
        let after = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(after.title, "Renamed");
        assert_eq!(after.account_id.as_deref(), Some("claude-acct"));
        assert_eq!(after.config_dir.as_deref(), Some("/home/u/.claude-alt"));

        // The switch's write path moves both together, to the default account (nothing pinned)
        // included, and reports a session that is gone as `None`.
        let moved = store
            .set_session_account("session-1", None, None)
            .unwrap()
            .expect("the session");
        assert_eq!((moved.account_id, moved.config_dir), (None, None));
        assert!(store
            .set_session_account("missing", None, None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn updating_a_console_persists_each_agents_account_reference() {
        let path = temp_db("update-console");
        let store = Store::open(&path).expect("store");
        store
            .insert_account(&account("alt", Agent::Claude, "/home/u/.alt"))
            .expect("account");
        store
            .insert_account(&account("other", Agent::Claude, "/home/u/other"))
            .expect("account");
        let mut stored = console(None, None, None);
        store.insert_console(&stored).expect("insert");
        let read = |store: &Store| {
            store
                .get_console("console-1")
                .unwrap()
                .unwrap()
                .accounts
                .claude
        };
        assert_eq!(read(&store), None);

        for account_id in [Some("alt"), Some("other"), None] {
            stored.accounts.claude = account_id.map(str::to_string);
            store.update_console(&stored).expect("update");
            assert_eq!(read(&store), account_id.map(str::to_string));
        }
    }
}
