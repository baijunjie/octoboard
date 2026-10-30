use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::*;
use crate::coordinator::handle;
use crate::protocol::{notice_code, AgentAvailability, Availability, RequestBody, SessionStatus};
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
    assert!(record.share_default_config && !record.share_default_config_incomplete);
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
    assert!(!record.share_default_config && !record.share_default_config_incomplete);
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
async fn a_change_that_fails_part_way_says_what_it_left_where_and_stays_incomplete() {
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
    assert_eq!(settings["cause"], "permission_denied");
    assert_eq!(coded.params["cause"], "permission_denied");
    assert!(!coded.message.contains("tried again"), "{}", coded.message);
    assert_eq!(read(&account, "settings.json"), "account settings");
    let record = fixture.record(Agent::Claude);
    assert!(record.share_default_config && record.share_default_config_incomplete);
    // The failure is kept in the account's sharing state, with its entries.
    let failure = sharing_state(&fixture, Agent::Claude)
        .failure
        .expect("the failure kept");
    assert!(failure.share);
    assert_eq!(failure.cause, link::Cause::PermissionDenied);
    assert!(failure.items.iter().any(|item| {
        item.item == "settings.json" && item.cause == link::Cause::PermissionDenied
    }));
    assert!(sharing_state(&fixture, Agent::Claude).progress.is_none());
    // Nothing retries it by itself: a launch leaves the directory as it is.
    let session = fixture.session("s", Agent::Claude);
    before_launch(&fixture.state, &session).await.unwrap();
    assert!(!is_link(&account.join("settings.json")));
    assert!(
        fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );

    // Asking again retries, and completes now that the directory can be written; the prompt
    // history is merged once.
    fixture.share(Agent::Claude, true).await.unwrap();
    assert!(
        !fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );
    assert!(sharing_state(&fixture, Agent::Claude).failure.is_none());
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
    assert!(
        !fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );
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
    assert_eq!(items[0]["cause"], "conflict");
    assert_eq!(coded.params["cause"], "conflict");
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
                trash_directory: false,
            },
        )
    };
    let claim = busy(&fixture, Agent::Claude);
    let err = delete().await.unwrap_err();
    assert_eq!(code_of(&err), error_code::ACCOUNT_STILL_SHARING);
    let record = fixture.record(Agent::Claude);
    assert!(record.share_default_config && !record.share_default_config_incomplete);
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

/// Moving a sharing account's directory to the Trash turns its sharing off first, so what goes is
/// the directory holding its own content, not links into the default one.
#[tokio::test]
async fn deleting_a_sharing_account_with_its_directory_turns_sharing_off_before_the_move() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    assert!(is_link(&account.join("settings.json")));
    // What the Trash is handed: the settings file, and whether it is still a link.
    static HANDED: Mutex<Vec<(Option<String>, bool)>> = Mutex::new(Vec::new());
    fixture.state.host_tools.replace(crate::host_tools::Tools {
        trash: |directory| {
            let settings = directory.join("settings.json");
            HANDED
                .lock()
                .unwrap()
                .push((std::fs::read_to_string(&settings).ok(), is_link(&settings)));
            std::fs::remove_dir_all(directory)?;
            Ok(())
        },
        ..test_support::host_tools()
    });
    handle(
        &fixture.state,
        None,
        RequestBody::DeleteAccount {
            account: "claude-a".to_string(),
            trash_directory: true,
        },
    )
    .await
    .unwrap();
    assert!(fixture
        .state
        .store
        .get_account("claude-a")
        .unwrap()
        .is_none());
    assert_eq!(
        *HANDED.lock().unwrap(),
        [(Some("account settings".to_string()), false)]
    );
    assert!(!account.exists());
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
    assert!(record.share_default_config && !record.share_default_config_incomplete);
    drop(claim);

    repoint().await.unwrap();
    let record = fixture.record(Agent::Claude);
    assert_eq!(PathBuf::from(&record.config_dir), elsewhere);
    assert!(!record.share_default_config && !record.share_default_config_incomplete);
    assert_eq!(read(&account, "settings.json"), "account settings");
    assert!(!is_link(&account.join("projects")));
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
    assert_eq!(prepared.notices[0].params["cause"], "default_moved");
    assert!(!is_link(&account.join("settings.json")));

    fixture
        .state
        .store
        .set_account_sharing("claude-a", true, true)
        .unwrap();
    let err = fixture.share(Agent::Claude, true).await.unwrap_err();
    assert_eq!(code_of(&err), error_code::ACCOUNT_SHARING_FAILED);
    let coded = err.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.params["cause"], "default_moved");
    assert_eq!(coded.params["items"], "[]");

    // Turning off finds links into the recorded default and into the current one alike.
    std::fs::remove_file(account.join("rules")).unwrap();
    std::os::unix::fs::symlink(real(&moved).join("rules"), account.join("rules")).unwrap();
    fixture.share(Agent::Claude, false).await.unwrap();
    assert!(
        !fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );
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
async fn a_switch_onto_an_account_that_stopped_sharing_that_fails_leaves_the_conversation_on_its_source(
) {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    fixture.share(Agent::Claude, false).await.unwrap();
    let session = default_session_with_conversation(&fixture, "moving");

    let launches = Launches::default();
    let shell = on_default_shell(&fixture);
    let err = crate::coordinator::switch_session_account_with(
        &fixture.state,
        "moving",
        Some(Some("claude-a".to_string())),
        None,
        None,
        crate::coordinator::LaunchEdit::default(),
        move || Ok(shell),
        launches.launch(true),
    )
    .await;
    assert!(err.is_err());
    let after = fixture.state.store.get_session("moving").unwrap().unwrap();
    assert_eq!(after.account_id, None);
    // The target does not share, so the copy is the account's own and the source keeps the
    // conversation.
    assert!(
        !fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );
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

/// The account's sharing state as a client connecting now would be told it.
fn sharing_state(fixture: &Fixture, agent: Agent) -> crate::protocol::AccountSharing {
    states(&fixture.state)
        .unwrap()
        .into_iter()
        .find(|sharing| sharing.account == account_id(agent))
        .expect("an owned account's state")
}

/// The `account_in_use` refusal's params, after checking nothing was recorded or touched.
fn refused_in_use(fixture: &Fixture, err: &anyhow::Error) -> crate::protocol::Params {
    assert_eq!(code_of(err), error_code::ACCOUNT_IN_USE);
    let record = fixture.record(Agent::Claude);
    assert!(!record.share_default_config && !record.share_default_config_incomplete);
    assert!(!is_link(
        &fixture.account(Agent::Claude).join("settings.json")
    ));
    assert!(sharing_state(fixture, Agent::Claude).progress.is_none());
    err.downcast_ref::<CodedError>().unwrap().params.clone()
}

#[tokio::test]
async fn a_change_is_refused_while_a_session_of_the_account_runs() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    fixture.session("running", Agent::Claude);
    let running = test_support::running(&fixture.state, "running");
    let in_use = sharing_state(&fixture, Agent::Claude)
        .in_use
        .expect("in use");
    assert_eq!((in_use.sessions, in_use.signing_in), (1, false));

    let err = fixture.share(Agent::Claude, true).await.unwrap_err();
    let params = refused_in_use(&fixture, &err);
    assert_eq!(params["sessions"], "1");
    assert_eq!(params["signing_in"], "false");

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
    assert!(sharing_state(&fixture, Agent::Claude).in_use.is_none());
    fixture.share(Agent::Claude, true).await.unwrap();
    assert!(
        !fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );
    assert!(is_link(
        &fixture.account(Agent::Claude).join("settings.json")
    ));
}

