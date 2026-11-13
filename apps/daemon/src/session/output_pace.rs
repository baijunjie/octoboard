//! What the agent's output has done lately, for telling whether the parsed screen shows a whole
//! frame: a screen that takes each chunk in as it is read can show half a redraw between two chunks
//! of one frame.

use std::time::{Duration, Instant};

/// How long the agent's output has to have been still for the screen to be taken as a whole frame:
/// the parsed screen takes each chunk in as it is read, so between two chunks of one frame it can
/// show the box cleared and not yet drawn again. The chunks of one frame follow each other within
/// a millisecond or so on a Mac; an idle agent is still for seconds.
const QUIET: Duration = Duration::from_millis(50);

/// How long a synchronized update may stay open before it counts as over. A program that opens one
/// and never closes it — killed in the middle of a frame, or a closing marker lost — would
/// otherwise keep the screen from ever being read as a whole frame; terminals time an update out
/// the same way, and a frame takes milliseconds to draw.
const FRAME_TIMEOUT: Duration = Duration::from_secs(1);

/// The start and the end of a synchronized update, which Codex and Grok Build wrap each frame in.
const FRAME_START: &[u8] = b"\x1b[?2026h";
const FRAME_END: &[u8] = b"\x1b[?2026l";

/// What the agent's output has done lately, for telling whether the parsed screen shows a whole
/// frame: when its latest chunk was read, and since when a synchronized update has been open.
#[derive(Default)]
pub(super) struct OutputPace {
    last: Option<Instant>,
    /// When the synchronized update that is open now was opened; `None` while none is. An update
    /// opened again without being closed keeps the time it was first opened, so that a program that
    /// never closes one cannot keep it open past [`FRAME_TIMEOUT`] by opening it again.
    frame_open: Option<Instant>,
    /// The end of the latest chunk, as long as a frame marker less one byte, for a marker that
    /// a chunk ends in the middle of.
    tail: Vec<u8>,
}

impl OutputPace {
    /// Takes in `chunk`, read from the agent's output at `at`, before the screen takes it in.
    pub(super) fn note(&mut self, chunk: &[u8], at: Instant) {
        self.last = Some(at);
        let last_marker = |bytes: &[u8]| {
            bytes
                .windows(FRAME_START.len())
                .rev()
                .find(|window| *window == FRAME_START || *window == FRAME_END)
                .map(|window| window == FRAME_START)
        };
        let closes = |bytes: &[u8]| {
            bytes
                .windows(FRAME_END.len())
                .any(|window| window == FRAME_END)
        };
        // A marker the chunk holds whole comes after one that began in the tail.
        let joint: Vec<u8> = self
            .tail
            .iter()
            .chain(chunk.iter().take(FRAME_START.len() - 1))
            .copied()
            .collect();
        if let Some(open) = last_marker(chunk).or_else(|| last_marker(&joint)) {
            // An update closed in this chunk and opened again is a new one.
            let reopened = closes(chunk) || closes(&joint);
            self.frame_open = match (open, self.frame_open) {
                (false, _) => None,
                (true, Some(since)) if !reopened => Some(since),
                (true, _) => Some(at),
            };
        }
        let kept = FRAME_START.len() - 1;
        if chunk.len() >= kept {
            self.tail = chunk[chunk.len() - kept..].to_vec();
        } else {
            let from = (self.tail.len() + chunk.len()).saturating_sub(kept);
            self.tail = self.tail.iter().chain(chunk).copied().skip(from).collect();
        }
    }

    /// Whether the parsed screen can be taken to show a whole frame at `now`: no synchronized
    /// update has been open for less than [`FRAME_TIMEOUT`], and nothing has been read for
    /// [`QUIET`].
    pub(super) fn whole_frame(&self, now: Instant) -> bool {
        self.frame_open
            .is_none_or(|since| now.saturating_duration_since(since) >= FRAME_TIMEOUT)
            && self
                .last
                .is_none_or(|at| now.saturating_duration_since(at) >= QUIET)
    }

