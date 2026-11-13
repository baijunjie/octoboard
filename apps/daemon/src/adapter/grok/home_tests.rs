//! Tests of the per-session `GROK_HOME` farm.

use std::path::{Path, PathBuf};

use super::tests::{plan_isolated, with_default_home};
use super::*;
use crate::adapter::tests::spec_fixture;
use crate::adapter::COMMAND_HOOK_TIMEOUT_SECS;
use crate::providers::grok::GROK_TRUST_STORE;

#[test]
fn the_farm_links_the_users_home_but_owns_the_files_grok_writes_back_to() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let plan = plan_isolated(&fixture.spec()).expect("plan");

    let farm = PathBuf::from(&plan.env[0].1);
    assert_eq!(plan.env[0].0, "GROK_HOME");

    // Login state and session records stay shared with the user's own `grok`.
    assert!(farm.join("auth.json").is_symlink());
    assert!(farm.join("sessions").is_symlink());
    // Grok persists session settings into `config.toml`, so against a symlink that write would
    // reach the user's own file.
    assert!(!farm.join("config.toml").is_symlink());
    assert!(std::fs::read_to_string(farm.join("config.toml"))
        .expect("the copy")
        .starts_with(&std::fs::read_to_string(home.join("config.toml")).expect("the original")));
    // The deployment sync deletes these at runtime, so the farm must not carry them.
    assert!(!farm.join("managed_config.toml").exists());
    assert!(!farm.join("requirements.toml").exists());
}

/// The farm's trust store is the user's, copied as it is: the project is not written into it,
/// so Grok shows its own confirmation for a folder the user has not trusted, and the plan names
/// where the entry Grok writes on a press is carried from and to.
#[test]
fn the_farms_trust_store_is_the_users_copied_without_the_project_added() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let users = "[folders.\"/somewhere/else\"]\ntrusted = true\n";
    std::fs::write(home.join(GROK_TRUST_STORE), users).expect("trust store");

    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);
    assert!(!farm.join(GROK_TRUST_STORE).is_symlink());
    assert_eq!(
        std::fs::read_to_string(farm.join(GROK_TRUST_STORE)).expect("the copy"),
        users
    );
    let carried = plan.carried_trust.expect("Grok's trust is carried");
    assert_eq!(carried.user_store(), home.join(GROK_TRUST_STORE));
    // The entry Grok writes into the session's copy for the canonical working directory is the one
    // carried into the user's store.
    let canonical = crate::hostfs::canonical(&fixture.cwd).expect("canonical path");
    // A TOML basic string, where a backslash (Windows' separator) is written escaped.
    let key = canonical.to_string_lossy().replace('\\', "\\\\");
    let entry = format!("\n[folders.\"{key}\"]\ntrusted = true\n");
    std::fs::write(farm.join(GROK_TRUST_STORE), format!("{users}{entry}")).expect("recorded");
    carried.carry().expect("carried");
    assert_eq!(
        std::fs::read_to_string(home.join(GROK_TRUST_STORE)).expect("the user's store"),
        format!("{users}{entry}")
    );
}

#[cfg(unix)]
#[test]
fn the_farms_config_is_readable_by_its_owner_only_even_when_the_users_is_not() {
    use std::os::unix::fs::PermissionsExt;

    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let users = home.join("config.toml");
    std::fs::set_permissions(&users, std::fs::Permissions::from_mode(0o644)).expect("mode");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);
    let mode = |path: &Path| {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode(&farm.join("config.toml")), 0o600);

    // With no config of the user's to copy, the file that carries the server is created alone.
    std::fs::remove_file(&users).expect("remove");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);
    assert_eq!(mode(&farm.join("config.toml")), 0o600);
}

#[test]
fn the_mcp_server_is_appended_to_the_farms_config_without_losing_the_users_own() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);
    let config = std::fs::read_to_string(farm.join("config.toml")).expect("the copy");
    assert!(
        config.contains("default_model = \"x\""),
        "the user's own configuration survives"
    );
    assert!(config.contains("[mcp_servers.octoboard]"));
    assert!(config.contains(&fixture.self_exe));
    assert!(config.contains(&fixture.mcp_token));
}