#[tokio::test]
async fn a_change_is_refused_while_a_session_of_the_account_starts() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let mut events = fixture.state.subscribe();
    let claim = busy(&fixture, Agent::Claude);
    // Every client is told, the one connecting later too.
    let told = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        matches!(event, crate::protocol::Event::AccountSharing(sharing)
            if sharing.account == "claude-a" && sharing.in_use.as_ref().is_some_and(|in_use| in_use.sessions == 1))
    });
    assert!(told);
    assert!(sharing_state(&fixture, Agent::Claude).in_use.is_some());

    let err = fixture.share(Agent::Claude, true).await.unwrap_err();
    assert_eq!(refused_in_use(&fixture, &err)["sessions"], "1");
    drop(claim);
    assert!(sharing_state(&fixture, Agent::Claude).in_use.is_none());
    fixture.share(Agent::Claude, true).await.unwrap();
}

#[tokio::test]
async fn a_change_is_refused_while_the_account_is_being_signed_in() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    let signing_in = running_long(Some(&account));
    publish(&fixture.state);
    let in_use = sharing_state(&fixture, Agent::Claude)
        .in_use
        .expect("in use");
    assert_eq!((in_use.sessions, in_use.signing_in), (0, true));

    let err = fixture.share(Agent::Claude, true).await.unwrap_err();
    let params = refused_in_use(&fixture, &err);
    assert_eq!(params["sessions"], "0");
    assert_eq!(params["signing_in"], "true");
    drop(signing_in);
    fixture.share(Agent::Claude, true).await.unwrap();
}

/// A change of the account's sharing to `share`, started and left waiting for a short read of the
/// account; it goes on once the answer is dropped.
async fn waiting_change(
    fixture: &Fixture,
    agent: Agent,
    share: bool,
) -> (tokio::task::JoinHandle<Result<()>>, Run) {
    let reading = reading_account(Some(&fixture.account(agent))).unwrap();
    let state = fixture.state.clone();
    let change = tokio::spawn(async move {
        handle(
            &state,
            None,
            RequestBody::SetAccountSharing {
                account: Some(account_id(agent).to_string()),
                share,
            },
        )
        .await
        .map(|_| ())
    });
    let deadline = std::time::Instant::now() + test_support::PATIENCE;
    while fixture.record(agent).share_default_config != share
        || fixture.state.sharing_locked(account_id(agent)).is_none()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the change never started"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    (change, reading)
}

#[tokio::test]
async fn a_change_waits_for_a_short_read_and_holds_new_ones_back() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    let (change, reading) = waiting_change(&fixture, Agent::Claude, true).await;
    // Recorded and told at once, waiting with no entry under way.
    let record = fixture.record(Agent::Claude);
    assert!(record.share_default_config && record.share_default_config_incomplete);
    let progress = sharing_state(&fixture, Agent::Claude)
        .progress
        .expect("a change running");
    assert_eq!(
        (progress.share, progress.done, progress.item),
        (true, 0, None)
    );
    assert!(!is_link(&account.join("settings.json")));

    // A read starting now waits for the change.
    let (sent, read_started) = std::sync::mpsc::channel();
    let next = {
        let account = account.clone();
        std::thread::spawn(move || {
            let reading = reading_account(Some(&account));
            sent.send(is_link(&account.join("settings.json"))).unwrap();
            drop(reading);
        })
    };
    assert!(read_started
        .recv_timeout(std::time::Duration::from_millis(200))
        .is_err());

    drop(reading);
    change.await.unwrap().unwrap();
    assert!(read_started.recv_timeout(test_support::PATIENCE).unwrap());
    next.join().unwrap();
    assert!(
        !fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );
    assert!(sharing_state(&fixture, Agent::Claude).progress.is_none());
}

#[tokio::test]
async fn nothing_of_the_account_starts_while_its_change_runs() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    std::fs::create_dir_all(fixture.dir.join("no-such-project")).unwrap();
    fixture.session("s", Agent::Claude);
    let moving = Session {
        account_id: None,
        config_dir: None,
        ..test_support::project_session("moving", Agent::Claude, SessionStatus::Idle)
    };
    fixture.state.store.insert_session(&moving).unwrap();
    let running = test_support::running(&fixture.state, "moving");
    let (change, reading) = waiting_change(&fixture, Agent::Claude, true).await;
    let in_progress = |result: Result<()>| {
        assert_eq!(
            code_of(&result.unwrap_err()),
            error_code::ACCOUNT_SHARING_IN_PROGRESS
        );
    };

    // A launch or resume of one of its sessions.
    in_progress(fixture.state.begin_launch("s").map(drop));
    // Creating one.
    let launches = Launches::default();
    let opened = crate::coordinator::open_session_with(
        &fixture.state,
        crate::coordinator::OpenRequest {
            console_id: "console-1".to_string(),
            project_id: Some("project-1".to_string()),
            agent: Some(Agent::Claude),
            account: Some(Some("claude-a".to_string())),
            model: None,
            effort: None,
            task: None,
            title: None,
            origin: crate::protocol::Origin::User,
            bound_to: None,
            resume_agent_session_id: None,
            session_id: None,
            terminal_appearance: None,
        },
        launches.launch(false),
    )
    .await;
    in_progress(opened.map(drop));
    assert_eq!(fixture.state.store.list_sessions().unwrap().len(), 2);
    // Switching a session to it, which keeps running.
    in_progress(
        crate::coordinator::switch_session_account_with(
            &fixture.state,
            "moving",
            Some(Some("claude-a".to_string())),
            None,
            None,
            crate::coordinator::LaunchEdit::default(),
            || Ok(std::collections::HashMap::new()),
            launches.launch(false),
        )
        .await
        .map(drop),
    );
    assert!(!running.poll_exit());
    assert!(fixture.state.live_session("moving").is_some());
    // Switching one of its sessions away from it, or handing one off.
    in_progress(
        crate::coordinator::switch_session_account_with(
            &fixture.state,
            "s",
            Some(None),
            None,
            None,
            crate::coordinator::LaunchEdit::default(),
            || Ok(std::collections::HashMap::new()),
            launches.launch(false),
        )
        .await
        .map(drop),
    );
    in_progress(
        crate::handoff::operation::hand_off_with(
            &fixture.state,
            crate::handoff::operation::Handoff {
                session: "s".to_string(),
                agent: Agent::Codex,
                account: None,
                model: None,
                effort: None,
                title: None,
                note: None,
                asked_by: None,
                terminal_appearance: None,
            },
            launches.launch(false),
        )
        .await
        .map(drop),
    );
    // A second change, and removing or repointing it.
    in_progress(fixture.share(Agent::Claude, false).await);
    in_progress(
        handle(
            &fixture.state,
            None,
            RequestBody::UpdateAccount {
                account: "claude-a".to_string(),
                name: None,
                config_dir: Some(
                    fixture
                        .dir
                        .join("claude-elsewhere")
                        .to_string_lossy()
                        .into_owned(),
                ),
            },
        )
        .await
        .map(drop),
    );
    in_progress(
        handle(
            &fixture.state,
            None,
            RequestBody::DeleteAccount {
                account: "claude-a".to_string(),
                trash_directory: false,
            },
        )
        .await
        .map(drop),
    );
    assert!(launches.given().is_empty());

    drop(reading);
    change.await.unwrap().unwrap();
    resumed_with(&fixture, "s").await;
}

