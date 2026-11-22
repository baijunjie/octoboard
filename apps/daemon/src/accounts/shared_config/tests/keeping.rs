//! Nothing sharing leaves in the default directory depends on the sharing account's directory:
//! the plugin registries are kept naming the default directory's own paths, turning sharing off
//! leaves the default directory as it was but for the history, and sharing is turned off for
//! every account at once.

use std::collections::BTreeMap;

use super::*;
use crate::coordinator::account_sharing::{turn_off_all, SharedHistory};
use crate::coordinator::system_changes::{self, SystemChange, SystemChangeKept};

/// Every provider's plugin registries, by the shared item they sit in.
const REGISTRIES: [(AgentId, &str, &str); 3] = [
    (AgentId::CLAUDE, "plugins", "installed_plugins.json"),
    (AgentId::CLAUDE, "plugins", "known_marketplaces.json"),
    (AgentId::GROK, "installed-plugins", "registry.json"),
];

/// Shares `agent`'s account and records, in `registry`, a plugin installed through the account's
/// link by both spellings of the account's directory, as a session of the account leaves it.
async fn installed_through_the_account(fixture: &Fixture, agent: AgentId, item: &str, file: &str) {
    let (default, account) = (fixture.default(agent), fixture.account(agent));
    std::fs::create_dir_all(default.join(item)).unwrap();
    if !fixture.record(agent).share_default_config {
        fixture.share(agent, true).await.unwrap();
    }
    write(&account, &format!("{item}/cache/tool/plugin.json"), "{}");
    record_install(&default, &account, item, file);
}

/// Writes `file` in `default`'s `item` as naming a plugin under `account`'s `item`, by the
/// account's spelling and by its resolved one.
fn record_install(default: &Path, account: &Path, item: &str, file: &str) {
    let spelled = account.join(item).join("cache/tool");
    let resolved = real(account).join(item).join("cache/tool");
    write(
        default,
        &format!("{item}/{file}"),
        &serde_json::json!({"plugins": {"tool": [
            {"installPath": spelled, "version": "1.0"},
            {"installPath": resolved, "version": "1.0"},
        ]}})
        .to_string(),
    );
}

/// The paths `file` in `default`'s `item` records.
fn recorded(default: &Path, item: &str, file: &str) -> Vec<PathBuf> {
    let registry: serde_json::Value =
        serde_json::from_str(&read(default, &format!("{item}/{file}"))).unwrap();
    registry["plugins"]["tool"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| PathBuf::from(entry["installPath"].as_str().unwrap()))
        .collect()
}

/// Whether every path `file` records lies in the default directory and leads to the plugin.
fn leads_into_the_default(default: &Path, item: &str, file: &str) -> bool {
    let shared = real(&default.join(item));
    recorded(default, item, file)
        .iter()
        .all(|path| path.starts_with(&shared) && path.join("plugin.json").is_file())
}

/// Runs a session `id` of `agent`'s account whose process ends by itself, and waits for its exit
/// watcher to finish, the registries' keeping at its end included. `while_running` runs once the
/// process is watched and before it ends.
async fn session_that_ends_after(
    fixture: &Fixture,
    id: &str,
    agent: AgentId,
    while_running: impl FnOnce(&Session),
) {
    let session = Session {
        status: SessionStatus::Idle,
        ..fixture.session(id, agent)
    };
    fixture.state.store.update_session(&session).unwrap();
    let live = test_support::fake_live_session(id, agent, 80, 24, "sleep 0.5");
    fixture.state.register_live(live.clone());
    crate::coordinator::exit_watch::watch_exit(&fixture.state, (*live).clone());
    while_running(&session);
    live.await_exit_watch().await;
    assert!(fixture.state.live_session(id).is_none());
}

async fn session_that_ends(fixture: &Fixture, id: &str, agent: AgentId) {
    session_that_ends_after(fixture, id, agent, |_| {}).await;
}

/// The bytes and the identity of `path`, to tell a file left alone from one written again.
fn as_it_is(path: &Path) -> (Vec<u8>, crate::platform::fs::Identity) {
    let identity = crate::platform::fs::identity(path, crate::platform::fs::Links::Keep).unwrap();
    (std::fs::read(path).unwrap(), identity)
}

