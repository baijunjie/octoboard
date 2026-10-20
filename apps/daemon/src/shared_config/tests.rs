use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::*;
use crate::coordinator::handle;
use crate::protocol::{AgentAvailability, Availability, RequestBody, SessionStatus};
use crate::test_support::{self, ScratchDir};

/// A console with accounts `claude-a`, `codex-a` and `grok-a` (directories `<dir>/<id>`), and
/// each agent's default directory at `<dir>/default-<agent>`, created.
struct Fixture {
    state: Arc<AppState>,
    dir: ScratchDir,
}

impl Fixture {
    fn new() -> Self {
        let (state, dir) = test_support::console_with_project_and_accounts("shared-config");
        state.set_agent_availability(
            Agent::ALL
                .into_iter()
                .map(|agent| {
                    let default = dir.join(format!("default-{}", agent.label()));
                    std::fs::create_dir_all(&default).unwrap();
                    AgentAvailability {
                        agent,
                        availability: Availability::Available,
                        default_account_dir: Some(default.to_string_lossy().into_owned()),
                    }
                })
                .collect(),
        );
        Self { state, dir }
    }

    fn default(&self, agent: Agent) -> PathBuf {
        self.dir.join(format!("default-{}", agent.label()))
    }

    fn account(&self, agent: Agent) -> PathBuf {
        self.dir.join(account_id(agent))
    }

    fn record(&self, agent: Agent) -> Account {
        self.state
            .store
            .get_account(account_id(agent))
            .unwrap()
            .unwrap()
    }

    async fn share(&self, agent: Agent, share: bool) -> Result<()> {
        handle(
            &self.state,
            None,
            RequestBody::SetAccountSharing {
                account: Some(account_id(agent).to_string()),
                share,
            },
        )
        .await
        .map(|_| ())
    }

    /// A session of the account recorded in the store, not running.
    fn session(&self, id: &str, agent: Agent) -> Session {
        let session = Session {
            account_id: Some(account_id(agent).to_string()),
            config_dir: Some(self.account(agent).to_string_lossy().into_owned()),
            has_conversation: false,
            ..test_support::project_session(id, agent, SessionStatus::Interrupted)
        };
        self.state.store.insert_session(&session).unwrap();
        session
    }
}

fn account_id(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude-a",
        Agent::Codex => "codex-a",
        Agent::Grok => "grok-a",
    }
}

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn read(root: &Path, relative: &str) -> String {
    std::fs::read_to_string(root.join(relative)).unwrap()
}

fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