/// The farm's files are Grok's to read and nobody's to list: the MCP server is registered with the
/// file holding the key, and the hook file names the hook script, neither holding the key.
#[test]
fn the_farm_names_the_key_file_and_holds_no_key() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);
    let config: toml::Table = std::fs::read_to_string(farm.join("config.toml"))
        .expect("the copy")
        .parse()
        .expect("TOML");
    let args: Vec<&str> = config["mcp_servers"][session_tools::catalogue::SERVER_KEY]["args"]
        .as_array()
        .expect("the server's arguments")
        .iter()
        .map(|arg| arg.as_str().expect("a string"))
        .collect();
    let at = args
        .iter()
        .position(|arg| *arg == "--key-file")
        .expect("--key-file");
    assert_eq!(Path::new(args[at + 1]), fixture.key_file);
    assert!(!args.contains(&"--key"), "{args:?}");
    for file in ["config.toml", "hooks/octoboard.json"] {
        let content = std::fs::read_to_string(farm.join(file)).expect("the file");
        assert!(!content.contains("feedface"), "{file}: {content}");
    }
}

/// Grok cancels a tool call it gives up on, and a cancellation withdraws a waiting request for
/// a console session, so its limit is set above Octoboard's forwarding limit, for this server
/// only. The ordering itself is asserted where the value is defined.
#[test]
fn the_servers_tool_timeout_sits_above_octoboards_own_call_limit() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);
    let config = std::fs::read_to_string(farm.join("config.toml")).expect("the copy");
    // Read out of Octoboard's own table rather than from anywhere in the file: emitted above
    // its header the key would be a top-level one, setting the limit for every MCP server the
    // user has.
    let table = config
        .split(&format!(
            "[mcp_servers.{}]\n",
            session_tools::catalogue::SERVER_KEY
        ))
        .nth(1)
        .expect("Octoboard's table")
        .split("\n[")
        .next()
        .expect("its end");
    let emitted: u64 = table
        .lines()
        .find_map(|line| line.strip_prefix("tool_timeout_sec = "))
        .expect("the limit")
        .parse()
        .expect("whole seconds");
    assert_eq!(emitted, 660);
}

#[test]
fn the_hooks_are_command_type_with_an_explicit_timeout() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);
    let document: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(farm.join("hooks").join("octoboard.json")).expect("hooks"),
    )
    .expect("the hooks file is JSON");
    let hooks = document["hooks"].as_object().expect("a hooks table");
    assert_eq!(hooks.len(), HOOK_EVENTS.len());
    for event in HOOK_EVENTS {
        let handler = &hooks[*event][0]["hooks"][0];
        // HTTP hooks cannot reach the daemon at all: Grok's SSRF protection rejects plain HTTP
        // and private addresses alike.
        assert_eq!(handler["type"], "command");
        // Grok defaults `Stop`, `SubagentStop` and `PostToolUse` to 600 s, which would stall
        // the UI for ten minutes on a wedged hook.
        assert_eq!(handler["timeout"], COMMAND_HOOK_TIMEOUT_SECS);
    }
}

