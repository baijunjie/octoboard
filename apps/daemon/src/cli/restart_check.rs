//! What `octoboardd restart` and `octoboardd update` (after an update or a rollback) ask the
//! running daemon before a planned restart, and what they decide from its answer: the same check
//! and confirmation as the in-app restart. The daemon tells what a restart into a build of a given
//! version and holder floor would interrupt (`POST /restart-check`, answered by
//! `coordinator::handover_check::restart_impact`); a restart that interrupts nothing goes ahead,
//! and one that would, or may, is asked about where there is a terminal to ask in, and otherwise
//! goes ahead or not as the [`Mode`] says. A daemon that does not answer the check (a build from
//! before it) is taken as one that cannot tell what the restart would interrupt.

use std::io::{BufRead, Write};
use std::time::Duration;

use anyhow::Result;

use super::background::Proceed;
use crate::coordinator::handover_check::{RestartImpact, RestartVerdict};
use crate::loopback;
use crate::owner_record::Owner;

/// How long the check may take: the daemon reads its holders and the store, briefly.
const CHECK_WAIT: Duration = Duration::from_secs(10);

/// The build a planned restart would start: its version and its holder floor (the oldest holder
/// version it adopts), each `None` where not known, which counts every running session as one the
/// restart may interrupt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Target {
    pub version: Option<String>,
    pub holder_floor: Option<String>,
}

impl Target {
    /// This build, as the one a restart starts: what `octoboardd restart` restarts into, taking
    /// the file it runs from for the one the daemon starts its next daemon from, as `update`
    /// replaces it.
    pub(super) fn this_build() -> Self {
        Self {
            version: Some(crate::self_update::RUNNING.to_owned()),
            holder_floor: Some(crate::holder::compat::FLOOR.to_owned()),
        }
    }
}

/// How a restart that would or may interrupt work is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// It goes ahead, asking nothing: `--yes`, or `restart` with no terminal to ask in.
    Regardless,
    /// The user is asked first: a terminal to ask in.
    Ask,
    /// It does not go ahead: `--if-safe`, or `update` with no terminal to ask in.
    OnlyIfSafe,
}

/// What came of the check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Decision {
    /// Restart.
    Restart,
    /// Do not: the user declined, or the mode does not restart what it would interrupt. What it
    /// would interrupt, and that it is not restarted, has been said.
    Leave,
    /// Do not ask for it: the daemon would refuse it, which has been said. The command fails.
    Refused,
    /// Do not ask for it: a stop or restart is under way already, which has been said. The
    /// command follows it to its end, as it follows a restart it requested.
    Follow,
}

/// Asks the daemon `owner` names, with `key`, what a restart into `target` would interrupt.
/// `None` where it cannot be told: no key recorded, or the daemon does not answer the check (a
/// build from before it answers `404`) or fails to.
pub(super) fn ask(owner: &Owner, key: Option<&str>, target: &Target) -> Option<RestartImpact> {
    let key = key?;
    let body = serde_json::json!({
        "holder_floor": target.holder_floor,
        "target_version": target.version,
    })
    .to_string();
    let answer = loopback::post(
        owner.port,
        key,
        "/restart-check",
        body.as_bytes(),
        CHECK_WAIT,
    )
    .map_err(|err| tracing::debug!(%err, "the restart check was not answered"))
    .ok()?;
    serde_json::from_slice(&answer).ok()
}

/// Tells the daemon that runs, if any, that `octoboardd update` has replaced its file, by asking
/// the check and leaving the answer: the daemon learns of the update then, and every browser
/// offers the restart, rather than at its next check. For an update that is not followed by a
/// restart here. Best effort.
pub(super) fn tell_daemon() {
    if let Some(owner) = Owner::recorded() {
        ask(&owner, owner.recorded_key().as_deref(), &Target::default());
    }
}

/// What [`restart_into`] came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Restarted {
    /// The command came to an end of its own with this exit status: the restart requested (or
    /// one under way followed), refused before it was asked for, or its daemon found ended.
    Ran(i32),
    /// It was not requested ([`Decision::Leave`]); what it would interrupt has been said.
    Left,
}

