//! Tests of how Grok's launch degrades: the reduced launch after Grok refused an option as it
//! started, read from captures of real runs (`apps/daemon/testdata/grok-early-exit-captures.md`),
//! when its first hook is due, and the wording of what a degraded launch reports.

use super::refusal::refused_option;
use super::tests::{plan_isolated, with_default_home};
use super::*;
use crate::adapter::tests::spec_fixture;

/// Grok refusing an option no release takes (1.0.92).
const UNKNOWN_OPTION: &[u8] = include_bytes!("../../../testdata/grok_1.0.92_unknown_option.bin");
/// The near-miss: an unknown option whose tip names `--no-leader`.
const LIKE_NO_LEADER: &[u8] =
    include_bytes!("../../../testdata/grok_1.0.92_unknown_option_like_no_leader.bin");
/// The near-miss: a known option given a value it does not take.
const INVALID_VALUE: &[u8] =
    include_bytes!("../../../testdata/grok_1.0.92_invalid_option_value.bin");
/// The near-miss: a fast exit on an unreadable configuration, refusing no option.
const CONFIG_UNREADABLE: &[u8] =
    include_bytes!("../../../testdata/grok_1.0.92_config_unreadable.bin");

/// An early exit as the daemon hands it to the provider: the output as plain text, trimmed
/// (`coordinator::early_exit`).
fn exit_of(bytes: &[u8]) -> EarlyExit {
    let output = String::from_utf8_lossy(bytes);
    EarlyExit {
        output: crate::terminal::plain_text::strip_escapes(&output)
            .trim()
            .to_string(),
        status: Some(2),
    }
}

/// The refusal a release without `--no-leader` would print: the captured refusal of an unknown
/// option with that option in its place, since no measured release lacks it.
fn no_leader_refused() -> EarlyExit {
    let captured = String::from_utf8_lossy(UNKNOWN_OPTION);
    exit_of(
        captured
            .replace("--octoboard-unknown-option", NO_LEADER)
            .as_bytes(),
    )
}

#[test]
fn the_refusal_line_names_the_option_grok_did_not_take() {
    assert_eq!(
        refused_option(&exit_of(UNKNOWN_OPTION).output),
        Some("--octoboard-unknown-option")
    );
    assert_eq!(
        refused_option(&exit_of(LIKE_NO_LEADER).output),
        Some("--no-leaderx"),
        "the tip's similar argument is not the option refused"
    );
    assert_eq!(refused_option(&exit_of(INVALID_VALUE).output), None);
    assert_eq!(refused_option(&exit_of(CONFIG_UNREADABLE).output), None);
    assert_eq!(
        refused_option("error: unexpected argument '--no-leader=1' found"),
        Some("--no-leader")
    );
}

/// After a refusal of `--no-leader`, the launch is the enhanced one with only that flag left
/// out, and it tells the user with a standing notice of its own.
#[test]
fn a_refused_no_leader_is_left_out_of_an_otherwise_enhanced_launch() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let mut spec = fixture.spec();
    spec.task = Some(NO_LEADER);
    let enhanced = plan_isolated(&spec).expect("plan");
    let system_dir = spec.scratch.join("etc-grok");
    let reduced =
        reduced_plan_under(&spec, &no_leader_refused(), Some(&system_dir)).expect("reduced");
    let expected: Vec<&String> = enhanced.args.iter().skip(1).collect();
    assert_eq!(enhanced.args[0], NO_LEADER);
    assert_eq!(reduced.args.iter().collect::<Vec<_>>(), expected);
    assert_eq!(
        reduced.args.last().map(String::as_str),
        Some(NO_LEADER),
        "a task that reads like the flag is still the task"
    );
    assert_eq!(reduced.env, enhanced.env);
    let notice = reduced.notice.expect("told");
    assert_eq!(notice.code, Code::of_provider(grok::LEADER_NOT_OVERRIDDEN));
    assert!(notice.standing);
}

/// Anything else Grok exits on at start gives no reduced launch, and the launch goes bare.
#[test]
fn any_other_early_exit_gives_no_reduced_launch() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let spec = fixture.spec();
    let system_dir = spec.scratch.join("etc-grok");
    for capture in [
        UNKNOWN_OPTION,
        LIKE_NO_LEADER,
        INVALID_VALUE,
        CONFIG_UNREADABLE,
    ] {
        let exit = exit_of(capture);
        assert!(
            reduced_plan_under(&spec, &exit, Some(&system_dir)).is_none(),
            "{}",
            exit.output
        );
    }
    let session_id = EarlyExit {
        output: "error: unexpected argument '--session-id' found".to_string(),
        status: Some(2),
    };
    assert!(reduced_plan_under(&spec, &session_id, Some(&system_dir)).is_none());
}

/// A reduced launch that cannot be prepared, under Grok's sandbox here, gives nothing either.
#[test]
fn a_reduced_launch_that_cannot_be_prepared_gives_nothing() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    fixture
        .shell_env
        .insert("GROK_SANDBOX".to_string(), "workspace".to_string());
    let spec = fixture.spec();
    let system_dir = spec.scratch.join("etc-grok");
    assert!(reduced_plan_under(&spec, &no_leader_refused(), Some(&system_dir)).is_none());
}

/// Grok's `SessionStart` comes as it starts, for the enhanced and the reduced launch alike.
#[test]
fn its_first_hook_is_due_as_it_starts() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let spec = fixture.spec();
    let system_dir = spec.scratch.join("etc-grok");
    let enhanced = plan_isolated(&spec).expect("plan");
    let reduced =
        reduced_plan_under(&spec, &no_leader_refused(), Some(&system_dir)).expect("reduced");
    for plan in [enhanced, reduced] {
        assert_eq!(
            GrokProvider.first_hook_due(&spec, &plan),
            FirstHook::AtStart
        );
    }
}

/// The sandbox's error is shown as the reason the session started bare, so it states the cause
/// only, not that the session was not launched.
#[test]
fn the_sandbox_error_states_only_its_cause() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    fixture
        .shell_env
        .insert("GROK_SANDBOX".to_string(), "workspace".to_string());
    let message = plan_isolated(&fixture.spec())
        .err()
        .expect("no enhanced launch")
        .to_string();
    assert!(message.contains("workspace"), "{message}");
    assert!(!message.to_lowercase().contains("launch"), "{message}");
    assert!(!message.contains("  "), "stray spaces in: {message}");
}

/// Under the sandbox the bare launch carries no role text, which names tools the session lacks,
/// and nothing else of Octoboard's.
#[test]
fn its_bare_plan_carries_no_role_text() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    fixture
        .shell_env
        .insert("GROK_SANDBOX".to_string(), "workspace".to_string());
    let mut spec = fixture.spec();
    spec.task = Some("do it");
    let bare = GrokProvider.bare_plan(&spec).into_launch_plan();
    assert_eq!(bare.args, ["--", "do it"]);
    assert!(bare.env.is_empty());
    assert!(bare.notice.is_none());
}
