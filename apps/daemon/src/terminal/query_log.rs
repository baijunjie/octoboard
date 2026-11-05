//! Logging a program's device attributes and cursor position queries, with who answered each and
//! how long it took. A host that holds its program up until such a query is answered
//! (`TermFacts::host_waits_on_queries`) stalls it meanwhile, which delays what the program does
//! with its input, a pasted message among it. A program asks rarely, after a paste or a submit, so
//! each query and its answer is logged at info.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::terminal::reply_route::{ReplyRoute, STALE_AFTER};

/// A query logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Query {
    /// The primary device attributes (`CSI c`), answered `CSI ? … c`.
    DeviceAttributes,
    /// The cursor position report (`CSI 6 n`), answered `CSI row ; column R`.
    CursorPosition,
}

/// The answers to those queries in `bytes`, in order, whatever else `bytes` holds: the parsed
/// terminal's own, or a client's reply.
pub(crate) fn answers_in(bytes: &[u8]) -> Vec<Query> {
    let mut answers = Vec::new();
    let mut at = 0;
    while at + 1 < bytes.len() {
        if bytes[at] != 0x1b || bytes[at + 1] != b'[' {
            at += 1;
            continue;
        }
        let params = at + 2;
        let Some(end) = bytes[params..]
            .iter()
            .position(|byte| (0x40..=0x7e).contains(byte))
            .map(|end| params + end)
        else {
            break;
        };
        let params = &bytes[params..end];
        match bytes[end] {
            b'c' if params.starts_with(b"?") => answers.push(Query::DeviceAttributes),
            b'R' if is_position(params) => answers.push(Query::CursorPosition),
            _ => {}
        }
        at = end + 1;
    }
    answers
}

fn is_position(params: &[u8]) -> bool {
    let mut parts = params.split(|&byte| byte == b';');
    let is_number = |part: Option<&[u8]>| {
        part.is_some_and(|part| !part.is_empty() && part.iter().all(u8::is_ascii_digit))
    };
    is_number(parts.next()) && is_number(parts.next()) && parts.next().is_none()
}

/// How long a query waits before it is logged as still unanswered, once: what the slowest prompt
/// library waits before it gives up (crossterm's 2 seconds).
const STILL_UNANSWERED_AFTER: Duration = Duration::from_secs(2);

/// The most queries kept waiting; past it the oldest is given up on.
const MOST_PENDING: usize = 32;

struct Pending {
    /// The end of the output frame whose parse asked it, which an answer names.
    offset: u64,
    at: Instant,
    query: Query,
    /// Logged as still unanswered.
    warned: bool,
}

/// The queries of one program waiting for an answer, logged as they come and go.
pub(crate) struct QueryLog {
    label: String,
    pending: VecDeque<Pending>,
    /// The offsets of the queries answered and of those given up on, in order, for the tests.
    #[cfg(test)]
    outcomes: Vec<(u64, bool)>,
}

fn log_not_answered(label: &str, pending: &Pending, now: Instant) {
    tracing::info!(
        terminal = %label,
        offset = pending.offset,
        query = ?pending.query,
        waited_ms = now.saturating_duration_since(pending.at).as_millis(),
        "terminal query not answered"
    );
}

fn by_name(by: u64) -> String {
    if by == ReplyRoute::DAEMON {
        "daemon".to_string()
    } else {
        format!("client {by}")
    }
}

impl QueryLog {
    /// A log of the program's queries under `label`, the terminal's.
    pub(crate) fn new(label: String) -> Self {
        Self {
            label,
            pending: VecDeque::new(),
            #[cfg(test)]
            outcomes: Vec::new(),
        }
    }

    /// Notes a frame of output ending at `end` that asked `queries`, at `now`, while `clients`
    /// were attached and the daemon answers where none is (`daemon_answers`).
    pub(crate) fn note(
        &mut self,
        end: u64,
        queries: &[Query],
        clients: usize,
        daemon_answers: bool,
        now: Instant,
    ) {
        self.age(now);
        for &query in queries {
            tracing::info!(
                terminal = %self.label,
                offset = end,
                ?query,
                clients,
                daemon_answers,
                "terminal query seen"
            );
            self.pending.push_back(Pending {
                offset: end,
                at: now,
                query,
                warned: false,
            });
            if self.pending.len() > MOST_PENDING {
                if let Some(oldest) = self.pending.pop_front() {
                    log_not_answered(&self.label, &oldest, now);
                    #[cfg(test)]
                    self.outcomes.push((oldest.offset, false));
                }
            }
        }
    }

