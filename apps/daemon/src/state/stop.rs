//! How the daemon's run is asked to end: an ordinary stop, which ends every session as interrupted,
//! or a planned stop, which leaves every terminal running in its holder for the next daemon to
//! adopt exactly (`crate::coordinator::planned_stop`). A planned stop whose daemon also starts the
//! next one is a planned restart, which is refused where no next daemon could come up.
//!
//! The stop asked for only ever moves from none to one, or from planned to ordinary, never back:
//! an ordinary stop takes the place of a planned one, and a planned stop never takes the place of
//! an ordinary one. Once `main` has begun a planned stop, an ordinary stop asked for escalates it:
//! `main`, the only one that ends anything once the daemon stops, watches for it
//! ([`AppState::ordinary_stop_asked`], [`AppState::await_ordinary_stop`]) while it hands the
//! terminals over, cuts the handover short where it stands, and ends what the planned stop was to
//! leave running as an ordinary stop ends it. Whatever asks for an ordinary stop escalates one: the
//! `shutdown` request on a connection still open, a termination signal, the application's quit
//! note (`super::application_note`). A new connection is not accepted once the stop has begun, so
//! a `POST /shutdown` made then does not reach the daemon.

use std::sync::atomic::Ordering;

use super::AppState;

/// How the daemon's run is to end, as `main` carries it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopKind {
    /// Every process the daemon runs is ended and every session left interrupted: the `shutdown`
    /// request, `POST /shutdown`, a termination signal, the application going away after its quit
    /// note.
    Ordinary,
    /// The terminals are handed over to their holders, the sessions left as they are: the
    /// `planned_stop` request, `POST /planned-stop`, the application going away without a quit
    /// note. With `successor`, a planned restart: the next daemon comes up on the same address.
    Planned { successor: bool },
}

/// Why a planned restart is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartRefusal {
    /// The daemon follows an application (`--parent-pid`), which starts the daemon it talks to
    /// itself.
    FollowsApplication,
    /// No service manager restarts the daemon, and the host cannot start one in the background.
    NoSuccessor,
}

impl RestartRefusal {
    /// The refusal's name, as `POST /restart` answers it.
    pub fn code(self) -> &'static str {
        match self {
            Self::FollowsApplication => "follows_application",
            Self::NoSuccessor => "no_successor",
        }
    }

    /// What the refusal says, in English, for a log or a caller that shows it as it is.
    pub fn message(self) -> &'static str {
        match self {
            Self::FollowsApplication => {
                "the daemon follows an application, which restarts the daemon it talks to itself"
            }
            Self::NoSuccessor => {
                "no service manager runs the daemon, and this host cannot start the next one in \
                 the background"
            }
        }
    }
}

impl AppState {
    /// Asks for an ordinary stop. The application sends this once the user has confirmed the
    /// exit; `main` waits on it ([`Self::await_stop`]). It takes the place of a planned stop asked
    /// for and not yet begun, and escalates one `main` has begun ([`Self::await_ordinary_stop`]).
    pub fn request_shutdown(&self) {
        self.stop.send_replace(Some(StopKind::Ordinary));
    }

    /// Asks for a planned stop with no successor. A stop asked for already, of either kind, stays
    /// the one made, and one asked for after the application said it is quitting is ordinary.
    pub fn request_planned_stop(&self) {
        self.ask_planned(false);
    }

    /// Asks for a planned restart, which is refused as [`Self::planned_restart_refusal`] says. A
    /// stop asked for already, of either kind, stays the one made.
    pub fn request_planned_restart(&self) -> Result<(), RestartRefusal> {
        if let Some(refusal) = self.planned_restart_refusal() {
            return Err(refusal);
        }
        self.ask_planned(true);
        Ok(())
    }

    /// A planned stop asked for once the application has said it is quitting is made ordinary:
    /// the quit escalates a planned stop asked for before it (`note_application_quit`), and one
    /// asked for after it (an update restart's request reaching the daemon after a session end's
    /// note) must not leave the sessions to the next start either. Read under the stop's lock,
    /// which `note_application_quit` reads the stop under after noting the quit, so that one of
    /// the two always sees the other.
    fn ask_planned(&self, successor: bool) {
        self.stop.send_if_modified(|stop| {
            if stop.is_some() {
                return false;
            }
            *stop = Some(if self.application_said_quit() {
                StopKind::Ordinary
            } else {
                StopKind::Planned { successor }
            });
            true
        });
    }

    /// Why a planned restart would be refused now, if it would.
    pub fn planned_restart_refusal(&self) -> Option<RestartRefusal> {
        if self.follows_application() {
            return Some(RestartRefusal::FollowsApplication);
        }
        if !self.service_managed() && !crate::platform::proc::runs_in_background() {
            return Some(RestartRefusal::NoSuccessor);
        }
        None
    }