/// The cross-project guard is a group of its own after the status hook of `PreToolUse`, matching
/// the edit tools, and with no `failClosed` (a guard that cannot decide stays silent); a launch
/// without one has neither.
#[test]
fn the_cross_project_guard_is_a_group_for_the_edit_tools_that_does_not_fail_closed() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let list = fixture.scratch.join("cross-project-guard.json");
    let script = fixture.scratch.join("guard-hook");
    let hooks_of = |guard: Option<crate::adapter::GuardHook<'_>>| {
        let mut spec = fixture.spec();
        spec.guard = guard;
        let plan = plan_isolated(&spec).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);
        let document: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(farm.join("hooks").join("octoboard.json")).expect("hooks"),
        )
        .expect("the hooks file is JSON");
        document["hooks"].clone()
    };

    let hooks = hooks_of(Some(crate::adapter::GuardHook {
        exe: "/opt/octoboard/install/octoboardd",
        list: &list,
        script: &script,
    }));

    let groups = hooks["PreToolUse"].as_array().expect("groups");
    assert_eq!(groups.len(), 2, "{groups:?}");
    assert!(groups[0].get("matcher").is_none());
    let matcher = groups[1]["matcher"].as_str().expect("a matcher");
    for tool in [
        "Edit",
        "Write",
        "MultiEdit",
        "NotebookEdit",
        "search_replace",
        "write",
        "hashline_edit",
        "StrReplace",
        "EditNotebook",
    ] {
        assert!(
            matcher.split('|').any(|name| name == tool),
            "{tool}: {matcher}"
        );
    }
    let handler = &groups[1]["hooks"][0];
    assert_eq!(handler["type"], "command");
    assert_eq!(handler["timeout"], crate::adapter::GUARD_HOOK_TIMEOUT_SECS);
    assert_eq!(handler["timeout"], 10);
    assert!(handler.get("async").is_none(), "synchronous: {handler}");
    assert!(
        !groups[1].to_string().contains("failClosed"),
        "{}",
        groups[1]
    );
    // The bare path of the per-session script, as the status hook's command is.
    assert_eq!(handler["command"], script.to_string_lossy().as_ref());
    for event in HOOK_EVENTS.iter().filter(|event| **event != "PreToolUse") {
        assert_eq!(hooks[*event].as_array().unwrap().len(), 1, "{event}");
    }

    let hooks = hooks_of(None);
    assert_eq!(hooks["PreToolUse"].as_array().unwrap().len(), 1);
    assert!(!hooks.to_string().contains("guard-hook"));
}

#[test]
fn the_entries_grok_protects_under_its_sandbox_are_real_copies() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    std::fs::create_dir_all(home.join("bundled/nested")).expect("bundled");
    std::fs::write(home.join("bundled/nested/tool"), "tool").expect("bundled file");
    std::fs::write(home.join("bundled/top"), "top").expect("bundled file");
    std::fs::create_dir_all(home.join("bundles")).expect("bundles");
    std::fs::write(home.join("hooks-paths"), "/some/hooks\n").expect("hooks-paths");
    std::fs::write(home.join("trusted_folders.toml"), "").expect("trust store");

    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);

    for name in ["bundled", "bundles", "hooks-paths"] {
        assert!(!farm.join(name).is_symlink(), "{name}");
    }
    assert!(farm.join("bundled").is_dir());
    assert!(farm.join("bundles").is_dir());
    assert_eq!(
        std::fs::read_to_string(farm.join("bundled/nested/tool")).expect("nested copy"),
        "tool"
    );
    assert_eq!(
        std::fs::read_to_string(farm.join("bundled/top")).expect("copy"),
        "top"
    );
    assert_eq!(
        std::fs::read_to_string(farm.join("hooks-paths")).expect("copy"),
        "/some/hooks\n"
    );
    // Absent in the source stays absent, for Grok to create.
    assert!(!farm.join("sandbox.toml").exists());
    assert!(!farm.join("sandbox.toml").is_symlink());
    // Writing into the copy never reaches the source.
    std::fs::write(farm.join("bundled/top"), "changed").expect("write");
    assert_eq!(
        std::fs::read_to_string(home.join("bundled/top")).expect("source"),
        "top"
    );
    // Everything else is as before.
    assert!(farm.join("auth.json").is_symlink());
    assert!(farm.join("sessions").is_symlink());
    assert!(!farm.join("config.toml").is_symlink());
    assert!(!farm.join(GROK_TRUST_STORE).is_symlink());
}

