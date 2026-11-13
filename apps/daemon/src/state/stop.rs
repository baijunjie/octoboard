//! How the daemon's run is asked to end: an ordinary stop, which ends every session as interrupted,
//! or a planned stop, which leaves every terminal running in its holder for the next daemon to
//! adopt exactly (`crate::coordinator::planned_stop`). A planned stop whose daemon also starts the
//! next one is a planned restart, which a daemon that follows an application refuses, and so does
//! a service-managed daemon while a holder with a program to run is inside its own systemd service
//! (`holder::client::Holders::claim_planned_stop`), which is decided atomically with the
//! admission of new programs there: once a restart is accepted, a session or terminal that would
//! run inside the service is refused until the restart completes. A planned stop with no
//! successor that a client asks for ([`AppState::request_planned_stop_unless_holders_in_service`])
//! is refused in that same case, since the service's stop ends those holders after it too.
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

/// Why a planned restart, or a planned stop a client asks for, is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartRefusal {
    /// The daemon follows an application (`--parent-pid`), which starts the daemon it talks to
    /// itself.
    FollowsApplication,
    /// A stop of either kind has been asked for already, a restart included. Only
    /// [`AppState::request_planned_restart_unless_stopping`] refuses with it; `POST /restart` and
    /// the restart signal take a second request as the first.
    StopInProgress,
    /// A service manager runs the daemon, and a holder with a program to run is inside the
    /// daemon's own systemd service, having no unit of its own (`holder::client::launch_holder`):
    /// the service's stop would end it with every terminal it runs.
    HoldersInService,
}

impl RestartRefusal {
    /// The refusal's name, as `POST /restart` and `POST /planned-stop` answer it.
    pub fn code(self) -> &'static str {
        match self {
            Self::FollowsApplication => "follows_application",
            Self::StopInProgress => "stop_in_progress",
            Self::HoldersInService => "holders_in_service",
        }
    }

    /// What the refusal says, in English, for a log or a caller that shows it as it is.
    pub fn message(self) -> &'static str {
        match self {
            Self::FollowsApplication => {
                "the daemon follows an application, which restarts the daemon it talks to itself"
            }
            Self::StopInProgress => "the daemon is already stopping or restarting",
            Self::HoldersInService => {
                "sessions and terminals run inside the daemon's systemd unit, so stopping or \
                 restarting the daemon would end them; to do it anyway, stop or restart the unit \
                 with systemctl on the server"
            }
        }
    }
}

/// Why a planned restart is refused, given whether the daemon follows an application, whether a
/// service manager restarts it, and `claim_holders`, which claims the restart where no holder
/// with a program to run is inside the daemon's own service and tells whether it did. The claim
/// is made only where it matters (a service manager restarts the daemon: elsewhere the stop is the
/// daemon's own) and after the other refusals, as it makes the restart one that no new program in
/// the service is admitted to.
fn restart_refusal(
    follows_application: bool,
    service_managed: bool,
    claim_holders: impl FnOnce() -> bool,
) -> Option<RestartRefusal> {
    if follows_application {
        return Some(RestartRefusal::FollowsApplication);
    }
    if service_managed && !claim_holders() {
        return Some(RestartRefusal::HoldersInService);
    }
    None
}

impl AppState {
    /// Asks for an ordinary stop. The application sends this once the user has confirmed the
    /// exit; `main` waits on it ([`Self::await_stop`]). It takes the place of a planned stop asked
    /// for and not yet begun, and escalates one `main` has begun ([`Self::await_ordinary_stop`]).
    pub fn request_shutdown(&self) {
        self.stop.send_replace(Some(StopKind::Ordinary));
    }

    /// Asks for a planned stop with no successor, never refused: what the daemon asks for itself
    /// (`super::application_note`). A stop asked for already, of either kind, stays the one made,
    /// and one asked for after the application said it is quitting is ordinary.
    pub fn request_planned_stop(&self) {
        self.ask_planned(false);
    }

    /// [`Self::request_planned_stop`] for a client's request, which is refused where a service
    /// manager runs the daemon and a holder with a program to run is inside its own service: the
    /// daemon exits 0 after the stop, the unit stops, and the service's stop ends that holder. The
    /// claim is the restart's ([`Self::claim_restart`]), so no new program is admitted in the
    /// service from then on either.
    pub fn request_planned_stop_unless_holders_in_service(&self) -> Result<(), RestartRefusal> {
        self.claim_planned_stop()?;
        self.ask_planned(false);
        Ok(())
    }