#[tokio::test]
async fn a_change_tells_its_progress_through_every_entry() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let mut events = fixture.state.subscribe();
    fixture.share(Agent::Claude, true).await.unwrap();
    let states: Vec<crate::protocol::AccountSharing> =
        std::iter::from_fn(|| events.try_recv().ok())
            .filter_map(|event| match event {
                crate::protocol::Event::AccountSharing(sharing)
                    if sharing.account == "claude-a" =>
                {
                    Some(sharing)
                }
                _ => None,
            })
            .collect();
    let catalog: Vec<&str> = crate::shared_items::shared_items(Agent::Claude)
        .iter()
        .map(|item| item.name)
        .collect();
    let progress: Vec<_> = states
        .iter()
        .filter_map(|state| state.progress.clone())
        .collect();
    assert!(progress
        .iter()
        .all(|progress| progress.share && progress.total == catalog.len()));
    assert_eq!((progress[0].done, progress[0].item.as_deref()), (0, None));
    let reached: Vec<(usize, &str)> = progress
        .iter()
        .filter_map(|progress| Some((progress.done, progress.item.as_deref()?)))
        .collect();
    let expected: Vec<(usize, &str)> = catalog.iter().copied().enumerate().collect();
    assert_eq!(reached, expected);
    // The last state sent says the change has ended.
    let last = states.last().unwrap();
    assert!(last.progress.is_none() && last.failure.is_none());

    // Turning it off counts each conversation taken back as an entry too.
    archived_with_conversation(&fixture, "own", Agent::Claude, CLAUDE_ID);
    write(
        &fixture.account(Agent::Claude),
        &format!("projects/p/{CLAUDE_ID}.jsonl"),
        "the account's conversation",
    );
    fixture.share(Agent::Claude, false).await.unwrap();
    let progress: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            crate::protocol::Event::AccountSharing(sharing) if sharing.account == "claude-a" => {
                sharing.progress
            }
            _ => None,
        })
        .collect();
    assert!(progress
        .iter()
        .all(|progress| !progress.share && progress.total == catalog.len() + 1));
    assert!(progress.iter().any(|progress| {
        progress.done == catalog.len() && progress.item.as_deref() == Some(CLAUDE_ID)
    }));
}

#[tokio::test]
async fn no_launch_or_switch_carries_a_change_out() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    // As an earlier version left a change it had not carried out yet.
    fixture
        .state
        .store
        .set_account_sharing("claude-a", true, true)
        .unwrap();
    fixture.session("s", Agent::Claude);
    assert_eq!(resumed_with(&fixture, "s").await, None);
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
        None,
        crate::coordinator::LaunchEdit::default(),
        || Ok(std::collections::HashMap::new()),
        launches.launch(false),
    )
    .await
    .unwrap();
    assert_eq!(launches.given(), vec![None]);
    assert!(
        fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );
    assert_eq!(read(&account, "settings.json"), "account settings");
    assert!(!is_link(&account.join("projects")));
}

#[tokio::test]
async fn a_launch_of_an_incomplete_account_skips_the_repair() {
    let fixture = Fixture::new();
    let (default, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    fixture
        .state
        .store
        .set_account_sharing("claude-a", true, true)
        .unwrap();
    std::fs::remove_file(account.join("settings.json")).unwrap();
    std::fs::write(account.join("settings.json"), "saved over the link").unwrap();
    fixture.session("s", Agent::Claude);
    assert_eq!(resumed_with(&fixture, "s").await, Some(real(&default)));
    assert_eq!(read(&account, "settings.json"), "saved over the link");
    assert!(
        fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );
}

#[tokio::test]
async fn an_incomplete_account_is_given_the_shared_directory_while_its_conversations_lead_there() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let (default, account) = claude_setup(&fixture);
    let session = fixture.session("s", Agent::Claude);
    let given = || async {
        before_launch(&fixture.state, &session)
            .await
            .unwrap()
            .shared_default_dir
    };

    // Turning sharing on that stopped before `projects/` was linked.
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o555)).unwrap();
    fixture.share(Agent::Claude, true).await.unwrap_err();
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );
    assert!(!is_link(&account.join("projects")));
    assert_eq!(given().await, None);

    // Turning sharing off that stopped with `projects/` still linked.
    fixture.share(Agent::Claude, true).await.unwrap();
    fixture
        .state
        .store
        .set_account_sharing("claude-a", false, true)
        .unwrap();
    assert!(is_link(&account.join("projects")));
    assert_eq!(given().await, Some(real(&default)));
}

#[tokio::test]
async fn turning_sharing_off_an_account_never_shared_touches_nothing() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    // As an earlier version left a change to sharing it had not started.
    fixture
        .state
        .store
        .set_account_sharing("claude-a", true, true)
        .unwrap();
    fixture.share(Agent::Claude, false).await.unwrap();
    let record = fixture.record(Agent::Claude);
    assert!(!record.share_default_config && !record.share_default_config_incomplete);
    assert_eq!(read(&account, "settings.json"), "account settings");
    assert!(backups(&account, "settings.json").is_empty());
    assert!(fixture
        .state
        .store
        .account_sharing_start("claude-a")
        .unwrap()
        .is_none());
    // Asking again for a choice that is complete does nothing.
    fixture.share(Agent::Claude, false).await.unwrap();
}

#[test]
fn a_failures_cause_is_told_from_the_error_kind() {
    use link::Cause;
    use std::io::{Error, ErrorKind};
    for (kind, cause) in [
        (ErrorKind::PermissionDenied, Cause::PermissionDenied),
        (ErrorKind::ReadOnlyFilesystem, Cause::PermissionDenied),
        (ErrorKind::ResourceBusy, Cause::Busy),
        (ErrorKind::ExecutableFileBusy, Cause::Busy),
        (ErrorKind::WouldBlock, Cause::Busy),
        (ErrorKind::NotFound, Cause::NotFound),
        (ErrorKind::StorageFull, Cause::NoSpace),
        (ErrorKind::QuotaExceeded, Cause::NoSpace),
        (ErrorKind::FileTooLarge, Cause::NoSpace),
        (ErrorKind::AlreadyExists, Cause::Conflict),
        (ErrorKind::DirectoryNotEmpty, Cause::Conflict),
        (ErrorKind::NotADirectory, Cause::Conflict),
        (ErrorKind::IsADirectory, Cause::Conflict),
        (ErrorKind::InvalidData, Cause::Other),
    ] {
        assert_eq!(Cause::of(&Error::from(kind)), cause, "{kind:?}");
    }
    // As the system reports them, not only as constructed.
    let missing = std::fs::metadata("/nonexistent-octoboard-path").unwrap_err();
    assert_eq!(Cause::of(&missing), Cause::NotFound);
    let busy =
        rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY), None);
    assert_eq!(Cause::of_database(&busy), Cause::Busy);
    for code in [rusqlite::ffi::SQLITE_NOTFOUND, rusqlite::ffi::SQLITE_TOOBIG] {
        let other = rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None);
        assert_eq!(Cause::of_database(&other), Cause::Other);
    }
    assert_eq!(Cause::shared([Cause::Busy, Cause::Busy]), Cause::Busy);
    assert_eq!(Cause::shared([Cause::Busy, Cause::NoSpace]), Cause::Other);
    assert_eq!(
        serde_json::to_value(Cause::PermissionDenied).unwrap(),
        Cause::PermissionDenied.as_str()
    );
}