fn real(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

/// The entries of `dir` named `<name>.bak-…`.
fn backups(dir: &Path, name: &str) -> Vec<PathBuf> {
    let prefix = format!("{name}.bak-");
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

fn code_of(err: &anyhow::Error) -> &'static str {
    err.downcast_ref::<CodedError>()
        .expect("a coded error")
        .code
}

/// A Claude Code default directory and account holding a bit of everything turning sharing on
/// treats differently.
fn claude_setup(fixture: &Fixture) -> (PathBuf, PathBuf) {
    let (default, account) = (
        fixture.default(Agent::Claude),
        fixture.account(Agent::Claude),
    );
    write(&default, "settings.json", "default settings");
    write(&default, "skills/shared/SKILL.md", "shared skill");
    write(&default, "projects/p/both.jsonl", "default's copy");
    write(&default, "projects/p/default-only.jsonl", "default");
    write(&default, "history.jsonl", "{\"display\":\"default\"}\n");
    write(&account, "settings.json", "account settings");
    write(&account, "keybindings.json", "account keys");
    write(&account, "projects/p/both.jsonl", "account's later copy");
    write(&account, "projects/p/account-only.jsonl", "account");
    write(&account, "projects/q/other.jsonl", "account");
    write(&account, "history.jsonl", "{\"display\":\"account\"}");
    write(&account, "plans/plan.md", "a plan");
    test_support::move_modification_time_back(&default.join("projects/p/both.jsonl"));
    (default, account)
}

#[tokio::test]
async fn turning_sharing_on_backs_up_configuration_and_carries_history_into_the_default() {
    let fixture = Fixture::new();
    let (default, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();

    let record = fixture.record(Agent::Claude);
    assert!(record.share_default_config && !record.share_default_config_pending);
    // Configuration: the account's own copy backed up, a link in its place.
    assert_eq!(
        std::fs::read_link(account.join("settings.json")).unwrap(),
        real(&default.join("settings.json"))
    );
    let settings_backups = backups(&account, "settings.json");
    assert_eq!(settings_backups.len(), 1);
    assert_eq!(
        std::fs::read_to_string(&settings_backups[0]).unwrap(),
        "account settings"
    );
    // A file the default directory lacks is skipped; a directory it lacks is created there.
    assert_eq!(read(&account, "keybindings.json"), "account keys");
    assert!(!is_link(&account.join("keybindings.json")));
    assert!(default.join("agents").is_dir());
    assert_eq!(
        std::fs::read_link(account.join("agents")).unwrap(),
        real(&default.join("agents"))
    );
    assert_eq!(read(&account, "skills/shared/SKILL.md"), "shared skill");
    // Conversation history: moved into the default directory, then linked. The record the
    // account went on writing takes the place of the default's older copy, which is kept aside.
    assert!(is_link(&account.join("projects")));
    assert_eq!(read(&default, "projects/p/account-only.jsonl"), "account");
    assert_eq!(read(&default, "projects/q/other.jsonl"), "account");
    assert_eq!(read(&default, "projects/p/default-only.jsonl"), "default");
    assert_eq!(
        read(&default, "projects/p/both.jsonl"),
        "account's later copy"
    );
    let aside = backups(&account, "projects");
    assert_eq!(aside.len(), 1);
    assert_eq!(read(&aside[0], "p/both.jsonl"), "default's copy");
    // The prompt histories are merged into the default's.
    assert_eq!(
        read(&default, "history.jsonl"),
        "{\"display\":\"default\"}\n{\"display\":\"account\"}\n"
    );
    assert!(is_link(&account.join("history.jsonl")));
    // `plans/` is not shared.
    assert!(!is_link(&account.join("plans")));
    assert_eq!(read(&account, "plans/plan.md"), "a plan");
}

#[tokio::test]
async fn turning_sharing_off_restores_backups_and_leaves_history_in_the_default() {
    let fixture = Fixture::new();
    let (default, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    write(
        &account,
        "projects/p/while-sharing.jsonl",
        "written while sharing",
    );
    fixture.share(Agent::Claude, false).await.unwrap();

    let record = fixture.record(Agent::Claude);
    assert!(!record.share_default_config && !record.share_default_config_pending);
    assert!(!is_link(&account.join("settings.json")));
    assert_eq!(read(&account, "settings.json"), "account settings");
    assert!(backups(&account, "settings.json").is_empty());
    // An item with no backup is left absent.
    assert!(std::fs::symlink_metadata(account.join("agents")).is_err());
    assert!(std::fs::symlink_metadata(account.join("skills")).is_err());
    assert_eq!(read(&account, "keybindings.json"), "account keys");
    // The account starts with empty conversation directories; the history stays in the default.
    assert!(!is_link(&account.join("projects")));
    assert_eq!(
        std::fs::read_dir(account.join("projects")).unwrap().count(),
        0
    );
    assert!(std::fs::symlink_metadata(account.join("history.jsonl")).is_err());
    assert_eq!(read(&default, "projects/p/account-only.jsonl"), "account");
    assert_eq!(
        read(&default, "projects/p/while-sharing.jsonl"),
        "written while sharing"
    );
    assert_eq!(read(&default, "settings.json"), "default settings");
}

#[tokio::test]
async fn a_link_names_the_default_entrys_resolved_target() {
    let fixture = Fixture::new();
    let (default, account) = (
        fixture.default(Agent::Claude),
        fixture.account(Agent::Claude),
    );
    write(&fixture.dir, "dotfiles/settings.json", "from dotfiles");
    std::os::unix::fs::symlink(
        fixture.dir.join("dotfiles/settings.json"),
        default.join("settings.json"),
    )
    .unwrap();
    fixture.share(Agent::Claude, true).await.unwrap();
    assert_eq!(
        std::fs::read_link(account.join("settings.json")).unwrap(),
        real(&fixture.dir.join("dotfiles/settings.json"))
    );
}

#[tokio::test]
async fn the_default_account_is_refused() {
    let fixture = Fixture::new();
    let err = handle(
        &fixture.state,
        None,
        RequestBody::SetAccountSharing {
            account: None,
            share: true,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&err), error_code::DEFAULT_ACCOUNT_NOT_SHAREABLE);

    // So is an account whose directory is the default's under another spelling.
    let default = fixture.default(Agent::Codex);
    std::os::unix::fs::symlink(&default, fixture.account(Agent::Codex)).unwrap();
    let err = fixture.share(Agent::Codex, true).await.unwrap_err();
    assert_eq!(code_of(&err), error_code::DEFAULT_ACCOUNT_NOT_SHAREABLE);
    assert!(!fixture.record(Agent::Codex).share_default_config);
}

#[tokio::test]
async fn a_change_waits_until_none_of_the_accounts_sessions_runs() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    fixture.session("running", Agent::Claude);
    let next = fixture.session("next", Agent::Claude);
    let running = test_support::running(&fixture.state, "running");

    fixture.share(Agent::Claude, true).await.unwrap();
    let record = fixture.record(Agent::Claude);
    assert!(record.share_default_config && record.share_default_config_pending);
    assert!(!is_link(&account.join("settings.json")));

    // Another of its sessions starting meanwhile leaves it waiting.
    let prepared = before_launch(&fixture.state, &next).await.unwrap();
    assert!(prepared.shared_default_dir.is_none());
    assert!(fixture.record(Agent::Claude).share_default_config_pending);
    assert!(!is_link(&account.join("settings.json")));

    fixture.state.watch_exit((*running).clone());
    running.terminate();
    let deadline = std::time::Instant::now() + test_support::PATIENCE;
    while fixture.state.live_session("running").is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "the stand-in never ended"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let prepared = before_launch(&fixture.state, &next).await.unwrap();
    assert_eq!(
        prepared.shared_default_dir,
        Some(real(&fixture.default(Agent::Claude)))
    );
    assert!(!fixture.record(Agent::Claude).share_default_config_pending);
    assert!(is_link(&account.join("settings.json")));
}

#[tokio::test]
async fn the_choice_survives_a_daemon_restart() {
    let fixture = Fixture::new();
    fixture.session("running", Agent::Claude);
    let _claim = fixture.state.begin_launch("running").unwrap();
    fixture.share(Agent::Claude, true).await.unwrap();
    let reopened = crate::store::Store::open(&fixture.dir.join("octoboard.db")).unwrap();
    let record = reopened.get_account("claude-a").unwrap().unwrap();
    assert!(record.share_default_config && record.share_default_config_pending);
    let listed = reopened.get_settings().unwrap().accounts;
    assert!(listed
        .iter()
        .any(|account| account.id == "claude-a" && account.share_default_config_pending));
}

/// What launches were given as the shared default directory, with the stand-ins started for
/// them, kept running for the test's length.
#[derive(Clone, Default)]
struct Launches {
    given: Arc<Mutex<Vec<Option<PathBuf>>>>,
    started: Arc<Mutex<Vec<test_support::StandIn>>>,
}

impl Launches {
    /// A launch that records what it was given and starts a stand-in, or, when `fails`, refuses.
    fn launch(
        &self,
        fails: bool,
    ) -> impl FnOnce(crate::term::LaunchRequest) -> Result<crate::term::Launch> + Send + 'static
    {
        let launches = self.clone();
        move |request| {
            launches
                .given
                .lock()
                .unwrap()
                .push(request.shared_default_dir.clone());
            if fails {
                anyhow::bail!("the agent would not start");
            }
            let live = test_support::fake_live_session(
                &request.session_id,
                request.agent,
                80,
                24,
                "sleep 30",
            );
            let process = (*live).clone();
            launches.started.lock().unwrap().push(live);
            Ok(crate::term::Launch {
                session: process,
                agent_session_id: None,
                notice: None,
            })
        }
    }

    fn given(&self) -> Vec<Option<PathBuf>> {
        self.given.lock().unwrap().clone()
    }
}

/// Resumes `session` through the coordinator's relaunch.
async fn resume(fixture: &Fixture, session: &str, launches: &Launches) -> Result<()> {
    let claim = fixture.state.begin_launch(session).unwrap();
    crate::coordinator::relaunch_session_with(
        &fixture.state,
        &claim,
        session,
        None,
        false,
        None,
        launches.launch(false),
    )
    .await
}

/// The shared default directory a resume of `session` is given.
async fn resumed_with(fixture: &Fixture, session: &str) -> Option<PathBuf> {
    let launches = Launches::default();
    resume(fixture, session, &launches).await.unwrap();
    launches.given().pop().expect("launched")
}

#[tokio::test]
async fn a_file_link_an_agent_replaced_is_repaired_before_a_resume() {
    let fixture = Fixture::new();
    let (default, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    // An agent saving through the link replaced it with a plain file.
    std::fs::remove_file(account.join("settings.json")).unwrap();
    std::fs::write(account.join("settings.json"), "saved over the link").unwrap();
    std::fs::remove_file(account.join("CLAUDE.md")).ok();
    write(&default, "CLAUDE.md", "default instructions");
    fixture.session("s", Agent::Claude);

    let given = resumed_with(&fixture, "s").await;
    assert_eq!(given, Some(real(&default)));
    assert_eq!(
        std::fs::read_link(account.join("settings.json")).unwrap(),
        real(&default.join("settings.json"))
    );
    // Backed up beside the turn-on backup, under the stamp turning sharing on recorded.
    let stamp = fixture
        .state
        .store
        .account_sharing_start("claude-a")
        .unwrap()
        .unwrap()
        .stamp;
    assert_eq!(
        read(&account, &format!("settings.json.bak-{stamp}-2")),
        "saved over the link"
    );
    // An item the default directory gained since is linked too.
    assert_eq!(
        std::fs::read_link(account.join("CLAUDE.md")).unwrap(),
        real(&default.join("CLAUDE.md"))
    );
}

#[tokio::test]
async fn only_a_sharing_accounts_launch_is_given_the_shared_directory() {
    let fixture = Fixture::new();
    fixture.session("s", Agent::Claude);
    assert_eq!(resumed_with(&fixture, "s").await, None);
}

#[tokio::test]
async fn a_switch_to_an_account_carries_out_its_pending_change_first() {
    let fixture = Fixture::new();
    let (default, account) = claude_setup(&fixture);
    fixture.session("other", Agent::Claude);
    let claim = fixture.state.begin_launch("other").unwrap();
    fixture.share(Agent::Claude, true).await.unwrap();
    drop(claim);
    assert!(fixture.record(Agent::Claude).share_default_config_pending);
    // A session on the default account, with no conversation to carry.
    let session = Session {
        account_id: None,
        config_dir: None,
        ..test_support::project_session("moving", Agent::Claude, SessionStatus::Interrupted)
    };
    fixture.state.store.insert_session(&session).unwrap();

    let launches = Launches::default();
    crate::coordinator::switch_session_account_with(
        &fixture.state,
        "moving",
        Some(Some("claude-a".to_string())),
        None,
        || Ok(std::collections::HashMap::new()),
        launches.launch(false),
    )
    .await
    .unwrap();
    assert_eq!(launches.given(), vec![Some(real(&default))]);
    assert!(!fixture.record(Agent::Claude).share_default_config_pending);
    assert!(is_link(&account.join("settings.json")));
}

#[tokio::test]
async fn a_change_that_fails_part_way_says_what_it_left_where_and_stays_pending() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    // Nothing can be renamed or created in the account's directory.
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o555)).unwrap();
    let err = fixture.share(Agent::Claude, true).await.unwrap_err();
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(code_of(&err), error_code::ACCOUNT_SHARING_FAILED);
    let coded = err.downcast_ref::<CodedError>().unwrap();
    let items: Vec<serde_json::Value> = serde_json::from_str(&coded.params["items"]).unwrap();
    let settings = items
        .iter()
        .find(|item| item["item"] == "settings.json")
        .expect("settings.json reported");
    assert_eq!(settings["left"], "unchanged");
    assert_eq!(read(&account, "settings.json"), "account settings");
    assert!(fixture.record(Agent::Claude).share_default_config_pending);

    // Asking again retries, and completes now that the directory can be written; the prompt
    // history is merged once.
    fixture.share(Agent::Claude, true).await.unwrap();
    assert!(!fixture.record(Agent::Claude).share_default_config_pending);
    assert!(is_link(&account.join("settings.json")));
    // A run that appended the account's lines and stopped before removing its aside is not
    // appended again.
    let stamp = fixture
        .state
        .store
        .account_sharing_start("claude-a")
        .unwrap()
        .unwrap()
        .stamp;
    write(
        &account,
        &format!("history.jsonl.bak-{stamp}"),
        "{\"display\":\"account\"}\n",
    );
    fixture
        .state
        .store
        .set_account_sharing("claude-a", true, true)
        .unwrap();
    fixture.share(Agent::Claude, true).await.unwrap();
    assert!(backups(&account, "history.jsonl").is_empty());
    assert_eq!(
        read(&fixture.default(Agent::Claude), "history.jsonl"),
        "{\"display\":\"default\"}\n{\"display\":\"account\"}\n"
    );
}

