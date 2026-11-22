//! `stop_impact`: what an `octoboardd stop` would interrupt, which is what the application's quit
//! confirmation asks the user about: a session at work (starting, working or dormant, or idle with
//! work left running in the background, which covers one paused with such work), a session waiting
//! for the user, and a shell terminal that is running and busy. Mirrors the UI's `quitHolders`
//! (`packages/ui/src/lifecycle/useAppExit.ts`), and the two must stay in step: a test of each runs
//! the cases in `apps/daemon/testdata/quit_holders_cases.json`.

use serde::{Deserialize, Serialize};

use super::handover_check::{ImpactSession, ImpactTerminal};
use crate::keep_awake::session_is_working;
use crate::protocol::{Session, SessionStatus, ShellTerminal, ShellTerminalState};
use crate::state::AppState;

/// The work a stop would interrupt; empty when it would interrupt none.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopImpact {
    pub sessions: Vec<ImpactSession>,
    pub shell_terminals: Vec<ImpactTerminal>,
}

impl StopImpact {
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty() && self.shell_terminals.is_empty()
    }
}

/// Whether `session`, as a client is shown it, holds a quit up. The cases in
/// `apps/daemon/testdata/quit_holders_cases.json` pin this rule and the UI's together.
fn holds_quit(session: &Session) -> bool {
    session_is_working(session) || session.status == SessionStatus::WaitingUser
}

/// Whether `terminal` holds a quit up: it runs something. Pinned with [`holds_quit`].
fn terminal_holds_quit(terminal: &ShellTerminal) -> bool {
    terminal.state == ShellTerminalState::Running && terminal.busy
}

/// What a stop would interrupt now; an error where the sessions cannot be read, since an answer
/// with none listed would let a stop interrupt them unasked. **Blocks** briefly on the state's
/// locks, the store, and a fresh look at each running shell terminal's processes.
pub fn stop_impact(state: &AppState) -> crate::store::Result<StopImpact> {
    let sessions = state
        .store
        .list_sessions()?
        .into_iter()
        .map(|session| state.wire_session(session))
        .filter(holds_quit)
        .map(|session| ImpactSession {
            status: serde_json::to_value(session.status)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default(),
            id: session.id,
            title: session.title,
        })
        .collect();
    // Read now: the watcher's last look may predate a command just started.
    let shell_terminals = crate::terminal::shell::list_busy_now(state)
        .into_iter()
        .filter(terminal_holds_quit)
        .map(|terminal| ImpactTerminal {
            id: terminal.id,
            title: terminal.title,
        })
        .collect();
    Ok(StopImpact {
        sessions,
        shell_terminals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AgentId, BackgroundTask};
    use crate::test_support::project_session;

    fn session(status: SessionStatus, background: bool) -> Session {
        let mut session = project_session("s", AgentId::CLAUDE, status);
        if background {
            session.background_tasks = vec![BackgroundTask {
                id: "t".to_owned(),
                kind: None,
                description: None,
            }];
        }
        session
    }

    /// The rule the application's quit confirmation applies, in the cases both sides run.
    #[test]
    fn the_shared_quit_holder_cases_hold() {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Case {
            name: String,
            sessions: Vec<SessionCase>,
            shell_terminals: Vec<TerminalCase>,
            holding_sessions: Vec<String>,
            holding_shell_terminals: Vec<String>,
        }
        #[derive(serde::Deserialize)]
        struct SessionCase {
            id: String,
            status: SessionStatus,
            #[serde(default)]
            background: bool,
        }
        #[derive(serde::Deserialize)]
        struct TerminalCase {
            id: String,
            state: String,
            busy: bool,
        }
        #[derive(serde::Deserialize)]
        struct Cases {
            cases: Vec<Case>,
        }
        let cases: Cases =
            serde_json::from_str(include_str!("../../testdata/quit_holders_cases.json")).unwrap();
        assert!(!cases.cases.is_empty());
        for case in cases.cases {
            let holding_sessions: Vec<String> = case
                .sessions
                .iter()
                .filter(|held| {
                    let mut record = session(held.status, held.background);
                    record.id = held.id.clone();
                    holds_quit(&record)
                })
                .map(|held| held.id.clone())
                .collect();
            assert_eq!(holding_sessions, case.holding_sessions, "{}", case.name);
            let holding_terminals: Vec<String> = case
                .shell_terminals
                .iter()
                .filter(|held| {
                    terminal_holds_quit(&ShellTerminal {
                        id: held.id.clone(),
                        project_id: "p".to_owned(),
                        worktree: None,
                        cwd: "/".to_owned(),
                        title: "zsh".to_owned(),
                        renamed: false,
                        state: match held.state.as_str() {
                            "running" => ShellTerminalState::Running,
                            "exited" => ShellTerminalState::Exited,
                            other => panic!("{}: unknown state {other}", case.name),
                        },
                        busy: held.busy,
                        created_at: 0,
                        screen_inexact: false,
                        holder_link: None,
                    })
                })
                .map(|held| held.id.clone())
                .collect();
            assert_eq!(
                holding_terminals, case.holding_shell_terminals,
                "{}",
                case.name
            );
        }
    }
}
