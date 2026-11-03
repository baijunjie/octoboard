use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::adapter::codex::rollout_watch::end_failed_turn;
use crate::hooks::TurnEnd;
use crate::protocol::Session;
use crate::test_support::owner_team::{hub_heard, hub_heard_of, team_state};
use crate::test_support::{output_containing, running, ScratchDir, StandIn};

/// How long each wait lasts under test, short enough to wait out.
const SHORT: Duration = Duration::from_millis(60);

/// A wait that nothing but a cancellation ends within a test.
const LONG: Duration = Duration::from_secs(600);

/// Another prompt than the retry's, which a turn opened by something else starts with.
const OTHER_PROMPT: &str = "something else entirely";

struct Fixture {
    state: Arc<AppState>,
    _dir: ScratchDir,
    hub: StandIn,
    worker: StandIn,
    /// The worker's id, which is the test's label: the automatic switch's records are global.
    id: String,
}

impl Fixture {
    /// A worker bound to a hub, with a process and an open turn, on a model and an effort.
    fn new(label: &str, delay: Duration) -> Self {
        let (state, dir, hub) = team_state(label, &[(label, "hub", false, SessionStatus::Working)]);
        state
            .set_session_launch_overrides(
                label,
                &Some(Some("a-model".to_string())),
                &Some(Some("high".to_string())),
            )
            .unwrap();
        let worker = running(&state, label);
        state.capacity_retries.set_delay_for_test(delay);
        state.turn_started(label, Some("work"));
        Self {
            state,
            _dir: dir,
            hub,
            worker,
            id: label.to_string(),
        }
    }

    /// The open turn fails at capacity.
    fn fail_at_capacity(&self) {
        end_failed_turn(
            &self.state,
            &self.id,
            Some("Selected model is at capacity. Please try a different model.".to_string()),
            true,
        );
    }

    fn is_waiting(&self) -> bool {
        self.state.capacity_retries.is_waiting_for_test(&self.id)
    }

    async fn worker_heard(&self, needle: &'static str) -> String {
        let worker = (*self.worker).clone();
        tokio::task::spawn_blocking(move || output_containing(&worker, needle))
            .await
            .unwrap()
    }

    fn worker_output(&self) -> String {
        String::from_utf8_lossy(&self.worker.recent_output(8 * 1024)).into_owned()
    }

    /// Waits until the worker was sent retry `n`, then has it start a turn with `prompt`.
    async fn retry_is_sent_and_starts_a_turn(&self, n: usize, prompt: &str) {
        let worker = (*self.worker).clone();
        let needle = format!("retry {n} of 3");
        let heard = tokio::task::spawn_blocking(move || output_containing(&worker, &needle))
            .await
            .unwrap();
        assert!(heard.contains(&format!("retry {n} of 3")), "{heard}");
        self.state.turn_started(&self.id, Some(prompt));
    }

    /// Waits until the owner has been given the failed report the retry held back.
    async fn owner_gets_the_failed_report(&self) {
        let heard = hub_heard_of(&self.hub, "did not go ahead").await;
        assert!(heard.contains("Report from session"), "{heard}");
        assert!(!self.is_waiting());
    }

    /// Nothing was sent to the worker and no report reached the owner.
    async fn nothing_came_of_the_wait(&self, name: &str) {
        tokio::time::sleep(SHORT * 5).await;
        let heard = self.worker_output();
        assert!(!heard.contains("retrying automatically"), "{name}: {heard}");
        let heard = hub_heard(&self.state, &self.hub).await;
        assert!(!heard.contains("Report from session"), "{name}: {heard}");
    }

    fn record(&self) -> Session {
        self.state.store.get_session(&self.id).unwrap().unwrap()
    }
}

#[test]
fn the_retries_wait_30_then_60_then_120_seconds_and_there_are_three() {
    assert_eq!(delay_after(0), Some(Duration::from_secs(30)));
    assert_eq!(delay_after(1), Some(Duration::from_secs(60)));
    assert_eq!(delay_after(2), Some(Duration::from_secs(120)));
    assert_eq!(delay_after(3), None);
}