/// After a session of a sharing account has ended, the registries name the default directory's
/// own paths, so deleting the account's directory leaves the default account's plugins working.
#[tokio::test]
async fn a_session_that_ends_leaves_no_registry_path_through_the_accounts_directory() {
    let fixture = Fixture::new();
    for (index, (agent, item, file)) in REGISTRIES.into_iter().enumerate() {
        let default = fixture.default(agent);
        installed_through_the_account(&fixture, agent, item, file).await;

        session_that_ends(&fixture, &format!("s{index}"), agent).await;
        assert!(
            leads_into_the_default(&default, item, file),
            "{agent:?} {file}: {:?}",
            recorded(&default, item, file)
        );
        // What the registry records beside the path is kept.
        let registry: serde_json::Value =
            serde_json::from_str(&read(&default, &format!("{item}/{file}"))).unwrap();
        assert_eq!(registry["plugins"]["tool"][0]["version"], "1.0");
    }
    for (agent, item, file) in REGISTRIES {
        let (default, account) = (fixture.default(agent), fixture.account(agent));
        if account.exists() {
            std::fs::remove_dir_all(&account).unwrap();
        }
        assert!(
            leads_into_the_default(&default, item, file),
            "{agent:?} {file}"
        );
    }
}

/// The repair before a launch keeps the registries too: what a session the daemon never saw end
/// (the user's own run of the agent against the account's directory, say) left there.
#[tokio::test]
async fn the_repair_before_a_launch_keeps_the_registries_in_the_default_directory() {
    let fixture = Fixture::new();
    for (index, (agent, item, file)) in REGISTRIES.into_iter().enumerate() {
        let (default, account) = (fixture.default(agent), fixture.account(agent));
        installed_through_the_account(&fixture, agent, item, file).await;
        // An item the repair links again is kept in the same launch.
        crate::platform::fs::remove_file_or_link(&account.join(item)).unwrap();
        let session = fixture.session(&format!("s{index}"), agent);

        let prepared = before_launch(&fixture.state, &session).await.unwrap();
        assert!(prepared.notices.is_empty(), "{:?}", prepared.notices);
        assert!(
            leads_into_the_default(&default, item, file),
            "{agent:?} {file}"
        );
    }
}

/// A registry that cannot be rewritten is told with what the repair could not fix, under the item
/// it sits in, and does not hold the launch up.
#[tokio::test]
async fn a_registry_the_repair_cannot_rewrite_is_told_as_a_repair_failure() {
    let fixture = Fixture::new();
    let default = fixture.default(AgentId::GROK);
    installed_through_the_account(
        &fixture,
        AgentId::GROK,
        "installed-plugins",
        "registry.json",
    )
    .await;
    // Names the account's directory, so it has to be read as JSON, and is none.
    let unreadable = format!(
        "not json {}",
        fixture
            .account(AgentId::GROK)
            .join("installed-plugins")
            .display()
    );
    write(&default, "installed-plugins/registry.json", &unreadable);
    let session = fixture.session("s", AgentId::GROK);

    let prepared = before_launch(&fixture.state, &session).await.unwrap();
    assert_eq!(prepared.shared_default_dir, Some(real(&default)));
    let [notice] = prepared.notices.as_slice() else {
        panic!("one notice expected: {:?}", prepared.notices);
    };
    assert_eq!(notice.code, notice_code::ACCOUNT_SHARING_REPAIR_FAILED);
    let items: serde_json::Value = serde_json::from_str(&notice.params["items"]).unwrap();
    assert_eq!(items[0]["item"], "installed-plugins");
    assert_eq!(items[0]["left"], "linked");
    assert_eq!(
        read(&default, "installed-plugins/registry.json"),
        unreadable
    );
}

/// As the daemon stops, every sharing account's registries are kept, as at its start: a session's
/// own exit may not get to it before the daemon is gone.
#[tokio::test]
async fn a_daemon_stop_keeps_the_registries_of_every_account_that_shares() {
    let fixture = Fixture::new();
    for (agent, item, file) in REGISTRIES {
        installed_through_the_account(&fixture, agent, item, file).await;
    }

    keep_registries_at_stop(&fixture.state).await;
    for (agent, item, file) in REGISTRIES {
        assert!(
            leads_into_the_default(&fixture.default(agent), item, file),
            "{agent:?} {file}"
        );
    }
}

/// What a rewrite cut short by the daemon's end left staged beside a registry is removed at the
/// next daemon start, and before the next launch, whether or not the registry has anything to
/// rewrite; a file beside it that is not named as a staged one stays.
#[tokio::test]
async fn a_staged_file_a_cut_rewrite_left_beside_a_registry_is_removed() {
    let fixture = Fixture::new();
    let default = fixture.default(AgentId::GROK);
    installed_through_the_account(
        &fixture,
        AgentId::GROK,
        "installed-plugins",
        "registry.json",
    )
    .await;
    keep_registries_at_start(&fixture.state).await;
    // A pid no process holds.
    let stale = default.join("installed-plugins/.registry.json.octoboard-2147483632");
    let users_own = default.join("installed-plugins/.registry.json.octoboard-notes");

    // Written an hour ago, longer than a replacement takes: fresh, it could be a replacement
    // under way by a daemon elsewhere.
    let left_long_ago = |path: &Path| {
        std::fs::write(path, "half a registry").unwrap();
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
            .unwrap();
    };
    left_long_ago(&stale);
    std::fs::write(&users_own, "the user's").unwrap();
    keep_registries_at_start(&fixture.state).await;
    assert!(!stale.exists());

    left_long_ago(&stale);
    let session = fixture.session("s", AgentId::GROK);
    before_launch(&fixture.state, &session).await.unwrap();
    assert!(!stale.exists());
    assert_eq!(std::fs::read_to_string(&users_own).unwrap(), "the user's");
}

