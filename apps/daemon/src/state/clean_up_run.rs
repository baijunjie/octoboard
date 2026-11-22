//! The clean-up run: a daemon started only to clear what its features have in effect outside
//! Octoboard's own directories (`crate::coordinator::system_changes`), by whoever is uninstalling
//! Octoboard, and stopped again once that is done. The desktop application's Windows uninstaller
//! starts its bundled daemon this way, and `octoboardd uninstall` the server form's where none
//! runs (`--clean-up-run`, an option of the command line that its help does not list).
//!
//! Such a daemon does what the clearing needs and what leaves nothing of Octoboard running, and
//! nothing else. It takes the instance lock, opens the store, takes over what the holders still
//! run (the sharing switch refuses an account whose session works, and the stop that follows ends
//! what was taken over), has the process ledger stop what the previous run's agents left, and
//! follows whoever started it: the application, or the command, whose process it watches
//! ([`AppState::follow_process`]), so that it never outlives it; a planned restart, which would
//! start a daemon that follows nobody, it refuses (`RestartRefusal::CleanUpRun`). It serves every
//! route a daemon serves, since the agents it took over still report to it, but launches no
//! agent, whoever asks ([`AppState::admit_launch`]). Everything a
//! daemon that is going to stay does besides is a [`StartWork`], which a clean-up run leaves out:
//! each would begin work for a user who is uninstalling, and be cut off moments later by the
//! stop.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use super::AppState;
use crate::platform::proc::ProcessIdentity;
use crate::protocol::{error_code, CodedError};

/// How often the process a clean-up run follows is looked for.
const FOLLOW_POLL: Duration = Duration::from_millis(500);

/// Work an ordinary start does that a clean-up run leaves out, each with why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartWork {
    /// Remote access and the browser's own socket: nobody connects to a daemon that is there for
    /// a moment, and it opens no port and no socket for them.
    ServingOthers,
    /// The messages the previous daemon's planned stop left queued for the sessions it handed
    /// over: written into an adopted session they would set its agent to work. The file stays for
    /// a daemon that starts to stay, neither read nor replaced by a planned stop of this one.
    QueuedMessages,
    /// Launching an agent, for a new session, a resume, a relaunch after a switch or a scheduled
    /// task's run, whoever asks: a client, a session's tool or the daemon itself.
    LaunchingAgents,
    /// Switching a working session to another account when its own runs out, which relaunches
    /// it.
    AutomaticAccountSwitch,
    /// Keeping for the user, among its project's processes, what a session's process had started
    /// when a restart ends that process and leaves the session down: it would run on after the
    /// uninstall. It is stopped instead.
    KeepingProcesses,
    /// Reading which agents are installed from the login shell: nothing shows it. The sharing
    /// switch reads the shell itself where it needs the default account's directory.
    AgentAvailability,
    /// The server form's check for an update of itself.
    SelfUpdate,
    /// Collecting the agents' conversation records into the sessions' histories.
    RecordCollector,
    /// The background runs of the agents' command lines that read the accounts' quotas.
    Quotas,
    /// Deleting the archived sessions that came due.
    ArchiveCleanup,
    /// Resuming the interrupted sessions at start, and the sessions the sharing switch ended
    /// around its change: an agent launched to be ended by the stop.
    ResumingSessions,
    /// The scheduled tasks, with the run each catches up on when it came due while no daemon ran.
    ScheduledTasks,
    /// Keeping the machine awake while sessions work.
    KeepAwake,
    /// The projects' git workers: the remote addresses, the periodic fetch and the automatic pull.
    ProjectGit,
}

impl AppState {
    /// Notes that this daemon was started for a clean-up run. Called at start, before anything is
    /// served.
    pub fn set_clean_up_run(&self) {
        self.clean_up_run.store(true, Ordering::Release);
    }

    /// Whether this daemon was started for a clean-up run, for what says so of it (its owner
    /// record) and what it refuses for it (a planned restart).
    pub fn clean_up_run(&self) -> bool {
        self.clean_up_run.load(Ordering::Acquire)
    }

    /// Whether this daemon does `work`: every daemon but one started for a clean-up run does all
    /// of it.
    pub fn does(&self, _work: StartWork) -> bool {
        !self.clean_up_run()
    }