    /// Asks for a planned restart, which is refused as [`RestartRefusal`] says. A stop asked for
    /// already, of either kind, stays the one made.
    pub fn request_planned_restart(&self) -> Result<(), RestartRefusal> {
        self.claim_restart()?;
        self.ask_planned(true);
        Ok(())
    }

    /// [`Self::request_planned_restart`] that refuses where a stop of either kind has been asked
    /// for already, instead of leaving that one as the stop made: what a client that is told
    /// "restarting" must not be given when the daemon is stopping for another reason.
    pub fn request_planned_restart_unless_stopping(&self) -> Result<(), RestartRefusal> {
        self.claim_restart()?;
        if self.ask_planned(true) {
            Ok(())
        } else {
            Err(RestartRefusal::StopInProgress)
        }
    }

    /// A planned stop asked for once the application has said it is quitting is made ordinary:
    /// the quit escalates a planned stop asked for before it (`note_application_quit`), and one
    /// asked for after it (an update restart's request reaching the daemon after a session end's
    /// note) must not leave the sessions to the next start either. Read under the stop's lock,
    /// which `note_application_quit` reads the stop under after noting the quit, so that one of
    /// the two always sees the other. Whether this request became the stop: false when a stop
    /// had been asked for already, which stays the one made.
    fn ask_planned(&self, successor: bool) -> bool {
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
        })
    }

    /// Refuses a planned restart as [`RestartRefusal`] says; where it is not refused, claims the
    /// restart from the holders, which admit no new program inside the daemon's service from then
    /// on ([`crate::holder::client::Holders::claim_planned_stop`]). A claim stays when the stop
    /// that follows is not made because another was asked for first: the daemon is stopping then.
    fn claim_restart(&self) -> Result<(), RestartRefusal> {
        restart_refusal(
            self.follows_application(),
            self.service_managed(),
            Self::claim_holders,
        )
        .map_or(Ok(()), Err)
    }

    /// [`Self::claim_restart`] without the refusal for following an application, which a
    /// planned stop has no use for: the stop a following daemon makes is its own.
    fn claim_planned_stop(&self) -> Result<(), RestartRefusal> {
        restart_refusal(false, self.service_managed(), Self::claim_holders).map_or(Ok(()), Err)
    }

    /// Claims the stop or restart from the holders, true where none has a program to run inside
    /// the daemon's own service.
    fn claim_holders() -> bool {
        crate::holder::client::current().is_none_or(|holders| holders.claim_planned_stop())
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

    /// Whether the daemon is stopping or about to: a stop of either kind asked for, an ordinary
    /// stop begun, or a planned one at its handover. What work that would outlive the stop looks
    /// at, cheaply and as often as it likes.
    pub fn stop_asked(&self) -> bool {
        self.stop.borrow().is_some() || self.is_shutting_down() || self.handover_barrier_raised()
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

    /// The restart a client asks for over the protocol is refused where any stop is asked for
    /// already, and a restart asked for twice is one restart; the HTTP route and the signal still
    /// take a second request as the first.
    #[tokio::test]
    async fn a_restart_that_must_not_stack_on_a_stop_is_refused_while_one_is_asked_for() {
        let (state, _dir) = app_state("stop-restart-unless-stopping");
        state.set_service_managed(true);
        assert_eq!(state.request_planned_restart_unless_stopping(), Ok(()));
        assert_eq!(
            state.request_planned_restart_unless_stopping(),
            Err(RestartRefusal::StopInProgress)
        );
        assert_eq!(state.request_planned_restart(), Ok(()));
        assert_eq!(
            state.await_stop().await,
            StopKind::Planned { successor: true }
        );

        let (other, _other_dir) = app_state("stop-restart-unless-stopping-ordinary");
        other.request_shutdown();
        assert_eq!(
            other.request_planned_restart_unless_stopping(),
            Err(RestartRefusal::StopInProgress)
        );
        assert_eq!(other.await_stop().await, StopKind::Ordinary);

        let (following, _following_dir) = app_state("stop-restart-unless-stopping-follows");
        following.set_follows_application();
        assert_eq!(
            following.request_planned_restart_unless_stopping(),
            Err(RestartRefusal::FollowsApplication)
        );
    }

    /// A planned stop a client asks for is refused only where a service manager runs the daemon
    /// and a holder in its service has a program to run; an internal planned stop is never
    /// refused.
    #[tokio::test]
    async fn a_planned_stop_asked_by_a_client_is_refused_only_for_holders_in_the_service() {
        // With no holders there is nothing in the service to end.
        let (managed, _dir) = app_state("stop-planned-managed");
        managed.set_service_managed(true);
        assert_eq!(
            managed.request_planned_stop_unless_holders_in_service(),
            Ok(())
        );
        assert_eq!(
            managed.await_stop().await,
            StopKind::Planned { successor: false }
        );

        let (alone, _alone_dir) = app_state("stop-planned-alone");
        assert_eq!(
            alone.request_planned_stop_unless_holders_in_service(),
            Ok(())
        );
        assert_eq!(
            alone.await_stop().await,
            StopKind::Planned { successor: false }
        );

        // A daemon that follows an application takes it too: its own stop is the internal one.
        let (following, _following_dir) = app_state("stop-planned-follows");
        following.set_follows_application();
        following.set_service_managed(true);
        assert_eq!(
            following.request_planned_stop_unless_holders_in_service(),
            Ok(())
        );
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

    /// A daemon on its own, which no service manager runs, takes a planned restart: it starts the
    /// next daemon itself.
    #[tokio::test]
    async fn a_daemon_on_its_own_takes_a_planned_restart() {
        let (state, _dir) = app_state("stop-restart-successor");
        assert_eq!(state.request_planned_restart(), Ok(()));
        assert_eq!(
            state.await_stop().await,
            StopKind::Planned { successor: true }
        );
    }

    /// A daemon a service manager restarts takes a planned restart too.
    #[tokio::test]
    async fn a_service_managed_daemon_takes_a_planned_restart() {
        let (state, _dir) = app_state("stop-restart-managed");
        state.set_service_managed(true);
        assert_eq!(state.request_planned_restart(), Ok(()));
        assert_eq!(
            state.await_stop().await,
            StopKind::Planned { successor: true }
        );
    }

    /// Every refusal tells a client why in a code and a message of its own.
    #[test]
    fn every_restart_refusal_has_a_code_and_a_message_of_its_own() {
        // A `match` with no wildcard, so that a new refusal cannot be left out of the list.
        fn listed(refusal: RestartRefusal) -> RestartRefusal {
            match refusal {
                RestartRefusal::FollowsApplication
                | RestartRefusal::StopInProgress
                | RestartRefusal::HoldersInService => refusal,
            }
        }
        let all = [
            listed(RestartRefusal::FollowsApplication),
            listed(RestartRefusal::StopInProgress),
            listed(RestartRefusal::HoldersInService),
        ];
        let codes: std::collections::HashSet<_> =
            all.iter().map(|refusal| refusal.code()).collect();
        let messages: std::collections::HashSet<_> =
            all.iter().map(|refusal| refusal.message()).collect();
        assert_eq!(codes.len(), all.len(), "two refusals share a code");
        assert_eq!(messages.len(), all.len(), "two refusals share a message");
        assert!(all.iter().all(|refusal| !refusal.message().is_empty()));
    }

    /// A holder in the daemon's own service refuses a restart only where a service manager
    /// restarts the daemon, following an application is refused first, and the claim is made only
    /// when nothing else refuses.
    #[test]
    fn a_holder_in_the_daemons_service_refuses_a_restart_only_under_a_service_manager() {
        use std::cell::Cell;
        let claimed = Cell::new(0);
        let claim = |granted: bool| {
            let claimed = &claimed;
            move || {
                claimed.set(claimed.get() + 1);
                granted
            }
        };
        assert_eq!(restart_refusal(false, false, claim(false)), None);
        assert_eq!(restart_refusal(false, true, claim(true)), None);
        assert_eq!(
            restart_refusal(false, true, claim(false)),
            Some(RestartRefusal::HoldersInService)
        );
        assert_eq!(claimed.get(), 2, "only a service-managed restart claims");
        assert_eq!(
            restart_refusal(true, false, claim(false)),
            Some(RestartRefusal::FollowsApplication)
        );
        assert_eq!(
            restart_refusal(true, true, claim(false)),
            Some(RestartRefusal::FollowsApplication)
        );
        assert_eq!(claimed.get(), 2, "a refused restart claims nothing");
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
