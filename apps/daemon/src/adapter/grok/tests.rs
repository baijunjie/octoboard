//! Tests of Grok's launch plan, terminal titles and pinned homes; also the helpers the sibling test
//! modules share.

use std::path::PathBuf;

use super::*;
use crate::adapter::tests::{spec_fixture, SpecFixture};

#[test]
fn native_download_versions_are_read_through_the_launcher_symlink() {
    let scratch = crate::test_support::ScratchDir::new("grok-installed-version");
    let release = scratch.join("grok-1.0.70-macos-aarch64");
    std::fs::write(&release, "not executed").unwrap();
    let launcher = scratch.join("grok");
    std::os::unix::fs::symlink(&release, &launcher).unwrap();
    assert_eq!(
        GrokAdapter.installed_version(launcher.to_str().unwrap()),
        Some((1, 0, 70))
    );
    for filename in [
        "grok-1.0.70",
        "grok-1.0.70.1-macos-aarch64",
        "grok-1.0.70-unknown",
        "grok-unknown-macos-aarch64",
    ] {
        let release = scratch.join(filename);
        std::fs::write(&release, "not executed").unwrap();
        assert_eq!(
            GrokAdapter.installed_version(release.to_str().unwrap()),
            None
        );
    }
}

#[test]
fn terminal_titles_lose_the_spinner_the_status_and_groks_own_name() {
    let task = |raw: &str| GrokAdapter.task_title(raw, &[]);
    for raw in [
        "grok",
        "⠋ - Waiting for response… - grok",
        "- Waiting for response… - grok",
        "",
    ] {
        assert_eq!(task(raw), None, "{raw}");
    }
    let title = "Explain in one sentence what a pseudo te…";
    assert_eq!(
        task(&format!("⠋ - Waiting for response… - {title} - grok")).as_deref(),
        Some(title)
    );
    assert_eq!(
        task(&format!("- Waiting for response… - {title} - grok")).as_deref(),
        Some(title),
        "the spinner may already be taken off"
    );
    assert_eq!(task(&format!("{title} - grok")).as_deref(), Some(title));
    assert_eq!(
        task("⠋ - Fix the build - grok").as_deref(),
        Some("Fix the build"),
        "an item that is no status stays"
    );
    assert_eq!(
        task("⠋ - Explain what a pseudo te… - grok").as_deref(),
        Some("Explain what a pseudo te…"),
        "a lone truncated title is not a status"
    );
}

/// `plan` against an empty system-wide configuration directory of the fixture's own.
pub(super) fn plan_isolated(spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
    plan_under(spec, &spec.scratch.join("etc-grok"))
}

/// Stands a fake `~/.grok` up inside the fixture and points the launch environment at it.
pub(super) fn with_default_home(fixture: &mut SpecFixture) -> PathBuf {
    let home = fixture.scratch.join("user-grok-home");
    std::fs::create_dir_all(home.join("sessions")).expect("sessions directory");
    std::fs::write(home.join("config.toml"), "default_model = \"x\"\n").expect("config");
    std::fs::write(home.join("auth.json"), "{}").expect("auth");
    std::fs::write(home.join("managed_config.toml"), "").expect("managed config");
    fixture
        .shell_env
        .insert("GROK_HOME".to_string(), home.to_string_lossy().into_owned());
    home
}

/// The terminal's appearance chooses Grok's theme. With none sent, Grok follows the operating
/// system rather than its compiled dark default.
#[test]
fn the_terminal_appearance_sets_grok_theme() {
    let theme = |appearance| {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        fixture.terminal_appearance = appearance;
        plan_isolated(&fixture.spec())
            .expect("plan")
            .env
            .into_iter()
            .find(|(key, _)| key == "GROK_THEME")
            .expect("GROK_THEME")
            .1
    };
    assert_eq!(theme(None), "auto");
    assert_eq!(theme(Some(TerminalAppearance::Light)), "grokday");
    assert_eq!(theme(Some(TerminalAppearance::Dark)), "groknight");
}

/// Claude compatibility turns Grok's own switches for Claude Code's configuration on at launch;
/// off, none of them is passed and Grok follows its own configuration.
#[test]
fn claude_compatibility_sets_groks_claude_switches_only_when_on() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let claude_vars = |plan: &LaunchPlan| -> Vec<(String, String)> {
        plan.env
            .iter()
            .filter(|(key, _)| key.starts_with("GROK_CLAUDE_"))
            .cloned()
            .collect()
    };
    assert!(claude_vars(&plan_isolated(&fixture.spec()).unwrap()).is_empty());

    let mut spec = fixture.spec();
    spec.claude_compatibility = true;
    let plan = plan_isolated(&spec).unwrap();
    assert_eq!(plan.env[0].0, "GROK_HOME");
    assert_eq!(
        claude_vars(&plan),
        CLAUDE_COMPATIBILITY_VARS
            .map(|var| (var.to_string(), "true".to_string()))
            .to_vec()
    );
}

/// With no model or effort set, neither flag is passed, so Grok keeps its own choice.
#[test]
fn no_model_or_effort_adds_no_flags() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    for flag in ["--model", "--reasoning-effort"] {
        assert!(!plan.args.iter().any(|arg| arg == flag), "{flag}");
    }
}

#[test]
fn a_model_and_effort_come_after_the_rules_and_before_the_task() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    fixture.model = Some("grok-4".to_string());
    fixture.effort = Some("low".to_string());
    let mut spec = fixture.spec();
    spec.task = Some("do it");
    let plan = plan_isolated(&spec).expect("plan");
    let rules = plan.args.iter().position(|arg| arg == "--rules").unwrap();
    assert_eq!(
        plan.args[rules + 2..],
        ["--model", "grok-4", "--reasoning-effort", "low", "do it"]
    );
}