/// The report's text ends in what the user can do; asserted here and not through the owner's
/// terminal, whose unread input line a stand-in cannot hold that much of.
#[test]
fn the_failed_report_tells_the_user_what_they_can_do() {
    for outcome in [Outcome::Exhausted, Outcome::Stopped] {
        let text = failure_text(Some("It was at capacity.".to_string()), outcome);
        assert!(text.starts_with("It was at capacity."), "{text}");
        assert!(text.ends_with("retry later or switch the session's model manually."));
    }
}

#[tokio::test]
async fn a_capacity_failure_is_retried_in_the_same_process_without_changing_anything() {
    let fixture = Fixture::new("capacity-retry-same", SHORT);
    fixture
        .state
        .store
        .set_auto_switch_on_usage_limit(true)
        .unwrap();
    let before = fixture.record();
    let live = fixture.state.live_session(&fixture.id).unwrap();

    fixture.fail_at_capacity();
    assert_eq!(fixture.record().status, SessionStatus::Idle);
    assert!(fixture.is_waiting());
    // The owner is told a retry is coming, not that the session failed.
    let heard = hub_heard_of(&fixture.hub, "do not send it a continue prompt").await;
    assert!(heard.contains("retry automatically in"), "{heard}");
    assert!(!heard.contains("Report from session"), "{heard}");

    let heard = fixture.worker_heard("retry 1 of 3").await;
    assert!(
        heard.contains("the model you selected was at capacity"),
        "{heard}"
    );

    // Only a message went into the running process: no relaunch, no other model, effort or account,
    // and the automatic account switch was never asked.
    let after = fixture.record();
    assert_eq!(
        (
            &after.model,
            &after.effort,
            &after.account_id,
            &after.config_dir
        ),
        (
            &before.model,
            &before.effort,
            &before.account_id,
            &before.config_dir
        )
    );
    assert!(Arc::ptr_eq(
        &live,
        &fixture.state.live_session(&fixture.id).unwrap()
    ));
    assert!(!crate::coordinator::auto_switch::ASKED
        .lock()
        .unwrap()
        .contains(&fixture.id));
}

#[tokio::test]
async fn the_third_retry_failing_again_gives_the_owner_the_failed_report() {
    let fixture = Fixture::new("capacity-retry-exhausted", SHORT);
    for n in 1..=3 {
        fixture.fail_at_capacity();
        fixture
            .retry_is_sent_and_starts_a_turn(n, &capacity_retry_prompt(n, 3))
            .await;
    }
    let heard = hub_heard(&fixture.state, &fixture.hub).await;
    assert!(!heard.contains("Report from session"), "{heard}");

    fixture.fail_at_capacity();
    let heard = hub_heard_of(&fixture.hub, "Report from session").await;
    assert!(heard.contains("Report from session"), "{heard}");
    assert!(!fixture.is_waiting());
    // The report leaves the session paused at its prompt, as a failed turn does.
    assert_eq!(fixture.record().status, SessionStatus::Idle);
}

/// Whatever prompt the turn after a retry starts with, the count of failures in a row is what ends
/// the sequence: a Codex that echoed the retry differently would otherwise retry for ever.
#[tokio::test]
async fn the_retries_end_after_three_whatever_prompt_the_next_turn_starts_with() {
    let fixture = Fixture::new("capacity-retry-backstop", SHORT);
    for n in 1..=3 {
        fixture.fail_at_capacity();
        fixture
            .retry_is_sent_and_starts_a_turn(n, OTHER_PROMPT)
            .await;
        assert_eq!(
            fixture
                .state
                .capacity_retries
                .failures_for_test(&fixture.id),
            n
        );
    }
    fixture.fail_at_capacity();
    let heard = hub_heard_of(&fixture.hub, "Report from session").await;
    assert!(heard.contains("Report from session"), "{heard}");
    assert_eq!(
        fixture
            .state
            .capacity_retries
            .failures_for_test(&fixture.id),
        0
    );
}