    /// Moves the time of the latest chunk back by [`QUIET`], as if it had been read that much
    /// earlier.
    #[cfg(test)]
    pub(super) fn quieten(&mut self) {
        if let Some(at) = &mut self.last {
            *at -= QUIET;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The screen shows a whole frame only once the output has been still for `QUIET` and no
    /// synchronized update is open, its markers found wherever a chunk cuts them.
    #[test]
    fn a_whole_frame_waits_for_quiet_output_and_the_end_of_a_synchronized_update() {
        let read = Instant::now();
        let mut pace = OutputPace::default();
        assert!(pace.whole_frame(read));
        pace.note(b"plain", read);
        assert!(!pace.whole_frame(read + QUIET - Duration::from_millis(1)));
        assert!(pace.whole_frame(read + QUIET));
        for (chunks, open) in [
            (&[&b"\x1b[?2026hframe"[..]][..], true),
            (&[b"\x1b[?20", b"26h"], true),
            (&[b"\x1b[?2026h", b"x", b"\x1b", b"[?2026", b"l"], false),
            (&[b"\x1b[?2026l\x1b[?2026h"], true),
            (&[b"\x1b[?2026h\x1b[?2026l"], false),
            (&[b"\x1b[?2026h", b"\x1b[?2026$p"], true),
        ] {
            let mut pace = OutputPace::default();
            for chunk in chunks {
                pace.note(chunk, read);
            }
            assert_eq!(!pace.whole_frame(read + QUIET), open, "{chunks:?}");
        }
    }

    const OPEN: &[u8] = b"\x1b[?2026h";
    const CLOSE: &[u8] = b"\x1b[?2026l";
    const MILLI: Duration = Duration::from_millis(1);

    /// An update left open counts as over once it has been open for `FRAME_TIMEOUT`, and the
    /// output still has to have been quiet for `QUIET` after that.
    #[test]
    fn an_update_left_open_counts_as_over_once_it_has_been_open_for_the_frame_timeout() {
        let opened = Instant::now();
        let mut pace = OutputPace::default();
        pace.note(b"\x1b[?2026hframe", opened);
        assert!(!pace.whole_frame(opened + FRAME_TIMEOUT - MILLI));
        assert!(pace.whole_frame(opened + FRAME_TIMEOUT));
        assert!(pace.whole_frame(opened + FRAME_TIMEOUT * 10));

        // A chunk with no marker in it leaves the update as it was, and is output the screen
        // must have been quiet after.
        pace.note(b"more of the frame", opened + FRAME_TIMEOUT);
        assert!(!pace.whole_frame(opened + FRAME_TIMEOUT + QUIET - MILLI));
        assert!(pace.whole_frame(opened + FRAME_TIMEOUT + QUIET));
    }

    /// An update closed in time is whole as soon as the output is quiet, and one opened again
    /// afterwards times out from its own opening.
    #[test]
    fn a_closed_update_is_whole_when_quiet_and_the_next_one_times_out_from_its_own_opening() {
        let opened = Instant::now();
        let mut pace = OutputPace::default();
        pace.note(OPEN, opened);
        let closed = opened + FRAME_TIMEOUT / 4;
        pace.note(CLOSE, closed);
        assert!(!pace.whole_frame(closed + QUIET - MILLI));
        assert!(pace.whole_frame(closed + QUIET));

        let again = opened + FRAME_TIMEOUT * 2;
        pace.note(OPEN, again);
        assert!(!pace.whole_frame(again + FRAME_TIMEOUT - MILLI));
        assert!(pace.whole_frame(again + FRAME_TIMEOUT));
    }

    /// An update opened again without being closed keeps the time it was first opened, so a program
    /// that keeps opening one cannot keep it from timing out.
    #[test]
    fn opening_an_update_that_was_never_closed_does_not_extend_it() {
        let opened = Instant::now();
        let mut pace = OutputPace::default();
        pace.note(OPEN, opened);
        pace.note(OPEN, opened + FRAME_TIMEOUT / 2);
        assert!(!pace.whole_frame(opened + FRAME_TIMEOUT - MILLI));
        assert!(pace.whole_frame(opened + FRAME_TIMEOUT));

        let mut pace = OutputPace::default();
        pace.note(OPEN, opened);
        let step = Duration::from_millis(100);
        for count in 1..=20 {
            pace.note(b"x\x1b[?2026hy", opened + step * count);
        }
        assert!(pace.whole_frame(opened + step * 20 + QUIET));

        // The same where the marker comes in two chunks, cut in the middle of it.
        let mut pace = OutputPace::default();
        pace.note(OPEN, opened);
        pace.note(b"\x1b[?20", opened + FRAME_TIMEOUT / 4);
        pace.note(b"26h", opened + FRAME_TIMEOUT / 2);
        assert!(!pace.whole_frame(opened + FRAME_TIMEOUT - MILLI));
        assert!(pace.whole_frame(opened + FRAME_TIMEOUT));
    }

    /// An update that is closed and opened again, in one chunk or across a chunk boundary, is a new
    /// one: its time is the time of the chunk that opened it.
    #[test]
    fn an_update_closed_and_opened_again_starts_a_new_timer_at_that_chunk() {
        let opened = Instant::now();
        let again = opened + FRAME_TIMEOUT * 9 / 10;
        type Chunks = Vec<(&'static [u8], Instant)>;
        let cases: [(&str, Chunks); 3] = [
            ("one chunk", vec![(&b"\x1b[?2026l\x1b[?2026h"[..], again)]),
            (
                "the close cut in two",
                vec![
                    (&b"\x1b[?2026"[..], opened + FRAME_TIMEOUT / 2),
                    (&b"l\x1b[?2026h"[..], again),
                ],
            ),
            (
                "the open cut in two",
                vec![
                    (&b"\x1b[?2026l\x1b[?20"[..], opened + FRAME_TIMEOUT / 2),
                    (&b"26h"[..], again),
                ],
            ),
        ];
        for (label, chunks) in cases {
            let mut pace = OutputPace::default();
            pace.note(OPEN, opened);
            for (chunk, at) in chunks {
                pace.note(chunk, at);
            }
            // Past the first opening's timeout, but not the new one's.
            assert!(!pace.whole_frame(opened + FRAME_TIMEOUT + QUIET), "{label}");
            assert!(!pace.whole_frame(again + FRAME_TIMEOUT - MILLI), "{label}");
            assert!(pace.whole_frame(again + FRAME_TIMEOUT), "{label}");
        }
    }
}
