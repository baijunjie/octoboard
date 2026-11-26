//! The watch on a launch's hooks, run on a paused clock with the bound taken from the production
//! constant and the provider's behaviour from the test-only provider.

use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::adapter::FirstHook;
use crate::protocol::{AgentId, SessionStatus};
use crate::session::trust::{Press, Screen, StartSignal, NO_SCREEN};
use crate::session::ReportedSignals;
use crate::state::AppState;
use crate::test_support::test_provider::{
    declaring_first_hook, declaring_reported_signals, declaring_trust_screen,
};
use crate::test_support::{watched_live_session, ScratchDir, StandIn};

/// A provider whose start hook holds its messages with no fallback of its own, so that only the
/// hooks' absence can release them.
static HELD_FOR_THE_HOOK: Screen = Screen {
    hook_fallback: None,
    ..NO_SCREEN
};

/// How long a poll on the paused clock waits for work handed to a blocking task.
const POLL: Duration = Duration::from_millis(250);

/// The test-only provider's input box, drawn over a cleared screen.
const INPUT_BOX: &[u8] = b"\x1b[2J\x1b[HINPUT> \r\n";

/// A session `s` of the test-only provider, registered and watched as a launch is, with a message
/// queued for it. The provider's status comes from its hooks, its start hook holds its messages,
/// and its first hook is due as `first` says.
fn launched(name: &str, first: FirstHook, adopted: bool) -> (Arc<AppState>, ScratchDir, StandIn) {
    launched_showing(name, first, adopted, &HELD_FOR_THE_HOOK)
}

/// [`launched`], for a provider whose trust screen is `screen`.
fn launched_showing(
    name: &str,
    first: FirstHook,
    adopted: bool,
    screen: &'static Screen,
) -> (Arc<AppState>, ScratchDir, StandIn) {
    let _screen = declaring_trust_screen(screen);
    let _signals = declaring_reported_signals(ReportedSignals::NONE);
    let _first = declaring_first_hook(first);
    let (state, dir) = crate::test_support::app_state(name);
    crate::state::test_support::insert_record(&state, &dir, "s", SessionStatus::Idle);
    let mut record = state.session_record("s").expect("record");
    record.agent = AgentId::TEST;
    state.store.delete_session("s").expect("delete");
    state.store.insert_session(&record).expect("record");
    let live = watched_live_session("s", AgentId::TEST, 120, 40, "sleep 30", None);
    assert!(!live.status_from_reports(), "the hooks give the status");
    if adopted {
        live.trust.note_adopted();
        live.hooks.note_adopted();
    }
    state.register_live(live.clone());
    crate::session::trust::supervise(&state, &live);
    watch(&state, &live, adopted);
    state.queue_message("s", "peer message");
    (state, dir, live)
}

fn notices(state: &AppState) -> Vec<String> {
    state
        .standing_notices_of("s")
        .into_iter()
        .map(|notice| notice.code)
        .collect()
}

/// Polls on the paused clock until `done`, for work handed to a blocking task, which holds the
/// clock still until it ends.
async fn until(done: impl Fn() -> bool) -> bool {
    for _ in 0..20 {
        if done() {
            return true;
        }
        tokio::time::sleep(POLL).await;
    }
    done()
}

