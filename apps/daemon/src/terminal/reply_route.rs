//! Which clients' answers to the queries passed through to them reach the program.
//!
//! A program asks its terminal things through the output — the cursor position (`CSI 6n`), the
//! background colour (`OSC 11 ?`), the device attributes — and reads the answer back as input.
//! The daemon answers those its parsed terminal can answer itself and leaves them out of what the
//! clients receive (`terminal::queries`). The rest passes through, and every client attached to
//! the terminal runs an emulator of its own that sees the same output and answers it, so with two
//! clients attached the program would get every answer twice: it reads the first and leaves the
//! second in its input, where the next prompt trips over it or the shell echoes it.
//!
//! So each answer names the point in the output it answers: the end of the output frame whose
//! parsing produced it, as a count of the bytes the clients have been handed. The first client to
//! answer the output up to a point is the one whose answers to it are written; answers to output up
//! to an earlier point come too late and are dropped, however long a client took to send them. Output
//! older than [`STALE_AFTER`] is settled too, answered or not: a program waits that long for an
//! answer at most, and one that has given up would read a late answer as input. A client attaching
//! starts from a first paint of the screen, which asks nothing, and answers only the output after
//! it.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How long a program is taken to wait for its terminal's answer: what prompt libraries wait
/// (termenv's 5 seconds, crossterm's 2).
#[cfg(not(test))]
pub(crate) const STALE_AFTER: Duration = Duration::from_secs(5);
/// Long under test, so that a loaded machine cannot settle output a test drives answers to.
#[cfg(test)]
pub(crate) const STALE_AFTER: Duration = Duration::from_secs(60);

/// The answers written so far, and the output recent enough to answer.
#[derive(Debug, Default)]
pub struct ReplyRoute {
    /// The furthest point in the output an answer was written for, and the client that answered it.
    last_answer: Option<(u64, u64)>,
    /// The end of each frame of output younger than [`STALE_AFTER`], and when it came, oldest first.
    recent: VecDeque<(u64, Instant)>,
    /// The end of the last frame of output older than [`STALE_AFTER`].
    stale_through: u64,
    /// The end of the last frame of output that passed a query through to the clients while one
    /// was attached to answer it ([`Self::owe`]).
    owed_through: u64,
}

impl ReplyRoute {
    /// Records a frame of output, which ends at `end`, as having come at `now`.
    pub fn note_output(&mut self, end: u64, now: Instant) {
        self.recent.push_back((end, now));
        self.forget_stale(now);
    }

    fn forget_stale(&mut self, now: Instant) {
        while let Some(&(end, at)) = self.recent.front() {
            if now.saturating_duration_since(at) < STALE_AFTER {
                break;
            }
            self.stale_through = end;
            self.recent.pop_front();
        }
    }

    /// Notes that the frame of output ending at `end` passed a query through to the clients, with
    /// one attached to answer it.
    pub fn owe(&mut self, end: u64) {
        self.owed_through = self.owed_through.max(end);
    }

    /// Whether, at `now`, a query passed through to the clients ([`Self::owe`]) may still be
    /// answered: no answer has been written for output up to it, and it is not yet older than
    /// [`STALE_AFTER`], past which no answer to it is written any more.
    pub fn owed(&mut self, now: Instant) -> bool {
        self.forget_stale(now);
        self.owed_through > self.stale_through
            && self
                .last_answer
                .is_none_or(|(answered, _)| answered < self.owed_through)
    }

    /// Whether this client's answer to the output up to `offset`, given at `now` when the program
    /// has written `total` bytes, is written to the program, recording it as the answer to that
    /// point when it is. A point past the output names no output this client has seen.
    pub fn admit(&mut self, id: u64, offset: u64, total: u64, now: Instant) -> bool {
        self.forget_stale(now);
        if offset <= self.stale_through || offset > total {
            return false;
        }
        let admitted = match self.last_answer {
            None => true,
            Some((answered, by)) => offset > answered || (offset == answered && by == id),
        };
        if admitted {
            self.last_answer = Some((offset, id));
        }
        admitted
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{ReplyRoute, STALE_AFTER};

    #[test]
    fn the_first_answer_to_a_point_is_written_and_another_clients_copy_is_dropped() {
        let mut route = ReplyRoute::default();
        let now = Instant::now();
        let (first, second) = (0, 1);
        route.note_output(10, now);
        assert!(route.admit(second, 10, 100, now));
        assert!(!route.admit(first, 10, 100, now));
        // The same client's further answers to the same output still go.
        assert!(route.admit(second, 10, 100, now));
    }

    #[test]
    fn an_answer_to_earlier_output_is_dropped_however_late() {
        let mut route = ReplyRoute::default();
        let now = Instant::now();
        let (fast, slow) = (0, 1);
        route.note_output(10, now);
        route.note_output(20, now);
        route.note_output(30, now);
        assert!(route.admit(fast, 10, 100, now));
        assert!(route.admit(fast, 20, 100, now));
        assert!(!route.admit(slow, 10, 100, now));
        assert!(!route.admit(slow, 20, 100, now));
        assert!(route.admit(slow, 30, 100, now));
        assert!(!route.admit(fast, 30, 100, now));
    }

    #[test]
    fn an_answer_to_output_not_yet_written_is_dropped() {
        let mut route = ReplyRoute::default();
        let now = Instant::now();
        route.note_output(10, now);
        assert!(!route.admit(0, 50, 10, now));
        assert!(route.admit(1, 10, 10, now));
    }

    #[test]
    fn output_nobody_answered_in_time_is_not_answered_later() {
        let mut route = ReplyRoute::default();
        let then = Instant::now();
        let client = 0;
        route.note_output(10, then);
        route.note_output(20, then + STALE_AFTER / 2);
        let later = then + STALE_AFTER;
        assert!(!route.admit(client, 10, 100, later));
        assert!(route.admit(client, 20, 100, later));
    }

    #[test]
    fn a_query_passed_to_the_clients_is_owed_until_answered_or_stale() {
        let mut route = ReplyRoute::default();
        let then = Instant::now();
        assert!(!route.owed(then));
        route.note_output(10, then);
        route.owe(10);
        assert!(route.owed(then));
        // An answer to earlier output settles nothing; one to it, or to later output, does.
        route.note_output(20, then);
        assert!(route.admit(0, 5, 100, then));
        assert!(route.owed(then));
        assert!(route.admit(0, 10, 100, then));
        assert!(!route.owed(then));
        // Nobody answers the next one: it is owed only until the program has stopped waiting.
        route.owe(20);
        assert!(route.owed(then));
        assert!(!route.owed(then + STALE_AFTER));
    }
}