#[tokio::test]
async fn a_default_directory_that_is_gone_is_told_as_not_found() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    std::fs::remove_dir_all(fixture.default(Agent::Claude)).unwrap();
    let err = fixture.share(Agent::Claude, true).await.unwrap_err();
    assert_eq!(code_of(&err), error_code::ACCOUNT_SHARING_FAILED);
    let coded = err.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.params["cause"], "not_found");
    let failure = sharing_state(&fixture, Agent::Claude).failure.unwrap();
    assert_eq!(failure.cause, link::Cause::NotFound);
    assert!(failure.items.is_empty());
}

#[tokio::test]
async fn a_conversation_with_nowhere_of_the_accounts_to_go_is_told_as_a_conflict() {
    let fixture = Fixture::new();
    let (default, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    let dirs = link::Dirs::for_undoing(&account, &default, &default)
        .unwrap()
        .unwrap();
    let conversation = take_back::Conversation {
        agent_session_id: CLAUDE_ID.to_string(),
        sessions: vec!["own".to_string()],
        held_elsewhere: false,
    };
    // The account's `projects/` is still the link into the default directory.
    write(
        &default,
        &format!("projects/p/{CLAUDE_ID}.jsonl"),
        "conversation",
    );
    let failure = take_back::take_back(
        &mut crate::agent_records::Locator::default(),
        Agent::Claude,
        &dirs,
        &conversation,
        "STAMP",
    )
    .unwrap_err();
    assert_eq!(failure.cause, link::Cause::Conflict);
    assert_eq!(serde_json::to_value(&failure).unwrap()["cause"], "conflict");
}

impl Launches {
    /// Resumes the sessions a change ended through [`Self::launch`] rather than their agents.
    fn resume_through(&self, state: &AppState, fails: bool) {
        let launches = self.clone();
        *state.account_sharing.relaunch.lock().unwrap() =
            Some(Arc::new(move |request| (launches.launch(fails))(request)));
    }
}

/// A session of the account with a process at its prompt, waiting for the user's next message.
fn idle_session(fixture: &Fixture, id: &str, agent: Agent) -> test_support::StandIn {
    let session = Session {
        status: SessionStatus::Idle,
        ..fixture.session(id, agent)
    };
    fixture.state.store.update_session(&session).unwrap();
    let running = test_support::running(&fixture.state, id);
    fixture.state.watch_exit((*running).clone());
    running
}

/// The `account_sharing` states sent for `agent`'s account since `events` was subscribed.
fn sharing_events(
    events: &mut tokio::sync::broadcast::Receiver<crate::protocol::Event>,
    agent: Agent,
) -> Vec<crate::protocol::AccountSharing> {
    std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            crate::protocol::Event::AccountSharing(sharing)
                if sharing.account == account_id(agent) =>
            {
                Some(sharing)
            }
            _ => None,
        })
        .collect()
}

/// The phases `states` went through, each once, in order.
fn phases(states: &[crate::protocol::AccountSharing]) -> Vec<crate::protocol::SharingPhase> {
    let mut phases = Vec::new();
    for phase in states
        .iter()
        .filter_map(|state| state.progress.as_ref().map(|p| p.phase))
    {
        if phases.last() != Some(&phase) {
            phases.push(phase);
        }
    }
    phases
}

#[tokio::test]
async fn an_idle_session_does_not_block_and_is_resumed_after_the_change() {
    use crate::protocol::{Event, SharingPhase};
    let fixture = Fixture::new();
    let (default, account) = claude_setup(&fixture);
    let first = idle_session(&fixture, "idle", Agent::Claude);
    let launches = Launches::default();
    launches.resume_through(&fixture.state, false);
    let before = sharing_state(&fixture, Agent::Claude);
    assert!(before.in_use.is_none());
    assert_eq!(before.idle_sessions, 1);

    let mut events = fixture.state.subscribe();
    fixture.share(Agent::Claude, true).await.unwrap();
    assert!(first.poll_exit());
    assert!(is_link(&account.join("settings.json")));
    // Resumed onto the directory as it is now, sharing.
    assert_eq!(launches.given(), vec![Some(real(&default))]);
    assert!(fixture.state.live_session("idle").is_some());
    assert_eq!(
        fixture
            .state
            .store
            .get_session("idle")
            .unwrap()
            .unwrap()
            .status,
        SessionStatus::Idle
    );
    let sent: Vec<Event> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    // While it is down a client sees it starting, never interrupted.
    let statuses: Vec<SessionStatus> = sent
        .iter()
        .filter_map(|event| match event {
            Event::SessionUpserted { session } if session.id == "idle" => Some(session.status),
            _ => None,
        })
        .collect();
    assert!(statuses.contains(&SessionStatus::Starting), "{statuses:?}");
    assert!(
        !statuses.contains(&SessionStatus::Interrupted),
        "{statuses:?}"
    );
    let states: Vec<_> = sent
        .into_iter()
        .filter_map(|event| match event {
            Event::AccountSharing(sharing) if sharing.account == "claude-a" => Some(sharing),
            _ => None,
        })
        .collect();
    assert_eq!(
        phases(&states),
        vec![
            SharingPhase::Stopping,
            SharingPhase::Changing,
            SharingPhase::Resuming
        ]
    );
    let after = sharing_state(&fixture, Agent::Claude);
    assert!(after.progress.is_none() && after.in_use.is_none());
    assert_eq!(after.idle_sessions, 1);
    assert!(fixture.state.sharing_locked("claude-a").is_none());
}

#[tokio::test]
async fn an_idle_session_is_resumed_after_a_change_that_failed() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    let first = idle_session(&fixture, "idle", Agent::Claude);
    let launches = Launches::default();
    launches.resume_through(&fixture.state, false);
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o555)).unwrap();
    let err = fixture.share(Agent::Claude, true).await.unwrap_err();
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(code_of(&err), error_code::ACCOUNT_SHARING_FAILED);
    assert!(first.poll_exit());
    // Onto the directory as it stands: `projects/` was not linked.
    assert_eq!(launches.given(), vec![None]);
    assert!(fixture.state.live_session("idle").is_some());
    assert!(fixture.state.sharing_locked("claude-a").is_none());
}

#[tokio::test]
async fn a_session_that_is_not_resumed_is_told_and_the_change_stands() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    idle_session(&fixture, "idle", Agent::Claude);
    let launches = Launches::default();
    launches.resume_through(&fixture.state, true);
    let mut events = fixture.state.subscribe();
    fixture.share(Agent::Claude, true).await.unwrap();
    assert!(is_link(&account.join("settings.json")));
    assert!(
        !fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );
    let told = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        matches!(event, crate::protocol::Event::SessionNotice { session, code, .. }
            if session == "idle" && code == notice_code::SHARING_RESUME_FAILED)
    });
    assert!(told);
    let session = fixture.state.store.get_session("idle").unwrap().unwrap();
    assert_eq!(session.status, SessionStatus::Interrupted);
    assert_eq!(
        fixture.state.wire_session(session).status,
        SessionStatus::Interrupted
    );
}

