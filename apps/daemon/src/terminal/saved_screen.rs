//! What a daemon hands its successor of a terminal at a planned restart: the parsed screen saved
//! as the bytes that rebuild it ([`SavedScreen`]), the one way it is restored
//! ([`SavedScreen::restore_into`]), and how the successor takes in the tail the terminal's holder
//! kept since ([`ReplayRule`]).

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::fan::Fan;
use super::pty::READ_BUF;
use super::snapshot::is_supported_size;

/// The size, as rows and columns, a preview is rebuilt at where nothing gives one it can have.
pub const PREVIEW_SIZE: (u16, u16) = (24, 80);

/// How old the output that asks a query may be for the query to be answered in a replay: a
/// program that asked longer ago than this has stopped waiting, and would read a late answer as
/// typed input.
pub const FRESH_FOR: Duration = Duration::from_secs(1);

/// A terminal's parsed screen saved at a planned restart, for the successor to restore before its
/// reader starts (`PtyCore::restore_screen`). `bytes`, taken into a fresh parsed screen of `rows`
/// by `cols`, rebuild it: the first paint with the whole scrollback
/// (`TerminalSnapshot::paint`), with the titles the program pushed pushed again right after its
/// reset, then what draws the character a repeat (REP) draws again and takes the screen back,
/// where the paint did not leave it, then the program's status records as status reports, oldest
/// updated first, then `CSI ? 2031 h` where the program subscribed to colour scheme updates, then
/// the bytes of the escape sequence or UTF-8 character the output had begun and not ended, so the
/// output that follows parses as it would have: what the parser had taken in of it, and what the
/// query scanner held back. `drawn` says the screen had shown something since titles were
/// reported, so that the restored one counts titles from the first byte after them, as this one
/// did.
///
/// `inexact` says the bytes rebuild the screen only in part, which a restore marks, as it marks a
/// screen rebuilt from a holder's tail: the screen was such a one already, or the terminal was in
/// a state the bytes leave out (`TerminalSnapshot::save`) — the normal screen hidden behind the
/// alternate one where anything was drawn on it, a character set, tab stops or palette colours
/// changed, a title stack the paint does not push again, a cursor the paint cannot place, or a
/// character to repeat it cannot draw. Not saved either, and of no matter to what follows: the
/// display the program's colour queries were answered with, which the clients of the successor
/// report again.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SavedScreen {
    pub rows: u16,
    pub cols: u16,
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
    pub inexact: bool,
    #[serde(default)]
    pub drawn: bool,
}

impl SavedScreen {
    /// Whether the screen's size is one a terminal can have. A file that was altered, or written
    /// by another build, may name one no screen has.
    pub fn has_supported_size(&self) -> bool {
        is_supported_size(self.rows, self.cols)
    }

    /// Takes this screen into `fan`, the parsed screen of the terminal it was saved for, in
    /// `chunk_size` pieces: the parsed screen is given the saved size and the saved bytes, which
    /// leave it in the state the predecessor's was in, with whether it had drawn anything. Nothing
    /// in the bytes is answered, and the status moves and title changes they make are dropped.
    /// The one way a saved screen is restored: for an adopted terminal (`PtyCore::restore_screen`)
    /// and for the preview of one whose holder is gone ([`Self::preview`]).
    ///
    /// Returns whether it was restored at its size. A size no screen has
    /// ([`Self::has_supported_size`]) is never given to the parsed screen, which would fail on it
    /// or allocate without bound: the bytes are taken in at the size the parsed screen has, which
    /// rebuilds the screen only in part.
    pub fn restore_into(&self, fan: &Fan, chunk_size: usize) -> bool {
        fan.set_drawn(self.drawn);
        if !self.has_supported_size() {
            fan.seed(&self.bytes, chunk_size);
            return false;
        }
        fan.restore(self.rows, self.cols, &self.bytes, chunk_size);
        true
    }

    /// The preview of a terminal whose holder is gone, that wrote `since` after this screen was
    /// saved: the screen restored into a parsed screen of its own ([`Self::restore_into`]), with
    /// `since` taken in over it, nothing in either answered, as the screen of an ended session is
    /// kept (`TerminalSnapshot::output`). A screen of a size no screen has is taken in at
    /// [`PREVIEW_SIZE`] instead.
    pub fn preview(&self, since: &[u8]) -> Vec<u8> {
        let (rows, cols) = if self.has_supported_size() {
            (self.rows, self.cols)
        } else {
            PREVIEW_SIZE
        };
        let fan = Fan::new(rows, cols, false);
        self.restore_into(&fan, READ_BUF);
        fan.seed(since, READ_BUF);
        fan.output()
    }