/// A launch whose start hook does not come within the bound is read as without hooks: its held
/// message goes in at its input box with no trust screen up, its status follows the program's
/// reports from the report it made before, and the user is told, also that the conversation has
/// no id. A hook arriving later withdraws the first notice, and the id recorded the second.
#[tokio::test(start_paused = true)]
async fn a_start_hook_that_does_not_come_is_read_as_no_hooks() {
    let (state, _dir, live) = launched("hooks-absent-at-start", FirstHook::AtStart, false);
    live.on_output(b"\x1b]7501;state=working\x07");
    live.on_output(INPUT_BOX);
    crate::test_support::tasks_idle().await;
    tokio::time::advance(HOOK_FALLBACK - Duration::from_secs(1)).await;
    crate::test_support::tasks_idle().await;
    assert_eq!(live.hooks.state(), Hooks::Expected);
    assert!(
        live.trust.holds_writes(),
        "held for the hook before the bound"
    );
    assert_eq!(state.outbox.len("s"), 1);
    assert!(notices(&state).is_empty());
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Idle,
        "a report gives no status while hooks are expected"
    );

    tokio::time::advance(Duration::from_secs(2)).await;
    crate::test_support::tasks_idle().await;
    assert_eq!(live.hooks.state(), Hooks::Absent);
    assert!(live.status_from_reports());
    assert_eq!(
        notices(&state),
        [
            notice_code::HOOKS_NOT_RECEIVED.as_str(),
            notice_code::CONVERSATION_ID_UNKNOWN.as_str()
        ]
    );
    assert!(
        until(|| state.outbox.len("s") == 0).await,
        "the held message went in at the input box"
    );
    assert!(!live.trust.holds_writes());
    assert!(
        until(|| state.session_record("s").unwrap().status == SessionStatus::Working).await,
        "the report made before is the status now"
    );

    on_hook(&state, &live);
    assert_eq!(live.hooks.state(), Hooks::Present);
    assert!(
        !live.status_from_reports(),
        "the hooks give the status again"
    );
    assert_eq!(
        notices(&state),
        [notice_code::CONVERSATION_ID_UNKNOWN.as_str()]
    );
    state.set_agent_session_id("s", "conversation-1").unwrap();
    assert!(notices(&state).is_empty());
}

/// A trust screen the agent shows before its start hook, as Claude Code, Grok Build and Pi do,
/// with no fallback of its own.
static TRUST_SCREEN: Screen = Screen {
    first_draw: &[&["Trustthisfolder?", "1.Yes,trustit"]],
    accept: Some("1.Yes,trustit"),
    others: &["2.No,exit"],
    cursor: None,
    press: Press::Yes,
    next_screen: None,
    input_ends_wait: false,
    start_signal: StartSignal::Hook,
    hook_fallback: None,
    waits_for_paste_mode: false,
};

/// The test screen's trust confirmation, drawn over a cleared screen.
const TRUST_CONFIRMATION: &[u8] =
    b"\x1b[2J\x1b[HTrust this folder?\r\n1. Yes, trust it\r\n2. No, exit\r\n";

/// A trust screen left unanswered past the bound raises nothing, since the agent runs no hook
/// while it waits: the bound starts once the screen has gone, and only then runs out. The screen
/// answered in the terminal with no hook ever coming does not keep the held message wedged: once
/// the hooks are found absent it goes in at the input box drawn with no screen up.
#[tokio::test(start_paused = true)]
async fn the_bound_waits_for_an_unanswered_trust_screen() {
    let (state, _dir, live) = launched_showing(
        "hooks-behind-trust-screen",
        FirstHook::AtStart,
        false,
        &TRUST_SCREEN,
    );
    live.on_output(TRUST_CONFIRMATION);
    assert!(crate::session::trust::screen_up(&live), "the screen is up");
    crate::test_support::tasks_idle().await;
    tokio::time::advance(HOOK_FALLBACK * 2).await;
    crate::test_support::tasks_idle().await;
    assert_eq!(live.hooks.state(), Hooks::Expected);
    assert!(notices(&state).is_empty());

    // Answered in the terminal: the screen goes, and the bound starts.
    live.on_output(INPUT_BOX);
    assert!(!crate::session::trust::screen_up(&live));
    tokio::time::advance(HOOK_FALLBACK - Duration::from_secs(1)).await;
    crate::test_support::tasks_idle().await;
    assert_eq!(live.hooks.state(), Hooks::Expected);
    assert!(notices(&state).is_empty());

    assert_eq!(
        state.outbox.len("s"),
        1,
        "held for the hook before the bound"
    );

    tokio::time::advance(Duration::from_secs(2)).await;
    crate::test_support::tasks_idle().await;
    assert_eq!(live.hooks.state(), Hooks::Absent);
    assert!(notices(&state).contains(&notice_code::HOOKS_NOT_RECEIVED.as_str().to_string()));
    assert!(
        until(|| state.outbox.len("s") == 0).await,
        "the held message went in at the input box"
    );
    assert!(!live.trust.holds_writes());
}

/// A hook within the bound leaves the launch with its hooks: nothing is told, and the message
/// stays for the hook's own release rules.
#[tokio::test(start_paused = true)]
async fn a_hook_within_the_bound_keeps_the_hooks() {
    let (state, _dir, live) = launched("hooks-in-time", FirstHook::AtStart, false);
    tokio::time::advance(Duration::from_secs(1)).await;
    on_hook(&state, &live);
    tokio::time::advance(HOOK_FALLBACK * 2).await;
    crate::test_support::tasks_idle().await;
    assert_eq!(live.hooks.state(), Hooks::Present);
    assert!(notices(&state).is_empty());
    assert!(!live.status_from_reports());
}