#[tokio::test]
async fn a_working_session_blocks_a_change_and_is_left_running() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let launches = Launches::default();
    launches.resume_through(&fixture.state, false);
    // In the middle of a turn, idle with background work still running, and idle behind a dialog
    // of its agent's that only the user can answer.
    for (id, status) in [
        ("turn", SessionStatus::Working),
        ("background", SessionStatus::Idle),
        ("dialog", SessionStatus::Idle),
    ] {
        let running = idle_session(&fixture, id, Agent::Claude);
        let session = Session {
            status,
            ..fixture.state.store.get_session(id).unwrap().unwrap()
        };
        fixture.state.store.update_session(&session).unwrap();
        if id == "background" {
            fixture.state.apply_background_tasks_change(
                id,
                crate::hooks::BackgroundTasksChange::Replace(vec![
                    crate::protocol::BackgroundTask {
                        id: "task".to_string(),
                        kind: None,
                        description: None,
                    },
                ]),
            );
        }
        if id == "dialog" {
            running.on_output(
                "  Resume this conversation?\r\n  \u{276f} 1. Resume\r\n    2. Start a new \
                 conversation\r\n  Enter to confirm \u{b7} Esc to resume\r\n"
                    .as_bytes(),
            );
            assert!(running.look_for_dialog().current.is_some());
        }
        publish(&fixture.state);
        let in_use = sharing_state(&fixture, Agent::Claude)
            .in_use
            .expect("in use");
        assert_eq!(in_use.sessions, 1, "{id}");
        assert_eq!(
            sharing_state(&fixture, Agent::Claude).idle_sessions,
            0,
            "{id}"
        );

        let err = fixture.share(Agent::Claude, true).await.unwrap_err();
        assert_eq!(refused_in_use(&fixture, &err)["sessions"], "1", "{id}");
        assert!(!running.poll_exit(), "{id}");
        assert!(launches.given().is_empty());

        fixture.state.watch_exit((*running).clone());
        running.terminate();
        let deadline = std::time::Instant::now() + test_support::PATIENCE;
        while fixture.state.live_session(id).is_some() {
            assert!(std::time::Instant::now() < deadline, "{id} never ended");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}

#[tokio::test]
async fn a_session_that_turned_working_before_it_was_ended_refuses_the_change() {
    use crate::protocol::SharingPhase;
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    let running = idle_session(&fixture, "idle", Agent::Claude);
    let launches = Launches::default();
    launches.resume_through(&fixture.state, false);
    // Another account's change of the same agent holds the change at waiting, explicitly.
    let other = fixture
        .state
        .account_sharing
        .locks
        .lock(Agent::Claude)
        .await;
    let mut events = fixture.state.subscribe();
    let state = fixture.state.clone();
    let change = tokio::spawn(async move { set(&state, Some("claude-a".to_string()), true).await });
    let deadline = std::time::Instant::now() + test_support::PATIENCE;
    while fixture.state.sharing_locked("claude-a").is_none() {
        assert!(std::time::Instant::now() < deadline, "never locked");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    tokio::task::yield_now().await;
    let waiting = sharing_state(&fixture, Agent::Claude);
    assert_eq!(
        waiting.progress.expect("a change").phase,
        SharingPhase::Waiting
    );
    // Not claimed while the change queues: still usable, and still counted idle.
    assert!(!fixture.state.is_launching("idle"));
    assert_eq!(waiting.idle_sessions, 1);
    assert!(sharing_events(&mut events, Agent::Claude)
        .iter()
        .any(|state| state
            .progress
            .as_ref()
            .is_some_and(|p| p.phase == SharingPhase::Waiting)));
    // The user sends it a message meanwhile.
    fixture
        .state
        .store
        .update_session_lifecycle_if("idle", SessionStatus::Idle, SessionStatus::Working, None)
        .unwrap();
    drop(other);

    let err = change.await.unwrap().unwrap_err();
    assert_eq!(code_of(&err), error_code::ACCOUNT_IN_USE);
    assert_eq!(
        err.downcast_ref::<CodedError>().unwrap().params["sessions"],
        "1"
    );
    assert!(!running.poll_exit());
    assert!(launches.given().is_empty());
    let record = fixture.record(Agent::Claude);
    assert!(!record.share_default_config && !record.share_default_config_incomplete);
    assert!(!is_link(&account.join("settings.json")));
    assert!(fixture.state.sharing_locked("claude-a").is_none());
    assert!(sharing_state(&fixture, Agent::Claude).progress.is_none());
    assert!(!fixture.state.is_launching("idle"));
}

#[tokio::test]
async fn the_account_stays_locked_until_its_sessions_are_resumed() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    idle_session(&fixture, "idle", Agent::Claude);
    fixture.session("other", Agent::Claude);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let launches = Launches::default();
    {
        let (state, seen, launches) = (fixture.state.clone(), seen.clone(), launches.clone());
        *fixture.state.account_sharing.relaunch.lock().unwrap() =
            Some(Arc::new(move |request: crate::term::LaunchRequest| {
                let locked = state.sharing_locked("claude-a").is_some();
                let other = state
                    .begin_launch("other")
                    .map(drop)
                    .map_err(|err| code_of(&err));
                seen.lock().unwrap().push((locked, other));
                (launches.launch(false))(request)
            }));
    }
    fixture.share(Agent::Claude, true).await.unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        vec![(true, Err(error_code::ACCOUNT_SHARING_IN_PROGRESS))]
    );
    assert!(fixture.state.sharing_locked("claude-a").is_none());
    assert!(fixture.state.begin_launch("other").is_ok());
}

#[tokio::test]
async fn removing_or_repointing_restarts_idle_sessions_and_refuses_working_ones() {
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    let launches = Launches::default();
    launches.resume_through(&fixture.state, false);
    let running = idle_session(&fixture, "idle", Agent::Claude);
    fixture
        .state
        .store
        .update_session_lifecycle_if("idle", SessionStatus::Idle, SessionStatus::Working, None)
        .unwrap();
    let repoint = |dir: &str| {
        handle(
            &fixture.state,
            None,
            RequestBody::UpdateAccount {
                account: "claude-a".to_string(),
                name: None,
                config_dir: Some(fixture.dir.join(dir).to_string_lossy().into_owned()),
            },
        )
    };
    let err = repoint("claude-elsewhere").await.unwrap_err();
    assert_eq!(code_of(&err), error_code::ACCOUNT_STILL_SHARING);
    assert!(!running.poll_exit());

    fixture
        .state
        .store
        .update_session_lifecycle_if("idle", SessionStatus::Working, SessionStatus::Idle, None)
        .unwrap();
    repoint("claude-elsewhere").await.unwrap();
    assert!(running.poll_exit());
    assert_eq!(launches.given(), vec![None]);
    assert!(!is_link(&account.join("settings.json")));
    assert!(fixture.state.live_session("idle").is_some());

    // Removal: shared again, then removed with the session idle.
    let fixture = Fixture::new();
    claude_setup(&fixture);
    fixture.share(Agent::Claude, true).await.unwrap();
    let launches = Launches::default();
    launches.resume_through(&fixture.state, false);
    let running = idle_session(&fixture, "idle", Agent::Claude);
    handle(
        &fixture.state,
        None,
        RequestBody::DeleteAccount {
            account: "claude-a".to_string(),
            trash_directory: false,
        },
    )
    .await
    .unwrap();
    assert!(running.poll_exit());
    assert_eq!(launches.given(), vec![None]);
    assert!(fixture
        .state
        .store
        .get_account("claude-a")
        .unwrap()
        .is_none());
}