/// Restarts the daemon that runs into `target` as `mode` decides, asking the user on the terminal
/// where it asks: what `octoboardd restart` and `octoboardd update` do. A restart made regardless
/// asks the daemon nothing first, as `octoboardd restart` always did.
pub(super) fn restart_into(target: &Target, mode: Mode) -> Result<Restarted> {
    let mut left = false;
    let version = target
        .version
        .clone()
        .unwrap_or_else(|| "the installed build".to_owned());
    let status = super::background::restart(&mut |owner, key| {
        if mode == Mode::Regardless {
            return Ok(Proceed::Request);
        }
        let impact = ask(owner, key, target);
        let stdin = std::io::stdin();
        let decision = decide(
            impact.as_ref(),
            &version,
            mode,
            &mut std::io::stdout(),
            &mut |out| confirm_from(&mut stdin.lock(), out),
        )?;
        Ok(match decision {
            Decision::Restart => Proceed::Request,
            Decision::Follow => Proceed::Follow,
            Decision::Leave => {
                left = true;
                Proceed::End(0)
            }
            Decision::Refused => Proceed::End(1),
        })
    })?;
    Ok(if left {
        Restarted::Left
    } else {
        Restarted::Ran(status)
    })
}

/// The [`Mode`] of a command given `--yes` (`regardless`) or `--if-safe` (`only_if_safe`), or
/// neither: it asks where its standard input is a terminal, and otherwise does as `unattended`.
pub(super) fn mode_of(regardless: bool, only_if_safe: bool, unattended: Mode) -> Mode {
    use std::io::IsTerminal as _;
    if regardless {
        Mode::Regardless
    } else if only_if_safe {
        Mode::OnlyIfSafe
    } else if std::io::stdin().is_terminal() {
        Mode::Ask
    } else {
        unattended
    }
}

/// The exit status of `octoboardd restart --if-safe` when it does not restart because the restart
/// would or may interrupt work, so that a script (the install script) can tell it from a restart
/// made.
pub(super) const DECLINED: i32 = 4;

/// `octoboardd restart`: a restart into this build, asked about as `mode` says, which with no
/// terminal and no option restarts regardless, as it always has. The exit status.
pub(super) fn run_restart(mode: Mode) -> Result<i32> {
    // What a restart not made says is said as it is decided ([`decide`]); with `--if-safe` it
    // names no command to run next, which the install script names as it runs it.
    let restarted = restart_into(&Target::this_build(), mode)?;
    Ok(exit_status(restarted, mode))
}

/// The exit status of `octoboardd restart` in `mode` once it came to `restarted`: a restart's own
/// status; for one not made, [`DECLINED`] where `--if-safe` declined it, and 0 where the user
/// declined it when asked.
pub(super) fn exit_status(restarted: Restarted, mode: Mode) -> i32 {
    match restarted {
        Restarted::Ran(status) => status,
        Restarted::Left if mode == Mode::OnlyIfSafe => DECLINED,
        Restarted::Left => 0,
    }
}