#[tokio::test]
async fn a_turn_that_ends_without_a_capacity_failure_starts_the_count_again() {
    let fixture = Fixture::new("capacity-retry-reset", SHORT);
    fixture.fail_at_capacity();
    fixture
        .retry_is_sent_and_starts_a_turn(1, &capacity_retry_prompt(1, 3))
        .await;
    fixture.fail_at_capacity();
    fixture
        .retry_is_sent_and_starts_a_turn(2, &capacity_retry_prompt(2, 3))
        .await;
    // The second retry's turn completes.
    fixture.state.close_turn(&fixture.id, false);
    assert_eq!(
        fixture
            .state
            .capacity_retries
            .failures_for_test(&fixture.id),
        0
    );

    fixture.state.turn_started(&fixture.id, Some("more work"));
    fixture.fail_at_capacity();
    // The notice for the first retry reaches the owner again: the sequence began from the start.
    let mut heard = String::new();
    for _ in 0..100 {
        heard = String::from_utf8_lossy(&fixture.hub.recent_output(8 * 1024)).into_owned();
        if heard.matches("retry 1 of 3").count() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(heard.matches("retry 1 of 3").count(), 2, "{heard}");
}

/// The retried turn finishing without a report of its own is reported on as any turn is: the owner
/// was told not to continue the session itself, so it is owed how the turn went.
#[tokio::test]
async fn a_retried_turn_that_finishes_without_a_report_is_reported_on() {
    let fixture = Fixture::new("capacity-retry-completes", SHORT);
    fixture.fail_at_capacity();
    fixture
        .retry_is_sent_and_starts_a_turn(1, &capacity_retry_prompt(1, 3))
        .await;
    fixture.state.set_paused(&fixture.id, false);

    crate::messaging::reporting::close_turn_and_report(
        &fixture.state,
        &fixture.id,
        TurnEnd {
            last_assistant_message: Some("finished-the-carried-on-work".to_string()),
            failed: false,
            backstop: false,
        },
    );
    let heard = hub_heard_of(&fixture.hub, "finished-the-carried-on-work").await;
    assert!(heard.contains("Report from session"), "{heard}");
}

#[tokio::test]
async fn a_turn_or_a_queued_message_ends_the_wait_without_a_send_or_a_report() {
    type Act = fn(&Fixture);
    let a_turn: Act = |fixture| {
        fixture
            .state
            .turn_started(&fixture.id, Some("the user typed this"))
    };
    let a_queued_message: Act = |fixture| {
        fixture
            .state
            .queue_message(&fixture.id, "the user's message");
    };
    for (name, act) in [("turn", a_turn), ("message", a_queued_message)] {
        let fixture = Fixture::new(&format!("capacity-retry-silent-{name}"), SHORT);
        fixture.fail_at_capacity();
        assert!(fixture.is_waiting());
        act(&fixture);
        assert!(!fixture.is_waiting(), "{name}");
        fixture.nothing_came_of_the_wait(name).await;
    }
}

/// An interrupt and an abandoned turn open no turn, so nothing else will say what became of the
/// session: the owner, told a retry was coming, is given the failed report.
#[tokio::test]
async fn an_interrupt_or_an_abandoned_turn_gives_the_owner_the_failed_report() {
    type Act = fn(&Fixture);
    let an_interrupt: Act = |fixture| drop(fixture.state.claim_interrupt(&fixture.id));
    let an_abandoned_turn: Act = |fixture| fixture.state.abandon_turn(&fixture.id);
    for (name, act) in [("interrupt", an_interrupt), ("abandon", an_abandoned_turn)] {
        let fixture = Fixture::new(&format!("capacity-retry-report-{name}"), LONG);
        fixture.fail_at_capacity();
        assert!(fixture.is_waiting());
        act(&fixture);
        fixture.owner_gets_the_failed_report().await;
        assert!(!fixture.worker_output().contains("retrying automatically"));
    }
}

#[tokio::test]
async fn a_lifecycle_change_ends_the_wait_and_gives_the_owner_the_failed_report() {
    let fixture = Fixture::new("capacity-retry-lifecycle", LONG);
    fixture.fail_at_capacity();
    // What a resume, a restart, an account switch, a model change, an archive or a delete do.
    fixture.state.cancel_recoveries(&fixture.id);

    fixture.owner_gets_the_failed_report().await;
    assert!(!fixture.worker_output().contains("retrying automatically"));
}

/// A report cannot be delivered for a session with no process, so the owner is told by the notice
/// of the exit that the session ended, and the wait is over.
#[tokio::test]
async fn the_process_exiting_ends_the_wait_and_the_owner_hears_of_the_exit() {
    let fixture = Fixture::new("capacity-retry-exit", LONG);
    fixture.fail_at_capacity();
    crate::coordinator::exit_watch::watch_exit(&fixture.state, (*fixture.worker).clone());
    fixture.worker.terminate();

    let heard = hub_heard_of(&fixture.hub, "ended on its own").await;
    assert!(heard.contains("ended on its own"), "{heard}");
    assert!(!fixture.is_waiting());
}

/// A change of the wait for [`Cause::Report`] while the retry is being written is not lost: the
/// owner is given the failed report once the write is done. One that is silent is.
#[tokio::test]
async fn an_end_while_the_retry_is_being_written_is_kept_for_when_it_is_done() {
    let fixture = Fixture::new("capacity-retry-sending", LONG);
    fixture.fail_at_capacity();
    let retries = &fixture.state.capacity_retries;
    let token = retries.lock()[&fixture.id]
        .pending
        .as_ref()
        .unwrap()
        .token
        .clone();
    let claimed = retries.claim(&fixture.id, &token).expect("claimed");

    retries.cancel(&fixture.id, Cause::Silent);
    retries.cancel(&fixture.id, Cause::Report);
    assert!(!retries.finish_send(&fixture.id, true));
    assert_eq!(retries.failures_for_test(&fixture.id), 0);
    give_up(
        &fixture.state,
        &fixture.id,
        claimed.message.clone(),
        claimed.owed,
        Outcome::Stopped,
        claimed.take_notice(),
    );
    fixture.owner_gets_the_failed_report().await;

    // A silent end leaves the retry written.
    let fixture = Fixture::new("capacity-retry-sending-silent", LONG);
    fixture.fail_at_capacity();
    let retries = &fixture.state.capacity_retries;
    let token = retries.lock()[&fixture.id]
        .pending
        .as_ref()
        .unwrap()
        .token
        .clone();
    retries.claim(&fixture.id, &token).expect("claimed");
    retries.cancel(&fixture.id, Cause::Silent);
    assert!(retries.finish_send(&fixture.id, true));
}

/// The failed report comes after the notice that a retry was coming, never before.
#[tokio::test]
async fn the_failed_report_never_overtakes_the_retry_notice() {
    let fixture = Fixture::new("capacity-retry-order", LONG);
    fixture.fail_at_capacity();
    fixture.state.cancel_recoveries(&fixture.id);
    fixture.owner_gets_the_failed_report().await;
    let heard = hub_heard_of(&fixture.hub, "did not go ahead").await;
    let notice = heard.find("will retry automatically").expect("notice");
    let report = heard.find("Report from session").expect("report");
    assert!(notice < report, "{heard}");
}

#[tokio::test]
async fn a_session_that_waits_for_the_user_when_the_wait_ends_is_not_sent_the_retry() {
    let fixture = Fixture::new("capacity-retry-waiting", SHORT);
    fixture.fail_at_capacity();
    fixture
        .state
        .apply_hook_status(&fixture.id, SessionStatus::WaitingUser)
        .unwrap();

    fixture.owner_gets_the_failed_report().await;
    assert!(!fixture.worker_output().contains("retrying automatically"));
}

#[tokio::test]
async fn a_failure_of_another_kind_is_reported_at_once_and_never_retried() {
    let fixture = Fixture::new("capacity-retry-other", SHORT);
    end_failed_turn(
        &fixture.state,
        &fixture.id,
        Some("a different error".to_string()),
        false,
    );

    let heard = hub_heard_of(&fixture.hub, "a different error").await;
    assert!(heard.contains("Report from session"), "{heard}");
    assert_eq!(
        fixture
            .state
            .capacity_retries
            .failures_for_test(&fixture.id),
        0
    );
}

/// An end that comes between judging the failure and starting its wait is not undone by the wait:
/// nothing is sent, and the owner is given the failed report.
#[tokio::test]
async fn an_end_before_the_wait_starts_keeps_it_from_starting() {
    let fixture = Fixture::new("capacity-retry-before-wait", SHORT);
    let Judgement::Retry(plan) = fixture.state.capacity_retries.judge(&fixture.id) else {
        panic!("the first capacity failure is retried");
    };
    fixture
        .state
        .capacity_retries
        .cancel(&fixture.id, Cause::Report);

    arm(
        &fixture.state,
        &fixture.id,
        plan,
        Some("Selected model is at capacity.".to_string()),
        true,
    );

    fixture.owner_gets_the_failed_report().await;
    tokio::time::sleep(SHORT * 5).await;
    assert!(!fixture.worker_output().contains("retrying automatically"));
}