/// Starts a change of `agent`'s account to `share` held at waiting behind another account's
/// change of the same agent, which the answer's sender lets go of.
async fn change_held_at_waiting(
    fixture: &Fixture,
    agent: Agent,
    share: bool,
) -> (
    tokio::task::JoinHandle<Result<()>>,
    tokio::sync::oneshot::Sender<()>,
) {
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    let (held, holding) = tokio::sync::oneshot::channel::<()>();
    {
        let state = fixture.state.clone();
        tokio::spawn(async move {
            let _other = state.account_sharing.locks.lock(agent).await;
            held.send(()).unwrap();
            let _ = released.await;
        });
    }
    holding.await.unwrap();
    let state = fixture.state.clone();
    let change =
        tokio::spawn(async move { set(&state, Some(account_id(agent).to_string()), share).await });
    let deadline = std::time::Instant::now() + test_support::PATIENCE;
    while sharing_state(fixture, agent)
        .progress
        .is_none_or(|progress| progress.phase != crate::protocol::SharingPhase::Waiting)
    {
        assert!(std::time::Instant::now() < deadline, "never waited");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    (change, release)
}

#[tokio::test]
async fn a_retry_refused_before_it_started_keeps_the_earlier_failure() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let (_, account) = claude_setup(&fixture);
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o555)).unwrap();
    fixture.share(Agent::Claude, true).await.unwrap_err();
    std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o755)).unwrap();
    let failed = sharing_state(&fixture, Agent::Claude)
        .failure
        .expect("a failure");

    let launches = Launches::default();
    launches.resume_through(&fixture.state, false);
    let running = idle_session(&fixture, "idle", Agent::Claude);
    let (change, release) = change_held_at_waiting(&fixture, Agent::Claude, true).await;
    // Still told while the retry waits.
    assert_eq!(
        sharing_state(&fixture, Agent::Claude).failure,
        Some(failed.clone())
    );
    fixture
        .state
        .store
        .update_session_lifecycle_if("idle", SessionStatus::Idle, SessionStatus::Working, None)
        .unwrap();
    release.send(()).unwrap();
    let err = change.await.unwrap().unwrap_err();
    assert_eq!(code_of(&err), error_code::ACCOUNT_IN_USE);
    assert!(!running.poll_exit());
    let after = sharing_state(&fixture, Agent::Claude);
    assert_eq!(after.failure, Some(failed));
    assert!(after.progress.is_none());
    assert!(
        fixture
            .record(Agent::Claude)
            .share_default_config_incomplete
    );
}