    /// A screen of `rows` by `cols` saved as `bytes`, exact and drawn on.
    #[cfg(test)]
    pub(crate) fn of_bytes(rows: u16, cols: u16, bytes: &[u8]) -> Self {
        Self {
            rows,
            cols,
            bytes: bytes.to_vec(),
            inexact: false,
            drawn: true,
        }
    }
}

/// How the successor takes in the tail a holder kept of a terminal since its handover, which its
/// reader pushes first (`PtyCore::begin_replay`): the next `len` bytes are that tail, output the
/// predecessor never read. Within it a query is answered only while the program may still be
/// waiting for the answer ([`Self::fresh`]), and never the primary device attributes where
/// `skip_device_attributes` (the holder answered them while no daemon was linked); a query not to
/// be answered is dropped, neither answered nor passed to the clients.
#[derive(Debug, Clone)]
pub struct ReplayRule {
    pub len: u64,
    /// `ages[i]` says the output from offset `ages[i].0` into the tail on was `ages[i].1` old at
    /// `measured_at`, or younger.
    pub ages: Vec<(u64, Duration)>,
    pub measured_at: Instant,
    pub skip_device_attributes: bool,
}

impl ReplayRule {
    /// Whether the output from `offset` into the tail on is fresh at `now`: it was at most
    /// [`FRESH_FOR`] old. Output before the first age known, or with none known, is not.
    pub fn fresh(&self, offset: u64, now: Instant) -> bool {
        let Some(&(_, age)) = self
            .ages
            .iter()
            .filter(|(from, _)| *from <= offset)
            .max_by_key(|(from, _)| *from)
        else {
            return false;
        };
        age + now.saturating_duration_since(self.measured_at) <= FRESH_FOR
    }
}

/// [`SavedScreen::bytes`] as base64 text.
mod base64_bytes {
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{ReplayRule, SavedScreen};
    use crate::terminal::fan::Fan;

    /// A saved screen of a size no screen has (an altered file, another build's) is never given
    /// to a parsed screen: restoring it takes the bytes in at the parsed screen's own size and
    /// says so, and its preview is rebuilt at the preview size, showing what it drew.
    #[test]
    fn a_saved_screen_of_a_size_no_screen_has_is_taken_in_at_another() {
        for (rows, cols) in [(0, 80), (24, 0), (u16::MAX, u16::MAX)] {
            let saved = SavedScreen::of_bytes(rows, cols, b"what it drew");
            let preview = String::from_utf8_lossy(&saved.preview(&[])).into_owned();
            assert!(
                preview.contains("what it drew"),
                "{rows}x{cols}: {preview:?}"
            );
            let fan = Fan::new(24, 80, false);
            assert!(!saved.restore_into(&fan, 8 * 1024), "{rows}x{cols}");
            assert_eq!(fan.screen_size(), (24, 80));
        }
        let saved = SavedScreen::of_bytes(10, 40, b"what it drew");
        let fan = Fan::new(24, 80, false);
        assert!(saved.restore_into(&fan, 8 * 1024));
        assert_eq!(fan.screen_size(), (10, 40));
    }

    #[test]
    fn a_saved_screen_carries_its_bytes_whole_as_base64() {
        let saved = SavedScreen {
            rows: 24,
            cols: 80,
            bytes: (0..=255).collect(),
            inexact: true,
            drawn: true,
        };
        let text = serde_json::to_string(&saved).expect("encodes");
        assert!(text.contains("\"bytes\":\"AAEC"), "{text}");
        assert_eq!(
            serde_json::from_str::<SavedScreen>(&text).expect("decodes"),
            saved
        );
    }

    #[test]
    fn output_is_fresh_by_the_age_of_the_last_entry_at_or_before_it() {
        let measured_at = Instant::now();
        let rule = ReplayRule {
            len: 100,
            ages: vec![
                (10, Duration::from_secs(5)),
                (40, Duration::from_millis(600)),
            ],
            measured_at,
            skip_device_attributes: false,
        };
        assert!(!rule.fresh(0, measured_at), "before the first entry");
        assert!(!rule.fresh(10, measured_at));
        assert!(!rule.fresh(39, measured_at));
        assert!(rule.fresh(40, measured_at));
        assert!(rule.fresh(99, measured_at), "past the last entry");
        assert!(rule.fresh(40, measured_at + Duration::from_millis(400)));
        assert!(
            !rule.fresh(40, measured_at + Duration::from_millis(401)),
            "aged since it was measured"
        );
        let unknown = ReplayRule {
            ages: Vec::new(),
            ..rule
        };
        assert!(!unknown.fresh(0, measured_at));
    }
}