/// Grok checks that the entry is a real directory, so a source `bundled` that is a link becomes
/// one in the farm; a link inside it stays a link rather than being followed.
#[test]
fn a_linked_source_directory_becomes_a_real_one_and_inner_links_stay_links() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let elsewhere = fixture.scratch.join("elsewhere");
    std::fs::create_dir_all(elsewhere.join("sub")).expect("target directory");
    std::fs::write(elsewhere.join("sub/tool"), "tool").expect("file");
    crate::test_support::symlink("sub/tool", elsewhere.join("inner")).expect("inner link");
    crate::test_support::symlink(&elsewhere, home.join("bundled")).expect("link");

    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);

    assert!(!farm.join("bundled").is_symlink());
    assert!(farm.join("bundled").is_dir());
    assert_eq!(
        std::fs::read_to_string(farm.join("bundled/sub/tool")).expect("copy"),
        "tool"
    );
    assert_eq!(
        std::fs::read_link(farm.join("bundled/inner")).expect("still a link"),
        Path::new("sub/tool")
    );
}

/// A sharing account's entries are links into the default directory; the farm links straight to
/// the final target rather than through the account's link, however long the chain.
#[test]
fn a_source_entry_that_is_a_link_is_reached_through_one_link() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let default = fixture.scratch.join("default-home");
    std::fs::create_dir_all(default.join("skills")).expect("default directory");
    std::fs::write(default.join("AGENTS.md"), "shared").expect("default file");
    crate::test_support::symlink(default.join("AGENTS.md"), home.join("AGENTS.md"))
        .expect("file link");
    crate::test_support::symlink(default.join("skills"), home.join("skills"))
        .expect("directory link");
    let middle = fixture.scratch.join("middle");
    crate::test_support::symlink(default.join("AGENTS.md"), &middle).expect("first hop");
    crate::test_support::symlink(&middle, home.join("chained.md")).expect("second hop");

    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);

    let file = crate::hostfs::canonical(default.join("AGENTS.md")).expect("canonical path");
    assert_eq!(
        std::fs::read_link(farm.join("AGENTS.md")).expect("a link"),
        file
    );
    assert_eq!(
        std::fs::read_link(farm.join("chained.md")).expect("a link"),
        file
    );
    assert_eq!(
        std::fs::read_link(farm.join("skills")).expect("a link"),
        crate::hostfs::canonical(default.join("skills")).expect("canonical path")
    );
    assert_eq!(
        std::fs::read_to_string(farm.join("AGENTS.md")).expect("read"),
        "shared"
    );
}

#[test]
fn a_plain_source_entry_is_linked_to_itself() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    std::fs::write(home.join("AGENTS.md"), "own").expect("file");

    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);

    assert_eq!(
        std::fs::read_link(farm.join("AGENTS.md")).expect("a link"),
        crate::hostfs::canonical(home.join("AGENTS.md")).expect("canonical path")
    );
}

/// With nothing to resolve to, the farm link keeps pointing at the source entry.
#[test]
fn a_dangling_source_link_is_linked_to_the_source_entry() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    crate::test_support::symlink(fixture.scratch.join("gone"), home.join("AGENTS.md"))
        .expect("dangling link");

    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);

    assert_eq!(
        std::fs::read_link(farm.join("AGENTS.md")).expect("a link"),
        home.join("AGENTS.md")
    );
}