#[tokio::test]
async fn a_session_that_does_not_stop_holds_the_change_back_and_the_others_are_resumed() {
    // The short stop timeout bounds the session that does stop as well, and a stalled machine can
    // keep its process from being seen down in time: that run tested nothing, so it is run again.
    let deadline = std::time::Instant::now() + test_support::PATIENCE;
    let (fixture, account, launches, _stuck, mut events, err) = loop {
        let fixture = Fixture::new();
        let (_, account) = claude_setup(&fixture);
        let launches = Launches::default();
        launches.resume_through(&fixture.state, false);
        *fixture.state.account_sharing.stop_timeout.lock().unwrap() =
            Some(std::time::Duration::from_millis(500));
        idle_session(&fixture, "fine", Agent::Claude);
        // Its process ends, but nothing watches it, so it is never seen down.
        let session = Session {
            status: SessionStatus::Idle,
            ..fixture.session("stuck", Agent::Claude)
        };
        fixture.state.store.update_session(&session).unwrap();
        let stuck = test_support::running(&fixture.state, "stuck");
        let events = fixture.state.subscribe();

        let err = fixture.share(Agent::Claude, true).await.unwrap_err();
        assert_eq!(code_of(&err), error_code::SHARING_SESSION_DID_NOT_STOP);
        // "fine" missed the point under test if its wait timed out (the error names the first
        // session to time out), or if it was not resumed: never claimed because it was already
        // down, or not seen down even by the time the others were resumed.
        let named = err.downcast_ref::<CodedError>().unwrap().params["session"].clone();
        if (named != "stuck" || launches.given().len() != 1) && std::time::Instant::now() < deadline
        {
            continue;
        }
        break (fixture, account, launches, stuck, events, err);
    };
    let params = &err.downcast_ref::<CodedError>().unwrap().params;
    assert_eq!(params["session"], "stuck");
    assert_eq!(params["account"], "claude-a");
    // Nothing recorded or changed, and no failure kept: the change never started.
    let record = fixture.record(Agent::Claude);
    assert!(!record.share_default_config && !record.share_default_config_incomplete);
    assert!(!is_link(&account.join("settings.json")));
    assert!(sharing_state(&fixture, Agent::Claude).failure.is_none());
    // The one that stopped is resumed; the other is told, not dropped silently.
    assert_eq!(launches.given().len(), 1);
    assert!(fixture.state.live_session("fine").is_some());
    let told: Vec<(String, String)> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            crate::protocol::Event::SessionNotice {
                session,
                code,
                params,
                ..
            } if code == notice_code::SHARING_RESUME_FAILED => {
                Some((session, params["still_running"].clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(told, vec![("stuck".to_string(), "true".to_string())]);
    assert!(fixture.state.sharing_locked("claude-a").is_none());
}

#[tokio::test]
async fn a_session_with_a_message_queued_or_left_unsent_is_working() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let running = idle_session(&fixture, "idle", Agent::Claude);
    assert_eq!(sharing_state(&fixture, Agent::Claude).idle_sessions, 1);

    fixture.state.queue_message("idle", "a message for it");
    assert_eq!(
        sharing_state(&fixture, Agent::Claude)
            .in_use
            .map(|in_use| in_use.sessions),
        Some(1)
    );
    let err = fixture.share(Agent::Claude, true).await.unwrap_err();
    assert_eq!(refused_in_use(&fixture, &err)["sessions"], "1");
    fixture.state.discard_outbox("idle");

    running.mark_unsent(running.prompts_sent());
    let err = fixture.share(Agent::Claude, true).await.unwrap_err();
    assert_eq!(refused_in_use(&fixture, &err)["sessions"], "1");
    assert!(!running.poll_exit());
}

#[tokio::test]
async fn a_message_sent_to_a_session_being_restarted_is_held_for_its_resumed_process() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    *fixture.state.send_wait.lock().unwrap() = None;
    idle_session(&fixture, "idle", Agent::Claude);
    let relaunched = Arc::new(Mutex::new(Vec::new()));
    {
        let relaunched = relaunched.clone();
        *fixture.state.account_sharing.relaunch.lock().unwrap() =
            Some(Arc::new(move |request: crate::term::LaunchRequest| {
                let live = test_support::fake_live_session(
                    &request.session_id,
                    request.agent,
                    80,
                    24,
                    "sleep 30",
                );
                crate::session::spawn_reader_thread((*live).clone(), 8 * 1024);
                let process = (*live).clone();
                relaunched.lock().unwrap().push(live);
                Ok(crate::term::Launch {
                    session: process,
                    agent_session_id: None,
                    notice: None,
                })
            }));
    }
    let state = fixture.state.clone();
    let change = tokio::spawn(async move { set(&state, Some("claude-a".to_string()), true).await });
    // Sent once the change has claimed it, while its process is being ended.
    let deadline = std::time::Instant::now() + test_support::PATIENCE;
    while !fixture.state.is_launching("idle") {
        assert!(std::time::Instant::now() < deadline, "never claimed");
        tokio::task::yield_now().await;
    }
    fixture.state.queue_message("idle", "held-for-the-resume");
    change.await.unwrap().unwrap();

    // Neither written into the process being ended nor forgotten with it: handed to the resumed
    // one. A real launch leaves it for the agent's own first hook (its trust watch holds writes);
    // the stand-in has none, so the change's letting the session go writes it.
    let resumed = relaunched.lock().unwrap().pop().expect("resumed");
    let heard = test_support::output_containing(&resumed, "held-for-the-resume");
    assert!(heard.contains("held-for-the-resume"), "{heard}");
    assert_eq!(fixture.state.queued_count("idle"), 0);
}

/// Waits for `id`'s process to be gone, so that what follows does not race the exit watcher.
async fn down(fixture: &Fixture, id: &str) {
    let deadline = std::time::Instant::now() + test_support::PATIENCE;
    while fixture.state.live_session(id).is_some() {
        assert!(std::time::Instant::now() < deadline, "never went down");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// Claims the idle session `id` for a change to restart and ends its process, as a change does
/// before the resume.
async fn claimed_and_down(fixture: &Fixture, id: &str) -> crate::state::AccountSharingLock {
    let running = idle_session(fixture, id, Agent::Claude);
    let account = fixture.record(Agent::Claude);
    let (_, mut lock) = fixture
        .state
        .lock_account_sharing(&account, |_, _| Ok(()))
        .unwrap();
    lock.claim_idle(|_| Ok(vec![id.to_string()])).unwrap();
    running.terminate();
    down(fixture, id).await;
    lock
}

#[tokio::test]
async fn messages_sent_once_the_process_is_down_are_held_for_the_resume() {
    use crate::reporting::{write_message, Delivery, WhenBlocked};
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let mut lock = claimed_and_down(&fixture, "idle").await;
    assert!(fixture.state.live_session("idle").is_none());

    // Queued whichever kind of write, where a dormant session would refuse it.
    for when_blocked in [WhenBlocked::Queue, WhenBlocked::Refuse] {
        let delivery = write_message(&fixture.state, "idle", "sent-while-down", when_blocked);
        assert_eq!(delivery.unwrap(), Delivery::Queued);
    }
    assert_eq!(fixture.state.queued_count("idle"), 2);

    // The resume's launch is handed the queue.
    lock.release_queue_for_relaunch("idle");
    assert_eq!(fixture.state.queued_count("idle"), 2);
    drop(lock);

    // Once the change lets the session go it is an ordinary dormant one again.
    let err = write_message(&fixture.state, "idle", "late", WhenBlocked::Queue).unwrap_err();
    assert_eq!(code_of(&err), "session_not_running");
}

#[tokio::test]
async fn a_message_queued_after_the_queue_went_to_the_launch_is_written_when_the_session_is_let_go()
{
    use crate::reporting::{write_message, Delivery, WhenBlocked};
    let fixture = Fixture::new();
    claude_setup(&fixture);
    *fixture.state.send_wait.lock().unwrap() = None;
    let running = idle_session(&fixture, "idle", Agent::Claude);
    let account = fixture.record(Agent::Claude);
    let (_, mut lock) = fixture
        .state
        .lock_account_sharing(&account, |_, _| Ok(()))
        .unwrap();
    lock.claim_idle(|_| Ok(vec!["idle".to_string()])).unwrap();
    // The resumed process has its queue, and is still starting when the message comes: nothing
    // writes it then, and the hook that reports it ready may already have come by the time the
    // session is let go.
    let starting = Session {
        status: SessionStatus::Starting,
        ..fixture.state.store.get_session("idle").unwrap().unwrap()
    };
    fixture.state.store.update_session(&starting).unwrap();
    lock.release_queue_for_relaunch("idle");
    let delivery = write_message(
        &fixture.state,
        "idle",
        "after-the-launch",
        WhenBlocked::Queue,
    );
    assert_eq!(delivery.unwrap(), Delivery::Queued);
    assert_eq!(fixture.state.queued_count("idle"), 1);
    let ready = Session {
        status: SessionStatus::Idle,
        ..starting
    };
    fixture.state.store.update_session(&ready).unwrap();

    lock.end_restart("idle");
    let heard = test_support::output_containing(&running, "after-the-launch");
    assert!(heard.contains("after-the-launch"), "{heard}");
    assert_eq!(fixture.state.queued_count("idle"), 0);
}

#[tokio::test]
async fn a_message_queued_after_a_failed_relaunch_is_dropped_and_told_when_the_session_is_let_go() {
    use crate::reporting::{write_message, WhenBlocked};
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let mut lock = claimed_and_down(&fixture, "idle").await;
    lock.release_queue_for_relaunch("idle");
    // The relaunch failed and discarded its queue already; this comes after.
    write_message(
        &fixture.state,
        "idle",
        "after-the-failure",
        WhenBlocked::Queue,
    )
    .unwrap();
    let mut events = fixture.state.subscribe();

    lock.end_restart("idle");
    assert_eq!(fixture.state.queued_count("idle"), 0);
    let told = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        matches!(event, crate::protocol::Event::SessionNotice { session, code, .. }
            if session == "idle" && code == notice_code::SHARING_MESSAGES_DROPPED)
    });
    assert!(told);
}

#[tokio::test]
async fn messages_held_for_a_session_that_is_not_relaunched_are_dropped_and_told() {
    use crate::reporting::{write_message, WhenBlocked};
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let lock = claimed_and_down(&fixture, "idle").await;
    write_message(
        &fixture.state,
        "idle",
        "sent-while-down",
        WhenBlocked::Queue,
    )
    .unwrap();
    let mut events = fixture.state.subscribe();

    // The change lets it go without a resume.
    drop(lock);
    assert_eq!(fixture.state.queued_count("idle"), 0);
    let told = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        matches!(event, crate::protocol::Event::SessionNotice { session, code, .. }
            if session == "idle" && code == notice_code::SHARING_MESSAGES_DROPPED)
    });
    assert!(told);
}