/// A Codex state database at `path` with a `threads` table naming `rows` (id, rollout path,
/// updated at), written through a write-ahead log that is left uncheckpointed.
fn codex_database(path: &Path, rows: &[(&str, &str, i64)]) -> rusqlite::Connection {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA wal_autocheckpoint = 0;
             CREATE TABLE IF NOT EXISTS thread_sections (id TEXT PRIMARY KEY);
             CREATE TABLE IF NOT EXISTS threads (id TEXT PRIMARY KEY, rollout_path TEXT NOT NULL,
                 updated_at INTEGER NOT NULL, title TEXT NOT NULL DEFAULT '',
                 thread_section_id TEXT REFERENCES thread_sections(id) ON DELETE SET NULL);
             INSERT OR IGNORE INTO thread_sections VALUES ('section');",
        )
        .unwrap();
    for (id, rollout, updated) in rows {
        connection
            .execute(
                "INSERT OR REPLACE INTO threads (id, rollout_path, updated_at, title,
                     thread_section_id) VALUES (?1, ?2, ?3, ?1, 'section')",
                rusqlite::params![id, rollout, updated],
            )
            .unwrap();
    }
    connection
}

fn rollout_of(database: &Path, id: &str) -> Option<String> {
    use rusqlite::OptionalExtension;
    rusqlite::Connection::open(database)
        .unwrap()
        .query_row(
            "SELECT rollout_path FROM threads WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .unwrap()
}

const ROLLOUT: &str = "sessions/2026/10/20/rollout-2026-10-20T00-00-00-a.jsonl";

#[tokio::test]
async fn a_codex_accounts_history_is_merged_and_its_threads_listed_through_the_shared_database() {
    let fixture = Fixture::new();
    let (default, account) = (fixture.default(Agent::Codex), fixture.account(Agent::Codex));
    write(
        &default,
        "session_index.jsonl",
        "{\"id\":\"d\",\"thread_name\":\"default\"}\n",
    );
    write(&default, "history.jsonl", "{\"text\":\"default\"}\n");
    write(
        &account,
        "session_index.jsonl",
        "{\"id\":\"a\",\"thread_name\":\"mine\"}\n",
    );
    write(&account, "history.jsonl", "{\"text\":\"account\"}\n");
    write(&account, ROLLOUT, "rollout a");
    let default_rollout = default.join("sessions/2026/10/19/rollout-d.jsonl");
    let _shared = codex_database(
        &default.join("state_5.sqlite"),
        &[("d", &default_rollout.to_string_lossy(), 1)],
    );
    // The account's rows, one of them only in its write-ahead log.
    let own = codex_database(
        &account.join("state_5.sqlite"),
        &[("a", &account.join(ROLLOUT).to_string_lossy(), 5)],
    );
    assert!(account.join("state_5.sqlite-wal").exists());

    fixture.share(Agent::Codex, true).await.unwrap();
    drop(own);

    assert_eq!(
        read(&default, "session_index.jsonl"),
        "{\"id\":\"d\",\"thread_name\":\"default\"}\n{\"id\":\"a\",\"thread_name\":\"mine\"}\n"
    );
    assert_eq!(
        read(&default, "history.jsonl"),
        "{\"text\":\"default\"}\n{\"text\":\"account\"}\n"
    );
    assert_eq!(read(&default, ROLLOUT), "rollout a");
    // The account's database is set aside whole, its log with it.
    let aside = backups(&account, "state_5.sqlite");
    let names: Vec<String> = aside
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
    assert!(names.iter().any(|name| name.ends_with("-wal")));
    assert!(names.iter().any(|name| name.ends_with("-shm")));
    // The shared database lists the carried thread, at the rollout's place in the default.
    let shared = default.join("state_5.sqlite");
    assert_eq!(
        rollout_of(&shared, "a").map(PathBuf::from),
        Some(
            real(&default.join("sessions")).join("2026/10/20/rollout-2026-10-20T00-00-00-a.jsonl")
        )
    );
    assert_eq!(
        rollout_of(&shared, "d").map(PathBuf::from),
        Some(default_rollout)
    );
    assert_eq!(
        std::fs::read_link(account.join("state_5.sqlite")).unwrap(),
        real(&shared)
    );
}

#[tokio::test]
async fn turning_codex_sharing_off_points_the_rows_written_through_the_account_at_the_default() {
    let fixture = Fixture::new();
    let (default, account) = (fixture.default(Agent::Codex), fixture.account(Agent::Codex));
    codex_database(&default.join("state_5.sqlite"), &[]);
    std::fs::create_dir_all(default.join("sessions")).unwrap();
    fixture.share(Agent::Codex, true).await.unwrap();
    // Codex in the account records rollouts by the account's own spelling.
    write(&account, ROLLOUT, "written while sharing");
    let archived = "archived_sessions/rollout-2026-10-04T00-00-00-b.jsonl";
    write(&account, archived, "archived while sharing");
    codex_database(
        &account.join("state_5.sqlite"),
        &[
            ("a", &account.join(ROLLOUT).to_string_lossy(), 1),
            ("b", &account.join(archived).to_string_lossy(), 1),
        ],
    );

    fixture.share(Agent::Codex, false).await.unwrap();
    assert_eq!(
        rollout_of(&default.join("state_5.sqlite"), "b").map(PathBuf::from),
        Some(real(&default.join("archived_sessions")).join("rollout-2026-10-04T00-00-00-b.jsonl"))
    );
    let named = PathBuf::from(rollout_of(&default.join("state_5.sqlite"), "a").unwrap());
    assert_eq!(
        named,
        real(&default.join("sessions")).join("2026/10/20/rollout-2026-10-20T00-00-00-a.jsonl")
    );
    assert_eq!(
        std::fs::read_to_string(named).unwrap(),
        "written while sharing"
    );
    assert!(!is_link(&account.join("sessions")));
    assert!(std::fs::symlink_metadata(account.join("state_5.sqlite")).is_err());
}

#[tokio::test]
async fn turning_sharing_off_keeps_plugins_installed_through_the_account_usable() {
    let fixture = Fixture::new();
    for (agent, item, registry) in [
        (Agent::Claude, "plugins", "installed_plugins.json"),
        (Agent::Claude, "plugins", "known_marketplaces.json"),
        (Agent::Grok, "installed-plugins", "registry.json"),
    ] {
        let (default, account) = (fixture.default(agent), fixture.account(agent));
        std::fs::create_dir_all(default.join(item)).unwrap();
        fixture.share(agent, true).await.unwrap();
        // Installed from the account, through its link.
        write(&account, &format!("{item}/cache/tool/plugin.json"), "{}");
        let installed = account.join(item).join("cache/tool");
        write(
            &default,
            &format!("{item}/{registry}"),
            &serde_json::json!({"plugins": {"tool": [{"installPath": installed, "version": "1.0"}]}})
                .to_string(),
        );

        fixture.share(agent, false).await.unwrap();
        let recorded: serde_json::Value =
            serde_json::from_str(&read(&default, &format!("{item}/{registry}"))).unwrap();
        let path = PathBuf::from(
            recorded["plugins"]["tool"][0]["installPath"]
                .as_str()
                .unwrap(),
        );
        assert_eq!(
            path,
            real(&default.join(item)).join("cache/tool"),
            "{agent:?}"
        );
        assert!(path.join("plugin.json").is_file(), "{agent:?}");
        assert_eq!(recorded["plugins"]["tool"][0]["version"], "1.0");
    }
}

#[tokio::test]
async fn a_codex_state_database_the_catalog_does_not_name_is_told_before_a_launch() {
    let fixture = Fixture::new();
    let account = fixture.account(Agent::Codex);
    fixture.share(Agent::Codex, true).await.unwrap();
    write(&account, "state_6.sqlite", "");
    let session = fixture.session("s", Agent::Codex);
    let prepared = before_launch(&fixture.state, &session).await.unwrap();
    let codes: Vec<&str> = prepared.notices.iter().map(|notice| notice.code).collect();
    assert_eq!(codes, vec![notice_code::UNSHARED_STATE_DATABASE]);
    assert_eq!(
        prepared.notices[0].params["path"],
        account.join("state_6.sqlite").to_string_lossy()
    );
}

#[tokio::test]
async fn a_thread_codex_archived_behind_a_sharing_accounts_links_is_judged_to_need_unarchiving() {
    let fixture = Fixture::new();
    let (default, account) = (fixture.default(Agent::Codex), fixture.account(Agent::Codex));
    std::fs::create_dir_all(default.join("sessions")).unwrap();
    fixture.share(Agent::Codex, true).await.unwrap();
    let thread = "019a0000-0000-7000-8000-0000000000c1";
    let session = Session {
        agent_session_id: Some(thread.to_string()),
        has_conversation: true,
        ..fixture.session("archived-by-codex", Agent::Codex)
    };
    let archived = format!("archived_sessions/rollout-2026-10-04T00-00-00-{thread}.jsonl");
    write(&account, &archived, "archived by codex");
    let judged = crate::agent_records::judge(&crate::agent_records::Locator::default(), &session);
    // Nothing is deleted on a verdict reached through links, but the unarchive is still due.
    assert_eq!(
        judged.resumability,
        crate::agent_records::Resumability::Unknown
    );
    assert!(judged.needs_unarchive);

    std::fs::rename(
        default.join(&archived),
        default.join(format!(
            "sessions/rollout-2026-10-04T00-00-00-{thread}.jsonl"
        )),
    )
    .unwrap();
    let judged = crate::agent_records::judge(&crate::agent_records::Locator::default(), &session);
    assert!(!judged.needs_unarchive);
}

/// Holds a launch claim on a session of the account, which counts as one of its sessions running.
fn busy(fixture: &Fixture, agent: Agent) -> crate::state::LaunchClaim {
    fixture.session("busy", agent);
    fixture.state.begin_launch("busy").unwrap()
}

#[tokio::test]
async fn a_failed_pending_change_refuses_the_resume_it_was_tried_before() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    fixture.session("s", Agent::Claude);
    let claim = busy(&fixture, Agent::Claude);
    fixture.share(Agent::Claude, true).await.unwrap();
    drop(claim);
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o555)).unwrap();
    let launches = Launches::default();
    let err = resume(&fixture, "s", &launches).await.unwrap_err();
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(code_of(&err), error_code::ACCOUNT_SHARING_FAILED);
    assert!(launches.given().is_empty());
    assert!(fixture.record(Agent::Claude).share_default_config_pending);
    assert_eq!(
        fixture
            .state
            .store
            .get_session("s")
            .unwrap()
            .unwrap()
            .status,
        SessionStatus::Interrupted
    );
}