/// Copies are taken from the source entries' contents, so links in the source change nothing
/// about which entries are real.
#[test]
fn linked_source_entries_still_yield_real_copies_of_the_owned_files() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let default = fixture.scratch.join("default-home");
    std::fs::create_dir_all(default.join("bundles")).expect("default directory");
    std::fs::write(default.join("bundles/one"), "1").expect("bundle");
    std::fs::write(default.join("hooks-paths"), "/h\n").expect("hooks-paths");
    std::fs::write(default.join("sandbox.toml"), "[profiles.p]\n").expect("sandbox");
    std::fs::write(default.join(GROK_TRUST_STORE), "[folders.\"/t\"]\n").expect("trust");
    std::fs::remove_file(home.join("config.toml")).expect("replace config");
    std::fs::write(default.join("config.toml"), "default_model = \"d\"\n").expect("config");
    for name in [
        "bundles",
        "hooks-paths",
        "sandbox.toml",
        GROK_TRUST_STORE,
        "config.toml",
    ] {
        crate::test_support::symlink(default.join(name), home.join(name)).expect("link");
    }

    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);

    for name in [
        "bundles",
        "hooks-paths",
        "sandbox.toml",
        GROK_TRUST_STORE,
        "config.toml",
    ] {
        assert!(!farm.join(name).is_symlink(), "{name}");
    }
    assert_eq!(
        std::fs::read_to_string(farm.join("bundles/one")).expect("copy"),
        "1"
    );
    assert_eq!(
        std::fs::read_to_string(farm.join("hooks-paths")).expect("copy"),
        "/h\n"
    );
    assert_eq!(
        std::fs::read_to_string(farm.join("sandbox.toml")).expect("copy"),
        "[profiles.p]\n"
    );
    assert_eq!(
        std::fs::read_to_string(farm.join(GROK_TRUST_STORE)).expect("copy"),
        "[folders.\"/t\"]\n"
    );
    assert!(std::fs::read_to_string(farm.join("config.toml"))
        .expect("copy")
        .contains("default_model = \"d\""));
}

#[test]
fn a_sandbox_profile_file_is_copied_too() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    std::fs::write(home.join("sandbox.toml"), "[profiles.p]\n").expect("sandbox.toml");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);
    assert!(!farm.join("sandbox.toml").is_symlink());
    assert_eq!(
        std::fs::read_to_string(farm.join("sandbox.toml")).expect("copy"),
        "[profiles.p]\n"
    );
}

/// Sets [`super::home::FILE_LINKS_OVERRIDE`] for as long as it is held.
pub(super) struct FileLinks;

impl FileLinks {
    pub(super) fn set(available: bool) -> Self {
        super::home::FILE_LINKS_OVERRIDE.set(Some(available));
        Self
    }
}

impl Drop for FileLinks {
    fn drop(&mut self) {
        super::home::FILE_LINKS_OVERRIDE.set(None);
    }
}

/// The value of `name` in the plan's environment.
fn env_value<'a>(plan: &'a LaunchPlan, name: &str) -> Option<&'a str> {
    plan.env
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// A source home with the entries Grok Build 1.0.77 leaves in one: locks beside the files they
/// guard, a temporary file a write left, a SQLite database with its index, state and caches.
fn with_a_lived_in_home(fixture: &mut crate::adapter::tests::SpecFixture) -> PathBuf {
    let home = with_default_home(fixture);
    std::fs::create_dir_all(home.join("skills")).expect("skills");
    for (name, text) in [
        ("auth.json.lock", "50843:1794022920"),
        (".config-init.lock", ""),
        ("active_sessions.json", "[]"),
        ("active_sessions.lock", ""),
        ("managed_config.lock", ""),
        (".59335.0.tmp", "half"),
        ("agent_id", "id"),
        ("worktrees.db", "database"),
        ("worktrees.db-wal", "log"),
        ("worktrees.db-shm", "index"),
        (GROK_TRUST_STORE, ""),
        ("trusted_folders.toml.lock", ""),
    ] {
        std::fs::write(home.join(name), text).expect(name);
    }
    home
}