/// Decides a restart into `target` (`version` names it in what is said) from the daemon's
/// `impact`, in `mode`, saying on `out` what it would interrupt, and asking `confirm` where the
/// mode asks:
///
/// - a stop or restart already under way is not added to: it is said, and followed to its end
///   ([`Decision::Follow`]), the command succeeding where a daemon comes up in its place, as
///   with `--yes`;
/// - a restart that interrupts nothing goes ahead, also where the daemon foresees a refusal other
///   than a stop under way: the request then says the daemon's reason;
/// - a restart that would or may interrupt work and that the daemon foresees refusing is not
///   asked about or requested: the refusal is said, and the command fails, so that nobody is
///   asked to confirm a restart that will not happen, and a refusal that clears meanwhile cannot
///   let an unconfirmed restart interrupt work;
/// - any other is described, then asked about, made or left as `mode` says.
///
/// It is total over the modes on purpose, though [`restart_into`] asks the daemon nothing for
/// [`Mode::Regardless`]: there a restart goes ahead whatever the daemon says.
pub(super) fn decide(
    impact: Option<&RestartImpact>,
    version: &str,
    mode: Mode,
    out: &mut dyn Write,
    confirm: &mut dyn FnMut(&mut dyn Write) -> bool,
) -> Result<Decision> {
    let Some(impact) = impact else {
        return unknown(version, mode, out, confirm);
    };
    if let Some(refusal) = impact.refusal.as_ref().filter(|_| mode != Mode::Regardless) {
        if refusal.reason == crate::state::RestartRefusal::StopInProgress.code() {
            writeln!(
                out,
                "{} is already stopping or restarting; following that rather than asking for \
                 another restart",
                crate::APP_NAME
            )?;
            return Ok(Decision::Follow);
        }
        if impact.verdict != RestartVerdict::Safe {
            writeln!(
                out,
                "{} would refuse the restart: {}",
                crate::APP_NAME,
                refusal.message
            )?;
            return Ok(Decision::Refused);
        }
    }
    if impact.verdict == RestartVerdict::Safe {
        writeln!(
            out,
            "restarting {} into {version}; every session keeps running",
            crate::APP_NAME
        )?;
        return Ok(Decision::Restart);
    }
    describe(impact, version, out)?;
    settle(
        mode,
        impact.verdict == RestartVerdict::Interrupts,
        out,
        confirm,
    )
}

/// [`decide`] where the daemon did not say what the restart would interrupt.
fn unknown(
    version: &str,
    mode: Mode,
    out: &mut dyn Write,
    confirm: &mut dyn FnMut(&mut dyn Write) -> bool,
) -> Result<Decision> {
    if mode == Mode::Regardless {
        return Ok(Decision::Restart);
    }
    writeln!(
        out,
        "the running {} cannot say what restarting into {version} would interrupt, so it may \
         interrupt any running session",
        crate::APP_NAME
    )?;
    settle(mode, false, out, confirm)
}

/// Goes ahead, asks or leaves it as `mode` says, once what the restart interrupts (`certain`:
/// would, rather than may) has been said, saying so where it is not made.
fn settle(
    mode: Mode,
    certain: bool,
    out: &mut dyn Write,
    confirm: &mut dyn FnMut(&mut dyn Write) -> bool,
) -> Result<Decision> {
    let decision = match mode {
        Mode::Regardless => Decision::Restart,
        Mode::OnlyIfSafe => Decision::Leave,
        Mode::Ask if confirm(out) => Decision::Restart,
        Mode::Ask => Decision::Leave,
    };
    if decision == Decision::Leave {
        match mode {
            Mode::OnlyIfSafe if certain => writeln!(
                out,
                "not restarting, as the restart would interrupt the work above"
            )?,
            Mode::OnlyIfSafe => writeln!(
                out,
                "not restarting, as the restart may interrupt the work above"
            )?,
            _ => writeln!(out, "not restarted")?,
        }
    }
    Ok(decision)
}

/// Says what a restart into `version` would interrupt, or may.
fn describe(impact: &RestartImpact, version: &str, out: &mut dyn Write) -> Result<()> {
    let sessions = impact.sessions.len();
    let terminals = impact.shell_terminals.len() + impact.unreached_shell_terminals;
    let mut what = Vec::new();
    if sessions > 0 {
        what.push(counted(sessions, "session", "sessions"));
    }
    if terminals > 0 {
        what.push(counted(terminals, "shell terminal", "shell terminals"));
    }
    let what = what.join(" and ");
    if impact.verdict == RestartVerdict::Interrupts {
        writeln!(
            out,
            "restarting into {version} would interrupt {what}, which {version} cannot take over:"
        )?;
    } else {
        writeln!(
            out,
            "restarting into {version} may interrupt {what}: which ones {version} takes over \
             cannot be told:"
        )?;
    }
    for session in &impact.sessions {
        let title = if session.title.is_empty() {
            String::new()
        } else {
            format!(
                " \"{}\"",
                crate::messaging::naming::one_line(&session.title)
            )
        };
        let status = status_phrase(&session.status)
            .map(|status| format!(", {status}"))
            .unwrap_or_default();
        writeln!(out, "  - session{title} ({}){status}", session.id)?;
    }
    for terminal in &impact.shell_terminals {
        let title = if terminal.title.is_empty() {
            String::new()
        } else {
            format!(
                " \"{}\"",
                crate::messaging::naming::one_line(&terminal.title)
            )
        };
        writeln!(
            out,
            "  - shell terminal{title} ({}), running a command",
            terminal.id
        )?;
    }
    if impact.unreached_shell_terminals > 0 {
        writeln!(
            out,
            "  - {} in a process holder that cannot be reached now",
            counted(
                impact.unreached_shell_terminals,
                "shell terminal",
                "shell terminals"
            )
        )?;
    }
    writeln!(
        out,
        "every other session keeps running; an interrupted session can be resumed"
    )?;
    Ok(())
}