    /// Waits until a stop is asked for, and says which. A stop asked for before the wait began is
    /// not lost.
    pub async fn await_stop(&self) -> StopKind {
        let mut stop = self.stop.subscribe();
        let asked = match stop.wait_for(Option::is_some).await {
            Ok(asked) => *asked,
            // Never: this state holds the sender.
            Err(_) => None,
        };
        asked.unwrap_or(StopKind::Ordinary)
    }

    /// Whether an ordinary stop has been asked for. During a planned stop `main` has begun, this is
    /// its escalation: the handover stops where it stands, and the stop goes on as an ordinary one.
    pub fn ordinary_stop_asked(&self) -> bool {
        matches!(*self.stop.borrow(), Some(StopKind::Ordinary))
    }

    /// Whether the stop asked for is a planned one, begun or not: one an ordinary stop asked for
    /// now would take the place of, or escalate.
    pub fn planned_stop_asked(&self) -> bool {
        matches!(*self.stop.borrow(), Some(StopKind::Planned { .. }))
    }

    /// Waits until an ordinary stop is asked for, which may have been before the wait began: what
    /// a planned stop waits on, alongside its own steps, to be escalated.
    pub async fn await_ordinary_stop(&self) {
        let mut stop = self.stop.subscribe();
        if stop
            .wait_for(|asked| matches!(asked, Some(StopKind::Ordinary)))
            .await
            .is_err()
        {
            // Never: this state holds the sender. Waiting on nothing escalates nothing.
            std::future::pending::<()>().await;
        }
    }

    /// Waits until a stop of either kind is asked for.
    #[cfg(test)]
    pub async fn await_shutdown(&self) {
        self.await_stop().await;
    }

    /// Notes that the daemon follows an application, which refuses a planned restart. Called at
    /// start, before anything is served.
    pub fn set_follows_application(&self) {
        self.follows_application.store(true, Ordering::Release);
    }

    /// Notes that a service manager restarts the daemon on the status it exits with after a
    /// planned restart, so that one is taken without the daemon starting its successor itself.
    /// Resolved once at start (see `main`) and used by the refusal check and the successor choice
    /// alike. Called at start, before anything is served.
    pub fn set_service_managed(&self, managed: bool) {
        self.service_managed.store(managed, Ordering::Release);
    }

    /// Whether a service manager restarts the daemon after a planned restart.
    pub fn service_managed(&self) -> bool {
        self.service_managed.load(Ordering::Acquire)
    }