/// Without file links, a launch no longer fails on the first file it would link: directories are
/// still linked, every other file is the session's own copy, and what no copy may share — the
/// login, locks, temporary files, SQLite's index — is left out.
#[test]
fn without_file_links_the_farm_copies_files_and_leaves_out_locks_and_the_login() {
    let _links = FileLinks::set(false);
    let mut fixture = spec_fixture();
    let home = with_a_lived_in_home(&mut fixture);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);

    for name in ["sessions", "skills"] {
        assert!(farm.join(name).is_symlink(), "{name}");
        assert!(farm.join(name).is_dir(), "{name}");
    }
    for (name, text) in [
        ("active_sessions.json", "[]"),
        ("agent_id", "id"),
        ("worktrees.db", "database"),
        ("worktrees.db-wal", "log"),
    ] {
        assert!(!farm.join(name).is_symlink(), "{name}");
        assert_eq!(std::fs::read_to_string(farm.join(name)).expect(name), text);
    }
    for name in [
        "auth.json",
        "auth.json.lock",
        ".config-init.lock",
        "active_sessions.lock",
        "managed_config.lock",
        "trusted_folders.toml.lock",
        ".59335.0.tmp",
        "worktrees.db-shm",
        "managed_config.toml",
    ] {
        assert!(
            std::fs::symlink_metadata(farm.join(name)).is_err(),
            "{name}"
        );
    }
    for entry in std::fs::read_dir(&farm).expect("farm") {
        let path = entry.expect("entry").path();
        assert!(!path.is_symlink() || path.is_dir(), "{}", path.display());
    }
    // A copy is the session's own: what Grok writes there stays out of the user's home.
    std::fs::write(farm.join("agent_id"), "changed").expect("write");
    assert_eq!(
        std::fs::read_to_string(home.join("agent_id")).expect("source"),
        "id"
    );
    // The files the farm owns are as on any host.
    assert!(std::fs::read_to_string(farm.join("config.toml"))
        .expect("config")
        .contains("[mcp_servers.octoboard]"));
    assert!(farm.join("hooks").join("octoboard.json").is_file());
    assert!(farm.join(GROK_TRUST_STORE).is_file());
}

/// Without file links Grok is pointed at the user's own login rather than at a copy, so a sign-in
/// or a refresh it saves is the user's at once, with nothing to write back; with no login yet, at
/// the place a sign-in creates it.
#[test]
fn without_file_links_grok_saves_its_login_in_the_users_own_file() {
    let _links = FileLinks::set(false);
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let store = PathBuf::from(env_value(&plan, "GROK_AUTH_PATH").expect("the login store"));
    assert_eq!(
        store,
        crate::hostfs::canonical(home.join("auth.json")).expect("canonical path")
    );
    std::fs::write(&store, "{\"refreshed\":true}").expect("Grok's save");
    assert_eq!(
        std::fs::read_to_string(home.join("auth.json")).expect("the user's login"),
        "{\"refreshed\":true}"
    );

    std::fs::remove_file(home.join("auth.json")).expect("signed out");
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    assert_eq!(
        env_value(&plan, "GROK_AUTH_PATH").map(PathBuf::from),
        Some(home.join("auth.json"))
    );
}

/// A login store the user's shell already names reaches Grok through the launch environment, and
/// is left to it.
#[test]
fn a_login_store_the_shell_names_is_left_as_it_is() {
    let _links = FileLinks::set(false);
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    fixture.shell_env.insert(
        "GROK_AUTH_PATH".to_string(),
        "/elsewhere/auth.json".to_string(),
    );
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    assert_eq!(env_value(&plan, "GROK_AUTH_PATH"), None);
}

/// Where the host makes file links nothing changes: the login and every lock beside a linked
/// file stay linked, so the session's Grok and the user's own take the same lock before writing
/// the same file, and no login store is named. A SQLite database is linked only where the host's
/// SQLite follows the link to name its log.
#[test]
fn with_file_links_the_login_and_its_locks_stay_linked() {
    let _links = FileLinks::set(true);
    let mut fixture = spec_fixture();
    with_a_lived_in_home(&mut fixture);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);

    for name in [
        "auth.json",
        "auth.json.lock",
        ".config-init.lock",
        "active_sessions.json",
        "active_sessions.lock",
        "managed_config.lock",
        "trusted_folders.toml.lock",
        "agent_id",
        "sessions",
    ] {
        assert!(farm.join(name).is_symlink(), "{name}");
    }
    assert_eq!(env_value(&plan, "GROK_AUTH_PATH"), None);
    if crate::platform::fs::SQLITE_FOLLOWS_FILE_ALIASES {
        for name in ["worktrees.db", "worktrees.db-wal", "worktrees.db-shm"] {
            assert!(farm.join(name).is_symlink(), "{name}");
        }
    } else {
        for name in ["worktrees.db", "worktrees.db-wal"] {
            assert!(farm.join(name).is_file() && !farm.join(name).is_symlink());
        }
        assert!(std::fs::symlink_metadata(farm.join("worktrees.db-shm")).is_err());
    }
}