/// Once the account's directory is gone, with its links, what the default directory still records
/// by the spelling the account is recorded under is rewritten all the same: it leads nowhere.
#[tokio::test]
async fn a_registry_naming_an_account_directory_that_is_gone_is_rewritten_from_its_recorded_spelling(
) {
    let fixture = Fixture::new();
    for (agent, item, file) in REGISTRIES {
        installed_through_the_account(&fixture, agent, item, file).await;
    }
    for agent in [AgentId::CLAUDE, AgentId::GROK] {
        std::fs::remove_dir_all(fixture.account(agent)).unwrap();
    }

    keep_registries_at_start(&fixture.state).await;
    for (agent, item, file) in REGISTRIES {
        let default = fixture.default(agent);
        let shared = real(&default.join(item));
        // The entry recorded by the account's spelling; the one recorded by the directory's
        // resolved path can no longer be told from any other path once the directory is gone.
        let spelled = &recorded(&default, item, file)[0];
        assert_eq!(*spelled, shared.join("cache/tool"), "{agent:?} {file}");
        assert!(spelled.join("plugin.json").is_file(), "{agent:?} {file}");
    }
}

/// A registry that names nothing of the account's keeps its bytes and its identity, however it is
/// formatted, through a session's end, a launch, a daemon start and a daemon stop.
#[tokio::test]
async fn a_registry_with_nothing_of_the_accounts_is_left_byte_for_byte() {
    let fixture = Fixture::new();
    for (index, (agent, item, file)) in REGISTRIES.into_iter().enumerate() {
        let default = fixture.default(agent);
        installed_through_the_account(&fixture, agent, item, file).await;
        let own = real(&default.join(item)).join("cache/tool");
        // Formatted as no writer here formats it.
        let text = format!(
            "{{\"plugins\":{{\"tool\":[ {{\"version\":\"1.0\",   \"installPath\":{}}} ]}}}}\n\n",
            serde_json::to_string(&own).unwrap()
        );
        write(&default, &format!("{item}/{file}"), &text);
        let registry = default.join(item).join(file);
        let before = as_it_is(&registry);

        session_that_ends(&fixture, &format!("s{index}"), agent).await;
        assert!(
            as_it_is(&registry) == before,
            "{agent:?} {file}: a session's end"
        );
        let session = fixture.session(&format!("l{index}"), agent);
        before_launch(&fixture.state, &session).await.unwrap();
        assert!(as_it_is(&registry) == before, "{agent:?} {file}: a launch");
        keep_registries_at_start(&fixture.state).await;
        keep_registries_at_stop(&fixture.state).await;
        assert!(
            as_it_is(&registry) == before,
            "{agent:?} {file}: a start and a stop"
        );
    }
}

/// What a session's process left belongs to the account it ran as: a session an account switch
/// records on another account while its process is still ending keeps the registries of the
/// account it left.
#[tokio::test]
async fn a_session_recorded_on_another_account_since_keeps_the_registries_of_the_one_it_ran_as() {
    let fixture = Fixture::new();
    let default = fixture.default(AgentId::GROK);
    installed_through_the_account(
        &fixture,
        AgentId::GROK,
        "installed-plugins",
        "registry.json",
    )
    .await;

    session_that_ends_after(&fixture, "s", AgentId::GROK, |session| {
        // As a switch to the default account records it.
        let switched = Session {
            account_id: None,
            config_dir: None,
            ..session.clone()
        };
        fixture.state.store.update_session(&switched).unwrap();
    })
    .await;
    assert!(
        leads_into_the_default(&default, "installed-plugins", "registry.json"),
        "{:?}",
        recorded(&default, "installed-plugins", "registry.json")
    );
}

