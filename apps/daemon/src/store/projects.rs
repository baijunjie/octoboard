//! The projects table, and the directories the user has trusted for every project under them.

use anyhow::Result;
use rusqlite::{params, OptionalExtension, Row};

use super::rows::{column_slot, enum_from_row, enum_from_row_opt, enum_to_text, launch_from_row};
use super::Store;
use crate::git_workflow::GitWorkflowOverride;
use crate::protocol::{Agent, CodedError, PerAgent, Project, ProjectAccount, ProjectSource};

const PROJECT_COLUMNS: &str = "id, console_id, host_id, name, path, default_agent, source,
                               remote_url, claude_account_id, codex_account_id, grok_account_id,
                               claude_account_default, codex_account_default, grok_account_default,
                               claude_session_model, claude_session_effort, codex_session_model,
                               codex_session_effort, grok_session_model, grok_session_effort,
                               trust_consent, pinned, tags, description, development,
                               rebase_before_merge, delivery, revert_gate, squash_before_delivery";

impl Store {
    pub fn insert_project(&self, project: &Project) -> Result<()> {
        self.lock().execute(
            "INSERT INTO projects (id, console_id, host_id, name, path, default_agent, source,
                                   remote_url, claude_account_id, codex_account_id,
                                   grok_account_id, claude_account_default,
                                   codex_account_default, grok_account_default,
                                   claude_session_model, claude_session_effort,
                                   codex_session_model, codex_session_effort, grok_session_model,
                                   grok_session_effort, trust_consent, pinned, tags,
                                   description, development, rebase_before_merge, delivery,
                                   revert_gate, squash_before_delivery)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29)",
            params![
                project.id,
                project.console_id,
                project.host_id,
                project.name,
                project.path,
                project.default_agent.as_ref().map(enum_to_text),
                enum_to_text(&project.source),
                project.remote_url,
                project.accounts.claude.account_id(),
                project.accounts.codex.account_id(),
                project.accounts.grok.account_id(),
                project.accounts.claude == ProjectAccount::Default,
                project.accounts.codex == ProjectAccount::Default,
                project.accounts.grok == ProjectAccount::Default,
                project.launch.claude.model,
                project.launch.claude.effort,
                project.launch.codex.model,
                project.launch.codex.effort,
                project.launch.grok.model,
                project.launch.grok.effort,
                project.trust_consent,
                project.pinned,
                tags_to_text(&project.tags),
                project.description,
                project.git_workflow.development.as_ref().map(enum_to_text),
                project.git_workflow.rebase_before_merge,
                project.git_workflow.delivery.as_ref().map(enum_to_text),
                project.git_workflow.revert_gate,
                project.git_workflow.squash_before_delivery,
            ],
        )?;
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

    pub fn update_project(&self, project: &Project) -> Result<()> {
        self.lock().execute(
            "UPDATE projects SET name = ?2, default_agent = ?3, claude_account_id = ?4,
                                 codex_account_id = ?5, grok_account_id = ?6,
                                 claude_account_default = ?7, codex_account_default = ?8,
                                 grok_account_default = ?9,
                                 claude_session_model = ?10, claude_session_effort = ?11,
                                 codex_session_model = ?12, codex_session_effort = ?13,
                                 grok_session_model = ?14, grok_session_effort = ?15,
                                 pinned = ?16, tags = ?17, description = ?18,
                                 development = ?19, rebase_before_merge = ?20, delivery = ?21,
                                 revert_gate = ?22, squash_before_delivery = ?23
             WHERE id = ?1",
            params![
                project.id,
                project.name,
                project.default_agent.as_ref().map(enum_to_text),
                project.accounts.claude.account_id(),
                project.accounts.codex.account_id(),
                project.accounts.grok.account_id(),
                project.accounts.claude == ProjectAccount::Default,
                project.accounts.codex == ProjectAccount::Default,
                project.accounts.grok == ProjectAccount::Default,
                project.launch.claude.model,
                project.launch.claude.effort,
                project.launch.codex.model,
                project.launch.codex.effort,
                project.launch.grok.model,
                project.launch.grok.effort,
                project.pinned,
                tags_to_text(&project.tags),
                project.description,
                project.git_workflow.development.as_ref().map(enum_to_text),
                project.git_workflow.rebase_before_merge,
                project.git_workflow.delivery.as_ref().map(enum_to_text),
                project.git_workflow.revert_gate,
                project.git_workflow.squash_before_delivery,
            ],
        )?;
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
        Ok(project)
    }

    /// The project `id`, or the `unknown_project` refusal when there is none.
    pub fn require_project(&self, id: &str) -> Result<Project> {
        self.get_project(id)?
            .ok_or_else(|| CodedError::unknown_project(id))
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
        let rows = stmt
            .query_map([], read_project)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
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

fn read_project(row: &Row<'_>) -> rusqlite::Result<Project> {
    let id: String = row.get(0)?;
    let tags = tags_from_text(&id, &row.get::<_, String>(22)?);
    Ok(Project {
        id,
        console_id: row.get(1)?,
        host_id: row.get(2)?,
        name: row.get(3)?,
        path: row.get(4)?,
        default_agent: enum_from_row_opt::<Agent>(row, 5)?,
        source: enum_from_row::<ProjectSource>(row, 6)?,
        remote_url: row.get(7)?,
        accounts: PerAgent::try_from_fn(|agent| {
            let slot = column_slot(agent);
            Ok::<_, rusqlite::Error>(project_account(row.get(8 + slot)?, row.get(11 + slot)?))
        })?,
        launch: PerAgent::try_from_fn(|agent| launch_from_row(row, 14 + 2 * column_slot(agent)))?,
        trust_consent: row.get(20)?,
        pinned: row.get(21)?,
        tags,
        description: row.get(23)?,
        git_workflow: GitWorkflowOverride {
            development: enum_from_row_opt(row, 24)?,
            squash_before_delivery: row.get(28)?,
            rebase_before_merge: row.get(25)?,
            delivery: enum_from_row_opt(row, 26)?,
            revert_gate: row.get(27)?,
        },
    })
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

        let refused = store.require_project("nope").expect_err("unknown");
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
        stored.accounts.claude = named("claude-acct");
        stored.accounts.codex = ProjectAccount::Default;
        store.insert_project(&stored).expect("insert");
        let read = || store.get_project("project-1").unwrap().unwrap();
        let states = |project: &Project| {
            [
                project.accounts.claude.clone(),
                project.accounts.codex.clone(),
                project.accounts.grok.clone(),
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
                stored.accounts.grok = from.clone();
                store.update_project(&stored).expect("update");
                assert_eq!(read().accounts.grok, *from);
                stored.accounts.grok = to.clone();
                store.update_project(&stored).expect("update");
                assert_eq!(read().accounts.grok, *to);
            }
        }

        // The default state stores no id at all, only its flag.
        stored.accounts.grok = ProjectAccount::Default;
        store.update_project(&stored).expect("update");
        let (id, flag): (Option<String>, bool) = store
            .lock()
            .query_row(
                "SELECT grok_account_id, grok_account_default FROM projects WHERE id = 'project-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((id, flag), (None, true));
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
        assert_eq!(kept.accounts.claude, ProjectAccount::Inherit);
        assert_eq!(kept.launch.grok.effort, None);
        kept.accounts.claude = named("claude-account");
        kept.accounts.codex = named("codex-account");
        kept.accounts.grok = named("grok-account");
        kept.launch.claude.model = Some("claude-model".to_string());
        kept.launch.claude.effort = Some("claude-effort".to_string());
        kept.launch.codex.model = Some("codex-model".to_string());
        kept.launch.codex.effort = Some("codex-effort".to_string());
        kept.launch.grok.model = Some("grok-model".to_string());
        kept.launch.grok.effort = Some("grok-effort".to_string());
        store.update_project(&kept).expect("writes the columns");

        let fields = |project: &Project| {
            [
                project.accounts.claude.account_id().map(str::to_string),
                project.accounts.codex.account_id().map(str::to_string),
                project.accounts.grok.account_id().map(str::to_string),
                project.launch.claude.model.clone(),
                project.launch.claude.effort.clone(),
                project.launch.codex.model.clone(),
                project.launch.codex.effort.clone(),
                project.launch.grok.model.clone(),
                project.launch.grok.effort.clone(),
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
        inserted.accounts.claude = kept.accounts.claude.clone();
        inserted.accounts.codex = kept.accounts.codex.clone();
        inserted.accounts.grok = kept.accounts.grok.clone();
        inserted.launch.claude.model = kept.launch.claude.model.clone();
        inserted.launch.claude.effort = kept.launch.claude.effort.clone();
        inserted.launch.codex.model = kept.launch.codex.model.clone();
        inserted.launch.codex.effort = kept.launch.codex.effort.clone();
        inserted.launch.grok.model = kept.launch.grok.model.clone();
        inserted.launch.grok.effort = kept.launch.grok.effort.clone();
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
}
