//! The projects table, and the directories the user has trusted for every project under them.

use super::{Result, StoreError};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use std::collections::HashMap;

use super::rows::{
    agent_from_row, enum_from_row, enum_from_row_opt, enum_to_text, registered_provider_clause,
};
use super::Store;
use crate::protocol::{
    error_code, AgentId, CodedError, GitWorkflowOverride, Launch, PerAgent, Project,
    ProjectAccount, ProjectSource,
};

const PROJECT_COLUMNS: &str = "id, console_id, host_id, name, path, default_agent, source,
                               remote_url, trust_consent, pinned, tags, description, development,
                               rebase_before_delivery, delivery, revert_gate,
                               squash_before_delivery";

impl Store {
    /// Inserts the project. A second project at a path its console already has trips the unique
    /// index on `(console_id, path)` and fails with `all_projects_already_added`, naming the path.
    /// `coordinator` checks and claims before it gets here, so this is a backstop for what that
    /// check cannot tell apart, such as two non-UTF-8 names that collide once converted to text.
    pub fn insert_project(&self, project: &Project) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO projects (id, console_id, host_id, name, path, default_agent, source,
                                   remote_url, trust_consent, pinned, tags, description,
                                   development, rebase_before_delivery, delivery, revert_gate,
                                   squash_before_delivery)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                project.id,
                project.console_id,
                project.host_id,
                project.name,
                project.path,
                project.default_agent.as_ref().map(enum_to_text),
                enum_to_text(&project.source),
                project.remote_url,
                project.trust_consent,
                project.pinned,
                tags_to_text(&project.tags),
                project.description,
                project.git_workflow.development.as_ref().map(enum_to_text),
                project.git_workflow.rebase_before_delivery,
                project.git_workflow.delivery.as_ref().map(enum_to_text),
                project.git_workflow.revert_gate,
                project.git_workflow.squash_before_delivery,
            ],
        )
        .map_err(|err| match err {
            rusqlite::Error::SqliteFailure(failure, _)
                if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
            {
                StoreError::from(CodedError::raised(
                    error_code::ALL_PROJECTS_ALREADY_ADDED,
                    format!(
                        "console {} already has a project at {}",
                        project.console_id, project.path
                    ),
                    &[],
                ))
            }
            err => err.into(),
        })?;
        write_provider_rows(&tx, project)?;
        tx.commit()?;
        Ok(())
    }

    /// Records the user's permission (or its withdrawal) for Octoboard to press any agent's trust
    /// confirmation for this project without asking. Its own statement rather than a field of
    /// `update_project`, which is the user's edit of the project and never reaches this decision.
    pub fn set_project_trust_consent(&self, id: &str, consent: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE projects SET trust_consent = ?2 WHERE id = ?1",
            params![id, consent],
        )?;
        Ok(())
    }

    /// Records the git remote URL read from the project's repository, or clears it. Its own
    /// statement rather than a field of `update_project`, which is the user's edit of the project:
    /// the URL is read off the repository by the daemon, never entered by the user, and an edit
    /// must not overwrite a value the git status sweep has just corrected.
    pub fn set_project_remote_url(&self, id: &str, remote_url: Option<&str>) -> Result<()> {
        self.lock().execute(
            "UPDATE projects SET remote_url = ?2 WHERE id = ?1",
            params![id, remote_url],
        )?;
        Ok(())
    }

    /// Writes the project's record and its registered providers' rows. A stored default agent
    /// naming a provider that is not registered is kept while the project still reads as it did
    /// (the first registered provider), so an edit of anything else does not lose it.
    pub fn update_project(&self, project: &Project) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            &format!(
                "UPDATE projects SET name = ?2,
                     default_agent = CASE WHEN ?3 = ?12 AND NOT ({}) THEN default_agent
                                          ELSE ?3 END,
                     pinned = ?4, tags = ?5, description = ?6, development = ?7,
                     rebase_before_delivery = ?8, delivery = ?9, revert_gate = ?10,
                     squash_before_delivery = ?11
                 WHERE id = ?1",
                registered_provider_clause("default_agent"),
            ),
            params![
                project.id,
                project.name,
                project.default_agent.as_ref().map(enum_to_text),
                project.pinned,
                tags_to_text(&project.tags),
                project.description,
                project.git_workflow.development.as_ref().map(enum_to_text),
                project.git_workflow.rebase_before_delivery,
                project.git_workflow.delivery.as_ref().map(enum_to_text),
                project.git_workflow.revert_gate,
                project.git_workflow.squash_before_delivery,
                enum_to_text(&AgentId::ALL[0]),
            ],
        )?;
        if changed > 0 {
            write_provider_rows(&tx, project)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_project(&self, id: &str) -> Result<()> {
        self.lock()
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_project(&self, id: &str) -> Result<Option<Project>> {
        let conn = self.lock();
        let project = conn
            .query_row(
                &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1"),
                params![id],
                read_project,
            )
            .optional()?;
        let Some(project) = project else {
            return Ok(None);
        };
        let mut projects = [project];
        read_provider_rows(&conn, Some(id), &mut projects)?;
        let [project] = projects;
        Ok(Some(project))
    }

    /// The project `id`, or the `unknown_project` refusal when there is none.
    pub fn require_project(&self, id: &str) -> Result<Project> {
        self.get_project(id)?
            .ok_or_else(|| CodedError::unknown_project(id).into())
    }

    /// The paths of this console's projects, as stored: absolute and lexically normalised.
    pub fn project_paths(&self, console_id: &str) -> Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT path FROM projects WHERE console_id = ?1")?;
        let paths = stmt
            .query_map(params![console_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(paths)
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {PROJECT_COLUMNS} FROM projects ORDER BY name"
        ))?;
        let mut projects = stmt
            .query_map([], read_project)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        read_provider_rows(&conn, None, &mut projects)?;
        Ok(projects)
    }

    /// The directories under which the user has agreed Octoboard may press any agent's trust
    /// confirmation, as stored: absolute and lexically normalised by whoever wrote them.
    pub fn trusted_directories(&self) -> Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT path FROM trusted_directories ORDER BY path")?;
        let rows = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(rows)
    }

    /// Whether the directory was not already trusted.
    pub fn add_trusted_directory(&self, path: &str) -> Result<bool> {
        let added = self.lock().execute(
            "INSERT OR IGNORE INTO trusted_directories (path) VALUES (?1)",
            params![path],
        )?;
        Ok(added > 0)
    }

    /// Whether the directory was trusted.
    pub fn remove_trusted_directory(&self, path: &str) -> Result<bool> {
        let removed = self.lock().execute(
            "DELETE FROM trusted_directories WHERE path = ?1",
            params![path],
        )?;
        Ok(removed > 0)
    }
}

