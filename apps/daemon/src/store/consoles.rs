//! The consoles table and its provider rows: a console's record, its per-provider account
//! references and launch defaults.

use std::collections::HashMap;

use super::Result;
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use super::rows::{agent_from_row, enum_to_text, registered_provider_clause};
use super::Store;
use crate::protocol::{AgentId, CodedError, Console, Launch, PerAgent};

const CONSOLE_COLUMNS: &str = "id, name, workdir, default_agent, icon, console_session_prompt,
                               created_at, project_session_agent";

/// The `session_kind` of the `console_launches` rows for a console's own sessions, and for the
/// project sessions opened in it.
const CONSOLE_SESSIONS: &str = "console";
const PROJECT_SESSIONS: &str = "project";

impl Store {
    pub fn insert_console(&self, console: &Console) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO consoles (id, name, workdir, default_agent, icon, console_session_prompt,
                                   created_at, project_session_agent)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                console.id,
                console.name,
                console.workdir,
                enum_to_text(&console.default_agent),
                console.icon,
                console.console_session_prompt,
                console.created_at,
                console.project_session_agent.as_ref().map(enum_to_text),
            ],
        )?;
        write_provider_rows(&tx, console)?;
        tx.commit()?;
        Ok(())
    }

    /// Writes the console's record and its registered providers' rows. A stored default agent or
    /// project session agent naming a provider that is not registered is kept while the console
    /// still reads as it did (the first registered provider, following the default agent), so an
    /// edit of anything else does not lose it.
    pub fn update_console(&self, console: &Console) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            &format!(
                "UPDATE consoles SET name = ?2,
                     default_agent = CASE WHEN ?3 = ?7 AND NOT ({}) THEN default_agent
                                          ELSE ?3 END,
                     icon = ?4, console_session_prompt = ?5,
                     project_session_agent = CASE WHEN ?6 IS NULL AND NOT ({})
                                                  THEN project_session_agent ELSE ?6 END
                 WHERE id = ?1",
                registered_provider_clause("default_agent"),
                registered_provider_clause("project_session_agent"),
            ),
            params![
                console.id,
                console.name,
                enum_to_text(&console.default_agent),
                console.icon,
                console.console_session_prompt,
                console.project_session_agent.as_ref().map(enum_to_text),
                enum_to_text(&AgentId::ALL[0]),
            ],
        )?;
        if changed > 0 {
            write_provider_rows(&tx, console)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_console(&self, id: &str) -> Result<()> {
        self.lock()
            .execute("DELETE FROM consoles WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_console(&self, id: &str) -> Result<Option<Console>> {
        let conn = self.lock();
        let console = conn
            .query_row(
                &format!("SELECT {CONSOLE_COLUMNS} FROM consoles WHERE id = ?1"),
                params![id],
                read_console,
            )
            .optional()?;
        let Some(console) = console else {
            return Ok(None);
        };
        let mut consoles = [console];
        read_provider_rows(&conn, Some(id), &mut consoles)?;
        let [console] = consoles;
        Ok(Some(console))
    }

    /// The console `id`, or the `unknown_console` refusal when there is none.
    pub fn require_console(&self, id: &str) -> Result<Console> {
        self.get_console(id)?
            .ok_or_else(|| CodedError::unknown_console(id).into())
    }

    pub fn list_consoles(&self) -> Result<Vec<Console>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {CONSOLE_COLUMNS} FROM consoles ORDER BY created_at"
        ))?;
        let mut consoles = stmt
            .query_map([], read_console)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        read_provider_rows(&conn, None, &mut consoles)?;
        Ok(consoles)
    }
}