/// Turning sharing off with an item that cannot be looked up keeps that item linked and says so:
/// its registry may still name the account's path, which must go on leading to the plugin.
// A link that loops is the lookup error at hand; Windows has no such link to stand in for it.
#[cfg(unix)]
#[tokio::test]
async fn an_item_that_cannot_be_looked_up_stays_linked_when_sharing_is_turned_off() {
    let fixture = Fixture::new();
    let (default, account) = (
        fixture.default(AgentId::CLAUDE),
        fixture.account(AgentId::CLAUDE),
    );
    installed_through_the_account(
        &fixture,
        AgentId::CLAUDE,
        "plugins",
        "installed_plugins.json",
    )
    .await;
    let leads = std::fs::read_link(account.join("plugins")).unwrap();
    // The default directory's entry now loops, so it resolves to an error, not to nothing.
    std::fs::rename(default.join("plugins"), default.join("plugins-aside")).unwrap();
    test_support::symlink(default.join("plugins"), default.join("plugins")).unwrap();

    let err = fixture.share(AgentId::CLAUDE, false).await.unwrap_err();
    assert_eq!(
        code_of(&err),
        crate::protocol::error_code::ACCOUNT_SHARING_FAILED
    );
    let items: serde_json::Value =
        serde_json::from_str(&test_support::param_of(&err, "items")).unwrap();
    let plugins: Vec<&serde_json::Value> = items
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["item"] == "plugins")
        .collect();
    assert_eq!(plugins.len(), 1, "{items}");
    assert_eq!(plugins[0]["left"], "linked");
    assert_eq!(std::fs::read_link(account.join("plugins")).unwrap(), leads);
    assert!(
        fixture
            .record(AgentId::CLAUDE)
            .share_default_config_incomplete
    );
}

/// At daemon start every account that shares has its registries kept, for the sessions the
/// previous daemon did not see end; an account whose last change is incomplete included, while
/// its item still links into the default directory.
#[tokio::test]
async fn a_daemon_start_keeps_the_registries_of_every_account_that_shares() {
    let fixture = Fixture::new();
    for (agent, item, file) in REGISTRIES {
        installed_through_the_account(&fixture, agent, item, file).await;
    }
    fixture
        .state
        .store
        .set_account_sharing(account_id(AgentId::GROK), true, true)
        .unwrap();

    keep_registries_at_start(&fixture.state).await;
    for (agent, item, file) in REGISTRIES {
        assert!(
            leads_into_the_default(&fixture.default(agent), item, file),
            "{agent:?} {file}"
        );
    }
}

/// A path through the directory of an account that does not link the registry's item into the
/// default directory names another place than the default's, and is left as it is.
#[tokio::test]
async fn a_registry_path_into_a_directory_the_account_does_not_share_is_left_alone() {
    let fixture = Fixture::new();
    let (default, account) = (
        fixture.default(AgentId::CLAUDE),
        fixture.account(AgentId::CLAUDE),
    );
    installed_through_the_account(
        &fixture,
        AgentId::CLAUDE,
        "plugins",
        "installed_plugins.json",
    )
    .await;
    // The account keeps plugins of its own in place of the link.
    crate::platform::fs::remove_file_or_link(&account.join("plugins")).unwrap();
    write(&account, "plugins/cache/tool/plugin.json", "{}");
    let registry = default.join("plugins/installed_plugins.json");
    let before = as_it_is(&registry);

    keep_registries_at_start(&fixture.state).await;
    session_that_ends(&fixture, "s", AgentId::CLAUDE).await;
    // An account that never shared is not looked at either.
    std::fs::create_dir_all(fixture.account(AgentId::GROK)).unwrap();
    record_install(
        &fixture.default(AgentId::GROK),
        &fixture.account(AgentId::GROK),
        "installed-plugins",
        "registry.json",
    );
    let unshared = recorded(
        &fixture.default(AgentId::GROK),
        "installed-plugins",
        "registry.json",
    );
    keep_registries_at_start(&fixture.state).await;

    assert!(as_it_is(&registry) == before, "the registry was written");
    assert_eq!(
        recorded(
            &fixture.default(AgentId::GROK),
            "installed-plugins",
            "registry.json"
        ),
        unshared
    );
}

/// As for the default directory's entry, so for the account's: an entry of the account's that
/// cannot be looked up keeps the item as it is and is told, since its registry may name it.
// A link that loops is the lookup error at hand; Windows has no such link to stand in for it.
#[cfg(unix)]
#[tokio::test]
async fn an_account_entry_that_cannot_be_looked_up_is_told_when_sharing_is_turned_off() {
    let fixture = Fixture::new();
    let account = fixture.account(AgentId::GROK);
    installed_through_the_account(
        &fixture,
        AgentId::GROK,
        "installed-plugins",
        "registry.json",
    )
    .await;
    let entry = account.join("installed-plugins");
    crate::platform::fs::remove_file_or_link(&entry).unwrap();
    test_support::symlink(&entry, &entry).unwrap();

    let err = fixture.share(AgentId::GROK, false).await.unwrap_err();
    let items: serde_json::Value =
        serde_json::from_str(&test_support::param_of(&err, "items")).unwrap();
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["item"] == "installed-plugins" && item["left"] == "linked"),
        "{items}"
    );
    assert!(is_link(&entry));
}