#[tokio::test]
async fn a_change_waits_for_a_cli_reading_the_account() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    let reading = reading_account(Some(&account));
    fixture.share(Agent::Claude, true).await.unwrap();
    assert!(fixture.record(Agent::Claude).share_default_config_pending);
    assert!(!is_link(&account.join("settings.json")));
    drop(reading);
    fixture.share(Agent::Claude, true).await.unwrap();
    assert!(!fixture.record(Agent::Claude).share_default_config_pending);
}

#[tokio::test]
async fn turning_sharing_off_before_it_was_carried_out_touches_nothing() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    let claim = busy(&fixture, Agent::Claude);
    fixture.share(Agent::Claude, true).await.unwrap();
    fixture.share(Agent::Claude, false).await.unwrap();
    drop(claim);
    fixture.share(Agent::Claude, false).await.unwrap();
    let record = fixture.record(Agent::Claude);
    assert!(!record.share_default_config && !record.share_default_config_pending);
    assert_eq!(read(&account, "settings.json"), "account settings");
    assert!(backups(&account, "settings.json").is_empty());
    assert!(fixture
        .state
        .store
        .account_sharing_start("claude-a")
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn turning_sharing_off_finds_the_links_into_a_default_directory_that_moved() {
    let fixture = Fixture::new();
    let (default, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    let moved = fixture.dir.join("default-moved");
    std::fs::rename(&default, &moved).unwrap();
    fixture
        .state
        .set_agent_availability(vec![AgentAvailability {
            agent: Agent::Claude,
            availability: Availability::Available,
            default_account_dir: Some(moved.to_string_lossy().into_owned()),
        }]);
    fixture.share(Agent::Claude, false).await.unwrap();
    assert!(!fixture.record(Agent::Claude).share_default_config_pending);
    assert_eq!(read(&account, "settings.json"), "account settings");
    for item in ["agents", "skills", "history.jsonl"] {
        assert!(
            std::fs::symlink_metadata(account.join(item)).is_err(),
            "{item}"
        );
    }
    assert!(!is_link(&account.join("projects")));
}

#[tokio::test]
async fn nothing_is_repaired_while_turning_sharing_off_waits() {
    let fixture = Fixture::new();
    let (default, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    fixture.session("s", Agent::Claude);
    let claim = busy(&fixture, Agent::Claude);
    fixture.share(Agent::Claude, false).await.unwrap();
    std::fs::remove_file(account.join("settings.json")).unwrap();
    std::fs::write(account.join("settings.json"), "saved over the link").unwrap();
    assert_eq!(resumed_with(&fixture, "s").await, Some(real(&default)));
    drop(claim);
    assert_eq!(read(&account, "settings.json"), "saved over the link");
    assert!(!is_link(&account.join("settings.json")));
}

#[tokio::test]
async fn a_history_item_the_account_links_elsewhere_itself_is_left_and_reported() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    let elsewhere = fixture.dir.join("elsewhere-projects");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::remove_dir_all(account.join("projects")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, account.join("projects")).unwrap();
    let err = fixture.share(Agent::Claude, true).await.unwrap_err();
    let coded = err.downcast_ref::<CodedError>().unwrap();
    let items: Vec<serde_json::Value> = serde_json::from_str(&coded.params["items"]).unwrap();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["item"], "projects");
    assert_eq!(items[0]["left"], "unchanged");
    assert_eq!(
        std::fs::read_link(account.join("projects")).unwrap(),
        elsewhere
    );
}

#[tokio::test]
async fn a_codex_row_carry_that_failed_is_redone_by_a_retry() {
    let fixture = Fixture::new();
    let (default, account) = (fixture.default(Agent::Codex), fixture.account(Agent::Codex));
    let shared = codex_database(&default.join("state_5.sqlite"), &[]);
    shared
        .execute_batch(
            "CREATE TRIGGER refuse BEFORE INSERT ON threads BEGIN SELECT RAISE(ABORT, 'no'); END;",
        )
        .unwrap();
    write(&account, ROLLOUT, "rollout a");
    codex_database(
        &account.join("state_5.sqlite"),
        &[("a", &account.join(ROLLOUT).to_string_lossy(), 1)],
    );
    let err = fixture.share(Agent::Codex, true).await.unwrap_err();
    assert_eq!(code_of(&err), error_code::ACCOUNT_SHARING_FAILED);
    assert_eq!(rollout_of(&default.join("state_5.sqlite"), "a"), None);

    shared.execute_batch("DROP TRIGGER refuse;").unwrap();
    fixture.share(Agent::Codex, true).await.unwrap();
    assert_eq!(
        rollout_of(&default.join("state_5.sqlite"), "a").map(PathBuf::from),
        Some(
            real(&default.join("sessions")).join("2026/10/20/rollout-2026-10-20T00-00-00-a.jsonl")
        )
    );
}

#[tokio::test]
async fn deleting_a_sharing_account_turns_its_sharing_off_first() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    let delete = || {
        handle(
            &fixture.state,
            None,
            RequestBody::DeleteAccount {
                account: "claude-a".to_string(),
            },
        )
    };
    let claim = busy(&fixture, Agent::Claude);
    let err = delete().await.unwrap_err();
    assert_eq!(code_of(&err), error_code::ACCOUNT_STILL_SHARING);
    let record = fixture.record(Agent::Claude);
    assert!(record.share_default_config && !record.share_default_config_pending);
    assert!(is_link(&account.join("settings.json")));
    drop(claim);

    delete().await.unwrap();
    assert!(fixture
        .state
        .store
        .get_account("claude-a")
        .unwrap()
        .is_none());
    assert_eq!(read(&account, "settings.json"), "account settings");
    assert!(!is_link(&account.join("projects")));
}