/// An idle session of an agent whose first hook comes with its first turn raises nothing however
/// long it sits; once a turn is seen running, the bound applies from then.
#[tokio::test(start_paused = true)]
async fn a_first_hook_with_the_first_turn_is_due_only_once_a_turn_runs() {
    let (state, _dir, live) = launched("hooks-first-turn", FirstHook::WithFirstTurn, false);
    tokio::time::advance(HOOK_FALLBACK * 3).await;
    crate::test_support::tasks_idle().await;
    assert_eq!(live.hooks.state(), Hooks::Expected);
    assert!(notices(&state).is_empty());

    live.on_output(b"\x1b]7501;state=working\x07");
    crate::test_support::tasks_idle().await;
    tokio::time::advance(HOOK_FALLBACK + Duration::from_secs(1)).await;
    crate::test_support::tasks_idle().await;
    assert_eq!(live.hooks.state(), Hooks::Absent);
    assert!(notices(&state).contains(&notice_code::HOOKS_NOT_RECEIVED.as_str().to_string()));
}

/// An adopted launch ran its start hook, if ever, before this daemon: an idle one raises nothing.
#[tokio::test(start_paused = true)]
async fn an_adopted_idle_launch_is_not_read_as_without_hooks() {
    let (state, _dir, live) = launched("hooks-adopted", FirstHook::AtStart, true);
    tokio::time::advance(HOOK_FALLBACK * 3).await;
    crate::test_support::tasks_idle().await;
    assert_eq!(live.hooks.state(), Hooks::Expected);
    assert!(notices(&state).is_empty());
}

/// A launch that expects no hook is never read as without them, and a bare launch is from its
/// start.
#[test]
fn the_starting_state_follows_the_tier_and_the_first_hook() {
    use crate::adapter::LaunchTier;
    use crate::session::integration::{HookPresence, Integration};
    let of = |tier, first_hook| {
        HookPresence::of(&Integration {
            tier,
            first_hook,
            ..Integration::unasked()
        })
        .state()
    };
    assert_eq!(
        of(LaunchTier::Enhanced, FirstHook::Never),
        Hooks::NotExpected
    );
    assert_eq!(
        of(LaunchTier::Enhanced, FirstHook::AtStart),
        Hooks::Expected
    );
    assert_eq!(
        of(LaunchTier::Reduced, FirstHook::WithFirstTurn),
        Hooks::Expected
    );
    assert_eq!(of(LaunchTier::Bare, FirstHook::Never), Hooks::Absent);
    assert_eq!(of(LaunchTier::Bare, FirstHook::AtStart), Hooks::Absent);
    let never = HookPresence::of(&Integration::unasked());
    assert!(!never.enter_absent(), "nothing expected, nothing absent");
}

/// An integrated launch that expects no hook and records no conversation id of its own is told
/// that its conversation id is unknown; one that resumes or was given a preallocated id, one that
/// expects a hook, and a bare one (told after its spawn) are not.
#[test]
fn an_integrated_launch_with_no_hook_and_no_id_is_told_the_id_is_unknown() {
    use crate::adapter::LaunchTier;
    use crate::session::integration::Integration;
    let of = |tier, first_hook, records_an_id| {
        unreported_id_notice(
            AgentId::TEST,
            &Integration {
                tier,
                first_hook,
                ..Integration::unasked()
            },
            records_an_id,
        )
        .map(|notice| (notice.code, notice.standing))
    };
    assert_eq!(
        of(LaunchTier::Reduced, FirstHook::Never, false),
        Some((notice_code::CONVERSATION_ID_UNKNOWN, true))
    );
    assert!(of(LaunchTier::Enhanced, FirstHook::Never, false).is_some());
    assert_eq!(of(LaunchTier::Reduced, FirstHook::Never, true), None);
    assert_eq!(
        of(LaunchTier::Enhanced, FirstHook::WithFirstTurn, false),
        None
    );
    assert_eq!(of(LaunchTier::Enhanced, FirstHook::AtStart, false), None);
    assert_eq!(of(LaunchTier::Bare, FirstHook::Never, false), None);
}