/// A console's record without its provider rows. A default agent naming no registered provider
/// reads as the first registered one, and a project session agent naming none as following the
/// default agent.
fn read_console(row: &Row<'_>) -> rusqlite::Result<Console> {
    let project_session_agent: Option<String> = row.get(7)?;
    Ok(Console {
        id: row.get(0)?,
        name: row.get(1)?,
        workdir: row.get(2)?,
        default_agent: agent_from_row(row, 3)?.unwrap_or(AgentId::ALL[0]),
        accounts: PerAgent::default(),
        launch: PerAgent::default(),
        icon: row.get(4)?,
        console_session_prompt: row.get(5)?,
        created_at: row.get(6)?,
        project_session_agent: project_session_agent.and_then(|id| AgentId::from_wire_name(&id)),
        project_session_launch: PerAgent::default(),
        versions: Default::default(),
    })
}

/// Replaces the provider rows of the console with the console's values, for the registered
/// providers only: rows naming another provider are left as they are. A provider holding nothing
/// but defaults has no row.
fn write_provider_rows(tx: &Transaction<'_>, console: &Console) -> Result<()> {
    tx.execute(
        &format!(
            "DELETE FROM console_providers WHERE console_id = ?1 AND {}",
            registered_provider_clause("provider")
        ),
        params![console.id],
    )?;
    tx.execute(
        &format!(
            "DELETE FROM console_launches WHERE console_id = ?1 AND {}",
            registered_provider_clause("provider")
        ),
        params![console.id],
    )?;
    for (agent, account) in console.accounts.iter() {
        if let Some(account_id) = account {
            tx.execute(
                "INSERT INTO console_providers (console_id, provider, account_id)
                 VALUES (?1, ?2, ?3)",
                params![console.id, agent.wire_name(), account_id],
            )?;
        }
    }
    for (session_kind, launches) in [
        (CONSOLE_SESSIONS, &console.launch),
        (PROJECT_SESSIONS, &console.project_session_launch),
    ] {
        for (agent, launch) in launches.iter() {
            if launch.model.is_some() || launch.effort.is_some() {
                tx.execute(
                    "INSERT INTO console_launches (console_id, provider, session_kind, model, effort)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        console.id,
                        agent.wire_name(),
                        session_kind,
                        launch.model,
                        launch.effort
                    ],
                )?;
            }
        }
    }
    Ok(())
}

