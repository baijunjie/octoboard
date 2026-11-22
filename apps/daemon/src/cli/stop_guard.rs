//! What `stop` checks before it stops a daemon: whether stopping would interrupt work, as the
//! application's quit confirmation counts it, which the daemon says (`POST /stop-check`). A stop
//! that would says what runs and stops nothing, unless forced.

use std::time::Duration;

use crate::coordinator::stop_check::StopImpact;
use crate::loopback;
use crate::owner_record::Owner;

/// The exit status of `stop` when it refuses to interrupt work.
pub(super) const REFUSED: i32 = 1;

/// How long the check may take: the daemon reads the store and its terminals, briefly.
const CHECK_WAIT: Duration = Duration::from_secs(10);

/// How many sessions and terminals a refusal names before it counts the rest.
const NAMED: usize = 10;

/// Asks the daemon `owner` names, with `key`, what a stop would interrupt. `None` where it cannot
/// be told: no key recorded, or the daemon does not answer the check (a build from before it
/// answers `404`) or fails to.
pub(super) fn ask(owner: &Owner, key: Option<&str>) -> Option<StopImpact> {
    let answer = loopback::post(owner.port, key?, "/stop-check", b"{}", CHECK_WAIT)
        .map_err(|err| tracing::debug!(%err, "the stop check was not answered"))
        .ok()?;
    serde_json::from_slice(&answer).ok()
}

/// How a session's status, as the protocol names it, reads in a refusal.
fn status_words(status: &str) -> &'static str {
    match status {
        "starting" => "starting",
        "working" => "working",
        "waiting_user" => "waiting for you",
        "dormant" => "dormant",
        "idle" => "working in the background",
        _ => "running",
    }
}

/// Why `stop` refused, having stopped nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refusal {
    /// Stopping would interrupt work, which was named.
    Work,
    /// The daemon could not be asked what stopping it would interrupt, so nothing is known of
    /// what it runs.
    NotAsked,
}

/// Whether `stop` of the daemon `owner` names may go ahead: `None` when it may (nothing runs that
/// it would interrupt, or `force`), else which refusal it ends with, at the status [`REFUSED`],
/// the reason said on stderr. A daemon that cannot be asked is taken as one that may be running
/// work.
pub(super) fn refused(owner: &Owner, force: bool) -> Option<Refusal> {
    if force {
        return None;
    }
    match ask(owner, owner.recorded_key().as_deref()) {
        Some(impact) if impact.is_empty() => None,
        Some(impact) => {
            eprintln!("{}", refusal(&impact));
            Some(Refusal::Work)
        }
        None => {
            eprintln!(
                "{} is running (pid {}), but it could not be asked whether stopping it would \
                 interrupt work (an earlier build, or its key is not recorded); nothing was \
                 stopped: run `octoboardd stop --force` to stop it regardless",
                crate::APP_NAME,
                owner.identity.pid.0
            );
            Some(Refusal::NotAsked)
        }
    }
}

/// What `stop` says on stderr when it refuses, naming `impact`.
pub(super) fn refusal(impact: &StopImpact) -> String {
    let mut named: Vec<String> = impact
        .sessions
        .iter()
        .map(|session| {
            let title = if session.title.is_empty() {
                session.id.as_str()
            } else {
                session.title.as_str()
            };
            format!("session \"{title}\" ({})", status_words(&session.status))
        })
        .chain(impact.shell_terminals.iter().map(|terminal| {
            if terminal.title.is_empty() {
                "a running shell terminal".to_owned()
            } else {
                format!("shell terminal \"{}\" (running)", terminal.title)
            }
        }))
        .collect();
    let more = named.len().saturating_sub(NAMED);
    named.truncate(NAMED);
    let mut lines = vec![format!(
        "{} is running work that a stop would interrupt; nothing was stopped:",
        crate::APP_NAME
    )];
    lines.extend(named.into_iter().map(|line| format!("  {line}")));
    if more > 0 {
        lines.push(format!("  and {more} more"));
    }
    lines.push("run `octoboardd stop --force` to stop it anyway".to_owned());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::background::tests::{answer_once, owner_on, KEY};
    use crate::coordinator::handover_check::{ImpactSession, ImpactTerminal};

    fn ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    /// What the daemon lists is read as the impact, from the route the command asks.
    #[test]
    fn the_daemons_answer_to_the_stop_check_is_read() {
        let body =
            r#"{"sessions":[{"id":"s","title":"T","status":"working"}],"shell_terminals":[]}"#;
        let (port, served) = answer_once(ok(body));
        let impact = ask(&owner_on(port), Some(KEY)).expect("an answer");
        assert_eq!(impact.sessions.len(), 1);
        assert!(!impact.is_empty());
        assert!(served.join().unwrap().starts_with("POST /stop-check?key="));
    }

    /// A daemon with no such route (an earlier build), and one whose key is not recorded, cannot
    /// say what a stop would interrupt.
    #[test]
    fn a_daemon_that_cannot_be_asked_says_nothing() {
        let (port, _served) =
            answer_once("HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
        assert!(ask(&owner_on(port), Some(KEY)).is_none());
        assert!(ask(&owner_on(port), None).is_none());
    }

    #[test]
    fn a_refusal_names_what_runs_and_the_way_to_force() {
        let impact = StopImpact {
            sessions: vec![
                ImpactSession {
                    id: "s1".to_owned(),
                    title: "Fix the build".to_owned(),
                    status: "waiting_user".to_owned(),
                },
                ImpactSession {
                    id: "s2".to_owned(),
                    title: String::new(),
                    status: "idle".to_owned(),
                },
            ],
            shell_terminals: vec![ImpactTerminal {
                id: "t".to_owned(),
                title: "zsh".to_owned(),
            }],
        };
        let text = refusal(&impact);
        assert!(
            text.contains("session \"Fix the build\" (waiting for you)"),
            "{text}"
        );
        assert!(
            text.contains("session \"s2\" (working in the background)"),
            "{text}"
        );
        assert!(text.contains("shell terminal \"zsh\" (running)"), "{text}");
        assert!(text.contains("octoboardd stop --force"), "{text}");
    }

    #[test]
    fn a_long_list_is_cut_and_counted() {
        let impact = StopImpact {
            sessions: (0..13)
                .map(|n| ImpactSession {
                    id: n.to_string(),
                    title: format!("s{n}"),
                    status: "working".to_owned(),
                })
                .collect(),
            shell_terminals: Vec::new(),
        };
        let text = refusal(&impact);
        assert!(text.contains("and 3 more"), "{text}");
        assert!(!text.contains("\"s12\""), "{text}");
    }
}