    /// Notes that `by` replied to the output up to `offset` with `reply` at `now`, the reply
    /// written where `admitted`. A reply names the end of the frame whose parse produced it, and
    /// carries nothing else to tell which query it answers: each answer in it is matched with the
    /// query of its kind asked by that frame, else with the latest one asked before it.
    ///
    /// An admitted reply settles the output up to `offset` ([`ReplyRoute::admit`]): no answer to a
    /// query asked before that point is written any more, so those still waiting are given up on.
    /// One asked by the frame at `offset` itself is kept, as the same client may answer it in a
    /// reply of its own.
    pub(crate) fn reply(
        &mut self,
        by: u64,
        offset: u64,
        reply: &[Query],
        admitted: bool,
        now: Instant,
    ) {
        self.age(now);
        for &query in reply {
            let of_kind = |pending: &Pending| pending.query == query;
            let Some(at) = self
                .pending
                .iter()
                .position(|pending| of_kind(pending) && pending.offset == offset)
                .or_else(|| {
                    self.pending
                        .iter()
                        .rposition(|pending| of_kind(pending) && pending.offset < offset)
                })
            else {
                continue;
            };
            let latency_ms = now
                .saturating_duration_since(self.pending[at].at)
                .as_millis();
            let pending_offset = self.pending[at].offset;
            if admitted {
                self.pending.remove(at);
                #[cfg(test)]
                self.outcomes.push((pending_offset, true));
                tracing::info!(terminal = %self.label, offset = pending_offset, ?query, by = %by_name(by), latency_ms, "terminal query answered");
            } else {
                tracing::info!(terminal = %self.label, offset = pending_offset, ?query, by = %by_name(by), latency_ms, "terminal query answer dropped");
            }
        }
        if admitted {
            while let Some(at) = self
                .pending
                .iter()
                .position(|pending| pending.offset < offset)
            {
                if let Some(pending) = self.pending.remove(at) {
                    log_not_answered(&self.label, &pending, now);
                    #[cfg(test)]
                    self.outcomes.push((pending.offset, false));
                }
            }
        }
    }

    /// Logs the queries waiting past [`STILL_UNANSWERED_AFTER`] once, and gives up on those past
    /// what [`ReplyRoute`] still takes an answer to. Only output or a reply runs it, so a query
    /// that a program stalls on with no client attached is logged once output resumes.
    fn age(&mut self, now: Instant) {
        let label = &self.label;
        self.pending.retain_mut(|pending| {
            let waited = now.saturating_duration_since(pending.at);
            if waited >= STALE_AFTER {
                log_not_answered(label, pending, now);
                return false;
            }
            if waited >= STILL_UNANSWERED_AFTER && !pending.warned {
                pending.warned = true;
                tracing::info!(terminal = %label, offset = pending.offset, query = ?pending.query, waited_ms = waited.as_millis(), "terminal query still unanswered");
            }
            true
        });
    }

    #[cfg(test)]
    fn waiting(&self) -> Vec<(u64, Query)> {
        self.pending
            .iter()
            .map(|pending| (pending.offset, pending.query))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use Query::{CursorPosition as Cpr, DeviceAttributes as Da};

    #[test]
    fn the_answers_are_read_out_of_any_bytes() {
        assert_eq!(answers_in(b"\x1b[?6c\x1b[12;40R"), [Da, Cpr]);
        assert_eq!(
            answers_in(b"\x1b[?1;2c"),
            [Da],
            "an emulator's own attributes"
        );
        assert_eq!(
            answers_in(b"\x1b]11;rgb:0000/0000/0000\x07\x1b[>0;10;1c\x1b[0n\x1b[1R"),
            [],
            "other answers"
        );
        assert_eq!(answers_in(b"x\x1b[3;4"), [], "an answer cut short");
    }

    #[test]
    fn each_answer_settles_the_query_of_its_kind_its_frame_asked() {
        let mut log = QueryLog::new("t".into());
        let now = Instant::now();
        log.note(10, &[Cpr], 1, false, now);
        log.note(20, &[Da, Cpr], 1, false, now);
        log.reply(1, 20, &[Cpr], false, now);
        assert_eq!(log.waiting().len(), 3, "a dropped answer settles none");
        log.reply(0, 20, &[Cpr], true, now);
        assert_eq!(
            log.waiting(),
            [(20, Da)],
            "the frame's own query answered, the earlier one given up on, and the frame's other \
             query kept for the same client's next reply"
        );
        log.reply(0, 20, &[], true, now);
        assert_eq!(log.waiting(), [(20, Da)], "a reply with no such answer");
        log.reply(0, 20, &[Da], true, now);
        assert!(log.waiting().is_empty());
    }

    #[test]
    fn an_answer_with_no_query_from_its_frame_settles_the_latest_before_it() {
        let mut log = QueryLog::new("t".into());
        let now = Instant::now();
        log.note(10, &[Cpr], 1, false, now);
        log.note(20, &[Cpr], 1, false, now);
        log.reply(0, 25, &[Cpr], false, now);
        log.reply(0, 5, &[Cpr], true, now);
        assert_eq!(log.waiting(), [(10, Cpr), (20, Cpr)], "nothing up to 5");
        log.reply(0, 25, &[Cpr], true, now);
        assert!(log.waiting().is_empty());
        assert_eq!(log.outcomes, [(20, true), (10, false)]);
    }

    #[test]
    fn a_query_a_client_left_unanswered_is_given_up_on_once_a_later_one_is_answered() {
        let mut log = QueryLog::new("t".into());
        let now = Instant::now();
        // Asked while a client was attached, which left without answering.
        log.note(10, &[Cpr], 1, true, now);
        // Asked with nobody attached, so the daemon answers it.
        log.note(20, &[Cpr], 0, true, now);
        log.reply(ReplyRoute::DAEMON, 20, &[Cpr], true, now);
        assert!(log.waiting().is_empty());
        assert_eq!(log.outcomes, [(20, true), (10, false)]);
    }

    #[test]
    fn a_query_is_waited_on_until_the_reply_route_gives_up_on_it() {
        let mut log = QueryLog::new("t".into());
        let then = Instant::now();
        log.note(10, &[Cpr], 0, false, then);
        log.note(11, &[], 0, false, then + STILL_UNANSWERED_AFTER);
        assert_eq!(log.waiting(), [(10, Cpr)], "still waited on");
        log.note(12, &[], 0, false, then + STALE_AFTER);
        assert!(log.waiting().is_empty());
    }
}
