use super::*;
use crate::test_support::owner_team::{interrupted_hub_state, kept_texts, team_state};

fn worker(state: &Arc<AppState>) -> Session {
    state.session_record("worker").unwrap()
}

/// Idle after a report means the report reached the owner and nothing has happened to the
/// session since: a turn opening clears it, and a session working or with a message on its way
/// is not idle.
#[test]
fn a_session_is_idle_after_its_report_until_a_turn_opens() {
    let (state, _dir, _hub) = team_state(
        "idle-after-report",
        &[("worker", "hub", false, SessionStatus::Idle)],
    );
    assert_eq!(idle_after_report(&state, &worker(&state)), None);

    let number = state
        .note_report_accepted("worker", false, state.turn_opened("worker"))
        .unwrap();
    let last = idle_after_report(&state, &worker(&state)).expect("idle after its report");
    assert_eq!((last.number, last.synthesised), (number, false));

    let mut working = worker(&state);
    working.status = SessionStatus::Working;
    state.store.update_session(&working).unwrap();
    assert_eq!(idle_after_report(&state, &worker(&state)), None);
    working.status = SessionStatus::Idle;
    state.store.update_session(&working).unwrap();

    state.queue_message("worker", "next");
    assert_eq!(idle_after_report(&state, &worker(&state)), None);
    let _ = state.take_queued_with_rows("worker");
    assert!(idle_after_report(&state, &worker(&state)).is_some());

    state.turn_started("worker", Some("go on"));
    assert_eq!(idle_after_report(&state, &worker(&state)), None);
    assert_eq!(state.last_report("worker"), None);
}

/// A synthesised report counts as one the owner got, and says so; a session with no owner is
/// never idle after a report.
#[test]
fn a_synthesised_report_counts_and_an_unbound_session_never_does() {
    let (state, _dir, _hub) = team_state(
        "idle-after-synthesised",
        &[("worker", "hub", false, SessionStatus::Idle)],
    );
    state
        .note_report_accepted("worker", true, state.turn_opened("worker"))
        .unwrap();
    assert!(idle_after_report(&state, &worker(&state)).is_some_and(|last| last.synthesised));

    let unbound = Session {
        bound_to: None,
        ..worker(&state)
    };
    assert_eq!(idle_after_report(&state, &unbound), None);
}

/// Work left running in the background is work not over.
#[tokio::test]
async fn background_work_keeps_a_session_from_being_idle_after_its_report() {
    let (state, _dir, _hub) = team_state(
        "idle-after-background",
        &[("worker", "hub", false, SessionStatus::Idle)],
    );
    state
        .note_report_accepted("worker", false, state.turn_opened("worker"))
        .unwrap();
    state.apply_background_tasks_change(
        "worker",
        crate::hooks::BackgroundTasksChange::Replace(vec![crate::protocol::BackgroundTask {
            id: "b1".to_string(),
            kind: None,
            description: None,
        }]),
    );
    assert_eq!(idle_after_report(&state, &worker(&state)), None);
}

/// A coordinator whose own sessions still run is waiting for them, not idle.
#[test]
fn a_coordinator_with_a_running_session_is_not_idle_after_its_report() {
    let (state, _dir, _hub) = team_state(
        "idle-after-coordinator",
        &[
            ("coordinator", "hub", true, SessionStatus::Idle),
            ("child", "coordinator", false, SessionStatus::Idle),
        ],
    );
    state
        .note_report_accepted("coordinator", false, state.turn_opened("coordinator"))
        .unwrap();
    let coordinator = state.session_record("coordinator").unwrap();
    assert!(idle_after_report(&state, &coordinator).is_some());
    let child = crate::test_support::idle_stand_in("child");
    state.register_live(child.clone());
    assert_eq!(idle_after_report(&state, &coordinator), None);
}

/// The owner is told once the session has been idle after its report for the whole delay without
/// a break, and the watch ends with the report no longer the session's last.
#[test]
fn the_watch_tells_after_the_delay_of_unbroken_idleness() {
    let (state, _dir, _hub) = team_state(
        "idle-watch",
        &[("worker", "hub", false, SessionStatus::Idle)],
    );
    let number = state
        .note_report_accepted("worker", false, state.turn_opened("worker"))
        .unwrap();
    let delay = NUDGE_AFTER.as_millis() as i64;
    let mut since = None;
    assert_eq!(
        look(&state, "worker", number, &mut since, 1_000),
        Look::Wait
    );
    assert_eq!(since, Some(1_000));

    let mut working = worker(&state);
    working.status = SessionStatus::Working;
    state.store.update_session(&working).unwrap();
    assert_eq!(
        look(&state, "worker", number, &mut since, 2_000),
        Look::Wait
    );
    assert_eq!(since, None);
    working.status = SessionStatus::Idle;
    state.store.update_session(&working).unwrap();

    assert_eq!(
        look(&state, "worker", number, &mut since, 3_000),
        Look::Wait
    );
    assert_eq!(
        look(&state, "worker", number, &mut since, 3_000 + delay - 1),
        Look::Wait
    );
    assert_eq!(
        look(&state, "worker", number, &mut since, 3_000 + delay),
        Look::Tell
    );

    state.turn_started("worker", Some("go on"));
    assert_eq!(
        look(&state, "worker", number, &mut since, i64::MAX),
        Look::Stop
    );
    let later = state
        .note_report_accepted("worker", true, state.turn_opened("worker"))
        .unwrap();
    assert_eq!(
        look(&state, "worker", number, &mut since, i64::MAX),
        Look::Stop
    );
    assert_ne!(later, number);
}