#[tokio::test]
async fn repointing_a_sharing_account_turns_its_sharing_off_first() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    let elsewhere = fixture.dir.join("claude-elsewhere");
    let repoint = || {
        handle(
            &fixture.state,
            None,
            RequestBody::UpdateAccount {
                account: "claude-a".to_string(),
                name: None,
                config_dir: Some(elsewhere.to_string_lossy().into_owned()),
            },
        )
    };
    // A name it may not take is refused before anything is turned off.
    let err = handle(
        &fixture.state,
        None,
        RequestBody::UpdateAccount {
            account: "claude-a".to_string(),
            name: Some("Default".to_string()),
            config_dir: Some(elsewhere.to_string_lossy().into_owned()),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&err), error_code::ACCOUNT_NAME_TAKEN);
    assert!(fixture.record(Agent::Claude).share_default_config);
    assert!(is_link(&account.join("settings.json")));

    let claim = busy(&fixture, Agent::Claude);
    let err = repoint().await.unwrap_err();
    assert_eq!(code_of(&err), error_code::ACCOUNT_STILL_SHARING);
    let record = fixture.record(Agent::Claude);
    assert_eq!(PathBuf::from(&record.config_dir), account);
    assert!(record.share_default_config && !record.share_default_config_pending);
    drop(claim);

    repoint().await.unwrap();
    let record = fixture.record(Agent::Claude);
    assert_eq!(PathBuf::from(&record.config_dir), elsewhere);
    assert!(!record.share_default_config && !record.share_default_config_pending);
    assert_eq!(read(&account, "settings.json"), "account settings");
    assert!(!is_link(&account.join("projects")));
}

