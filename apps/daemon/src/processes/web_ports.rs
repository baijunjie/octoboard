//! Which of a process tree's listening ports serve web pages: the ports a browser can open, so the
//! process list can offer them as links.
//!
//! A listening port says nothing of what is spoken on it: a dev server's page, its live-reload
//! socket, a debugger or a database all listen alike. So each port is asked, on loopback, for
//! `HEAD /`, and counts when it answers with an HTTP status line that is not a refusal of a plain
//! request (see [`serves_pages`]). `HEAD` asks for no body, so a server that renders its pages per
//! request does the least work for the question. Lists are polled every few seconds, so each
//! answer is kept for a while (`ANSWERED_FOR`, `UNANSWERED_FOR`) rather than asked again each
//! time.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::protocol::ProcessTree;

/// How long a port that answered HTTP is taken to keep serving it before it is asked again.
const ANSWERED_FOR: Duration = Duration::from_secs(60);

/// How long a port that did not answer HTTP is taken not to, before it is asked again: shorter,
/// so a server that listens a moment before it serves gets its link soon after.
const UNANSWERED_FOR: Duration = Duration::from_secs(15);

/// How long a connection to one loopback address may take. Loopback takes a connection at once
/// where something listens, so one not taken by then counts as nothing listening there; on
/// Windows a refusal itself takes longer than this.
const CONNECT_WITHIN: Duration = Duration::from_millis(300);

/// How long a port may take to answer the request once connected. A server that speaks another
/// protocol often waits silently for its own greeting, so this bounds what such a port costs.
const ANSWER_WITHIN: Duration = Duration::from_millis(800);

/// [`ANSWER_WITHIN`]; under test `PATIENCE`, so a stand-in server slowed by the suite's load still
/// answers in time. A test proving the window runs out passes [`ANSWER_WITHIN`] itself.
fn answer_within() -> Duration {
    #[cfg(test)]
    return crate::test_support::PATIENCE;
    #[cfg(not(test))]
    ANSWER_WITHIN
}

/// How many ports are asked at once: each probe holds a thread of its own while it waits, so a
/// listing is held to the daemon's width for a per-entity pass.
const PROBES_AT_ONCE: usize = crate::blocking_pass::BLOCKING_WIDTH;

/// The answers kept, by tree id and port: the id names the tree's top process with its start
/// time, so a port another process listens on later is asked again.
type Answers = HashMap<(String, u16), (bool, Instant)>;

/// The answers every listing shares.
static ANSWERS: LazyLock<WebPorts> = LazyLock::new(WebPorts::default);

/// Fills in `http_ports` of each of `trees`, asking each port not answered lately, on the calling
/// thread and threads it waits for. Blocking: called from a listing's blocking task.
pub(crate) fn fill(trees: &mut [ProcessTree]) {
    let within = answer_within();
    ANSWERS.fill(trees, |port| answers_http(port, within), Instant::now());
}

/// The answers kept between listings.
#[derive(Default)]
struct WebPorts {
    answers: Mutex<Answers>,
}