/// A copied directory carries no lock or temporary file on any host. Without file links, a link
/// inside it to a file becomes a copy of that file, and one to a directory is made again to where
/// it resolves.
#[test]
fn a_copied_directory_without_file_links_leaves_out_groks_locks_and_follows_its_links() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let bundled = home.join("bundled");
    std::fs::create_dir_all(bundled.join("skills")).expect("bundled");
    std::fs::write(bundled.join("manifest.json"), "{}").expect("manifest");
    let grok_scratch = [".lock", "manifest.json.lock", ".tmp2tPTzx", ".59335.0.tmp"];
    let users_files = [
        "uv.lock",
        "Cargo.lock",
        "yarn.lock",
        "bun.lock",
        ".tmplate",
        ".tmpl2tPTz",
        "notes.tmp",
    ];
    for name in grok_scratch.iter().chain(&users_files) {
        std::fs::write(bundled.join(name), "").expect(name);
    }
    crate::test_support::symlink("manifest.json", bundled.join("linked.json")).expect("file link");
    crate::test_support::symlink("skills", bundled.join("linked-skills")).expect("directory link");

    for file_links in [true, false] {
        let _links = FileLinks::set(file_links);
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let copy = PathBuf::from(&plan.env[0].1).join("bundled");
        assert!(!copy.is_symlink());
        assert!(copy.join("manifest.json").is_file());
        // Where the host makes file links, the copy is the whole directory, as it always was.
        for name in grok_scratch {
            assert_eq!(copy.join(name).is_file(), file_links, "{name}");
        }
        for name in users_files {
            assert!(copy.join(name).is_file(), "{name}");
        }
        if file_links {
            assert_eq!(
                std::fs::read_link(copy.join("linked.json")).expect("a link"),
                Path::new("manifest.json")
            );
        } else {
            assert!(!copy.join("linked.json").is_symlink());
            assert_eq!(
                std::fs::read_to_string(copy.join("linked.json")).expect("copy"),
                "{}"
            );
            assert_eq!(
                crate::hostfs::canonical(copy.join("linked-skills")).expect("resolves"),
                crate::hostfs::canonical(bundled.join("skills")).expect("canonical path")
            );
        }
    }
}

/// Which source entries are linked, copied or left out, by name and host.
#[test]
fn entries_are_carried_by_what_they_are_and_what_the_host_links() {
    use super::home::{carried, Carried};

    for name in [
        "auth.json.lock",
        ".config-init.lock",
        ".59335.0.tmp",
        "x.sock",
        "auth.json",
    ] {
        assert_eq!(carried(name, false, false), Carried::Left, "{name}");
        assert_eq!(carried(name, false, true), Carried::Linked, "{name}");
    }
    for name in [
        "agent_id",
        "models_cache.json",
        "worktrees.db",
        "worktrees.db-wal",
    ] {
        assert_eq!(carried(name, false, false), Carried::Copied, "{name}");
    }
    assert_eq!(carried("worktrees.db-shm", false, false), Carried::Left);
    for name in ["sessions", "skills", "x.lock"] {
        assert_eq!(carried(name, true, false), Carried::Linked, "{name}");
    }
    let database = if crate::platform::fs::SQLITE_FOLLOWS_FILE_ALIASES {
        [Carried::Linked; 3]
    } else {
        [Carried::Copied, Carried::Copied, Carried::Left]
    };
    for (name, expected) in ["worktrees.db", "state.sqlite-journal", "worktrees.db-shm"]
        .into_iter()
        .zip(database)
    {
        assert_eq!(carried(name, false, true), expected, "{name}");
    }
}