    /// Whether the daemon follows an application.
    pub fn follows_application(&self) -> bool {
        self.follows_application.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::test_support::{app_state, PATIENCE};

    #[tokio::test]
    async fn a_stop_asked_for_before_the_wait_is_not_lost() {
        let (state, _dir) = app_state("stop-before-wait");
        state.request_planned_stop();
        let kind = tokio::time::timeout(PATIENCE, state.await_stop())
            .await
            .expect("the stop was remembered");
        assert_eq!(kind, StopKind::Planned { successor: false });
    }

    /// An ordinary stop takes the place of a planned one not yet begun, and a planned stop never
    /// takes the place of an ordinary one.
    #[tokio::test]
    async fn an_ordinary_stop_wins_over_a_planned_one() {
        let (state, _dir) = app_state("stop-ordinary-wins");
        state.request_planned_stop();
        state.request_shutdown();
        assert_eq!(state.await_stop().await, StopKind::Ordinary);
        state.request_planned_stop();
        assert_eq!(state.await_stop().await, StopKind::Ordinary);
    }

    /// A planned stop asked for after the application said it is quitting is made ordinary: a
    /// session end's note followed by an update restart's late request still ends the sessions.
    #[tokio::test]
    async fn a_planned_stop_asked_after_a_quit_is_ordinary() {
        let (state, _dir) = app_state("stop-planned-after-quit");
        state.note_application_quit();
        assert!(!state.ordinary_stop_asked());
        state.request_planned_stop();
        assert_eq!(state.await_stop().await, StopKind::Ordinary);
    }

    /// An ordinary stop asked for once a planned stop has begun escalates it: what the planned
    /// stop watches says so, at once, and the stop asked for reads as ordinary from then on.
    #[tokio::test(start_paused = true)]
    async fn an_ordinary_stop_asked_during_a_planned_one_escalates_it() {
        let (state, _dir) = app_state("stop-escalates");
        state.request_planned_stop();
        assert_eq!(
            state.await_stop().await,
            StopKind::Planned { successor: false }
        );
        assert!(state.planned_stop_asked());
        assert!(!state.ordinary_stop_asked());
        // On the paused clock this runs out only once every task is idle.
        assert!(
            tokio::time::timeout(Duration::from_millis(100), state.await_ordinary_stop())
                .await
                .is_err(),
            "a planned stop alone read as escalated"
        );

        let (escalated, ()) = tokio::join!(
            tokio::time::timeout(PATIENCE, state.await_ordinary_stop()),
            async {
                // Asked once the wait is under way: every task is idle by then.
                crate::test_support::tasks_idle().await;
                state.request_shutdown();
            },
        );
        escalated.expect("the planned stop's wait saw the ordinary stop");
        assert!(state.ordinary_stop_asked());
        assert!(!state.planned_stop_asked());
        // Asked for before this wait began: not lost.
        tokio::time::timeout(PATIENCE, state.await_ordinary_stop())
            .await
            .expect("the escalation was remembered");
        assert_eq!(state.await_stop().await, StopKind::Ordinary);
    }

    /// A planned stop or restart asked for after an ordinary stop leaves it ordinary, so nothing
    /// is handed over.
    #[tokio::test]
    async fn a_planned_stop_asked_after_an_ordinary_one_stays_ordinary() {
        let (state, _dir) = app_state("stop-ordinary-stays");
        state.set_service_managed(true);
        state.request_shutdown();
        state.request_planned_stop();
        // Not refused, and still no planned stop.
        assert_eq!(state.request_planned_restart(), Ok(()));
        assert_eq!(state.await_stop().await, StopKind::Ordinary);
        assert!(state.ordinary_stop_asked());
        assert!(!state.planned_stop_asked());
    }

    /// With no planned stop, an ordinary stop is as it ever was: asked for and taken as ordinary,
    /// and nothing reads as escalated before it is asked for.
    #[tokio::test]
    async fn an_ordinary_stop_with_no_planned_one_is_unchanged() {
        let (state, _dir) = app_state("stop-ordinary-alone");
        assert!(!state.ordinary_stop_asked());
        assert!(!state.planned_stop_asked());
        state.request_shutdown();
        assert_eq!(state.await_stop().await, StopKind::Ordinary);
        assert!(state.ordinary_stop_asked());
        assert!(!state.planned_stop_asked());
        tokio::time::timeout(PATIENCE, state.await_ordinary_stop())
            .await
            .expect("an ordinary stop asked for reads as asked for");
    }

    /// A daemon that follows an application refuses a planned restart, and asks for no stop; a
    /// planned stop with no successor it still takes.
    #[tokio::test]
    async fn a_daemon_following_an_application_refuses_a_planned_restart() {
        let (state, _dir) = app_state("stop-restart-refused");
        state.set_follows_application();
        assert_eq!(
            state.request_planned_restart(),
            Err(RestartRefusal::FollowsApplication)
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), state.await_stop())
                .await
                .is_err(),
            "a refused restart asked for a stop"
        );
        state.request_planned_stop();
        assert_eq!(
            state.await_stop().await,
            StopKind::Planned { successor: false }
        );
    }

    /// Where the host can start the next daemon itself, a daemon on its own takes a planned
    /// restart; where it cannot and no service manager runs it, it refuses for want of one.
    #[tokio::test]
    async fn a_planned_restart_needs_a_way_to_bring_up_the_next_daemon() {
        let (state, _dir) = app_state("stop-restart-successor");
        let possible = crate::platform::proc::runs_in_background();
        match state.request_planned_restart() {
            Ok(()) => {
                assert!(possible);
                assert_eq!(
                    state.await_stop().await,
                    StopKind::Planned { successor: true }
                );
            }
            Err(refusal) => {
                assert!(!possible);
                assert_eq!(refusal, RestartRefusal::NoSuccessor);
            }
        }
    }

    /// A daemon a service manager restarts takes a planned restart wherever the host cannot start
    /// the next daemon itself.
    #[tokio::test]
    async fn a_service_managed_daemon_takes_a_planned_restart() {
        let (state, _dir) = app_state("stop-restart-managed");
        state.set_service_managed(true);
        assert_eq!(state.planned_restart_refusal(), None);
        assert_eq!(state.request_planned_restart(), Ok(()));
        assert_eq!(
            state.await_stop().await,
            StopKind::Planned { successor: true }
        );
    }

    /// Once a planned stop has raised its handover barrier, no launch or switch is claimed, and no
    /// shell terminal is started or closed, whatever is still to come of the stop.
    #[tokio::test]
    async fn nothing_is_started_once_the_handover_barrier_is_raised() {
        let (state, _dir) = app_state("stop-refuse-handing-over");
        assert!(state.refuse_while_shutting_down().is_ok());
        assert!(state.raise_handover_barrier(PATIENCE).await);
        for refused in [
            state.refuse_while_shutting_down().err(),
            state.begin_launch("s").err(),
            state.begin_switch("s").err(),
            state.admit_change().err(),
        ] {
            let err = refused.expect("refused");
            assert_eq!(
                crate::test_support::code_of(&err),
                crate::protocol::error_code::DAEMON_SHUTTING_DOWN
            );
        }
    }
}