/// Turning sharing off for an account whose directory is gone has no link left to remove: it
/// rewrites what the default directory still records by the account's spelling, and completes.
#[tokio::test]
async fn turning_sharing_off_with_the_accounts_directory_gone_rewrites_the_registries() {
    let fixture = Fixture::new();
    for (agent, item, file) in REGISTRIES {
        installed_through_the_account(&fixture, agent, item, file).await;
    }
    for agent in [AgentId::CLAUDE, AgentId::GROK] {
        std::fs::remove_dir_all(fixture.account(agent)).unwrap();
        fixture.share(agent, false).await.unwrap();
        let record = fixture.record(agent);
        assert!(!record.share_default_config && !record.share_default_config_incomplete);
    }
    for (agent, item, file) in REGISTRIES {
        let default = fixture.default(agent);
        assert_eq!(
            recorded(&default, item, file)[0],
            real(&default.join(item)).join("cache/tool"),
            "{agent:?} {file}"
        );
    }
}

/// A registry that cannot be rewritten then is told as an item that is absent, not linked: the
/// link went with the directory.
#[tokio::test]
async fn a_registry_not_rewritten_with_the_accounts_directory_gone_is_told_as_absent() {
    let fixture = Fixture::new();
    let (default, account) = (
        fixture.default(AgentId::GROK),
        fixture.account(AgentId::GROK),
    );
    installed_through_the_account(
        &fixture,
        AgentId::GROK,
        "installed-plugins",
        "registry.json",
    )
    .await;
    let unreadable = format!("not json {}", account.join("installed-plugins").display());
    write(&default, "installed-plugins/registry.json", &unreadable);
    std::fs::remove_dir_all(&account).unwrap();

    let err = fixture.share(AgentId::GROK, false).await.unwrap_err();
    assert_eq!(
        code_of(&err),
        crate::protocol::error_code::ACCOUNT_SHARING_FAILED
    );
    let items: serde_json::Value =
        serde_json::from_str(&test_support::param_of(&err, "items")).unwrap();
    assert_eq!(items.as_array().unwrap().len(), 1, "{items}");
    assert_eq!(items[0]["item"], "installed-plugins");
    assert_eq!(items[0]["left"], "absent");
    assert_eq!(
        read(&default, "installed-plugins/registry.json"),
        unreadable
    );
}

/// A registry that is not JSON and does not name the account's directory is no failure: sharing
/// is turned off, the item unlinked, and the file left as it is.
#[tokio::test]
async fn a_registry_that_is_not_json_and_names_nothing_of_the_accounts_does_not_keep_the_item_linked(
) {
    let fixture = Fixture::new();
    let (default, account) = (
        fixture.default(AgentId::GROK),
        fixture.account(AgentId::GROK),
    );
    installed_through_the_account(
        &fixture,
        AgentId::GROK,
        "installed-plugins",
        "registry.json",
    )
    .await;
    write(&default, "installed-plugins/registry.json", "not json");

    fixture.share(AgentId::GROK, false).await.unwrap();
    assert!(!is_link(&account.join("installed-plugins")));
    assert_eq!(
        read(&default, "installed-plugins/registry.json"),
        "not json"
    );
}

/// A path is found by what it says, not by how the file spells it: a registry whose writer
/// escapes the separators still has the account's paths rewritten.
#[tokio::test]
async fn a_registry_that_spells_its_paths_with_escapes_is_rewritten_all_the_same() {
    let fixture = Fixture::new();
    let (default, account) = (
        fixture.default(AgentId::CLAUDE),
        fixture.account(AgentId::CLAUDE),
    );
    installed_through_the_account(
        &fixture,
        AgentId::CLAUDE,
        "plugins",
        "installed_plugins.json",
    )
    .await;
    let spelled = serde_json::to_string(&account.join("plugins").join("cache/tool"))
        .unwrap()
        .replace('/', "\\/");
    write(
        &default,
        "plugins/installed_plugins.json",
        &format!("{{\"plugins\":{{\"tool\":[{{\"installPath\":{spelled}}}]}}}}"),
    );

    keep_registries_at_start(&fixture.state).await;
    assert_eq!(
        recorded(&default, "plugins", "installed_plugins.json"),
        [real(&default.join("plugins")).join("cache/tool")]
    );
}

