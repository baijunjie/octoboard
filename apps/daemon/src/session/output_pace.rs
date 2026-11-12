//! What the agent's output has done lately, for telling whether the parsed screen shows a whole
//! frame: a screen that takes each chunk in as it is read can show half a redraw between two chunks
//! of one frame.

use std::time::{Duration, Instant};

/// How long the agent's output has to have been still for the screen to be taken as a whole frame:
/// the parsed screen takes each chunk in as it is read, so between two chunks of one frame it can
/// show the box cleared and not yet drawn again. The chunks of one frame follow each other within
/// a millisecond or so on a Mac; an idle agent is still for seconds.
const QUIET: Duration = Duration::from_millis(50);

/// The start and the end of a synchronized update, which Codex and Grok Build wrap each frame in.
const FRAME_START: &[u8] = b"\x1b[?2026h";
const FRAME_END: &[u8] = b"\x1b[?2026l";

/// What the agent's output has done lately, for telling whether the parsed screen shows a whole
/// frame: when its latest chunk was read, and whether a synchronized update is open.
#[derive(Default)]
pub(super) struct OutputPace {
    last: Option<Instant>,
    frame_open: bool,
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
        // A marker the chunk holds whole comes after one that began in the tail.
        let joint: Vec<u8> = self
            .tail
            .iter()
            .chain(chunk.iter().take(FRAME_START.len() - 1))
            .copied()
            .collect();
        if let Some(open) = last_marker(chunk).or_else(|| last_marker(&joint)) {
            self.frame_open = open;
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
    /// update is open, and nothing has been read for [`QUIET`].
    pub(super) fn whole_frame(&self, now: Instant) -> bool {
        !self.frame_open
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
}
