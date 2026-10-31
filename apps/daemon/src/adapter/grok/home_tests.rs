//! Tests of the per-session `GROK_HOME` farm.

use std::path::{Path, PathBuf};

use super::tests::{plan_isolated, with_default_home};
use super::*;
use crate::adapter::tests::spec_fixture;
use crate::adapter::HOOK_TIMEOUT_SECS;
use crate::agents::layout::GROK_TRUST_STORE;

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
    assert_eq!(carried.session_store, farm.join(GROK_TRUST_STORE));
    assert_eq!(carried.user_store, home.join(GROK_TRUST_STORE));
    let canonical = std::fs::canonicalize(&fixture.cwd).expect("canonical path");
    assert_eq!(carried.folder, canonical.to_string_lossy());
}

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
        .split(&format!("[mcp_servers.{}]\n", mcp::SERVER_KEY))
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
        assert_eq!(handler["timeout"], HOOK_TIMEOUT_SECS);
    }
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
    std::os::unix::fs::symlink("sub/tool", elsewhere.join("inner")).expect("inner link");
    std::os::unix::fs::symlink(&elsewhere, home.join("bundled")).expect("link");

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
    std::os::unix::fs::symlink(default.join("AGENTS.md"), home.join("AGENTS.md"))
        .expect("file link");
    std::os::unix::fs::symlink(default.join("skills"), home.join("skills"))
        .expect("directory link");
    let middle = fixture.scratch.join("middle");
    std::os::unix::fs::symlink(default.join("AGENTS.md"), &middle).expect("first hop");
    std::os::unix::fs::symlink(&middle, home.join("chained.md")).expect("second hop");

    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);

    let file = std::fs::canonicalize(default.join("AGENTS.md")).expect("canonical path");
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
        std::fs::canonicalize(default.join("skills")).expect("canonical path")
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
        std::fs::canonicalize(home.join("AGENTS.md")).expect("canonical path")
    );
}

/// With nothing to resolve to, the farm link keeps pointing at the source entry.
#[test]
fn a_dangling_source_link_is_linked_to_the_source_entry() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    std::os::unix::fs::symlink(fixture.scratch.join("gone"), home.join("AGENTS.md"))
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
        std::os::unix::fs::symlink(default.join(name), home.join(name)).expect("link");
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