/// The notice names the session and whether its report was its own, points the owner to the
/// tools to check and archive it with, and comes once per report.
#[test]
fn the_owner_is_told_once_that_its_session_stayed_idle() {
    let (state, _dir, hub) = team_state(
        "idle-notice",
        &[("worker", "hub", false, SessionStatus::Idle)],
    );
    let number = state
        .note_report_accepted("worker", true, state.turn_opened("worker"))
        .unwrap();
    tell_owner_it_is_idle(&state, "worker", number);
    let heard = crate::test_support::output_containing(&hub, "next instruction");
    assert!(heard.contains("`worker`"), "{heard}");
    assert!(heard.contains("synthesised that report"), "{heard}");
    assert!(heard.contains("mcp__octoboard__archive_session"), "{heard}");
    assert!(!state.claim_idle_nudge("worker", number), "claimed once");

    // A report since: the nudge for the first one is no longer owed.
    let second = state
        .note_report_accepted("worker", false, state.turn_opened("worker"))
        .unwrap();
    tell_owner_it_is_idle(&state, "worker", number);
    assert!(state.claim_idle_nudge("worker", second));
}

/// The notice is worded as fixed, in the words of the owner's agent.
#[test]
fn the_idle_notice_is_worded_as_fixed() {
    let session = crate::test_support::owner_team::project_session("worker");
    assert_eq!(
        idle_notice(&session, false, |name| format!("octoboard__{name}")),
        format!(
            "Message from Octoboard: the project session `{id}` (\"worker\") has been idle for 10 \
             minutes since its last report reached you, with nothing running. That report was its \
             own, and did not archive it. If its work is over, check it with \
             `octoboard__get_session` and archive it with `octoboard__archive_session`; otherwise \
             give it its next instruction with `octoboard__instruct_session`. You are not \
             reminded again until it reports again.",
            id = session.id
        )
    );
}

/// For an interrupted owner the notice is kept, dated; a later notice about the same session
/// drops it, and so does the owner's resume once the session is no longer idle after its report.
#[test]
fn a_kept_idle_notice_gives_way_to_what_came_after_it() {
    let (state, _dir) = interrupted_hub_state("idle-kept");
    let mut idle = worker(&state);
    idle.status = SessionStatus::Idle;
    state.store.update_session(&idle).unwrap();

    let number = state
        .note_report_accepted("worker", false, state.turn_opened("worker"))
        .unwrap();
    tell_owner_it_is_idle(&state, "worker", number);
    let kept = kept_texts(&state);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(
        kept[0].contains("Kept for you while you were interrupted"),
        "{kept:?}"
    );
    assert!(crate::messaging::texts::is_idle_notice(&kept[0], &idle));
    assert_eq!(
        crate::messaging::texts::idle_notice_session(&kept[0]),
        Some("worker")
    );

    let current = crate::messaging::delivery::current_kept_messages(&state, "hub").unwrap();
    assert_eq!(current.len(), 1, "still idle after its report");
    state.turn_started("worker", Some("go on"));
    let current = crate::messaging::delivery::current_kept_messages(&state, "hub").unwrap();
    assert!(current.is_empty(), "{current:?}");

    let number = state
        .note_report_accepted("worker", false, state.turn_opened("worker"))
        .unwrap();
    tell_owner_it_is_idle(&state, "worker", number);
    crate::messaging::owner_notices::tell_owner_it_ended(&state, &idle);
    let kept = kept_texts(&state);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].contains("ended on its own"), "{kept:?}");
}

/// A report accepted once a turn of the session has opened since it was made is from before that
/// turn, which is not over: it is not noted.
#[test]
fn a_report_from_before_a_turn_that_opened_is_not_noted() {
    let (state, _dir, _hub) = team_state(
        "idle-stale-report",
        &[("worker", "hub", false, SessionStatus::Idle)],
    );
    let opened = state.turn_opened("worker");
    state.turn_started("worker", Some("next"));
    assert_eq!(state.note_report_accepted("worker", false, opened), None);
    assert_eq!(state.last_report("worker"), None);
}

/// An archived session is watched no further, and its owner is not told.
#[test]
fn the_watch_ends_with_the_archive() {
    let (state, _dir, _hub) = team_state(
        "idle-watch-archived",
        &[("worker", "hub", false, SessionStatus::Idle)],
    );
    let number = state
        .note_report_accepted("worker", false, state.turn_opened("worker"))
        .unwrap();
    let mut archived = worker(&state);
    archived.status = SessionStatus::Archived;
    state.store.update_session(&archived).unwrap();
    let mut since = Some(0);
    assert_eq!(
        look(&state, "worker", number, &mut since, i64::MAX),
        Look::Stop
    );
}

/// A turn Octoboard credited as its own, which nothing reports back on, leaves the report before
/// it the session's last; a turn of the session's own clears it.
#[test]
fn a_credited_turn_keeps_the_last_report() {
    let (state, _dir, _hub) = team_state(
        "idle-credited-turn",
        &[("worker", "hub", false, SessionStatus::Idle)],
    );
    let number = state
        .note_report_accepted("worker", false, state.turn_opened("worker"))
        .unwrap();
    let before = state.turn_opened("worker");
    state.credit_turn_opened_by("worker", "Approved:".to_string());
    state.turn_started("worker", Some("Approved: the session"));
    assert_eq!(
        state.turn_opened("worker"),
        before,
        "a report made before it still counts"
    );
    state.close_turn("worker", false);
    assert_eq!(
        state.last_report("worker").map(|last| last.number),
        Some(number)
    );

    state.turn_started("worker", Some("go on"));
    assert_eq!(state.last_report("worker"), None);
}