impl WebPorts {
    /// Fills in `http_ports` of each of `trees` from the answers kept at `now`, asking `ask` for
    /// each port without one, at most [`PROBES_AT_ONCE`] at a time, the next starting as soon as
    /// one ends. Answers kept past their time are dropped.
    fn fill(&self, trees: &mut [ProcessTree], ask: impl Fn(u16) -> bool + Sync, now: Instant) {
        let fresh = |(answered, at): &(bool, Instant)| {
            now.saturating_duration_since(*at)
                < if *answered {
                    ANSWERED_FOR
                } else {
                    UNANSWERED_FOR
                }
        };
        let unasked: Vec<(String, u16)> = {
            let mut answers = self.answers.lock().unwrap_or_else(|e| e.into_inner());
            answers.retain(|_, answer| fresh(answer));
            trees
                .iter()
                .flat_map(|tree| tree.ports.iter().map(|port| (tree.id.clone(), *port)))
                .filter(|key| !answers.contains_key(key))
                .collect()
        };
        let next = AtomicUsize::new(0);
        let answered: Vec<AtomicBool> = unasked.iter().map(|_| AtomicBool::new(false)).collect();
        std::thread::scope(|scope| {
            for _ in 0..PROBES_AT_ONCE.min(unasked.len()) {
                scope.spawn(|| loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some((_, port)) = unasked.get(index) else {
                        break;
                    };
                    // A probe that panics answers no, as a port that gave no answer does, rather
                    // than failing the listing and leaving the ports after it unasked.
                    let served = std::panic::catch_unwind(AssertUnwindSafe(|| ask(*port)));
                    answered[index].store(served.unwrap_or(false), Ordering::Relaxed);
                });
            }
        });
        let asked = unasked
            .into_iter()
            .zip(answered.into_iter().map(AtomicBool::into_inner));
        let mut answers = self.answers.lock().unwrap_or_else(|e| e.into_inner());
        for (key, answered) in asked {
            answers.insert(key, (answered, now));
        }
        for tree in trees.iter_mut() {
            tree.http_ports = Some(
                tree.ports
                    .iter()
                    .copied()
                    .filter(|port| {
                        answers
                            .get(&(tree.id.clone(), *port))
                            .is_some_and(|(answered, _)| *answered)
                    })
                    .collect(),
            );
        }
    }
}

/// Whether `port` answers HTTP on loopback: over IPv4, or over IPv6 when nothing takes the IPv4
/// connection (a server listening on `::1` alone). The answer may take `within`.
fn answers_http(port: u16, within: Duration) -> bool {
    let addresses = [
        SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        SocketAddr::from((Ipv6Addr::LOCALHOST, port)),
    ];
    addresses.iter().find_map(|address| {
        TcpStream::connect_timeout(address, CONNECT_WITHIN)
            .ok()
            .map(|stream| speaks_http(stream, port, within))
    }) == Some(true)
}

/// Asks the server on `stream` for `HEAD /` and reads the start of its answer, within `within` in
/// all.
fn speaks_http(mut stream: TcpStream, port: u16, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    // The host a browser would send, which servers that check it (Vite's, for one) accept.
    let request = format!(
        "HEAD / HTTP/1.1\r\nHost: localhost:{port}\r\nUser-Agent: Octoboard\r\nAccept: */*\r\n\
         Connection: close\r\n\r\n"
    );
    if stream.set_write_timeout(Some(within)).is_err()
        || stream.write_all(request.as_bytes()).is_err()
    {
        return false;
    }
    let mut head = [0u8; STATUS_LINE_START];
    let mut read = 0;
    while read < head.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || stream.set_read_timeout(Some(left)).is_err() {
            return false;
        }
        match stream.read(&mut head[read..]) {
            Ok(0) | Err(_) => return false,
            Ok(n) => read += n,
        }
    }
    serves_pages(&head)
}

/// How much of an answer [`serves_pages`] needs: `HTTP/1.1 200`.
const STATUS_LINE_START: usize = 12;

/// Whether an answer starting with `head` is one a browser can show: an HTTP status line, other
/// than a refusal of a plain request by a server that takes only another kind: `426 Upgrade
/// Required` from a WebSocket server (a dev server's live-reload port), `400 Bad Request` from
/// Node's inspector (a debugger port). Neither has a page to show.
fn serves_pages(head: &[u8]) -> bool {
    let Some(rest) = head.strip_prefix(b"HTTP/") else {
        return false;
    };
    let Some(space) = rest.iter().position(|byte| *byte == b' ') else {
        return false;
    };
    let status = &rest[space + 1..];
    status.len() >= 3
        && status[..3].iter().all(u8::is_ascii_digit)
        && !matches!(&status[..3], b"400" | b"426")
}

#[cfg(test)]
mod tests;
