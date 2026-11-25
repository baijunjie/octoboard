//! `octoboardd channel` — the helper an extension inside
//! an agent's process starts to carry the session's messages (`crate::session::channel`). It
//! long-polls the daemon for the messages to send and prints each as one JSON line on its standard
//! output (`{"id", "text"}`); it reads one JSON line per outcome on its standard input (`{"id",
//! "outcome": "taken" | "refused" | "returned", "reason"?}`) and posts it. The extension holds no
//! key and speaks no HTTP: both stay here.
//!
//! Each run of the helper makes up an instance id and names it in every poll: the daemon hands a
//! message over again only to the instance it was handed to, so a helper started in another's
//! place (the extension reloaded) is never handed a message the first extension may have sent.
//!
//! It prints nothing else and nothing on standard error, and exits 0 whatever happens: once its
//! standard input closes (the extension, or the agent, is gone), once the daemon answers that the
//! session has no channel (`404`; a daemon or a holder that predates the channel answers the
//! same), once its key is refused, or once nothing answers on the port any more. While a holder
//! answers that no daemon is there, or the daemon that the session is still being started (both
//! `503`), it waits and asks again: the daemon that starts next takes the channel up, and a
//! launch's channel is there once the launch is registered.

use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::session::channel::{Handed, Outcome, PollReply, PollRequest, Reported};

/// How long one poll waits for the daemon's answer: longer than the daemon holds a poll open
/// (`crate::session::channel::Timing::poll`) with the time a holder waits for a daemon to link and
/// the daemon for its adoption to end on top.
const POLL_TIMEOUT: Duration = Duration::from_secs(90);

/// How long posting one outcome waits for the daemon's answer.
const REPORT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the helper waits before it asks again after an answer that was no answer.
const RETRY_PAUSE: Duration = Duration::from_secs(1);

/// How many times in a row nothing may answer on the port before the helper ends: the port is the
/// daemon's or the holder's, and the session's terminal does not outlive either.
const UNREACHABLE_LIMIT: u32 = 5;

/// How many times an outcome is posted before it is given up: about as long as a daemon being
/// replaced is away.
const REPORT_ATTEMPTS: u32 = 30;

/// The ids the helper holds at most. Past it the oldest is forgotten, which at worst has that
/// message printed again.
const HELD_LIMIT: usize = 256;

/// Where the helper posts, who it is, and what it waits between attempts.
pub(crate) struct Link {
    pub(crate) port: u16,
    pub(crate) key: String,
    pub(crate) session: String,
    /// This run's instance id ([`new_instance`]), named in every poll.
    pub(crate) instance: String,
    pub(crate) retry_pause: Duration,
}

/// An instance id for one run of the helper: unlike any other run's, for the session's lifetime.
pub(crate) fn new_instance() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

impl Link {
    fn post(&self, request: &PollRequest, timeout: Duration) -> std::io::Result<Vec<u8>> {
        let body = serde_json::to_vec(request).expect("a poll is JSON");
        crate::loopback::post(
            self.port,
            &self.key,
            &format!("/channel/{}", self.session),
            &body,
            timeout,
        )
    }
}

/// The ids of the messages printed whose outcome the daemon has not been told yet. The daemon
/// hands a message over again to a poll that does not list it, so an id stays here until its
/// outcome is posted.
pub(crate) type Held = Arc<Mutex<Vec<String>>>;

/// Runs the helper until it ends. Never fails: the caller exits 0 regardless.
pub fn run(session: &str, port: u16, key_file: &Path) {
    let Some(key) = crate::key_file::read(key_file) else {
        return;
    };
    let link = Arc::new(Link {
        port,
        key,
        session: session.to_string(),
        instance: new_instance(),
        retry_pause: RETRY_PAUSE,
    });
    let held = Held::default();
    {
        let (link, held) = (link.clone(), held.clone());
        std::thread::spawn(move || {
            report_outcomes(&link, std::io::stdin().lock().lines(), &held);
            // The extension closed the pipe: it is gone, and a poll in flight is nothing to wait
            // for.
            std::process::exit(0);
        });
    }
    let mut stdout = std::io::stdout().lock();
    hand_over_messages(&link, &held, &|| false, &mut |message| {
        serde_json::to_writer(&mut stdout, message)?;
        stdout.write_all(b"\n")?;
        stdout.flush()
    });
}