/// A Grok Build session of a sharing account: its own after-exit work names the plugin by the
/// account's spelling, as Grok run against the account's home would, and the keeping that
/// follows the exit points it at the default directory, so nothing there names either the
/// per-session home or the account's directory.
#[tokio::test]
async fn a_grok_sessions_install_ends_up_named_by_the_default_directory() {
    let fixture = Fixture::new();
    let (default, account) = (
        fixture.default(AgentId::GROK),
        fixture.account(AgentId::GROK),
    );
    installed_through_the_account(
        &fixture,
        AgentId::GROK,
        "installed-plugins",
        "registry.json",
    )
    .await;
    let shared = real(&default.join("installed-plugins"));
    let session_home = fixture.dir.join("run").join("s").join("grok-home");
    std::fs::create_dir_all(&session_home).unwrap();
    crate::hostfs::alias(&shared, &session_home.join("installed-plugins"), true).unwrap();
    write(&shared, "tool-abc/plugin.json", "{}");
    let under_the_session = session_home.join("installed-plugins").join("tool-abc");
    write(
        &default,
        "installed-plugins/registry.json",
        &serde_json::json!({"version": 1, "repos": {"tool-abc": {"path": under_the_session}}})
            .to_string(),
    );
    let path = || {
        let registry: serde_json::Value =
            serde_json::from_str(&read(&default, "installed-plugins/registry.json")).unwrap();
        PathBuf::from(registry["repos"]["tool-abc"]["path"].as_str().unwrap())
    };

    crate::adapter::grok::plugin_paths_after_exit_for_test(&account, &session_home)
        .expect("the session's home links the account's plugin directory")
        .run();
    assert_eq!(path(), account.join("installed-plugins").join("tool-abc"));

    keep_registries_at_start(&fixture.state).await;
    assert_eq!(path(), shared.join("tool-abc"));
    assert!(path().join("plugin.json").is_file());
}