#[tokio::test]
async fn a_switch_whose_target_cannot_change_its_sharing_leaves_the_session_running() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    let claim = busy(&fixture, Agent::Claude);
    fixture.share(Agent::Claude, true).await.unwrap();
    drop(claim);
    let session = Session {
        account_id: None,
        config_dir: None,
        ..test_support::project_session("moving", Agent::Claude, SessionStatus::Idle)
    };
    fixture.state.store.insert_session(&session).unwrap();
    let running = test_support::running(&fixture.state, "moving");
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o555)).unwrap();
    let launches = Launches::default();
    let err = crate::coordinator::switch_session_account_with(
        &fixture.state,
        "moving",
        Some(Some("claude-a".to_string())),
        None,
        || Ok(std::collections::HashMap::new()),
        launches.launch(false),
    )
    .await
    .unwrap_err();
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(code_of(&err), error_code::ACCOUNT_SHARING_FAILED);
    assert!(!running.poll_exit());
    assert!(fixture.state.live_session("moving").is_some());
    assert!(launches.given().is_empty());
}

#[tokio::test]
async fn a_default_directory_that_moved_refuses_a_retry_and_holds_the_repair_back() {
    let fixture = Fixture::new();
    let (default, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    let moved = fixture.dir.join("default-now");
    std::fs::create_dir_all(&moved).unwrap();
    fixture
        .state
        .set_agent_availability(vec![AgentAvailability {
            agent: Agent::Claude,
            availability: Availability::Available,
            default_account_dir: Some(moved.to_string_lossy().into_owned()),
        }]);

    // A replaced link is not relinked into the new default, and the session is still pointed at
    // the directory its links lead into.
    std::fs::remove_file(account.join("settings.json")).unwrap();
    std::fs::write(account.join("settings.json"), "saved over the link").unwrap();
    let session = fixture.session("s", Agent::Claude);
    let prepared = before_launch(&fixture.state, &session).await.unwrap();
    assert_eq!(prepared.shared_default_dir, Some(real(&default)));
    let codes: Vec<&str> = prepared.notices.iter().map(|notice| notice.code).collect();
    assert_eq!(codes, vec![notice_code::ACCOUNT_SHARING_REPAIR_FAILED]);
    assert!(!is_link(&account.join("settings.json")));

    fixture
        .state
        .store
        .set_account_sharing("claude-a", true, true)
        .unwrap();
    let err = fixture.share(Agent::Claude, true).await.unwrap_err();
    assert_eq!(code_of(&err), error_code::ACCOUNT_SHARING_FAILED);

    // Turning off finds links into the recorded default and into the current one alike.
    std::fs::remove_file(account.join("rules")).unwrap();
    std::os::unix::fs::symlink(real(&moved).join("rules"), account.join("rules")).unwrap();
    fixture.share(Agent::Claude, false).await.unwrap();
    assert!(!fixture.record(Agent::Claude).share_default_config_pending);
    assert!(std::fs::symlink_metadata(account.join("rules")).is_err());
    assert!(std::fs::symlink_metadata(account.join("agents")).is_err());
}

#[tokio::test]
async fn codex_rows_carried_once_are_not_carried_again() {
    let fixture = Fixture::new();
    let (default, account) = (fixture.default(Agent::Codex), fixture.account(Agent::Codex));
    codex_database(&default.join("state_5.sqlite"), &[]);
    write(&account, ROLLOUT, "rollout a");
    codex_database(
        &account.join("state_5.sqlite"),
        &[("a", &account.join(ROLLOUT).to_string_lossy(), 1)],
    );
    fixture.share(Agent::Codex, true).await.unwrap();
    assert!(rollout_of(&default.join("state_5.sqlite"), "a").is_some());
    // The thread's row went since (taken back with a session, say); a later retry of turning
    // sharing on does not bring the stale row back from the account's set-aside database.
    rusqlite::Connection::open(default.join("state_5.sqlite"))
        .unwrap()
        .execute("DELETE FROM threads WHERE id = 'a'", [])
        .unwrap();
    fixture
        .state
        .store
        .set_account_sharing("codex-a", true, true)
        .unwrap();
    fixture.share(Agent::Codex, true).await.unwrap();
    assert_eq!(rollout_of(&default.join("state_5.sqlite"), "a"), None);
}

const CLAUDE_ID: &str = "0b5c6c8e-1d1a-4c55-9b5e-3f0f2a7d9c01";
const NOT_OWNED_ID: &str = "0b5c6c8e-1d1a-4c55-9b5e-3f0f2a7d9c02";
const THREAD: &str = "019a0000-0000-7000-8000-000000000001";
const ARCHIVED_THREAD: &str = "019a0000-0000-7000-8000-000000000002";

/// An archived session of the account that has a conversation with the agent's id `id`.
fn archived_with_conversation(fixture: &Fixture, session: &str, agent: Agent, id: &str) -> Session {
    let session = Session {
        account_id: Some(account_id(agent).to_string()),
        config_dir: Some(fixture.account(agent).to_string_lossy().into_owned()),
        agent_session_id: Some(id.to_string()),
        has_conversation: true,
        ..test_support::project_session(session, agent, SessionStatus::Archived)
    };
    fixture.state.store.insert_session(&session).unwrap();
    session
}

fn judged(session: &Session) -> crate::agent_records::Judgement {
    crate::agent_records::judge(&crate::agent_records::Locator::default(), session)
}

#[tokio::test]
async fn turning_sharing_off_takes_the_accounts_own_claude_conversations_back() {
    let fixture = Fixture::new();
    let (default, account) = (
        fixture.default(Agent::Claude),
        fixture.account(Agent::Claude),
    );
    let session = archived_with_conversation(&fixture, "own", Agent::Claude, CLAUDE_ID);
    let transcript = format!("projects/p/{CLAUDE_ID}.jsonl");
    let sidecar = format!("projects/p/{CLAUDE_ID}/subagents/agent.jsonl");
    let checkpoint = format!("file-history/{CLAUDE_ID}/v1");
    write(&account, &transcript, "the account's conversation");
    write(&account, &sidecar, "a subagent");
    write(&account, &checkpoint, "a checkpoint");
    let not_owned = format!("projects/p/{NOT_OWNED_ID}.jsonl");
    write(&default, &not_owned, "the default's conversation");
    fixture.share(Agent::Claude, true).await.unwrap();
    assert_eq!(read(&default, &transcript), "the account's conversation");
    assert_eq!(
        judged(&session).resumability,
        crate::agent_records::Resumability::Unknown
    );

    fixture.share(Agent::Claude, false).await.unwrap();
    for (relative, content) in [
        (&transcript, "the account's conversation"),
        (&sidecar, "a subagent"),
        (&checkpoint, "a checkpoint"),
    ] {
        assert_eq!(read(&account, relative), content);
        assert!(
            std::fs::symlink_metadata(default.join(relative)).is_err(),
            "{relative}"
        );
    }
    assert_eq!(read(&default, &not_owned), "the default's conversation");
    assert!(std::fs::symlink_metadata(account.join(&not_owned)).is_err());
    assert_eq!(
        judged(&session).resumability,
        crate::agent_records::Resumability::Resumable
    );
}

#[tokio::test]
async fn turning_sharing_off_takes_the_accounts_own_codex_threads_back() {
    let fixture = Fixture::new();
    let (default, account) = (fixture.default(Agent::Codex), fixture.account(Agent::Codex));
    codex_database(&default.join("state_5.sqlite"), &[]);
    std::fs::create_dir_all(default.join("sessions")).unwrap();
    fixture.share(Agent::Codex, true).await.unwrap();
    // Written while sharing: a thread with reverted turns, one archived by Codex, and the rows
    // Codex records of them through the account's link.
    let session = archived_with_conversation(&fixture, "own", Agent::Codex, THREAD);
    let archived = archived_with_conversation(&fixture, "archived", Agent::Codex, ARCHIVED_THREAD);
    let chain = test_support::codex_chain(&account, THREAD, true);
    let archived_rollout =
        format!("archived_sessions/rollout-2026-10-04T00-00-00-{ARCHIVED_THREAD}.jsonl");
    write(&account, &archived_rollout, "archived thread");
    codex_database(
        &account.join("state_5.sqlite"),
        &[
            (THREAD, &chain.head.to_string_lossy(), 1),
            (
                ARCHIVED_THREAD,
                &account.join(&archived_rollout).to_string_lossy(),
                1,
            ),
            (
                "019a0000-0000-7000-8000-00000000000f",
                "/elsewhere/rollout.jsonl",
                1,
            ),
        ],
    );
    let relative = |path: &Path| path.strip_prefix(&account).unwrap().to_path_buf();
    let chain_files: Vec<PathBuf> = chain.all().iter().map(|path| relative(path)).collect();

    fixture.share(Agent::Codex, false).await.unwrap();
    for file in &chain_files {
        assert!(account.join(file).is_file(), "{file:?}");
        assert!(
            std::fs::symlink_metadata(default.join(file)).is_err(),
            "{file:?}"
        );
    }
    assert_eq!(read(&account, &archived_rollout), "archived thread");
    // The shared database no longer names the moved threads; another thread's row is kept.
    let shared = default.join("state_5.sqlite");
    assert_eq!(rollout_of(&shared, THREAD), None);
    assert_eq!(rollout_of(&shared, ARCHIVED_THREAD), None);
    assert!(rollout_of(&shared, "019a0000-0000-7000-8000-00000000000f").is_some());
    assert_eq!(
        judged(&session).resumability,
        crate::agent_records::Resumability::Resumable
    );
    let judgement = judged(&archived);
    assert_eq!(
        judgement.resumability,
        crate::agent_records::Resumability::Resumable
    );
    assert!(judgement.needs_unarchive);
}
/// A session on the default account whose conversation `CLAUDE_ID` is in the default directory.
fn default_session_with_conversation(fixture: &Fixture, id: &str) -> Session {
    let session = Session {
        account_id: None,
        config_dir: None,
        agent_session_id: Some(CLAUDE_ID.to_string()),
        has_conversation: true,
        ..test_support::project_session(id, Agent::Claude, SessionStatus::Interrupted)
    };
    fixture.state.store.insert_session(&session).unwrap();
    write(
        &fixture.default(Agent::Claude),
        &format!("projects/p/{CLAUDE_ID}.jsonl"),
        "the conversation",
    );
    session
}

/// The default account's directory as a session on it resolves it.
fn on_default_shell(fixture: &Fixture) -> std::collections::HashMap<String, String> {
    std::collections::HashMap::from([(
        "CLAUDE_CONFIG_DIR".to_string(),
        fixture
            .default(Agent::Claude)
            .to_string_lossy()
            .into_owned(),
    )])
}

#[tokio::test]
async fn a_switch_onto_an_account_turning_sharing_off_that_fails_leaves_the_conversation_on_its_source(
) {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    let claim = busy(&fixture, Agent::Claude);
    fixture.share(Agent::Claude, false).await.unwrap();
    drop(claim);
    let session = default_session_with_conversation(&fixture, "moving");

    let launches = Launches::default();
    let shell = on_default_shell(&fixture);
    let err = crate::coordinator::switch_session_account_with(
        &fixture.state,
        "moving",
        Some(Some("claude-a".to_string())),
        None,
        move || Ok(shell),
        launches.launch(true),
    )
    .await;
    assert!(err.is_err());
    let after = fixture.state.store.get_session("moving").unwrap().unwrap();
    assert_eq!(after.account_id, None);
    // The target stopped sharing before the copy, so the copy is the account's own and the
    // source keeps the conversation.
    assert!(!fixture.record(Agent::Claude).share_default_config_pending);
    assert!(!is_link(&fixture.account(Agent::Claude).join("projects")));
    let resumable = crate::agent_records::judge(
        &crate::agent_records::Locator::with_shell(on_default_shell(&fixture)),
        &session,
    );
    assert_eq!(
        resumable.resumability,
        crate::agent_records::Resumability::Resumable
    );
}

#[tokio::test]
async fn a_conversation_another_session_records_is_copied_back_not_moved() {
    let fixture = Fixture::new();
    let (default, account) = (
        fixture.default(Agent::Claude),
        fixture.account(Agent::Claude),
    );
    fixture.share(Agent::Claude, true).await.unwrap();
    default_session_with_conversation(&fixture, "on-default");
    let own = archived_with_conversation(&fixture, "own", Agent::Claude, CLAUDE_ID);
    fixture.share(Agent::Claude, false).await.unwrap();
    let transcript = format!("projects/p/{CLAUDE_ID}.jsonl");
    assert_eq!(read(&account, &transcript), "the conversation");
    assert_eq!(read(&default, &transcript), "the conversation");
    assert_eq!(
        judged(&own).resumability,
        crate::agent_records::Resumability::Resumable
    );
}

#[tokio::test]
async fn a_record_the_account_holds_already_is_kept_when_it_is_the_later_one() {
    let fixture = Fixture::new();
    let (default, account) = (
        fixture.default(Agent::Claude),
        fixture.account(Agent::Claude),
    );
    fixture.share(Agent::Claude, true).await.unwrap();
    archived_with_conversation(&fixture, "own", Agent::Claude, CLAUDE_ID);
    let transcript = format!("projects/p/{CLAUDE_ID}.jsonl");
    write(&default, &transcript, "older");
    test_support::move_modification_time_back(&default.join(&transcript));
    std::fs::remove_file(account.join("projects")).unwrap();
    write(&account, &transcript, "later");
    let dirs = link::Dirs::for_undoing(&account, &default, &default)
        .unwrap()
        .unwrap();
    let conversation = take_back::Conversation {
        agent_session_id: CLAUDE_ID.to_string(),
        sessions: vec!["own".to_string()],
        held_elsewhere: false,
    };
    take_back::take_back(
        &mut crate::agent_records::Locator::default(),
        Agent::Claude,
        &dirs,
        &conversation,
        "STAMP",
    )
    .unwrap_or_else(|failure| {
        panic!("{failure:?}");
    });
    assert_eq!(read(&account, &transcript), "later");
    assert_eq!(
        read(&account, &format!("projects.bak-STAMP/p/{CLAUDE_ID}.jsonl")),
        "older"
    );
    assert!(std::fs::symlink_metadata(default.join(&transcript)).is_err());
}

#[tokio::test]
async fn a_codex_row_cleanup_that_failed_is_redone_by_a_retry() {
    let fixture = Fixture::new();
    let (default, account) = (fixture.default(Agent::Codex), fixture.account(Agent::Codex));
    let shared = codex_database(&default.join("state_5.sqlite"), &[]);
    std::fs::create_dir_all(default.join("sessions")).unwrap();
    fixture.share(Agent::Codex, true).await.unwrap();
    archived_with_conversation(&fixture, "own", Agent::Codex, THREAD);
    let chain = test_support::codex_chain(&account, THREAD, false);
    codex_database(
        &account.join("state_5.sqlite"),
        &[(THREAD, &chain.head.to_string_lossy(), 1)],
    );
    shared
        .execute_batch(
            "CREATE TRIGGER refuse BEFORE DELETE ON threads BEGIN SELECT RAISE(ABORT, 'no'); END;",
        )
        .unwrap();
    let err = fixture.share(Agent::Codex, false).await.unwrap_err();
    let coded = err.downcast_ref::<CodedError>().unwrap();
    let items: Vec<serde_json::Value> = serde_json::from_str(&coded.params["items"]).unwrap();
    assert_eq!(items[0]["item"], THREAD);
    assert_eq!(items[0]["sessions"], serde_json::json!(["own"]));
    assert!(account
        .join(chain.head.strip_prefix(&account).unwrap())
        .is_file());
    assert!(rollout_of(&default.join("state_5.sqlite"), THREAD).is_some());

    shared.execute_batch("DROP TRIGGER refuse;").unwrap();
    fixture.share(Agent::Codex, false).await.unwrap();
    assert_eq!(rollout_of(&default.join("state_5.sqlite"), THREAD), None);
}

#[tokio::test]
async fn conversations_are_taken_back_from_a_default_records_directory_that_is_a_link() {
    let fixture = Fixture::new();
    // Claude: the default's `projects/` is a link to a directory of the user's.
    let (default, account) = (
        fixture.default(Agent::Claude),
        fixture.account(Agent::Claude),
    );
    let projects = fixture.dir.join("claude-projects");
    std::fs::create_dir_all(&projects).unwrap();
    std::os::unix::fs::symlink(&projects, default.join("projects")).unwrap();
    fixture.share(Agent::Claude, true).await.unwrap();
    archived_with_conversation(&fixture, "own", Agent::Claude, CLAUDE_ID);
    let transcript = format!("projects/p/{CLAUDE_ID}.jsonl");
    write(&account, &transcript, "the account's conversation");
    fixture.share(Agent::Claude, false).await.unwrap();
    assert_eq!(read(&account, &transcript), "the account's conversation");
    assert!(std::fs::symlink_metadata(projects.join(format!("p/{CLAUDE_ID}.jsonl"))).is_err());

    // Codex: the default's `sessions/` is one; the thread's row goes only with its rollout.
    let (default, account) = (fixture.default(Agent::Codex), fixture.account(Agent::Codex));
    let sessions = fixture.dir.join("codex-sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::os::unix::fs::symlink(&sessions, default.join("sessions")).unwrap();
    codex_database(&default.join("state_5.sqlite"), &[]);
    fixture.share(Agent::Codex, true).await.unwrap();
    archived_with_conversation(&fixture, "own-thread", Agent::Codex, THREAD);
    archived_with_conversation(&fixture, "gone-thread", Agent::Codex, ARCHIVED_THREAD);
    let chain = test_support::codex_chain(&account, THREAD, false);
    codex_database(
        &account.join("state_5.sqlite"),
        &[
            (THREAD, &chain.head.to_string_lossy(), 1),
            (ARCHIVED_THREAD, "/nowhere/rollout.jsonl", 1),
        ],
    );
    let head = chain.head.strip_prefix(&account).unwrap().to_path_buf();
    fixture.share(Agent::Codex, false).await.unwrap();
    assert!(account.join(&head).is_file());
    assert_eq!(rollout_of(&default.join("state_5.sqlite"), THREAD), None);
    // No file of this thread was found anywhere: its row is the only trace left, and stays.
    assert!(rollout_of(&default.join("state_5.sqlite"), ARCHIVED_THREAD).is_some());
}