fn counted(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// How a session's status reads in the list; `None` for one not known to this build.
fn status_phrase(status: &str) -> Option<&'static str> {
    Some(match status {
        "starting" | "working" | "dormant" => "working",
        "waiting_user" => "waiting for input",
        "idle" => "idle",
        _ => return None,
    })
}

/// Asks on `out` whether to restart now, reading the answer from `input`: only a yes restarts,
/// and the end of the input is a no.
pub(super) fn confirm_from(input: &mut dyn BufRead, out: &mut dyn Write) -> bool {
    if write!(out, "Restart now? [y/N] ")
        .and_then(|()| out.flush())
        .is_err()
    {
        return false;
    }
    let mut answer = String::new();
    if input.read_line(&mut answer).unwrap_or(0) == 0 {
        writeln!(out).ok();
        return false;
    }
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::background::tests::{accept_request, owner_on, KEY};
    use crate::coordinator::handover_check::{ImpactRefusal, ImpactSession, ImpactTerminal};
    use crate::test_support::ClosedPort;

    fn impact(verdict: RestartVerdict) -> RestartImpact {
        RestartImpact {
            verdict,
            known: verdict != RestartVerdict::MayInterrupt,
            sessions: Vec::new(),
            shell_terminals: Vec::new(),
            unreached_shell_terminals: 0,
            refusal: None,
        }
    }

    fn interrupting(verdict: RestartVerdict) -> RestartImpact {
        RestartImpact {
            sessions: vec![
                ImpactSession {
                    id: "s1".into(),
                    title: "Fix\nthe build".into(),
                    status: "working".into(),
                },
                ImpactSession {
                    id: "s2".into(),
                    title: String::new(),
                    status: "something new".into(),
                },
            ],
            shell_terminals: vec![ImpactTerminal {
                id: "t1".into(),
                title: "zsh".into(),
            }],
            unreached_shell_terminals: 2,
            ..impact(verdict)
        }
    }

    /// Runs [`decide`]: the decision, what was said, and how often the user was asked.
    fn run(impact: Option<&RestartImpact>, mode: Mode, yes: bool) -> (Decision, String, usize) {
        let mut out = Vec::new();
        let mut asked = 0;
        let decision = decide(impact, "1.2.0", mode, &mut out, &mut |_| {
            asked += 1;
            yes
        })
        .unwrap();
        (decision, String::from_utf8(out).unwrap(), asked)
    }

    #[test]
    fn a_safe_restart_goes_ahead_in_every_mode_without_asking() {
        for mode in [Mode::Regardless, Mode::Ask, Mode::OnlyIfSafe] {
            let (decision, out, asked) = run(Some(&impact(RestartVerdict::Safe)), mode, false);
            assert_eq!((decision, asked), (Decision::Restart, 0), "{mode:?}");
            assert!(out.contains("every session keeps running"), "{out}");
        }
    }

    #[test]
    fn an_interrupting_restart_is_asked_about_left_or_made_as_the_mode_says() {
        let impact = interrupting(RestartVerdict::Interrupts);
        let (decision, out, asked) = run(Some(&impact), Mode::Ask, true);
        assert_eq!((decision, asked), (Decision::Restart, 1));
        assert!(
            out.contains("would interrupt 2 sessions and 3 shell terminals"),
            "{out}"
        );
        assert!(
            out.contains("session \"Fix the build\" (s1), working"),
            "{out}"
        );
        assert!(out.contains("  - session (s2)\n"), "{out}");
        assert!(
            out.contains("shell terminal \"zsh\" (t1), running a command"),
            "{out}"
        );
        assert!(
            out.contains("2 shell terminals in a process holder"),
            "{out}"
        );

        let (decision, _, asked) = run(Some(&impact), Mode::Ask, false);
        assert_eq!((decision, asked), (Decision::Leave, 1));
        let (decision, out, asked) = run(Some(&impact), Mode::OnlyIfSafe, true);
        assert_eq!((decision, asked), (Decision::Leave, 0));
        assert!(out.contains("would interrupt"), "{out}");
        let (decision, out, asked) = run(Some(&impact), Mode::Regardless, false);
        assert_eq!((decision, asked), (Decision::Restart, 0));
        assert!(out.contains("would interrupt"), "{out}");
    }

    #[test]
    fn a_restart_whose_range_is_not_known_may_interrupt() {
        let impact = interrupting(RestartVerdict::MayInterrupt);
        let (decision, out, asked) = run(Some(&impact), Mode::Ask, false);
        assert_eq!((decision, asked), (Decision::Leave, 1));
        assert!(out.contains("may interrupt 2 sessions"), "{out}");
    }

    /// A daemon that does not answer the check cannot tell: asked about, or left where nobody can
    /// be asked, except where the restart goes ahead regardless.
    #[test]
    fn a_daemon_that_does_not_answer_is_taken_as_one_that_cannot_tell() {
        let (decision, out, asked) = run(None, Mode::Ask, true);
        assert_eq!((decision, asked), (Decision::Restart, 1));
        assert!(out.contains("cannot say what restarting"), "{out}");
        let (decision, _, _) = run(None, Mode::OnlyIfSafe, true);
        assert_eq!(decision, Decision::Leave);
        let (decision, out, asked) = run(None, Mode::Regardless, false);
        assert_eq!((decision, asked, out.as_str()), (Decision::Restart, 0, ""));
    }

    /// A refusal the daemon foresees is left to the restart request, which says it: nobody is asked
    /// to confirm a restart that will not happen.
    #[test]
    fn a_foreseen_refusal_is_not_asked_about() {
        let mut impact = interrupting(RestartVerdict::Interrupts);
        impact.refusal = Some(ImpactRefusal {
            reason: "agent_update_running".into(),
            message: "an agent CLI update is running".into(),
        });
        for mode in [Mode::Ask, Mode::OnlyIfSafe] {
            let (decision, out, asked) = run(Some(&impact), mode, false);
            assert_eq!((decision, asked), (Decision::Refused, 0), "{mode:?}");
            assert!(
                out.contains("would refuse the restart: an agent CLI update is running"),
                "{out}"
            );
            assert!(!out.contains("would interrupt"), "{out}");
        }
        // Where nothing would be interrupted the restart is requested, and the daemon says why
        // it refuses, or restarts where the refusal has cleared.
        let mut safe = self::impact(RestartVerdict::Safe);
        safe.refusal = impact.refusal.clone();
        let (decision, _, asked) = run(Some(&safe), Mode::OnlyIfSafe, false);
        assert_eq!((decision, asked), (Decision::Restart, 0));
        // Regardless, the restart is requested whatever the daemon foresees.
        let (decision, _, asked) = run(Some(&impact), Mode::Regardless, false);
        assert_eq!((decision, asked), (Decision::Restart, 0));
    }

    /// A stop or restart under way is not added to, whatever the restart would interrupt: it is
    /// said and followed.
    #[test]
    fn a_stop_under_way_is_said_and_followed() {
        for verdict in [RestartVerdict::Safe, RestartVerdict::Interrupts] {
            let mut impact = interrupting(verdict);
            impact.refusal = Some(ImpactRefusal {
                reason: "stop_in_progress".into(),
                message: "the daemon is already stopping or restarting".into(),
            });
            for mode in [Mode::Ask, Mode::OnlyIfSafe] {
                let (decision, out, asked) = run(Some(&impact), mode, true);
                assert_eq!((decision, asked), (Decision::Follow, 0), "{mode:?}");
                assert!(out.contains("already stopping or restarting"), "{out}");
                assert!(!out.contains("would interrupt"), "{out}");
            }
        }
    }

    /// A restart made regardless is requested whatever the daemon foresees, a stop under way
    /// included, and nobody is asked; where nothing would be interrupted the request goes ahead in
    /// every mode that is not told a stop is under way.
    #[test]
    fn a_restart_regardless_is_requested_whatever_the_daemon_foresees() {
        for reason in [
            "stop_in_progress",
            "agent_update_running",
            "follows_application",
        ] {
            for verdict in [
                RestartVerdict::Safe,
                RestartVerdict::Interrupts,
                RestartVerdict::MayInterrupt,
            ] {
                let mut impact = interrupting(verdict);
                impact.refusal = Some(ImpactRefusal {
                    reason: reason.into(),
                    message: "it would not".into(),
                });
                let (decision, out, asked) = run(Some(&impact), Mode::Regardless, false);
                assert_eq!(
                    (decision, asked),
                    (Decision::Restart, 0),
                    "{reason} {verdict:?}"
                );
                assert!(!out.contains("would refuse"), "{out}");
                assert!(!out.contains("already stopping"), "{out}");
            }
        }
        // A refusal that is not a stop under way, where nothing would be interrupted, is left to
        // the request, in a mode that asks too.
        let mut safe = impact(RestartVerdict::Safe);
        safe.refusal = Some(ImpactRefusal {
            reason: "agent_update_running".into(),
            message: "an agent CLI update is running".into(),
        });
        let (decision, out, asked) = run(Some(&safe), Mode::Ask, false);
        assert_eq!((decision, asked), (Decision::Restart, 0));
        assert!(!out.contains("would refuse"), "{out}");
    }

    /// The refusal is said with the daemon's own message, for a restart that may interrupt work
    /// as for one that would.
    #[test]
    fn a_foreseen_refusal_is_said_with_the_daemons_message_for_would_and_may() {
        for verdict in [RestartVerdict::Interrupts, RestartVerdict::MayInterrupt] {
            let mut impact = interrupting(verdict);
            impact.refusal = Some(ImpactRefusal {
                reason: "holders_in_service".into(),
                message: "a holder is in service".into(),
            });
            let (decision, out, _) = run(Some(&impact), Mode::OnlyIfSafe, false);
            assert_eq!(decision, Decision::Refused, "{verdict:?}");
            assert!(
                out.contains(&format!(
                    "{} would refuse the restart: a holder is in service",
                    crate::APP_NAME
                )),
                "{out}"
            );
        }
    }

    /// A restart `--if-safe` leaves says whether it would or only may interrupt work, and names
    /// no command to run next, which the install script names itself.
    #[test]
    fn a_restart_left_says_would_or_may_as_the_verdict_does() {
        let (_, out, _) = run(
            Some(&interrupting(RestartVerdict::Interrupts)),
            Mode::OnlyIfSafe,
            false,
        );
        assert!(
            out.ends_with("not restarting, as the restart would interrupt the work above\n"),
            "{out}"
        );
        let (_, out, _) = run(
            Some(&interrupting(RestartVerdict::MayInterrupt)),
            Mode::OnlyIfSafe,
            false,
        );
        assert!(
            out.ends_with("not restarting, as the restart may interrupt the work above\n"),
            "{out}"
        );
        assert!(!out.contains("octoboardd restart"), "{out}");
        let (_, out, _) = run(None, Mode::OnlyIfSafe, false);
        assert!(out.ends_with("may interrupt the work above\n"), "{out}");
        let (_, out, _) = run(
            Some(&interrupting(RestartVerdict::Interrupts)),
            Mode::Ask,
            false,
        );
        assert!(out.ends_with("not restarted\n"), "{out}");
    }

    #[test]
    fn only_a_yes_confirms() {
        for (input, expected) in [
            ("y\n", true),
            ("Yes\n", true),
            (" y \n", true),
            ("n\n", false),
            ("\n", false),
            ("", false),
            ("yeah\n", false),
        ] {
            let mut out = Vec::new();
            assert_eq!(
                confirm_from(&mut input.as_bytes(), &mut out),
                expected,
                "{input:?}"
            );
            assert!(String::from_utf8(out)
                .unwrap()
                .starts_with("Restart now? [y/N] "));
        }
    }

    /// Answers one request with `response` and gives its request line and its body back through
    /// the handle.
    fn serve_once(response: impl Into<String>) -> (u16, std::thread::JoinHandle<(String, String)>) {
        let response = response.into();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let served = std::thread::spawn(move || {
            // A probe of the port (`test_support::is_port_probe`) is answered and left out.
            let (mut reader, request_line, body) = accept_request(&listener);
            reader
                .get_mut()
                .write_all(response.as_bytes())
                .expect("the answer");
            (request_line, String::from_utf8(body).expect("a text body"))
        });
        (port, served)
    }

    fn ok_with(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\
             Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    /// The check is asked of the daemon with its key, naming the holder floor and version the next
    /// daemon starts from, and the answer is read as the daemon gives it.
    #[test]
    fn the_check_is_asked_with_the_key_and_the_target_and_the_answer_is_read() {
        let answer = serde_json::to_string(&interrupting(RestartVerdict::Interrupts)).unwrap();
        let (port, served) = serve_once(ok_with(&answer));
        let target = Target {
            version: Some("1.2.0".into()),
            holder_floor: Some("1.0.0".into()),
        };
        let impact = ask(&owner_on(port), Some(KEY), &target).expect("an answer");
        assert_eq!(impact, interrupting(RestartVerdict::Interrupts));
        let (request_line, body) = served.join().unwrap();
        assert_eq!(
            request_line,
            format!("POST /restart-check?key={KEY} HTTP/1.1\r\n")
        );
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"holder_floor": "1.0.0", "target_version": "1.2.0"})
        );
    }

    /// What is not known of the build is said as unknown, not left to be taken as something else.
    #[test]
    fn a_target_not_known_is_asked_as_null() {
        let answer = serde_json::to_string(&impact(RestartVerdict::MayInterrupt)).unwrap();
        let (port, served) = serve_once(ok_with(&answer));
        ask(&owner_on(port), Some(KEY), &Target::default()).expect("an answer");
        let (_, body) = served.join().unwrap();
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"holder_floor": null, "target_version": null})
        );
    }

    /// A daemon that cannot answer is one that cannot tell: a build from before the route answers
    /// `404`, one with no key recorded is not asked, and one that fails, is not listening or
    /// answers what does not read says nothing either.
    #[test]
    fn a_daemon_that_cannot_answer_the_check_gives_no_impact() {
        let not_found = "HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
        let (port, served) = serve_once(not_found);
        assert_eq!(ask(&owner_on(port), Some(KEY), &Target::default()), None);
        served.join().unwrap();

        let failed =
            "HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
        let (port, served) = serve_once(failed);
        assert_eq!(ask(&owner_on(port), Some(KEY), &Target::default()), None);
        served.join().unwrap();

        let (port, served) = serve_once(ok_with("not an impact"));
        assert_eq!(ask(&owner_on(port), Some(KEY), &Target::default()), None);
        served.join().unwrap();

        let (port, served) = serve_once(ok_with(r#"{"verdict": "something new"}"#));
        assert_eq!(ask(&owner_on(port), Some(KEY), &Target::default()), None);
        served.join().unwrap();

        // No key to present: nobody is asked, so nothing need listen.
        let closed = ClosedPort::new();
        assert_eq!(
            ask(&owner_on(closed.port()), None, &Target::default()),
            None
        );
        // A daemon that is not listening.
        assert_eq!(
            ask(&owner_on(closed.port()), Some(KEY), &Target::default()),
            None
        );
    }

    /// Said of one session, or one shell terminal, the list is in the singular, and a session
    /// shows the status the daemon gave it in words: waiting for input and idle, working for the
    /// states that run, and nothing for one this build does not know.
    #[test]
    fn what_would_be_interrupted_is_described_in_the_singular_and_by_status() {
        for (status, words) in [
            ("working", Some("working")),
            ("starting", Some("working")),
            ("waiting_user", Some("waiting for input")),
            ("idle", Some("idle")),
            ("from a later build", None),
            ("", None),
        ] {
            let impact = RestartImpact {
                sessions: vec![ImpactSession {
                    id: "s1".into(),
                    title: "Only".into(),
                    status: status.into(),
                }],
                shell_terminals: vec![ImpactTerminal {
                    id: "t1".into(),
                    title: String::new(),
                }],
                ..impact(RestartVerdict::Interrupts)
            };
            let (_, out, _) = run(Some(&impact), Mode::OnlyIfSafe, false);
            assert!(
                out.contains("would interrupt 1 session and 1 shell terminal,"),
                "{out}"
            );
            let line = out
                .lines()
                .find(|line| line.contains("(s1)"))
                .expect("the session is listed");
            match words {
                Some(words) => assert!(line.ends_with(&format!(", {words}")), "{line}"),
                None => assert!(line.ends_with("(s1)"), "{line}"),
            }
            assert!(
                out.contains("  - shell terminal (t1), running a command"),
                "{out}"
            );
        }
    }

    /// Only terminals the daemon could not reach are enough to ask about, and are said so.
    #[test]
    fn unreached_terminals_alone_are_enough_to_ask_about() {
        let impact = RestartImpact {
            unreached_shell_terminals: 1,
            ..impact(RestartVerdict::Interrupts)
        };
        let (decision, out, asked) = run(Some(&impact), Mode::Ask, false);
        assert_eq!((decision, asked), (Decision::Leave, 1));
        assert!(out.contains("would interrupt 1 shell terminal,"), "{out}");
        assert!(
            out.contains("1 shell terminal in a process holder that cannot be reached"),
            "{out}"
        );
    }

    /// The option picks the mode whatever the terminal is; with neither, a terminal asks and
    /// anything else does as the command does when unattended.
    #[test]
    fn the_mode_follows_the_option_and_otherwise_the_terminal() {
        use std::io::IsTerminal as _;
        for unattended in [Mode::Regardless, Mode::OnlyIfSafe] {
            assert_eq!(mode_of(true, false, unattended), Mode::Regardless);
            assert_eq!(mode_of(false, true, unattended), Mode::OnlyIfSafe);
            let expected = if std::io::stdin().is_terminal() {
                Mode::Ask
            } else {
                unattended
            };
            assert_eq!(mode_of(false, false, unattended), expected);
        }
    }

    /// A restart made keeps its own status; one `--if-safe` declined is told from it by a status of
    /// its own, never 0; one the user declined when asked, or that goes ahead regardless, is no
    /// failure.
    #[test]
    fn the_exit_status_tells_a_restart_made_from_one_declined() {
        for mode in [Mode::Regardless, Mode::Ask, Mode::OnlyIfSafe] {
            for status in [0, 1, 2, 75] {
                assert_eq!(
                    exit_status(Restarted::Ran(status), mode),
                    status,
                    "{mode:?}"
                );
            }
        }
        assert_ne!(DECLINED, 0);
        assert_eq!(exit_status(Restarted::Left, Mode::OnlyIfSafe), DECLINED);
        assert_eq!(exit_status(Restarted::Left, Mode::Ask), 0);
        assert_eq!(exit_status(Restarted::Left, Mode::Regardless), 0);
        // A restart that failed is not mistaken for one declined.
        assert_ne!(DECLINED, 1);
    }

    /// `octoboardd restart` restarts into the build it is: its version and its holder floor.
    #[test]
    fn a_restart_from_the_command_line_is_into_this_build() {
        let target = Target::this_build();
        assert_eq!(target.version.as_deref(), Some(crate::self_update::RUNNING));
        assert_eq!(
            target.holder_floor.as_deref(),
            Some(crate::holder::compat::FLOOR)
        );
    }
}