/// Fills in the provider rows of `consoles`, those of the console `only` or of all of them. A row
/// naming a provider this build does not register is left out.
fn read_provider_rows(
    conn: &Connection,
    only: Option<&str>,
    consoles: &mut [Console],
) -> Result<()> {
    let positions: HashMap<String, usize> = consoles
        .iter()
        .enumerate()
        .map(|(position, console)| (console.id.clone(), position))
        .collect();
    let mut stmt = conn.prepare(
        "SELECT console_id, provider, account_id FROM console_providers
         WHERE ?1 IS NULL OR console_id = ?1",
    )?;
    let accounts = stmt
        .query_map(params![only], |row| {
            Ok((
                row.get::<_, String>(0)?,
                agent_from_row(row, 1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (console_id, agent, account_id) in accounts {
        if let (Some(&position), Some(agent)) = (positions.get(&console_id), agent) {
            consoles[position].accounts[agent] = Some(account_id);
        }
    }
    let mut stmt = conn.prepare(
        "SELECT console_id, provider, session_kind, model, effort FROM console_launches
         WHERE ?1 IS NULL OR console_id = ?1",
    )?;
    let launches = stmt
        .query_map(params![only], |row| {
            Ok((
                row.get::<_, String>(0)?,
                agent_from_row(row, 1)?,
                row.get::<_, String>(2)?,
                Launch {
                    model: row.get(3)?,
                    effort: row.get(4)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (console_id, agent, session_kind, launch) in launches {
        let (Some(&position), Some(agent)) = (positions.get(&console_id), agent) else {
            continue;
        };
        let console = &mut consoles[position];
        match session_kind.as_str() {
            CONSOLE_SESSIONS => console.launch[agent] = launch,
            PROJECT_SESSIONS => console.project_session_launch[agent] = launch,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{error_code, error_parts};

    use crate::store::test_support::*;

    /// A console that is stored is read back, and one that is not is refused with the
    /// `unknown_console` error naming the id that was asked for.
    #[test]
    fn requiring_a_console_reads_a_stored_one_and_refuses_an_unknown_one_with_its_code() {
        let path = temp_db("require-console");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");

        assert_eq!(
            store.require_console("console-1").expect("stored").id,
            "console-1"
        );

        let refused: anyhow::Error = store.require_console("nope").expect_err("unknown").into();
        assert_eq!(
            error_parts(&refused),
            error_parts(&CodedError::unknown_console("nope"))
        );
        assert_eq!(error_parts(&refused).0, error_code::UNKNOWN_CONSOLE);
        assert_eq!(error_parts(&refused).2, "unknown console nope");
    }

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
        assert_eq!(kept.launch[AgentId::CLAUDE].model, None);
        // Six distinct values, so a pair of columns swapped between the SELECT list and the
        // positional reads fails here.
        kept.launch[AgentId::CLAUDE].model = Some("claude-model".to_string());
        kept.launch[AgentId::CLAUDE].effort = Some("claude-effort".to_string());
        kept.launch[AgentId::CODEX].model = Some("codex-model".to_string());
        kept.launch[AgentId::CODEX].effort = Some("codex-effort".to_string());
        kept.launch[AgentId::GROK].model = Some("grok-model".to_string());
        kept.launch[AgentId::GROK].effort = Some("grok-effort".to_string());
        store.update_console(&kept).expect("writes the columns");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(
            reread.launch[AgentId::CLAUDE].model.as_deref(),
            Some("claude-model")
        );
        assert_eq!(
            reread.launch[AgentId::CLAUDE].effort.as_deref(),
            Some("claude-effort")
        );
        assert_eq!(
            reread.launch[AgentId::CODEX].model.as_deref(),
            Some("codex-model")
        );
        assert_eq!(
            reread.launch[AgentId::CODEX].effort.as_deref(),
            Some("codex-effort")
        );
        assert_eq!(
            reread.launch[AgentId::GROK].model.as_deref(),
            Some("grok-model")
        );
        assert_eq!(
            reread.launch[AgentId::GROK].effort.as_deref(),
            Some("grok-effort")
        );

        // A console inserted with all six set reads them back the same way.
        let mut inserted = console(None, None, None);
        inserted.id = "console-2".to_string();
        inserted.workdir = "/tmp/console-2".to_string();
        inserted.launch[AgentId::CLAUDE].model = kept.launch[AgentId::CLAUDE].model.clone();
        inserted.launch[AgentId::CLAUDE].effort = kept.launch[AgentId::CLAUDE].effort.clone();
        inserted.launch[AgentId::CODEX].model = kept.launch[AgentId::CODEX].model.clone();
        inserted.launch[AgentId::CODEX].effort = kept.launch[AgentId::CODEX].effort.clone();
        inserted.launch[AgentId::GROK].model = kept.launch[AgentId::GROK].model.clone();
        inserted.launch[AgentId::GROK].effort = kept.launch[AgentId::GROK].effort.clone();
        store.insert_console(&inserted).expect("insert");
        let listed = store.get_console("console-2").unwrap().expect("listed");
        assert_eq!(
            [
                listed.launch[AgentId::CLAUDE].model.clone(),
                listed.launch[AgentId::CLAUDE].effort.clone(),
                listed.launch[AgentId::CODEX].model.clone(),
                listed.launch[AgentId::CODEX].effort.clone(),
                listed.launch[AgentId::GROK].model.clone(),
                listed.launch[AgentId::GROK].effort.clone(),
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
        assert_eq!(
            again.launch[AgentId::CODEX].effort.as_deref(),
            Some("codex-effort")
        );
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
        kept.project_session_agent = Some(AgentId::GROK);
        store.update_console(&kept).expect("writes the column");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(reread.project_session_agent, Some(AgentId::GROK));
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
        assert_eq!(kept.project_session_launch[AgentId::CLAUDE].model, None);
        assert_eq!(kept.project_session_launch[AgentId::GROK].effort, None);
        // Six distinct values, so a pair of columns swapped between the SELECT list and the
        // positional reads fails here.
        kept.project_session_launch[AgentId::CLAUDE].model = Some("claude-model".to_string());
        kept.project_session_launch[AgentId::CLAUDE].effort = Some("claude-effort".to_string());
        kept.project_session_launch[AgentId::CODEX].model = Some("codex-model".to_string());
        kept.project_session_launch[AgentId::CODEX].effort = Some("codex-effort".to_string());
        kept.project_session_launch[AgentId::GROK].model = Some("grok-model".to_string());
        kept.project_session_launch[AgentId::GROK].effort = Some("grok-effort".to_string());
        store.update_console(&kept).expect("writes the columns");
        let reread = store.get_console("console-1").unwrap().expect("kept");
        assert_eq!(
            reread.project_session_launch[AgentId::CLAUDE]
                .model
                .as_deref(),
            Some("claude-model")
        );
        assert_eq!(
            reread.project_session_launch[AgentId::CLAUDE]
                .effort
                .as_deref(),
            Some("claude-effort")
        );
        assert_eq!(
            reread.project_session_launch[AgentId::CODEX]
                .model
                .as_deref(),
            Some("codex-model")
        );
        assert_eq!(
            reread.project_session_launch[AgentId::CODEX]
                .effort
                .as_deref(),
            Some("codex-effort")
        );
        assert_eq!(
            reread.project_session_launch[AgentId::GROK]
                .model
                .as_deref(),
            Some("grok-model")
        );
        assert_eq!(
            reread.project_session_launch[AgentId::GROK]
                .effort
                .as_deref(),
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
                AgentId::CLAUDE,
                "/home/u/.claude-alt",
            ))
            .expect("account");
        store
            .insert_account(&account("codex-acct", AgentId::CODEX, "/home/u/.codex-alt"))
            .expect("account");
        store
            .insert_account(&account("grok-acct", AgentId::GROK, "/home/u/.grok-alt"))
            .expect("account");
        store
            .insert_console(&console(
                Some("claude-acct"),
                Some("codex-acct"),
                Some("grok-acct"),
            ))
            .expect("insert");
        let stored = store.get_console("console-1").unwrap().unwrap();
        assert_eq!(
            stored.accounts[AgentId::CLAUDE].as_deref(),
            Some("claude-acct")
        );
        assert_eq!(
            stored.accounts[AgentId::CODEX].as_deref(),
            Some("codex-acct")
        );
        assert_eq!(stored.accounts[AgentId::GROK].as_deref(), Some("grok-acct"));

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
            .insert_account(&account("alt", AgentId::CLAUDE, "/home/u/.alt"))
            .expect("account");
        store
            .insert_account(&account("other", AgentId::CLAUDE, "/home/u/other"))
            .expect("account");
        let mut stored = console(None, None, None);
        store.insert_console(&stored).expect("insert");
        let read = |store: &Store| {
            store.get_console("console-1").unwrap().unwrap().accounts[AgentId::CLAUDE].clone()
        };
        assert_eq!(read(&store), None);

        for account_id in [Some("alt"), Some("other"), None] {
            stored.accounts[AgentId::CLAUDE] = account_id.map(str::to_string);
            store.update_console(&stored).expect("update");
            assert_eq!(read(&store), account_id.map(str::to_string));
        }
    }

    /// Every registered provider's account and both launch pairs round-trip, each independent of
    /// the others, and a provider holding only defaults has no row.
    #[test]
    fn every_registered_providers_values_round_trip_and_defaults_have_no_rows() {
        let path = temp_db("console-every-provider");
        let store = Store::open(&path).expect("store");
        let mut stored = console(None, None, None);
        for agent in AgentId::ALL {
            let id = agent.wire_name();
            stored.accounts[agent] = Some(format!("{id}-account"));
            stored.launch[agent].model = Some(format!("{id}-console-model"));
            stored.project_session_launch[agent].effort = Some(format!("{id}-project-effort"));
        }
        store.insert_console(&stored).expect("insert");
        let read = store.get_console("console-1").unwrap().expect("stored");
        assert_eq!(read.accounts, stored.accounts);
        assert_eq!(read.launch, stored.launch);
        assert_eq!(read.project_session_launch, stored.project_session_launch);
        assert_eq!(store.list_consoles().unwrap()[0].accounts, stored.accounts);

        let count = |table: &str| -> i64 {
            store
                .lock()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap()
        };
        assert_eq!(count("console_providers"), AgentId::ALL.len() as i64);
        assert_eq!(count("console_launches"), 2 * AgentId::ALL.len() as i64);

        let mut cleared = stored.clone();
        cleared.accounts = Default::default();
        cleared.launch = Default::default();
        cleared.project_session_launch = Default::default();
        store.update_console(&cleared).expect("update");
        assert_eq!(count("console_providers"), 0);
        assert_eq!(count("console_launches"), 0);
        assert_eq!(
            store.get_console("console-1").unwrap().unwrap().launch,
            cleared.launch
        );
    }

    /// Rows naming a provider that is not registered stay in the store through writes and are left
    /// out of a load, and the console's default and project session agents naming one fall back.
    #[test]
    fn rows_of_an_unregistered_provider_are_kept_and_left_out_of_a_load() {
        let path = temp_db("console-unregistered-provider");
        let store = Store::open(&path).expect("store");
        let mut stored = console(Some("kept-account"), None, None);
        store.insert_console(&stored).expect("insert");
        {
            let conn = store.lock();
            conn.execute_batch(
                "INSERT INTO console_providers (console_id, provider, account_id)
                     VALUES ('console-1', 'retired', 'retired-account');
                 INSERT INTO console_launches (console_id, provider, session_kind, model, effort)
                     VALUES ('console-1', 'retired', 'console', 'm', 'e'),
                            ('console-1', 'retired', 'project', 'm', 'e');
                 UPDATE consoles SET default_agent = 'retired', project_session_agent = 'retired';",
            )
            .unwrap();
        }
        let read = store.get_console("console-1").unwrap().expect("not hidden");
        assert_eq!(read.default_agent, AgentId::ALL[0]);
        assert_eq!(read.project_session_agent, None);
        assert_eq!(read.project_session_default(), AgentId::ALL[0]);
        assert_eq!(
            read.accounts[AgentId::CLAUDE].as_deref(),
            Some("kept-account")
        );
        assert!(AgentId::ALL
            .iter()
            .all(|agent| read.launch[*agent] == Launch::default()));
        assert_eq!(store.list_consoles().unwrap().len(), 1);

        // Writing the console replaces the registered providers' rows and leaves the others.
        stored.accounts[AgentId::CLAUDE] = None;
        stored.launch[AgentId::CODEX].model = Some("codex-model".to_string());
        store.update_console(&stored).expect("update");
        let count = |table: &str, provider: &str| -> i64 {
            store
                .lock()
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE provider = ?1"),
                    params![provider],
                    |row| row.get(0),
                )
                .unwrap()
        };
        assert_eq!(count("console_providers", "retired"), 1);
        assert_eq!(count("console_launches", "retired"), 2);
        assert_eq!(count("console_providers", "claude"), 0);
        assert_eq!(count("console_launches", "codex"), 1);

        // The agents it named stay named through a write that reads as they did, and a write
        // that chooses another replaces them.
        let agents = || -> (String, Option<String>) {
            store
                .lock()
                .query_row(
                    "SELECT default_agent, project_session_agent FROM consoles",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap()
        };
        assert_eq!(agents(), ("retired".into(), Some("retired".into())));
        stored.default_agent = AgentId::CODEX;
        stored.project_session_agent = Some(AgentId::GROK);
        store.update_console(&stored).expect("update");
        assert_eq!(agents(), ("codex".into(), Some("grok".into())));
        store.delete_console("console-1").unwrap();
        assert_eq!(count("console_providers", "retired"), 0);
        assert_eq!(count("console_launches", "retired"), 0);
    }
}