/// Every entry below `root`, by its path relative to it: a link by where it leads, a file by its
/// content, a directory by nothing.
fn tree(root: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, found: &mut BTreeMap<String, String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let kind = entry.file_type().unwrap();
            if kind.is_symlink() {
                let leads = std::fs::read_link(&path).unwrap();
                found.insert(name, format!("-> {}", leads.display()));
            } else if kind.is_dir() {
                found.insert(format!("{name}/"), String::new());
                walk(root, &path, found);
            } else {
                found.insert(name, std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    let mut found = BTreeMap::new();
    walk(root, root, &mut found);
    found
}

/// Turning sharing off removes every link turning it on made and leaves the default directory as
/// it was before sharing, but for two things it keeps: the conversation history moved into it,
/// and the directories of the shared list it lacked, created there empty.
#[tokio::test]
async fn turning_sharing_off_leaves_the_default_directory_as_it_was_but_for_the_history() {
    let fixture = Fixture::new();
    for agent in [AgentId::CLAUDE, AgentId::CODEX, AgentId::GROK] {
        let (default, account) = (fixture.default(agent), fixture.account(agent));
        let history_dir = items::shared_items(agent)
            .into_iter()
            .find(|item| item.class.is_history() && item.kind == items::ItemKind::Directory)
            .expect("a history directory")
            .name;
        let config_file = items::shared_items(agent)
            .into_iter()
            .find(|item| !item.class.is_history() && item.kind == items::ItemKind::File)
            .expect("a configuration file")
            .name;
        write(&default, config_file, "default configuration");
        write(&default, "skills/shared/SKILL.md", "shared skill");
        write(&default, "unshared-entry", "the default's own");
        write(&account, config_file, "account configuration");
        write(&account, &format!("{history_dir}/own/record"), "account");
        write(&account, "unshared-entry", "the account's own");
        let default_before = tree(&default);
        let account_before = tree(&account);

        fixture.share(agent, true).await.unwrap();
        fixture.share(agent, false).await.unwrap();

        // Nothing of the account's leads into the default directory any more, and what it held
        // is back, the history moved out of it excepted.
        let account_after = tree(&account);
        let default_spellings = [default.clone(), real(&default)];
        for (entry, content) in &account_after {
            let leads = content.strip_prefix("-> ").map(PathBuf::from);
            assert!(
                !leads.is_some_and(|leads| default_spellings.iter().any(|d| leads.starts_with(d))),
                "{agent:?}: `{entry}` still links into the default directory"
            );
        }
        for (entry, content) in &account_before {
            if entry.starts_with(&format!("{history_dir}/own")) {
                continue;
            }
            assert_eq!(account_after.get(entry), Some(content), "{agent:?} {entry}");
        }

        // The default directory: every entry it had is as it was, nothing in it leads into the
        // account's directory or names it, and what it gained is the moved history and empty
        // directories of the shared list.
        let default_after = tree(&default);
        let account_spellings = [account.clone(), real(&account)];
        for (entry, content) in &default_before {
            assert_eq!(default_after.get(entry), Some(content), "{agent:?} {entry}");
        }
        let shared_dirs: Vec<String> = items::shared_items(agent)
            .into_iter()
            .filter(|item| item.kind == items::ItemKind::Directory)
            .map(|item| format!("{}/", item.name))
            .collect();
        for (entry, content) in &default_after {
            for spelling in &account_spellings {
                assert!(
                    !content.contains(&*spelling.to_string_lossy()),
                    "{agent:?}: `{entry}` names the account's directory"
                );
            }
            if default_before.contains_key(entry) {
                continue;
            }
            let moved_history = entry.starts_with(&format!("{history_dir}/own"));
            let created_empty = shared_dirs.contains(entry) && content.is_empty();
            assert!(
                moved_history || created_empty,
                "{agent:?}: turning sharing off left `{entry}` in the default directory"
            );
        }
        assert_eq!(
            default_after.get(&format!("{history_dir}/own/record")),
            Some(&"account".to_string()),
            "{agent:?}"
        );
    }
}

/// One call turns sharing off for every account that shares or whose last change is incomplete,
/// and names the history each leaves in the default account's directory.
#[tokio::test]
async fn sharing_is_turned_off_for_every_account_at_once_and_the_history_left_is_named() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    fixture.share(AgentId::CLAUDE, true).await.unwrap();
    write(
        &fixture.account(AgentId::GROK),
        "sessions/own/summary.json",
        "{}",
    );
    fixture.share(AgentId::GROK, true).await.unwrap();
    // Marked incomplete with nothing ever linked: turned off too, with no history to name.
    fixture
        .state
        .store
        .set_account_sharing(account_id(AgentId::CODEX), true, true)
        .unwrap();

    let done = turn_off_all(&fixture.state).await.unwrap();
    assert!(done.failed.is_empty(), "{:?}", done.failed);
    let mut turned_off: Vec<(&str, Option<&SharedHistory>)> = done
        .turned_off
        .iter()
        .map(|off| (off.account.id.as_str(), off.history.as_ref()))
        .collect();
    turned_off.sort_by_key(|(id, _)| *id);
    let claude_default = real(&fixture.default(AgentId::CLAUDE));
    let grok_default = real(&fixture.default(AgentId::GROK));
    let [("claude-a", Some(claude)), ("codex-a", None), ("grok-a", Some(grok))] =
        turned_off.as_slice()
    else {
        panic!("{turned_off:?}");
    };
    assert_eq!(claude.default_dir, claude_default);
    assert!(claude.entries.contains(&"projects") && claude.entries.contains(&"history.jsonl"));
    assert!(!claude.entries.contains(&"settings.json"));
    assert_eq!(
        **grok,
        SharedHistory {
            default_dir: grok_default,
            entries: vec!["sessions"],
        }
    );
    for agent in [AgentId::CLAUDE, AgentId::CODEX, AgentId::GROK] {
        let record = fixture.record(agent);
        assert!(
            !record.share_default_config && !record.share_default_config_incomplete,
            "{agent:?}"
        );
    }
    assert!(!is_link(
        &fixture.account(AgentId::CLAUDE).join("settings.json")
    ));
    assert!(!is_link(&fixture.account(AgentId::GROK).join("sessions")));
    // Nothing shares any more: a second call has nothing to do.
    let again = turn_off_all(&fixture.state).await.unwrap();
    assert!(again.turned_off.is_empty() && again.failed.is_empty());
}

/// An account whose record cannot be read is answered with that failure, and the others are
/// turned off all the same.
#[tokio::test]
async fn an_account_whose_record_cannot_be_read_does_not_keep_the_others_from_being_turned_off() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    fixture.share(AgentId::CLAUDE, true).await.unwrap();
    fixture.share(AgentId::GROK, true).await.unwrap();
    fixture
        .state
        .store
        .damage_account_sharing_start(account_id(AgentId::CLAUDE));

    let done = turn_off_all(&fixture.state).await.unwrap();
    let [(account, _)] = done.failed.as_slice() else {
        panic!("{:?}", done.failed);
    };
    assert_eq!(account.id, "claude-a");
    let [off] = done.turned_off.as_slice() else {
        panic!("{:?}", done.turned_off);
    };
    assert_eq!(off.account.id, "grok-a");
    assert!(!fixture.record(AgentId::GROK).share_default_config);
    assert!(fixture.record(AgentId::CLAUDE).share_default_config);
}