#[test]
fn a_pinned_grok_home_replaces_the_users_as_the_source_of_the_farm() {
    let mut fixture = spec_fixture();
    let shell_home = with_default_home(&mut fixture);
    std::fs::write(
        shell_home.join("trusted_folders.toml"),
        "[folders.\"/from/shell\"]\ntrusted = true\n",
    )
    .expect("trust store");
    let pinned = fixture.scratch.join("grok-alt");
    std::fs::create_dir_all(pinned.join("sessions")).expect("sessions directory");
    std::fs::write(pinned.join("config.toml"), "default_model = \"pinned\"\n").expect("config");
    std::fs::write(pinned.join("auth.json"), "{\"pinned\":true}").expect("auth");
    std::fs::write(
        pinned.join("trusted_folders.toml"),
        "[folders.\"/from/pinned\"]\ntrusted = true\n",
    )
    .expect("trust store");
    fixture.config_dir = Some(pinned.clone());

    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let farm = PathBuf::from(&plan.env[0].1);
    // Still Octoboard's own farm that Grok runs against, never the chosen directory itself.
    assert_eq!(
        plan.env
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        ["GROK_HOME", "GROK_THEME"]
    );
    assert!(farm.starts_with(&*fixture.scratch), "{}", farm.display());
    // Grok persists into `config.toml`, so it stays a copy and never a link into the source.
    assert!(farm.join("config.toml").is_file());
    assert!(!farm.join("config.toml").is_symlink());
    let config = std::fs::read_to_string(farm.join("config.toml")).expect("the copy");
    assert!(config.contains("default_model = \"pinned\""), "{config}");
    assert!(!config.contains("default_model = \"x\""), "{config}");
    let trust = std::fs::read_to_string(farm.join("trusted_folders.toml")).expect("the copy");
    assert!(trust.contains("/from/pinned"), "{trust}");
    assert!(!trust.contains("/from/shell"), "{trust}");
    // Login and session records are the chosen directory's, so a resume finds its transcript.
    assert_eq!(
        std::fs::read_link(farm.join("auth.json")).expect("a link"),
        std::fs::canonicalize(pinned.join("auth.json")).expect("canonical path")
    );
    assert_eq!(
        std::fs::read_link(farm.join("sessions")).expect("a link"),
        std::fs::canonicalize(pinned.join("sessions")).expect("canonical path")
    );
}

#[test]
fn a_pinned_grok_home_that_has_gone_refuses_resuming_a_conversation() {
    let mut fixture = spec_fixture();
    fixture.config_dir = Some(fixture.scratch.join("missing"));
    let mut spec = fixture.spec();
    spec.resume_agent_session_id = Some("agent-side-id");
    let err = plan_isolated(&spec).err().expect("refused");
    let message = err.to_string();
    assert!(message.contains("Grok"), "{message}");
    assert!(message.contains("missing"), "{message}");
    assert!(!message.contains("  "), "stray spaces in: {message}");
}

/// Unlike Claude Code and Codex, a fresh Grok session pinned to a directory that does not exist
/// is refused too, not only a resumed one: Grok cannot safely create a home the way the other
/// two can, so there is no "let the agent create it" case for a missing pinned source home —
/// it fails `require_initialized_grok_home` instead of the generic vanished-directory check.
/// A pinned directory that exists but Grok has never been run against carries no login and no
/// session history for the per-session farm to link, so it is refused the same way rather
/// than silently handed to Grok, which would write a login and a conversation into the
/// throw-away farm and lose both.
#[test]
fn a_pinned_grok_home_that_has_gone_or_was_never_initialized_refuses_the_launch() {
    for (name, exists) in [("missing", false), ("never-run", true)] {
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join(name);
        if exists {
            std::fs::create_dir_all(&home).expect("empty directory");
        }
        fixture.config_dir = Some(home);
        let err = plan_isolated(&fixture.spec()).err().expect("refused");
        assert_eq!(
            err.downcast_ref::<crate::protocol::CodedError>()
                .expect("coded")
                .code,
            crate::protocol::error_code::GROK_HOME_NOT_INITIALIZED,
            "{name}"
        );
        let message = err.to_string();
        assert!(message.contains(name), "{message}");
        assert!(!message.contains("  "), "stray spaces in: {message}");
    }
}

/// The default home is never held to the same requirement: it is the user's own existing
/// setup, not an account Octoboard is minting, so an uninitialized one is still left to Grok.
/// Unlike `with_default_home`, the home here is stripped to an empty directory — no
/// `sessions/`, no `auth.json` — so the check is actually exercised rather than skipped by
/// construction, and the test stays off the machine's real `~/.grok`: leaving `GROK_HOME`
/// unset would fall through to `default_grok_home`'s `home_dir().join(".grok")`, which this
/// test does not own and cannot assume either way about.
#[test]
fn the_default_home_is_not_checked_for_initialization() {
    let mut fixture = spec_fixture();
    let home = fixture.scratch.join("user-grok-home");
    std::fs::create_dir_all(&home).expect("empty home directory");
    fixture
        .shell_env
        .insert("GROK_HOME".to_string(), home.to_string_lossy().into_owned());
    plan_isolated(&fixture.spec()).expect("not refused");
}

/// The role text travels in `--rules`, which is passed for every session, not just a console
/// session's.
#[test]
fn the_role_text_travels_in_rules() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let plan = plan_isolated(&fixture.spec()).expect("plan");
    let at = plan
        .args
        .iter()
        .position(|arg| arg == "--rules")
        .expect("--rules is passed");
    assert_eq!(
        plan.args[at + 1],
        mcp::role::role_description(
            fixture.role,
            fixture.owner.as_ref(),
            Agent::Grok,
            &fixture.spec().role_options(),
        )
    );
}