/// A project's record without its provider rows. A default agent naming no registered provider
/// reads as the first registered one.
fn read_project(row: &Row<'_>) -> rusqlite::Result<Project> {
    let id: String = row.get(0)?;
    let tags = tags_from_text(&id, &row.get::<_, String>(10)?);
    let default_agent: Option<String> = row.get(5)?;
    Ok(Project {
        id,
        console_id: row.get(1)?,
        host_id: row.get(2)?,
        name: row.get(3)?,
        path: row.get(4)?,
        default_agent: default_agent
            .map(|id| AgentId::from_wire_name(&id).unwrap_or(AgentId::ALL[0])),
        source: enum_from_row::<ProjectSource>(row, 6)?,
        remote_url: row.get(7)?,
        accounts: PerAgent::default(),
        launch: PerAgent::default(),
        trust_consent: row.get(8)?,
        pinned: row.get(9)?,
        tags,
        description: row.get(11)?,
        git_workflow: GitWorkflowOverride {
            development: enum_from_row_opt(row, 12)?,
            squash_before_delivery: row.get(16)?,
            rebase_before_delivery: row.get(13)?,
            delivery: enum_from_row_opt(row, 14)?,
            revert_gate: row.get(15)?,
        },
    })
}

/// Replaces the provider rows of the project with the project's values, for the registered
/// providers only: rows naming another provider are left as they are. A provider holding nothing
/// but defaults has no row.
fn write_provider_rows(tx: &Transaction<'_>, project: &Project) -> Result<()> {
    tx.execute(
        &format!(
            "DELETE FROM project_providers WHERE project_id = ?1 AND {}",
            registered_provider_clause("provider")
        ),
        params![project.id],
    )?;
    for (agent, account) in project.accounts.iter() {
        let launch = &project.launch[agent];
        let account_id = account.account_id();
        let account_default = *account == ProjectAccount::Default;
        if account_id.is_none()
            && !account_default
            && launch.model.is_none()
            && launch.effort.is_none()
        {
            continue;
        }
        tx.execute(
            "INSERT INTO project_providers (project_id, provider, account_id, account_default,
                                            model, effort)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                project.id,
                agent.wire_name(),
                account_id,
                account_default,
                launch.model,
                launch.effort
            ],
        )?;
    }
    Ok(())
}