/// An account that cannot stop sharing now is answered with its refusal, and the others are
/// turned off all the same.
#[tokio::test]
async fn an_account_in_use_does_not_keep_the_others_from_being_turned_off() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    fixture.share(AgentId::CLAUDE, true).await.unwrap();
    fixture.share(AgentId::GROK, true).await.unwrap();
    fixture.session("working", AgentId::CLAUDE);
    let _running = test_support::running(&fixture.state, "working");

    let done = turn_off_all(&fixture.state).await.unwrap();
    let [(account, err)] = done.failed.as_slice() else {
        panic!("{:?}", done.failed);
    };
    assert_eq!(account.id, "claude-a");
    assert_eq!(code_of(err), crate::protocol::error_code::ACCOUNT_IN_USE);
    let [off] = done.turned_off.as_slice() else {
        panic!("{:?}", done.turned_off);
    };
    assert_eq!(off.account.id, "grok-a");
    assert!(fixture.record(AgentId::CLAUDE).share_default_config);
    assert!(!fixture.record(AgentId::GROK).share_default_config);
}

/// The accounts the clean-up before an uninstall lists, as the daemon answers its local route.
async fn listed(fixture: &Fixture) -> Vec<SystemChange> {
    system_changes::listed(&fixture.state).unwrap().changes
}

fn sharing(fixture: &Fixture, agent: AgentId) -> SystemChange {
    SystemChange::AccountSharing {
        account: account_id(agent).to_string(),
        name: fixture.record(agent).name,
        agent,
        config_dir: fixture.account(agent).to_string_lossy().into_owned(),
    }
}

/// The clean-up lists every account that shares and every account whose change is incomplete, and
/// no other; clearing turns all of them off, names the history that stays, and leaves nothing
/// listed.
#[tokio::test]
async fn the_clean_up_lists_the_accounts_that_share_and_clears_them_all() {
    let fixture = Fixture::new();
    assert!(listed(&fixture).await.is_empty());
    claude_setup(&fixture);
    fixture.share(AgentId::CLAUDE, true).await.unwrap();
    // Marked incomplete with nothing ever linked: listed too, since its switch would undo it.
    fixture
        .state
        .store
        .set_account_sharing(account_id(AgentId::CODEX), true, true)
        .unwrap();
    let mut before = listed(&fixture).await;
    before.sort_by_key(|change| match change {
        SystemChange::AccountSharing { account, .. } => account.clone(),
    });
    assert_eq!(
        before,
        [
            sharing(&fixture, AgentId::CLAUDE),
            sharing(&fixture, AgentId::CODEX)
        ]
    );

    let system_changes::SystemChangesCleared {
        results, failed, ..
    } = system_changes::cleared(&fixture.state).await;
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|result| result.failure.is_none()));
    let claude = results
        .iter()
        .find(|result| result.change == sharing(&fixture, AgentId::CLAUDE))
        .unwrap();
    let Some(SystemChangeKept { dir, entries }) = &claude.kept else {
        panic!("{claude:?}");
    };
    assert_eq!(
        Path::new(dir),
        real(&fixture.default(AgentId::CLAUDE)).as_path()
    );
    assert!(entries.iter().any(|entry| entry == "projects"));
    let codex = results
        .iter()
        .find(|result| result.change == sharing(&fixture, AgentId::CODEX))
        .unwrap();
    assert_eq!(codex.kept, None);
    assert!(listed(&fixture).await.is_empty());
}

/// An account that cannot stop sharing now is answered with its refusal after the ones cleared,
/// which are cleared all the same, and it stays listed.
#[tokio::test]
async fn an_account_the_clean_up_cannot_clear_is_answered_with_its_refusal_and_stays_listed() {
    let fixture = Fixture::new();
    claude_setup(&fixture);
    fixture.share(AgentId::CLAUDE, true).await.unwrap();
    fixture.share(AgentId::GROK, true).await.unwrap();
    fixture.session("working", AgentId::CLAUDE);
    let _running = test_support::running(&fixture.state, "working");

    let system_changes::SystemChangesCleared {
        results, failed, ..
    } = system_changes::cleared(&fixture.state).await;
    assert!(failed.is_empty(), "{failed:?}");
    let [cleared, left] = results.as_slice() else {
        panic!("{results:?}");
    };
    assert_eq!(cleared.change, sharing(&fixture, AgentId::GROK));
    assert_eq!(cleared.failure, None);
    assert_eq!(left.change, sharing(&fixture, AgentId::CLAUDE));
    assert_eq!(
        left.failure.as_ref().map(|failure| failure.code),
        Some(crate::protocol::error_code::ACCOUNT_IN_USE)
    );
    assert_eq!(left.kept, None);
    assert_eq!(listed(&fixture).await, [sharing(&fixture, AgentId::CLAUDE)]);
}