#[tokio::test]
async fn a_session_being_restarted_is_described_to_agents_as_starting() {
    use crate::mcp::exec::describe_session;
    use crate::mcp::reads::list_sessions;
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let caller = fixture.session("caller", Agent::Claude);
    let running = idle_session(&fixture, "idle", Agent::Claude);
    let account = fixture.record(Agent::Claude);
    let (_, mut lock) = fixture
        .state
        .lock_account_sharing(&account, |_, _| Ok(()))
        .unwrap();
    lock.claim_idle(|_| Ok(vec!["idle".to_string()])).unwrap();
    // Its process has ended and the record says so, until the resume's launch.
    running.terminate();
    down(&fixture, "idle").await;
    let session = Session {
        status: SessionStatus::Interrupted,
        ..fixture.state.store.get_session("idle").unwrap().unwrap()
    };
    fixture.state.store.update_session(&session).unwrap();

    assert_eq!(
        fixture.state.shown_status(&session),
        SessionStatus::Starting
    );
    let described = describe_session(
        &fixture.state,
        &session,
        &caller,
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(described["status"], "starting");
    let listed = list_sessions(&fixture.state, &caller, &Default::default())
        .await
        .unwrap();
    assert_eq!(listed["sessions"][0]["session"], "idle");
    assert_eq!(listed["sessions"][0]["status"], "starting");

    // Once the change lets it go it is interrupted again.
    drop(lock);
    assert_eq!(
        fixture.state.shown_status(&session),
        SessionStatus::Interrupted
    );
    let listed = list_sessions(&fixture.state, &caller, &Default::default())
        .await
        .unwrap();
    assert!(listed["sessions"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn the_hold_a_sharing_change_takes_on_a_queue_is_told_apart_from_a_handoffs() {
    use crate::mcp::exec::describe_session;
    use crate::state::WriteHold;
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let caller = fixture.session("caller", Agent::Claude);
    let _running = idle_session(&fixture, "idle", Agent::Claude);
    let session = fixture.state.store.get_session("idle").unwrap().unwrap();
    let held = |fixture: &Fixture| {
        describe_session(
            &fixture.state,
            &session,
            &caller,
            &Default::default(),
            &Default::default(),
        )
        .unwrap()["writes_held"]["reason"]
            .clone()
    };
    assert_eq!(fixture.state.write_hold("idle"), None);

    let hold = fixture.state.hold_outbox_for_sharing("idle");
    assert_eq!(
        fixture.state.write_hold("idle"),
        Some(WriteHold::SharingChange)
    );
    assert_eq!(held(&fixture), "sharing_change");
    drop(hold);

    let hold = fixture.state.hold_outbox("idle");
    assert_eq!(fixture.state.write_hold("idle"), Some(WriteHold::Handoff));
    assert_eq!(held(&fixture), "handoff");
    drop(hold);
}

/// The stored session `session` of the account at its prompt, as [`idle_session`] makes one, whose
/// process has left one process running in a terminal session of its own, as an agent's dev server
/// is. Answers the session's process and that process's pid.
fn idle_session_that_started_a_server(
    fixture: &Fixture,
    session: Session,
) -> (test_support::StandIn, i32) {
    let session = Session {
        status: SessionStatus::Idle,
        ..session
    };
    fixture.state.store.update_session(&session).unwrap();
    test_support::running_leaving_one(&fixture.state, &fixture.dir, &session.id)
}

/// A relaunch that starts a stand-in carrying the request's process tag, as an agent's launch
/// does, or, when `fails`, refuses.
fn relaunch_as_a_launch_does(
    fixture: &Fixture,
    fails: bool,
) -> Arc<Mutex<Vec<test_support::StandIn>>> {
    let started = Arc::new(Mutex::new(Vec::new()));
    let launches = Arc::new(Mutex::new(Vec::new()));
    let kept = started.clone();
    *fixture.state.account_sharing.relaunch.lock().unwrap() = Some(Arc::new(move |request| {
        if fails {
            anyhow::bail!("the agent would not start");
        }
        (test_support::stand_in_launch(&kept, &launches, || {}))(request)
    }));
    started
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_of_sharing_carries_what_a_sessions_process_started_over_to_its_relaunch() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let (first, child) =
        idle_session_that_started_a_server(&fixture, fixture.session("idle", Agent::Claude));
    let _guard = test_support::KillOnDrop(vec![child]);
    let started = relaunch_as_a_launch_does(&fixture, false);

    fixture.share(Agent::Claude, true).await.unwrap();

    assert!(first.poll_exit());
    let relaunched = fixture.state.live_session("idle").expect("resumed");
    assert_ne!(relaunched.launch_id, first.launch_id);
    fixture.state.process_ledger.tick_now();
    assert!(
        test_support::is_running(child),
        "stopped with the process the change restarted"
    );
    let trees = fixture
        .state
        .process_ledger
        .trees_of_sessions(&["idle".to_string()], std::time::Duration::ZERO);
    assert_eq!(trees.len(), 1, "{trees:?}");
    assert!(trees[0].id.starts_with(&format!("{child}:")));

    // The resumed process stops it when it ends, as any session's.
    started.lock().unwrap().pop().expect("started").terminate();
    assert!(
        test_support::stops_running(&fixture.state, child).await,
        "kept running past the resumed process"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_of_sharing_whose_resume_fails_keeps_what_the_process_started_under_its_project() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let (_first, child) =
        idle_session_that_started_a_server(&fixture, fixture.session("idle", Agent::Claude));
    let _guard = test_support::KillOnDrop(vec![child]);
    relaunch_as_a_launch_does(&fixture, true);
    let mut events = fixture.state.subscribe();

    // The change itself goes through; the session it could not resume is told as a notice.
    fixture.share(Agent::Claude, true).await.unwrap();
    assert!(std::iter::from_fn(|| events.try_recv().ok()).any(|event| matches!(
        event,
        crate::protocol::Event::SessionNotice { code, .. } if code == notice_code::SHARING_RESUME_FAILED
    )));

    assert_eq!(
        fixture
            .state
            .store
            .get_session("idle")
            .unwrap()
            .unwrap()
            .status,
        SessionStatus::Interrupted
    );
    fixture.state.process_ledger.tick_now();
    assert!(
        test_support::is_running(child),
        "stopped with the failed resume"
    );
    let listed = fixture
        .state
        .process_ledger
        .trees_of_project(&[], "project-1", &[]);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert!(listed[0].id.starts_with(&format!("{child}:")));
    assert!(listed[0].kept && listed[0].session_id.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_console_session_whose_resume_fails_after_a_change_of_sharing_stops_what_it_started() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    let console = Session {
        account_id: Some(account_id(Agent::Claude).to_string()),
        config_dir: Some(
            fixture
                .account(Agent::Claude)
                .to_string_lossy()
                .into_owned(),
        ),
        ..test_support::console_session("hub", SessionStatus::Idle)
    };
    fixture.state.store.insert_session(&console).unwrap();
    let (_first, child) = idle_session_that_started_a_server(&fixture, console);
    let _guard = test_support::KillOnDrop(vec![child]);
    relaunch_as_a_launch_does(&fixture, true);

    fixture.share(Agent::Claude, true).await.unwrap();

    assert!(
        test_support::stops_running(&fixture.state, child).await,
        "kept running past the failed resume"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_archived_while_the_change_runs_stops_what_its_process_started() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    idle_session(&fixture, "first", Agent::Claude);
    let (_second, child) =
        idle_session_that_started_a_server(&fixture, fixture.session("second", Agent::Claude));
    let _guard = test_support::KillOnDrop(vec![child]);
    // The user archives the second while the first is being resumed.
    let state = fixture.state.clone();
    let started = Arc::new(Mutex::new(Vec::new()));
    let launches = Arc::new(Mutex::new(Vec::new()));
    *fixture.state.account_sharing.relaunch.lock().unwrap() = Some(Arc::new(move |request| {
        let mut second = state.store.get_session("second").unwrap().unwrap();
        second.status = SessionStatus::Archived;
        state.store.update_session(&second).unwrap();
        (test_support::stand_in_launch(&started, &launches, || {}))(request)
    }));

    fixture.share(Agent::Claude, true).await.unwrap();

    assert!(
        test_support::stops_running(&fixture.state, child).await,
        "kept running for an archived session"
    );
    assert!(fixture
        .state
        .process_ledger
        .trees_of_project(&[], "project-1", &[])
        .is_empty());
}

#[tokio::test]
async fn a_session_the_change_restarts_cannot_be_claimed_by_a_switch_meanwhile() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    idle_session(&fixture, "idle", Agent::Claude);
    let account = fixture.record(Agent::Claude);
    let (_, mut lock) = fixture
        .state
        .lock_account_sharing(&account, |_, _| Ok(()))
        .unwrap();
    lock.claim_idle(|_| Ok(vec!["idle".to_string()])).unwrap();

    // What a process handover, a switch or a model change takes first.
    let err = fixture.state.begin_switch("idle").map(|_| ()).unwrap_err();
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
}