/// Fills in the provider rows of `projects`, those of the project `only` or of all of them. A row
/// naming a provider this build does not register is left out.
fn read_provider_rows(
    conn: &Connection,
    only: Option<&str>,
    projects: &mut [Project],
) -> Result<()> {
    let positions: HashMap<String, usize> = projects
        .iter()
        .enumerate()
        .map(|(position, project)| (project.id.clone(), position))
        .collect();
    let mut stmt = conn.prepare(
        "SELECT project_id, provider, account_id, account_default, model, effort
         FROM project_providers WHERE ?1 IS NULL OR project_id = ?1",
    )?;
    let rows = stmt
        .query_map(params![only], |row| {
            Ok((
                row.get::<_, String>(0)?,
                agent_from_row(row, 1)?,
                project_account(row.get(2)?, row.get(3)?),
                Launch {
                    model: row.get(4)?,
                    effort: row.get(5)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (project_id, agent, account, launch) in rows {
        if let (Some(&position), Some(agent)) = (positions.get(&project_id), agent) {
            projects[position].accounts[agent] = account;
            projects[position].launch[agent] = launch;
        }
    }
    Ok(())
}

/// A project's account setting from its id column and its pins-the-default flag. An id wins over
/// the flag, though nothing writes both.
fn project_account(id: Option<String>, pins_default: bool) -> ProjectAccount {
    match id {
        Some(id) => ProjectAccount::Account { id },
        None if pins_default => ProjectAccount::Default,
        None => ProjectAccount::Inherit,
    }
}

fn tags_to_text(tags: &[String]) -> String {
    serde_json::to_string(tags).expect("a list of strings serializes to JSON")
}

/// A malformed value reads as no tags, with a warning, rather than failing the whole project read.
/// The daemon is the only writer, so this can only mean a hand-edited database, or one written by a later version.
fn tags_from_text(project_id: &str, text: &str) -> Vec<String> {
    serde_json::from_str(text).unwrap_or_else(|err| {
        tracing::warn!(project = %project_id, %err, "a project's tags are not a JSON array of strings; reading it as untagged");
        Vec::new()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{error_code, error_parts};

    use crate::store::test_support::*;

    /// A project that is stored is read back, and one that is not is refused with the
    /// `unknown_project` error naming the id that was asked for.
    #[test]
    fn requiring_a_project_reads_a_stored_one_and_refuses_an_unknown_one_with_its_code() {
        let path = temp_db("require-project");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");

        assert_eq!(
            store.require_project("project-1").expect("stored").id,
            "project-1"
        );

        let refused: anyhow::Error = store.require_project("nope").expect_err("unknown").into();
        assert_eq!(
            error_parts(&refused),
            error_parts(&CodedError::unknown_project("nope"))
        );
        assert_eq!(error_parts(&refused).0, error_code::UNKNOWN_PROJECT);
        assert_eq!(error_parts(&refused).2, "unknown project nope");
    }

    /// The three states of a project's account for an agent round-trip through insert, update and
    /// read, each agent's independent, and the id column never holds anything but an account id.
    #[test]
    fn a_projects_three_account_states_round_trip() {
        let path = temp_db("project-account-states");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut stored = project(false);
        stored.accounts[AgentId::CLAUDE] = named("claude-acct");
        stored.accounts[AgentId::CODEX] = ProjectAccount::Default;
        store.insert_project(&stored).expect("insert");
        let read = || store.get_project("project-1").unwrap().unwrap();
        let states = |project: &Project| {
            [
                project.accounts[AgentId::CLAUDE].clone(),
                project.accounts[AgentId::CODEX].clone(),
                project.accounts[AgentId::GROK].clone(),
            ]
        };
        assert_eq!(
            states(&read()),
            [
                named("claude-acct"),
                ProjectAccount::Default,
                ProjectAccount::Inherit
            ]
        );

        // Every state can be moved to every other by an update.
        let all = [
            ProjectAccount::Inherit,
            ProjectAccount::Default,
            named("claude-acct"),
        ];
        for from in &all {
            for to in &all {
                stored.accounts[AgentId::GROK] = from.clone();
                store.update_project(&stored).expect("update");
                assert_eq!(read().accounts[AgentId::GROK], *from);
                stored.accounts[AgentId::GROK] = to.clone();
                store.update_project(&stored).expect("update");
                assert_eq!(read().accounts[AgentId::GROK], *to);
            }
        }

        // The default state stores no id at all, only its flag.
        stored.accounts[AgentId::GROK] = ProjectAccount::Default;
        store.update_project(&stored).expect("update");
        let row = |provider: &str| -> Option<(Option<String>, bool)> {
            store
                .lock()
                .query_row(
                    "SELECT account_id, account_default FROM project_providers
                     WHERE project_id = 'project-1' AND provider = ?1",
                    params![provider],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .unwrap()
        };
        assert_eq!(row("grok"), Some((None, true)));

        // A provider that inherits and sets nothing else has no row.
        stored.accounts[AgentId::GROK] = ProjectAccount::Inherit;
        store.update_project(&stored).expect("update");
        assert_eq!(row("grok"), None);
    }

    /// A project's own accounts, models and efforts start unset; all nine are written and read back
    /// distinct.
    #[test]
    fn the_project_setting_columns_round_trip() {
        let path = temp_db("project-setting-columns");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(true)).expect("project");
        let mut kept = store.get_project("project-1").unwrap().expect("kept");
        assert!(kept.trust_consent);
        assert_eq!(kept.accounts[AgentId::CLAUDE], ProjectAccount::Inherit);
        assert_eq!(kept.launch[AgentId::GROK].effort, None);
        kept.accounts[AgentId::CLAUDE] = named("claude-account");
        kept.accounts[AgentId::CODEX] = named("codex-account");
        kept.accounts[AgentId::GROK] = named("grok-account");
        kept.launch[AgentId::CLAUDE].model = Some("claude-model".to_string());
        kept.launch[AgentId::CLAUDE].effort = Some("claude-effort".to_string());
        kept.launch[AgentId::CODEX].model = Some("codex-model".to_string());
        kept.launch[AgentId::CODEX].effort = Some("codex-effort".to_string());
        kept.launch[AgentId::GROK].model = Some("grok-model".to_string());
        kept.launch[AgentId::GROK].effort = Some("grok-effort".to_string());
        store.update_project(&kept).expect("writes the columns");

        let fields = |project: &Project| {
            [
                project.accounts[AgentId::CLAUDE]
                    .account_id()
                    .map(str::to_string),
                project.accounts[AgentId::CODEX]
                    .account_id()
                    .map(str::to_string),
                project.accounts[AgentId::GROK]
                    .account_id()
                    .map(str::to_string),
                project.launch[AgentId::CLAUDE].model.clone(),
                project.launch[AgentId::CLAUDE].effort.clone(),
                project.launch[AgentId::CODEX].model.clone(),
                project.launch[AgentId::CODEX].effort.clone(),
                project.launch[AgentId::GROK].model.clone(),
                project.launch[AgentId::GROK].effort.clone(),
            ]
            .map(|field| field.expect("set"))
        };
        let expected = [
            "claude-account",
            "codex-account",
            "grok-account",
            "claude-model",
            "claude-effort",
            "codex-model",
            "codex-effort",
            "grok-model",
            "grok-effort",
        ];
        let reread = store.get_project("project-1").unwrap().expect("kept");
        assert_eq!(fields(&reread), expected);
        // The trust permission and the pin sit after those columns in the row and are read by
        // position, so they are checked to have stayed where they were.
        assert!(reread.trust_consent);
        assert!(!reread.pinned);

        // A project inserted with all nine set reads them back the same way.
        let mut inserted = project(false);
        inserted.id = "project-2".to_string();
        inserted.path = "/tmp/project-2".to_string();
        inserted.accounts[AgentId::CLAUDE] = kept.accounts[AgentId::CLAUDE].clone();
        inserted.accounts[AgentId::CODEX] = kept.accounts[AgentId::CODEX].clone();
        inserted.accounts[AgentId::GROK] = kept.accounts[AgentId::GROK].clone();
        inserted.launch[AgentId::CLAUDE].model = kept.launch[AgentId::CLAUDE].model.clone();
        inserted.launch[AgentId::CLAUDE].effort = kept.launch[AgentId::CLAUDE].effort.clone();
        inserted.launch[AgentId::CODEX].model = kept.launch[AgentId::CODEX].model.clone();
        inserted.launch[AgentId::CODEX].effort = kept.launch[AgentId::CODEX].effort.clone();
        inserted.launch[AgentId::GROK].model = kept.launch[AgentId::GROK].model.clone();
        inserted.launch[AgentId::GROK].effort = kept.launch[AgentId::GROK].effort.clone();
        store.insert_project(&inserted).expect("insert");
        let listed = store
            .list_projects()
            .unwrap()
            .into_iter()
            .find(|project| project.id == "project-2")
            .expect("listed");
        assert_eq!(fields(&listed), expected);
        assert!(!listed.trust_consent);
    }

    /// The remote URL is written and cleared on its own, and an edit of the project leaves it be.
    #[test]
    fn a_projects_remote_url_is_set_and_cleared_apart_from_its_edits() {
        let path = temp_db("remote-url");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");

        store
            .set_project_remote_url("project-1", Some("git@example.com:o/r.git"))
            .unwrap();
        let mut edited = store.get_project("project-1").unwrap().unwrap();
        assert_eq!(
            edited.remote_url.as_deref(),
            Some("git@example.com:o/r.git")
        );
        edited.name = "Renamed".to_string();
        edited.remote_url = None;
        store.update_project(&edited).unwrap();
        let stored = store.get_project("project-1").unwrap().unwrap();
        assert_eq!(stored.name, "Renamed");
        assert_eq!(
            stored.remote_url.as_deref(),
            Some("git@example.com:o/r.git")
        );

        store.set_project_remote_url("project-1", None).unwrap();
        assert_eq!(
            store.get_project("project-1").unwrap().unwrap().remote_url,
            None
        );
    }

    /// A project's pinned flag starts unset and round-trips through an update.
    #[test]
    fn a_projects_pinned_flag_round_trips() {
        let path = temp_db("project-pinned");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        assert!(!store.get_project("project-1").unwrap().unwrap().pinned);

        let mut pinned = store.get_project("project-1").unwrap().expect("kept");
        pinned.pinned = true;
        store.update_project(&pinned).unwrap();
        assert!(store.get_project("project-1").unwrap().unwrap().pinned);
    }

    /// The consent is its own statement: a project starts without it, recording it survives a read
    /// back, and the user's own edit of the project neither grants nor withdraws it.
    #[test]
    fn the_trust_consent_is_recorded_on_its_own_and_an_edit_leaves_it_alone() {
        let path = temp_db("trust-consent");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        let consented = |store: &Store| {
            store
                .get_project("project-1")
                .unwrap()
                .unwrap()
                .trust_consent
        };
        assert!(!consented(&store));

        store
            .set_project_trust_consent("project-1", true)
            .expect("recorded");
        assert!(consented(&store));
        assert!(store.list_projects().unwrap()[0].trust_consent);

        let mut edited = store.get_project("project-1").unwrap().unwrap();
        edited.name = "Renamed".to_string();
        edited.trust_consent = false;
        store.update_project(&edited).expect("edited");
        assert!(consented(&store), "an edit must not withdraw the consent");

        store
            .set_project_trust_consent("project-1", false)
            .expect("withdrawn");
        assert!(!consented(&store));
    }

    /// A console's project paths are listed as stored, and only its own.
    #[test]
    fn a_console_lists_only_its_own_project_paths() {
        let path = temp_db("project-paths");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut stored = project(false);
        stored.path = "/work/project".to_string();
        store.insert_project(&stored).expect("project");

        assert_eq!(
            store.project_paths("console-1").unwrap(),
            vec!["/work/project".to_string()]
        );
        assert!(store.project_paths("console-2").unwrap().is_empty());
    }

    /// A fresh database starts with no trusted directories, and what is added survives opening it
    /// again.
    #[test]
    fn the_trusted_directories_start_empty_and_round_trip() {
        let path = temp_db("trusted-directories");
        let store = Store::open(&path).expect("store");
        assert!(store.trusted_directories().unwrap().is_empty());

        assert!(store.add_trusted_directory("/work").unwrap());
        assert!(
            !store.add_trusted_directory("/work").unwrap(),
            "a repeat adds nothing"
        );
        assert!(store.add_trusted_directory("/another").unwrap());
        drop(store);

        let store = Store::open(&path).expect("opens again");
        assert_eq!(store.trusted_directories().unwrap(), ["/another", "/work"]);
        assert!(store.remove_trusted_directory("/work").unwrap());
        assert!(!store.remove_trusted_directory("/work").unwrap());
        assert_eq!(store.trusted_directories().unwrap(), ["/another"]);
    }

    /// A freshly created project has no tags, and tags round-trip through an update.
    #[test]
    fn a_projects_tags_round_trip() {
        let path = temp_db("project-tags");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        let mut project = store.get_project("project-1").unwrap().expect("kept");
        assert!(project.tags.is_empty());

        project.tags = vec!["backend".to_string(), "Rust".to_string()];
        store.update_project(&project).unwrap();
        assert_eq!(
            store.get_project("project-1").unwrap().unwrap().tags,
            ["backend", "Rust"]
        );
    }

    /// A freshly created project has no description, and a description round-trips through an
    /// update and is cleared by one.
    #[test]
    fn a_projects_description_round_trips() {
        let path = temp_db("project-description");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        let mut project = store.get_project("project-1").unwrap().expect("kept");
        assert_eq!(project.description, None);

        project.description = Some("A billing service.".to_string());
        store.update_project(&project).unwrap();
        assert_eq!(
            store
                .get_project("project-1")
                .unwrap()
                .unwrap()
                .description
                .as_deref(),
            Some("A billing service.")
        );

        project.description = None;
        store.update_project(&project).unwrap();
        assert_eq!(
            store.get_project("project-1").unwrap().unwrap().description,
            None
        );
    }

    #[test]
    fn tags_that_are_not_a_json_array_of_strings_read_as_untagged() {
        assert!(tags_from_text("project-1", "not json").is_empty());
    }

    /// Every registered provider's account setting and launch pair round-trip, each independent of
    /// the others, and a provider holding only defaults has no row.
    #[test]
    fn every_registered_providers_values_round_trip_and_defaults_have_no_rows() {
        let path = temp_db("project-every-provider");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut stored = project(false);
        for (position, agent) in AgentId::ALL.into_iter().enumerate() {
            let id = agent.wire_name();
            stored.accounts[agent] = match position % 3 {
                0 => named(&format!("{id}-account")),
                1 => ProjectAccount::Default,
                _ => ProjectAccount::Inherit,
            };
            stored.launch[agent].model = Some(format!("{id}-model"));
            stored.launch[agent].effort = Some(format!("{id}-effort"));
        }
        store.insert_project(&stored).expect("insert");
        let read = store.get_project("project-1").unwrap().expect("stored");
        assert_eq!(read.accounts, stored.accounts);
        assert_eq!(read.launch, stored.launch);
        assert_eq!(store.list_projects().unwrap()[0].launch, stored.launch);

        let rows = || -> i64 {
            store
                .lock()
                .query_row("SELECT COUNT(*) FROM project_providers", [], |row| {
                    row.get(0)
                })
                .unwrap()
        };
        assert_eq!(rows(), AgentId::ALL.len() as i64);
        stored.accounts = Default::default();
        stored.launch = Default::default();
        store.update_project(&stored).expect("update");
        assert_eq!(rows(), 0);
    }

    /// Rows naming a provider that is not registered stay in the store through writes and are left
    /// out of a load, and a default agent naming one falls back to the first registered provider.
    #[test]
    fn rows_of_an_unregistered_provider_are_kept_and_left_out_of_a_load() {
        let path = temp_db("project-unregistered-provider");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut stored = project(false);
        stored.accounts[AgentId::CODEX] = named("codex-account");
        store.insert_project(&stored).expect("insert");
        store
            .lock()
            .execute_batch(
                "INSERT INTO project_providers (project_id, provider, account_id, account_default,
                                                model, effort)
                     VALUES ('project-1', 'retired', 'retired-account', 0, 'm', 'e');
                 UPDATE projects SET default_agent = 'retired';",
            )
            .unwrap();
        let read = store.get_project("project-1").unwrap().expect("not hidden");
        assert_eq!(read.default_agent, Some(AgentId::ALL[0]));
        assert_eq!(read.accounts[AgentId::CODEX], named("codex-account"));
        assert_eq!(store.list_projects().unwrap().len(), 1);

        stored.accounts[AgentId::CODEX] = ProjectAccount::Inherit;
        store.update_project(&stored).expect("update");
        let rows = |provider: &str| -> i64 {
            store
                .lock()
                .query_row(
                    "SELECT COUNT(*) FROM project_providers WHERE provider = ?1",
                    params![provider],
                    |row| row.get(0),
                )
                .unwrap()
        };
        assert_eq!(rows("retired"), 1);
        assert_eq!(rows("codex"), 0);

        // The agent it named stays named through a write that reads as it did, and a write that
        // chooses another, or none, replaces it.
        let default_agent = || -> Option<String> {
            store
                .lock()
                .query_row("SELECT default_agent FROM projects", [], |row| row.get(0))
                .unwrap()
        };
        store
            .lock()
            .execute_batch("UPDATE projects SET default_agent = 'retired';")
            .unwrap();
        stored.default_agent = Some(AgentId::ALL[0]);
        store.update_project(&stored).expect("update");
        assert_eq!(default_agent().as_deref(), Some("retired"));
        stored.default_agent = Some(AgentId::GROK);
        store.update_project(&stored).expect("update");
        assert_eq!(default_agent().as_deref(), Some("grok"));
        stored.default_agent = None;
        store.update_project(&stored).expect("update");
        assert_eq!(default_agent(), None);

        store.delete_project("project-1").unwrap();
        assert_eq!(rows("retired"), 0);
    }
}