/// Polls for messages until the channel is gone and gives each one to `emit` once: a message
/// handed over again while it is held is not a new one. Ends once `emit` fails (the extension no
/// longer reads), once `ended` says so, and as the module says.
pub(crate) fn hand_over_messages(
    link: &Link,
    held: &Held,
    ended: &dyn Fn() -> bool,
    emit: &mut dyn FnMut(&Handed) -> std::io::Result<()>,
) {
    let mut unreachable = 0;
    while !ended() {
        let request = PollRequest {
            instance: link.instance.clone(),
            outcomes: Vec::new(),
            holding: locked(held).clone(),
            wait: true,
        };
        let answered = match link.post(&request, POLL_TIMEOUT) {
            Ok(answered) => answered,
            Err(err) => match after(&err) {
                Next::End => return,
                Next::AskAgain => {
                    unreachable = 0;
                    std::thread::sleep(link.retry_pause);
                    continue;
                }
                Next::Unreachable => {
                    unreachable += 1;
                    if unreachable >= UNREACHABLE_LIMIT {
                        return;
                    }
                    std::thread::sleep(link.retry_pause);
                    continue;
                }
            },
        };
        unreachable = 0;
        // An answer that does not read hands nothing over; the daemon hands it over again.
        let reply: PollReply = serde_json::from_slice(&answered).unwrap_or_default();
        for message in reply.messages {
            {
                let mut held = locked(held);
                if held.contains(&message.id) {
                    continue;
                }
                if held.len() == HELD_LIMIT {
                    held.remove(0);
                }
                held.push(message.id.clone());
            }
            if emit(&message).is_err() {
                return;
            }
        }
    }
}

/// Posts each outcome the extension writes, one JSON line each, in order, until its lines end. A
/// line that is no outcome is skipped. An outcome is posted before its message stops being held:
/// a poll made meanwhile still lists it, so the daemon does not hand the message over again.
pub(crate) fn report_outcomes(
    link: &Link,
    lines: impl Iterator<Item = std::io::Result<String>>,
    held: &Held,
) {
    for line in lines {
        let Ok(line) = line else {
            return;
        };
        let Ok(reported) = serde_json::from_str::<Reported>(&line) else {
            continue;
        };
        if reported.outcome == Outcome::Unknown {
            continue;
        }
        let id = reported.id.clone();
        let request = PollRequest {
            instance: link.instance.clone(),
            outcomes: vec![reported],
            holding: Vec::new(),
            wait: false,
        };
        for _ in 0..REPORT_ATTEMPTS {
            match link.post(&request, REPORT_TIMEOUT) {
                Ok(_) => break,
                Err(err) => match after(&err) {
                    Next::End => return,
                    Next::AskAgain | Next::Unreachable => std::thread::sleep(link.retry_pause),
                },
            }
        }
        locked(held).retain(|held| *held != id);
    }
}

fn locked(held: &Held) -> std::sync::MutexGuard<'_, Vec<String>> {
    held.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the helper does after a post that brought no answer.
#[derive(Debug, PartialEq, Eq)]
enum Next {
    /// The session has no channel, or the key is no longer good: nothing more to carry.
    End,
    /// A holder with no daemon linked, or one that lost it with the call in flight, a daemon
    /// still starting the session, or an answer that did not come in time: the next daemon
    /// takes the channel up, and a launch's channel is there once it is registered.
    AskAgain,
    /// Nothing answered on the port.
    Unreachable,
}

fn after(err: &std::io::Error) -> Next {
    match crate::loopback::error_status(err) {
        Some(crate::loopback::NO_DAEMON | crate::loopback::DAEMON_LOST) => Next::AskAgain,
        Some(status) if (400..500).contains(&status) => Next::End,
        Some(_) => Next::AskAgain,
        None if matches!(
            err.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ) =>
        {
            Next::AskAgain
        }
        None => Next::Unreachable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(status: u16) -> std::io::Error {
        let response = format!("HTTP/1.1 {status} X\r\nContent-Length: 0\r\n\r\n");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let answering = std::thread::spawn(move || {
            use std::io::Read;
            let mut stream = crate::test_support::accept_past_probes(&listener);
            // Past the request's head and its two-byte body.
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n{}") {
                stream.read_exact(&mut byte).expect("the request");
                request.push(byte[0]);
            }
            stream.write_all(response.as_bytes()).expect("an answer");
        });
        let err = crate::loopback::post(
            port,
            "k",
            "/channel/s",
            b"{}",
            crate::test_support::PATIENCE,
        )
        .expect_err("an error status");
        answering.join().expect("the stand-in answered");
        err
    }

    /// What the helper does with each answer that is none: it ends on a session without a channel
    /// (which is also what a daemon or holder that predates the channel answers) and on a refused
    /// key, asks again while a holder has no daemon or the answer was late, and counts a port
    /// nothing answers on.
    #[test]
    fn the_helper_ends_asks_again_or_counts_by_what_the_answer_was() {
        for (answered, next) in [
            (404, Next::End),
            (401, Next::End),
            (400, Next::End),
            (crate::loopback::NO_DAEMON, Next::AskAgain),
            (crate::loopback::DAEMON_LOST, Next::AskAgain),
            (500, Next::AskAgain),
        ] {
            assert_eq!(after(&status(answered)), next, "{answered}");
        }
        assert_eq!(
            after(&std::io::Error::from(std::io::ErrorKind::TimedOut)),
            Next::AskAgain
        );
        assert_eq!(
            after(&std::io::Error::from(std::io::ErrorKind::ConnectionRefused)),
            Next::Unreachable
        );
    }
}