    /// Follows the process `followed`, the command that started this daemon for a clean-up run:
    /// once it is gone, by its pid and its start time, so that another process given the pid
    /// later is not taken for it, the daemon makes a planned stop that starts no successor, which
    /// leaves what it runs to the holders. A stop the command asked for meanwhile stays the one
    /// made.
    pub fn follow_process(self: &Arc<Self>, followed: ProcessIdentity) {
        let state = Arc::downgrade(self);
        std::thread::spawn(move || {
            while followed.is_running() {
                std::thread::sleep(FOLLOW_POLL);
            }
            if let Some(state) = state.upgrade() {
                tracing::info!(
                    pid = followed.pid.0,
                    "the command this daemon was started by is gone; handing the sessions over"
                );
                state.request_planned_stop();
            }
        });
    }

    /// Admits the launch of an agent, which a clean-up run refuses with `daemon_shutting_down`,
    /// the code a stopping daemon refuses with: it is about to stop, and starts nothing.
    pub fn admit_launch(&self) -> Result<()> {
        if self.does(StartWork::LaunchingAgents) {
            return Ok(());
        }
        Err(CodedError::raised(
            error_code::DAEMON_SHUTTING_DOWN,
            "this daemon runs only to clear what Octoboard changed outside its own directories \
             before it is uninstalled, and stops once that is done, so it starts no agent",
            &[],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::console_with_project_and_accounts;

    /// `does` asks nothing of which work it is: a clean-up run leaves out every kind there is, one
    /// added later included, and any other daemon none.
    #[test]
    fn only_a_clean_up_run_leaves_the_start_work_out_and_it_leaves_all_of_it_out() {
        let (state, _dir) = console_with_project_and_accounts("clean-up-run-work");
        let asked = [StartWork::ScheduledTasks, StartWork::KeepingProcesses];
        assert!(!state.clean_up_run());
        assert!(asked.iter().all(|work| state.does(*work)));
        state.set_clean_up_run();
        assert!(state.clean_up_run());
        assert!(asked.iter().all(|work| !state.does(*work)));
    }

    /// The command gone, by its start time though its pid is in use, the daemon that follows it
    /// makes a planned stop with no successor.
    #[tokio::test]
    async fn a_daemon_whose_command_is_gone_makes_a_planned_stop() {
        let (state, _dir) = console_with_project_and_accounts("clean-up-run-follow");
        let own = crate::owner_record::own_identity().expect("this process's start is known");
        assert!(own.is_running());
        // This process's pid, as a process that started at another time had it: gone.
        let gone = ProcessIdentity {
            start_micros: own.start_micros + 1,
            ..own
        };
        assert!(!gone.is_running());
        state.follow_process(gone);
        let stop = tokio::time::timeout(crate::test_support::PATIENCE, state.await_stop()).await;
        assert_eq!(
            stop.expect("the daemon asked for no stop"),
            crate::state::StopKind::Planned { successor: false }
        );
    }

    #[test]
    fn only_a_clean_up_run_refuses_to_launch_an_agent_and_says_why() {
        let (state, _dir) = console_with_project_and_accounts("clean-up-run-launch");
        state.admit_launch().unwrap();
        state.set_clean_up_run();
        let refused = state.admit_launch().unwrap_err();
        assert_eq!(
            crate::protocol::error_parts(&refused).0,
            error_code::DAEMON_SHUTTING_DOWN
        );
        assert!(
            format!("{refused:#}").contains("so it starts no agent"),
            "{refused:#}"
        );
    }

    /// A claim to launch or resume a session, which every resume and relaunch takes first, is
    /// refused before anything is claimed, recorded or shown, with the reason; any other daemon
    /// grants it.
    #[test]
    fn a_clean_up_run_grants_no_claim_to_launch_and_leaves_the_session_alone() {
        use crate::protocol::{AgentId, SessionStatus};
        use crate::test_support::project_session;

        let (state, _dir) = console_with_project_and_accounts("clean-up-run-claim");
        state
            .store
            .insert_session(&project_session(
                "s",
                AgentId::CLAUDE,
                SessionStatus::Interrupted,
            ))
            .unwrap();
        drop(
            state
                .begin_launch("s")
                .expect("an ordinary daemon grants it"),
        );
        state.set_clean_up_run();
        let mut events = state.subscribe();

        let refused = match state.begin_launch("s") {
            Ok(_) => panic!("a clean-up run granted a claim to launch"),
            Err(err) => err,
        };

        assert_eq!(
            crate::protocol::error_parts(&refused).0,
            error_code::DAEMON_SHUTTING_DOWN
        );
        assert!(
            format!("{refused:#}").contains("so it starts no agent"),
            "{refused:#}"
        );
        assert!(!state.is_launching("s"));
        assert!(state.claim_kind("s").is_none());
        assert!(events.try_recv().is_err(), "something was published");
        let after = state.store.get_session("s").unwrap().unwrap();
        assert_eq!(after.status, SessionStatus::Interrupted);
    }
}
