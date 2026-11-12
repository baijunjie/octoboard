//! A conservative mirror of input, used only to match actual submitted prompts to edit generations,
//! to tell a prompt submitted before a partly written message from one submitted after it, and to
//! tell when an empty input box read off the screen shows what the keys typed last did. What the
//! agent's output has done lately is `output_pace`'s.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::protocol::AgentId;

#[derive(Default)]
pub(super) struct Draft {
    edited: u64,
    submitted: u64,
    submissions: VecDeque<(Option<u64>, String)>,
    text: Option<Vec<u8>>,
    started: bool,
    paste: bool,
    escape: Vec<u8>,
    overflowed: bool,
    utf8_remaining: u8,
    /// The texts of the last prompts the agent reported sending, which it can send again by itself:
    /// a retry, or a prompt it had queued. Bounded by [`RECENTLY_SENT`] and [`RECENTLY_SENT_BYTES`].
    recently_sent: VecDeque<String>,
    /// The prompts submitted before a message was partly written ([`Self::mark_fragment`]), which
    /// a prompt the agent reports sending later may still be: the submissions still pending then
    /// and [`Self::recently_sent`] as it stood.
    before_fragment: Option<Vec<String>>,
    /// When the latest key went in ([`Self::typed`]), and how many bytes the agent had printed
    /// by then.
    typed_at: Option<(Instant, u64)>,
    /// Whether every key since the last moment nothing typed was held ([`Self::holds`]) is an Esc
    /// written on its own: the keys an agent may answer by printing nothing.
    escapes_only: bool,
    /// The edit count at the latest key that may put text in the box the mirror cannot know
    /// ([`may_insert`]): an empty box read off the screen proves nothing until a submission at or
    /// past it is acknowledged, or a message of Octoboard's has gone in after it
    /// ([`Self::message_written`]).
    inserted_at: Option<u64>,
    /// When that key was typed.
    inserted_when: Option<Instant>,
    /// When the latest write was an Esc on its own, so that a second one soon after is told apart
    /// ([`DOUBLE_ESCAPE`]); forgotten once Octoboard writes a message ([`Self::message_written`]),
    /// which the agent's own double-press window does not outlast, but not when Octoboard only
    /// reads the box.
    lone_escape_at: Option<Instant>,
}

/// How soon after an Esc a second one counts as the double press that brings back an earlier
/// prompt to edit (Claude Code's rewind, Codex's backtrack). Claude Code's own window is about a
/// second; this is wider, so a double press is never missed, and a second Esc long after the first
/// is a key of its own.
const DOUBLE_ESCAPE: Duration = Duration::from_secs(2);

/// How long a key typed into the session may not be drawn yet: an agent reads a key and redraws
/// its box within milliseconds (Claude Code echoes one, or puts back the prompt an Esc cancelled,
/// in some 20 ms), and the margin covers a slower host. Output that arrives sooner after the key
/// may have been drawn before the agent read it.
const KEY_SETTLE: Duration = Duration::from_millis(500);

/// How many of the prompts last reported sent are kept, for telling one sent again apart.
const RECENTLY_SENT: usize = 32;
/// How many bytes of them are kept, the same budget as the submissions'; here a long prompt pushes
/// the oldest out, and one longer than this is not kept at all.
const RECENTLY_SENT_BYTES: usize = 1024 * 1024;

impl Draft {
    /// Takes in `data`, keys written into the session at `at`, when the agent had printed `output`
    /// bytes in all.
    pub(super) fn typed(&mut self, data: &[u8], output: u64, at: Instant) {
        let lone_escape = data == b"\x1b";
        let escapes_only = lone_escape && (!self.holds() || self.escapes_only);
        // An Esc left waiting from an earlier write is the Esc key once it has settled, not the
        // start of a sequence this write would finish; what it did to the box is not known.
        if self.lone_escape_settled(at) {
            self.escape.clear();
            self.text = None;
        }
        let inserted_before = self.inserted_at;
        self.input(data);
        if lone_escape
            && self
                .lone_escape_at
                .is_some_and(|first| at.saturating_duration_since(first) < DOUBLE_ESCAPE)
        {
            self.inserted_at = Some(self.edited);
        }
        if self.inserted_at != inserted_before {
            self.inserted_when = Some(at);
        }
        self.lone_escape_at = lone_escape.then_some(at);
        self.typed_at = Some((at, output));
        self.escapes_only = escapes_only;
    }

    fn input(&mut self, data: &[u8]) {
        if !self.started {
            self.text = Some(Vec::new());
            self.started = true;
        }
        for &byte in data {
            self.edited += 1;
            self.utf8_remaining = match byte {
                0xc2..=0xdf => 1,
                0xe0..=0xef => 2,
                0xf0..=0xf4 => 3,
                0x80..=0xbf => self.utf8_remaining.saturating_sub(1),
                _ => 0,
            };
            if byte == 0x1b || !self.escape.is_empty() {
                self.escape.push(byte);
                match self.escape.as_slice() {
                    b"\x1b[200~" => self.paste = true,
                    b"\x1b[201~" => self.paste = false,
                    b"\x1b\r" if !self.paste => self.append(b'\n'),
                    sequence
                        if b"\x1b[200~".starts_with(sequence)
                            || b"\x1b[201~".starts_with(sequence) =>
                    {
                        continue
                    }
                    sequence if incomplete_escape(sequence) => {
                        if self.escape.len() > 128 {
                            self.escape.truncate(2);
                        }
                        continue;
                    }
                    sequence => {
                        if may_insert(sequence) {
                            self.inserted_at = Some(self.edited);
                        }
                        self.text = None;
                    }
                }
                self.escape.clear();
                continue;
            }
            if self.paste {
                self.append(if byte == b'\r' { b'\n' } else { byte });
                continue;
            }
            match byte {
                b'\r' => {
                    // An Enter can take a completion its menu offers for a command or a mention.
                    if self
                        .text
                        .as_ref()
                        .is_some_and(|text| text.first() == Some(&b'/') || text.contains(&b'@'))
                    {
                        self.inserted_at = Some(self.edited);
                    }
                    if let Some(text) = self
                        .text
                        .take()
                        .and_then(|text| String::from_utf8(text).ok())
                    {
                        if !text.is_empty() && !text.starts_with('/') {
                            self.submission(Some(self.edited), text);
                        }
                    }
                    // If this Enter did not submit, the next candidate will not match the hook's
                    // full prompt. An unknown edit can recover after a subsequent simple prompt.
                    self.text = Some(Vec::new());
                }
                0x08 | 0x7f => {
                    if let Some(text) = &mut self.text {
                        while text.pop().is_some_and(|byte| byte & 0xc0 == 0x80) {}
                    }
                }
                0x15 => {
                    if let Some(text) = &mut self.text {
                        let start = text
                            .iter()
                            .rposition(|&byte| byte == b'\n')
                            .map_or(0, |at| at + 1);
                        text.truncate(start);
                    }
                }
                b'\n' | 0x20..=0xff => self.append(byte),
                byte => {
                    if may_insert(&[byte]) {
                        self.inserted_at = Some(self.edited);
                    }
                    self.text = None;
                }
            }
        }
    }

    fn append(&mut self, byte: u8) {
        if let Some(text) = &mut self.text {
            if text.len() < 1024 * 1024 {
                text.push(byte);
            } else {
                self.text = None;
            }
        }
    }

    pub(super) fn submission(&mut self, generation: Option<u64>, text: String) {
        if self.submissions.len() < 256
            && self
                .submissions
                .iter()
                .map(|(_, text)| text.len())
                .sum::<usize>()
                + text.len()
                <= 1024 * 1024
            && !self.overflowed
        {
            self.submissions.push_back((generation, text));
        } else {
            // Losing an origin could make an old program prompt acknowledge a later user edit.
            self.overflowed = true;
        }
    }

    pub(super) fn prompt_sent(&mut self, text: Option<&str>, agent: AgentId) {
        if let Some(text) = text.filter(|text| text.len() <= RECENTLY_SENT_BYTES) {
            while self.recently_sent.len() >= RECENTLY_SENT
                || self.recently_sent.iter().map(String::len).sum::<usize>() + text.len()
                    > RECENTLY_SENT_BYTES
            {
                self.recently_sent.pop_front();
            }
            self.recently_sent.push_back(text.to_string());
        }
        if self.overflowed {
            return;
        }
        let Some(at) = text.and_then(|text| {
            self.submissions
                .iter()
                .position(|(_, candidate)| same_prompt(agent, candidate, text))
        }) else {
            return;
        };
        if let Some((Some(generation), _)) = self.submissions.remove(at) {
            self.submitted = self.submitted.max(generation);
        }
    }

    /// Marks that `fragment`, a message Octoboard wrote, reached the input only in part. Its own
    /// submission goes, since only the user's Enter can now send what of it is there; every other
    /// prompt submitted or sent so far is remembered, as one the agent may still report sending.
    pub(super) fn mark_fragment(&mut self, fragment: &str) {
        if let Some(at) = self
            .submissions
            .iter()
            .rposition(|(generation, text)| generation.is_none() && text == fragment)
        {
            self.submissions.remove(at);
        }
        self.before_fragment = Some(
            self.submissions
                .iter()
                .map(|(_, text)| text.clone())
                .chain(self.recently_sent.iter().cloned())
                .collect(),
        );
    }

    /// Whether a prompt the agent reported sending ([`Self::prompt_sent`]) was submitted after the
    /// last [`Self::mark_fragment`], so from an input the fragment can no longer be part of: it is
    /// none of the prompts remembered there, each of which can stand for one such report. A prompt
    /// whose text is unknown counts as later, and so does one submitted before the mark that an
    /// overflowing record had already lost or that was too long to keep.
    ///
    /// Two misreadings are accepted: a later prompt with the same text as one remembered is taken
    /// for it, once, which only delays the end of a hold; and an agent that sends several prompts
    /// from before the mark as one is reported as one text none of them matches, which counts as
    /// later.
    pub(super) fn sent_after_fragment(&mut self, text: Option<&str>, agent: AgentId) -> bool {
        let (Some(before), Some(text)) = (self.before_fragment.as_mut(), text) else {
            return true;
        };
        match before
            .iter()
            .position(|candidate| same_prompt(agent, candidate, text))
        {
            Some(at) => {
                before.remove(at);
                false
            }
            None => true,
        }
    }

    /// Whether no key typed so far is still waiting for the rest of its bytes, so a key of
    /// Octoboard's written now cannot land inside one. A lone Esc counts as a whole key once it is
    /// [`KEY_SETTLE`] old: a terminal writes an escape sequence in one go, and the Esc key alone
    /// leaves the mirror waiting for a next byte that may never come, as when the user's Esc
    /// cancelled a turn and the agent put its prompt back in the box.
    pub(super) fn at_input_boundary(&self, now: Instant) -> bool {
        !self.paste
            && self.utf8_remaining == 0
            && (self.escape.is_empty() || self.lone_escape_settled(now))
    }

    fn lone_escape_settled(&self, now: Instant) -> bool {
        self.escape == [0x1b]
            && self
                .typed_at
                .is_some_and(|(at, _)| now.saturating_duration_since(at) >= KEY_SETTLE)
    }

    pub(super) fn holds(&self) -> bool {
        self.overflowed || self.edited > self.submitted
    }

    /// Whether an empty input box read at `now`, off a screen parsed from more than `output` bytes
    /// of the agent's output, shows what the keys typed so far did: the agent printed something
    /// after the last key went in, the key is [`KEY_SETTLE`] old, no key is still waiting for the
    /// rest of its bytes, and the mirror does not know of text typed that nothing since removed,
    /// which an empty box would show only if it was drawn before the text was. Nor may any key
    /// since the last acknowledged submission be one that can put text the mirror cannot know in
    /// the box ([`may_insert`]): drawn late, that text would take the message in after it. Where
    /// the keys since nothing was held are lone Escs typed with no turn running
    /// ([`Self::typed_mid_turn`]), nothing need have been printed after them: Claude Code at an
    /// idle empty box at times prints nothing for one (2.1.322), and an Esc there types no text.
    pub(super) fn settled(&self, output: u64, now: Instant) -> bool {
        let Some((at, before)) = self.typed_at else {
            return false;
        };
        self.at_input_boundary(now)
            && !self.knows_text()
            && self.inserted_at.is_none_or(|at| at <= self.submitted)
            && (output > before || self.escapes_only)
            && now.saturating_duration_since(at) >= KEY_SETTLE
    }

    /// Takes the agent's input box, read empty at `now` off a screen parsed from more than
    /// `output` bytes of its output, as showing that nothing typed is left in it, where that
    /// reading is [`Self::settled`]: every edit so far counts as acknowledged, which ends the hold
    /// ([`Self::holds`]), and the mirror starts again from that empty box, a lone Esc left
    /// waiting included, so that a prompt typed next can be matched again. The caller reads the
    /// box only off a whole frame ([`super::output_pace::OutputPace::whole_frame`]). A record that
    /// has overflowed still holds.
    pub(super) fn shown_empty(&mut self, output: u64, now: Instant) {
        if self.overflowed || !self.holds() || !self.settled(output, now) {
            return;
        }
        self.submitted = self.edited;
        // A lone Esc left waiting is settled by now, a whole key the box shows done with: the
        // next key is a key of its own, not the second byte of a sequence.
        self.escape.clear();
        self.text = Some(Vec::new());
    }

    /// Notes that a message of Octoboard's went into the input at `now`, after every key typed so
    /// far. The agent took those keys before the message, so whatever text they brought back was
    /// in the box with it, sent or stashed with it, or drawn back where the box shows it: no key
    /// typed before can still bring text in late. Except the key that may insert typed less than
    /// [`KEY_SETTLE`] before: what it does can take longer than reading it (an image pasted with
    /// Ctrl-V, a prompt loaded from history), so it still holds. Nor is an Esc after the message
    /// a second press.
    pub(super) fn message_written(&mut self, now: Instant) {
        if self
            .inserted_when
            .is_none_or(|at| now.saturating_duration_since(at) >= KEY_SETTLE)
        {
            self.inserted_at = None;
        }
        self.lone_escape_at = None;
    }

    /// Whether the mirror knows of text typed into the box that nothing since removed.
    fn knows_text(&self) -> bool {
        self.escape.is_empty() && self.text.as_ref().is_some_and(|text| !text.is_empty())
    }

    /// Whether the latest key, typed less than [`KEY_SETTLE`] before `now`, may have left text in
    /// the box that the screen does not show yet: text the mirror knows was typed, a key that may
    /// bring back text the mirror cannot know ([`Self::may_hold_unknown_text`]), or any key while
    /// the mirror does not know what the box holds (after Left, Home or an Esc), when characters
    /// typed are not recorded at all.
    pub(super) fn text_may_be_undrawn(&self, now: Instant) -> bool {
        let text_unknown = self.text.is_none() || !self.escape.is_empty();
        self.typed_at
            .is_some_and(|(at, _)| now.saturating_duration_since(at) < KEY_SETTLE)
            && (self.knows_text() || self.may_hold_unknown_text() || text_unknown)
    }

    /// Whether a key that may have put text the mirror cannot know in the box
    /// ([`may_insert`]) has been typed since the last acknowledged submission.
    pub(super) fn may_hold_unknown_text(&self) -> bool {
        self.inserted_at.is_some_and(|at| at > self.submitted)
    }

    /// Marks the key just typed as written while the agent's title showed a turn running, or at a
    /// time that is not known: an Esc then cancels the turn, and the prompt the agent puts back in
    /// the box can come later than any output, so the key waits for output like any other.
    pub(super) fn typed_mid_turn(&mut self) {
        self.escapes_only = false;
    }

    /// How many bytes have been typed in all, a mark that moves with every key.
    pub(super) fn edits(&self) -> u64 {
        self.edited
    }

    /// Moves the time of the latest key, and of the latest key that may insert, to [`KEY_SETTLE`]
    /// ago at the latest, as if they had been typed that much earlier.
    #[cfg(test)]
    pub(super) fn settle_typing(&mut self) {
        let settled = Instant::now() - KEY_SETTLE;
        if let Some((at, _)) = &mut self.typed_at {
            *at = (*at).min(settled);
        }
        if let Some(at) = &mut self.inserted_when {
            *at = (*at).min(settled);
        }
    }

    /// Moves the time of the latest key, and of the latest key that may insert, an hour ahead, so
    /// that no check the test makes finds them [`KEY_SETTLE`] old, however slowly it runs, until
    /// [`Self::settle_typing`].
    #[cfg(test)]
    pub(super) fn fresh_typing(&mut self) {
        let fresh = Instant::now() + Duration::from_secs(3600);
        if let Some((at, _)) = &mut self.typed_at {
            *at = fresh;
        }
        if let Some(at) = &mut self.inserted_when {
            *at = fresh;
        }
    }
}

/// Whether `text`, a prompt the agent reported sending, is `candidate` as it was submitted.
fn same_prompt(agent: AgentId, candidate: &str, text: &str) -> bool {
    crate::adapter::provider(agent)
        .screen()
        .as_submitted(candidate)
        == text
}

/// Whether `key`, a control byte or a complete escape sequence the mirror does not follow, may put
/// text in the box that the mirror cannot know: a prompt from history (Up, Down, Ctrl-P, Ctrl-N,
/// Ctrl-R), a completion (Tab), a suggestion taken (Right, End, Ctrl-E, Ctrl-F), a yank (Ctrl-Y,
/// Alt-Y), a paste or a stash brought back (Ctrl-V, Ctrl-S), an undo, an editor's text. Only keys
/// known to move the cursor back, remove text or do nothing are not; any other key counts as one
/// that may.
fn may_insert(key: &[u8]) -> bool {
    let removes_or_moves_back = match key {
        // Ctrl-A, Ctrl-B, Ctrl-C, Ctrl-D, Backspace, Ctrl-K, Ctrl-L, Ctrl-W, Delete.
        [0x01..=0x04 | 0x08 | 0x0b | 0x0c | 0x17 | 0x7f] => true,
        // Alt-B, Alt-D, Alt-Backspace.
        [0x1b, b'b' | b'd' | 0x08 | 0x7f] => true,
        // Left and Home, plain or modified.
        [0x1b, b'O', b'D' | b'H'] => true,
        [0x1b, b'[', params @ .., last]
            if params.iter().all(|b| b.is_ascii_digit() || *b == b';') =>
        {
            match last {
                b'D' | b'H' => true,
                // Home and Delete.
                b'~' => matches!(
                    params.split(|b| *b == b';').next(),
                    Some(b"1" | b"3" | b"7")
                ),
                _ => false,
            }
        }
        _ => false,
    };
    !removes_or_moves_back
}

fn incomplete_escape(sequence: &[u8]) -> bool {
    match sequence {
        b"\x1b" | b"\x1b[" | b"\x1bO" => true,
        [0x1b, b'[', rest @ ..] => rest.iter().all(|byte| (0x20..=0x3f).contains(byte)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record that has overflowed still tells a prompt from before a fragment, one it kept, from
    /// a later one, which ends a takeover.
    #[test]
    fn an_overflowed_record_still_tells_a_later_prompt_from_an_earlier_one() {
        let mut draft = Draft::default();
        draft.prompt_sent(Some("sent before"), AgentId::GROK);
        for at in 0..300 {
            draft.submission(None, format!("message {at}"));
        }
        assert!(draft.overflowed);
        draft.mark_fragment("message 299");
        draft.prompt_sent(Some("later"), AgentId::GROK);
        assert!(draft.sent_after_fragment(Some("later"), AgentId::GROK));
        draft.prompt_sent(Some("sent before"), AgentId::GROK);
        assert!(!draft.sent_after_fragment(Some("sent before"), AgentId::GROK));
    }

    /// Keys that leave the box empty — a lone Esc, text cleared again, a recalled prompt whose
    /// text the mirror cannot know, a prompt sent that no hook matched — stop holding once an
    /// empty box is read off a screen the agent printed after them, at least `KEY_SETTLE` after.
    #[test]
    fn an_empty_box_ends_the_hold_only_once_the_keys_can_have_been_drawn() {
        let typed = Instant::now();
        for keys in [
            &b"abc\x15"[..],
            b"typed\r",
            b"ab\x1b[D\x17",
            b"x\x1b[1;5D\x1b[3~",
            b"\x7f\x0b",
        ] {
            let mut draft = Draft::default();
            draft.typed(keys, 10, typed);
            assert!(draft.holds());
            draft.shown_empty(10, typed + KEY_SETTLE);
            assert!(draft.holds(), "{keys:?}: nothing printed since the keys");
            draft.shown_empty(11, typed + KEY_SETTLE - Duration::from_millis(1));
            assert!(
                draft.holds(),
                "{keys:?}: printed perhaps before the keys were read"
            );
            draft.shown_empty(11, typed + KEY_SETTLE);
            assert!(!draft.holds(), "{keys:?}");
        }
    }

    /// Text the mirror knows was typed and not removed, and a key still waiting for the rest of
    /// its bytes, keep the hold whatever the box shows: the screen showing it empty was drawn
    /// before them, or reads a box the key has yet to change.
    #[test]
    fn known_text_or_an_unfinished_key_holds_over_an_empty_box() {
        let typed = Instant::now();
        for keys in [
            &b"draft"[..],
            b"\x1b[200~pasted",
            b"\x1b[1;",
            &[0xe7, 0x8c][..],
        ] {
            let mut draft = Draft::default();
            draft.typed(keys, 10, typed);
            draft.shown_empty(11, typed + KEY_SETTLE * 4);
            assert!(draft.holds(), "{keys:?}");
        }
        let mut overflowed = Draft::default();
        for at in 0..300 {
            overflowed.submission(None, format!("message {at}"));
        }
        overflowed.typed(b"\x1b", 10, typed);
        overflowed.shown_empty(11, typed + KEY_SETTLE);
        assert!(overflowed.holds(), "an overflowed record still holds");
    }

    /// After an empty box ended the hold the mirror starts from it, so a prompt typed next is
    /// matched to its hook again although the keys before left the mirror unknown.
    #[test]
    fn a_prompt_typed_after_an_empty_box_is_matched_again() {
        let typed = Instant::now();
        let mut draft = Draft::default();
        draft.typed(b"x\x03", 10, typed);
        draft.shown_empty(11, typed + KEY_SETTLE);
        assert!(!draft.holds());
        draft.typed(b"next\r", 20, typed + KEY_SETTLE);
        assert!(draft.holds());
        draft.prompt_sent(Some("next"), AgentId::CLAUDE);
        assert!(!draft.holds());
    }

    /// A lone Esc typed while nothing was held needs nothing printed after it: an empty box read
    /// `KEY_SETTLE` after it ends the hold. One after other keys still held waits for output.
    #[test]
    fn a_lone_escape_typed_while_nothing_was_held_needs_no_output_after_it() {
        let typed = Instant::now();
        let mut draft = Draft::default();
        draft.typed(b"\x1b", 10, typed);
        draft.shown_empty(10, typed + KEY_SETTLE - Duration::from_millis(1));
        assert!(draft.holds());
        draft.shown_empty(10, typed + KEY_SETTLE);
        assert!(!draft.holds());

        let mut draft = Draft::default();
        draft.typed(b"abc\x15", 10, typed);
        draft.typed(b"\x1b", 10, typed);
        draft.shown_empty(10, typed + KEY_SETTLE * 4);
        assert!(draft.holds(), "keys before the Esc were still held");
        draft.shown_empty(11, typed + KEY_SETTLE);
        assert!(!draft.holds());

        let mut draft = Draft::default();
        draft.typed(b"\x1b", 10, typed);
        draft.typed_mid_turn();
        draft.shown_empty(10, typed + KEY_SETTLE * 4);
        assert!(
            draft.holds(),
            "an Esc that cancels a turn may have the prompt put back late"
        );
        draft.shown_empty(11, typed + KEY_SETTLE);
        assert!(!draft.holds());
    }

    /// A key that may put text the mirror cannot know in the box keeps the hold over an empty box
    /// however long ago it was typed and whatever was printed since: drawn late, its text would
    /// take a message in after it. A matching submission after it ends that hold.
    #[test]
    fn a_key_that_may_put_unknown_text_in_the_box_holds_over_an_empty_box() {
        let typed = Instant::now();
        for keys in [
            &b"\x1b[A"[..],
            b"\x1bOA",
            b"\x1b[B",
            b"\x1b[1;5A",
            b"\t",
            b"\x19",
            b"\x12",
            b"\x10",
            b"\x0e",
            b"\x16",
            b"\x13",
            b"\x1f",
            b"\x05",
            b"\x06",
            b"\x1b[C",
            b"\x1b[F",
            b"\x1b[4~",
            b"\x1by",
            b"\x1b\x1b",
            b"/mo\r",
            b"see @fi\r",
            b"\x1b[Z",
            b"\x1b[<0;10;5M",
        ] {
            let mut draft = Draft::default();
            draft.typed(keys, 10, typed);
            draft.shown_empty(11, typed + KEY_SETTLE * 4);
            assert!(draft.holds(), "{keys:?}");
        }
        let mut draft = Draft::default();
        draft.typed(b"\x1b", 10, typed);
        draft.typed(b"\x1b", 10, typed + KEY_SETTLE);
        draft.shown_empty(11, typed + KEY_SETTLE * 4);
        assert!(draft.holds(), "a second Esc on its own");

        let mut draft = Draft::default();
        // The recalled prompt sent, whose text the mirror does not know, then a typed one.
        draft.typed(b"\x1b[A\r", 10, typed);
        draft.typed(b"new\r", 11, typed);
        draft.prompt_sent(Some("new"), AgentId::CLAUDE);
        assert!(!draft.holds());
        draft.typed(b"\x1b", 12, typed);
        draft.shown_empty(13, typed + KEY_SETTLE);
        assert!(
            !draft.holds(),
            "an acknowledged submission ends what the Up held"
        );
    }

    /// Two Escs make a double press only when the second comes soon after the first, whether or not
    /// the box was read empty between them: one long after, or after a message of Octoboard's went
    /// in between, is a key of its own and needs no output.
    #[test]
    fn only_a_second_escape_soon_after_the_first_is_a_double_press() {
        let typed = Instant::now();
        let mut draft = Draft::default();
        draft.typed(b"\x1b", 10, typed);
        draft.shown_empty(10, typed + KEY_SETTLE);
        assert!(!draft.holds());
        let later = typed + Duration::from_secs(10);
        draft.typed(b"\x1b", 10, later);
        draft.shown_empty(10, later + KEY_SETTLE);
        assert!(
            !draft.holds(),
            "an Esc long after the first, the box shown empty between"
        );

        let mut draft = Draft::default();
        draft.typed(b"\x1b", 10, typed);
        draft.typed(b"\x1b", 10, later);
        draft.shown_empty(10, later + KEY_SETTLE);
        assert!(!draft.holds(), "an Esc long after the first");

        let mut draft = Draft::default();
        draft.typed(b"\x1b", 10, typed);
        draft.shown_empty(10, typed + KEY_SETTLE);
        assert!(!draft.holds());
        let soon = typed + KEY_SETTLE + Duration::from_millis(100);
        draft.typed(b"\x1b", 10, soon);
        draft.shown_empty(11, soon + KEY_SETTLE * 4);
        assert!(
            draft.holds(),
            "a double press, though the box was shown empty between the two"
        );

        let mut draft = Draft::default();
        draft.typed(b"\x1b", 10, typed);
        draft.message_written(typed + KEY_SETTLE);
        draft.typed(b"\x1b", 10, soon);
        draft.shown_empty(11, soon + KEY_SETTLE * 4);
        assert!(
            !draft.holds(),
            "a message of Octoboard's went in between the two"
        );

        let mut draft = Draft::default();
        draft.typed(b"\x1b", 10, typed);
        draft.typed(
            b"\x1b",
            10,
            typed + DOUBLE_ESCAPE - Duration::from_millis(1),
        );
        draft.shown_empty(11, typed + DOUBLE_ESCAPE * 4);
        assert!(draft.holds(), "a double press");
        assert!(draft.may_hold_unknown_text());
    }

    /// A message of Octoboard's written after a key that may bring back unknown text ends what that
    /// key holds: an empty box read after it ends the hold again.
    #[test]
    fn a_message_written_after_a_history_key_lets_an_empty_box_end_the_hold() {
        let typed = Instant::now();
        let mut draft = Draft::default();
        draft.typed(b"\x1b[A", 10, typed);
        draft.typed(b"\x15\x15", 11, typed);
        draft.shown_empty(12, typed + KEY_SETTLE * 4);
        assert!(draft.holds());
        draft.submission(None, "message".to_string());
        draft.message_written(typed + KEY_SETTLE);
        assert!(!draft.may_hold_unknown_text());
        draft.shown_empty(12, typed + KEY_SETTLE * 4);
        assert!(!draft.holds());
    }

    /// A key typed less than `KEY_SETTLE` ago may have left text the screen does not show yet when
    /// it typed text, may bring back text, or came while the mirror does not know the box's text.
    /// Text cleared again, or a key that only moves back over an empty box, leaves none.
    #[test]
    fn a_young_key_may_leave_undrawn_text_unless_it_leaves_the_box_known_empty() {
        let typed = Instant::now();
        let young = typed + KEY_SETTLE - Duration::from_millis(1);
        for (keys, undrawn) in [
            (&b"abc"[..], true),
            (b"\x1b[A", true),
            (b"ab\x1b[D", true),
            (b"\x1b", true),
            (b"ab\x1b[Dc", true),
            (b"abc\x15", false),
            (b"typed\r", false),
        ] {
            let mut draft = Draft::default();
            draft.typed(keys, 10, typed);
            assert_eq!(draft.text_may_be_undrawn(young), undrawn, "{keys:?}");
            assert!(!draft.text_may_be_undrawn(typed + KEY_SETTLE), "{keys:?}");
        }
    }

    /// A message written less than `KEY_SETTLE` after a key that may insert does not end what that
    /// key holds: its text can land after the message.
    #[test]
    fn a_message_written_right_after_an_inserting_key_keeps_its_hold() {
        let typed = Instant::now();
        let mut draft = Draft::default();
        draft.typed(b"\x16", 10, typed);
        draft.message_written(typed + KEY_SETTLE - Duration::from_millis(1));
        assert!(draft.may_hold_unknown_text());
        draft.message_written(typed + KEY_SETTLE);
        assert!(!draft.may_hold_unknown_text());
    }

    /// A lone Esc the empty box settled is a whole key: the next key is not read as Alt and a
    /// prompt typed after it is matched to its hook.
    #[test]
    fn a_lone_escape_an_empty_box_settled_leaves_the_next_key_alone() {
        let typed = Instant::now();
        let mut draft = Draft::default();
        draft.typed(b"\x1b", 10, typed);
        draft.shown_empty(11, typed + KEY_SETTLE);
        assert!(!draft.holds());
        draft.typed(b"next\r", 20, typed + KEY_SETTLE);
        draft.prompt_sent(Some("next"), AgentId::CLAUDE);
        assert!(!draft.holds());
    }

    /// A lone Esc leaves the mirror waiting for a next byte; once it is `KEY_SETTLE` old it is a
    /// whole key, and a key of Octoboard's may follow it. The start of a longer sequence is not.
    #[test]
    fn a_lone_escape_becomes_an_input_boundary_once_it_is_settled() {
        let typed = Instant::now();
        let mut draft = Draft::default();
        draft.typed(b"\x1b", 10, typed);
        assert!(!draft.at_input_boundary(typed));
        assert!(draft.at_input_boundary(typed + KEY_SETTLE));
        draft.typed(b"[1;", 10, typed);
        assert!(!draft.at_input_boundary(typed + KEY_SETTLE * 4));
    }

    #[test]
    fn a_partial_paste_escape_or_utf8_character_cannot_be_taken_over() {
        for (first, rest) in [
            (&b"\x1b[200~draft"[..], &b"\x1b[201~"[..]),
            (&b"\x1b[1;"[..], &b"5D"[..]),
            (&b"\x1bO"[..], &b"D"[..]),
            (&[0xe7, 0x8c][..], &[0xab][..]),
        ] {
            let mut draft = Draft::default();
            draft.input(first);
            assert!(!draft.at_input_boundary(Instant::now()));
            draft.input(rest);
            assert!(draft.at_input_boundary(Instant::now()));
        }
    }

    #[test]
    fn only_a_matching_submission_acknowledges_the_input_before_its_enter() {
        let mut draft = Draft::default();
        draft.submission(None, "program".to_string());
        draft.input(b"first\rnext draft");
        draft.prompt_sent(Some("program"), AgentId::GROK);
        draft.prompt_sent(Some("background notification"), AgentId::GROK);
        assert!(draft.holds());
        draft.prompt_sent(Some("first"), AgentId::GROK);
        assert!(draft.holds(), "typing after Enter is a new draft");
        draft.input(b"\r");
        draft.prompt_sent(Some("next draft"), AgentId::GROK);
        assert!(!draft.holds());
    }

    #[test]
    fn paste_and_enter_in_one_chunk_can_be_acknowledged_but_embedded_enters_cannot() {
        let mut draft = Draft::default();
        for chunk in [&b"\x1b[20"[..], b"0~line one\rline two\x1b[201~"] {
            draft.input(chunk);
        }
        draft.prompt_sent(Some("line one\nline two"), AgentId::GROK);
        assert!(draft.holds());
        draft.input(b"\r");
        draft.prompt_sent(Some("line one\nline two"), AgentId::GROK);
        assert!(!draft.holds());
        draft.input(b"\x1b[200~next\nlines\x1b[201~\r");
        draft.prompt_sent(Some("next\nlines"), AgentId::GROK);
        assert!(!draft.holds());
    }

    #[test]
    fn non_submitting_enters_and_unknown_edits_do_not_make_later_submissions_lag() {
        let mut draft = Draft::default();
        draft.input(b"\r/model\rhello\x1b[A\r");
        draft.prompt_sent(Some("a recalled prompt"), AgentId::GROK);
        assert!(draft.holds());
        draft.input(b"a simple prompt\r");
        draft.prompt_sent(Some("a simple prompt"), AgentId::GROK);
        assert!(!draft.holds());
    }

    #[test]
    fn clearing_a_draft_does_not_acknowledge_it_and_alt_enter_inserts_a_newline() {
        let mut draft = Draft::default();
        draft.input(b"draft\x15");
        draft.prompt_sent(Some("draft"), AgentId::GROK);
        assert!(draft.holds());
        draft.input(b"first\x1b\rsecond\r");
        draft.prompt_sent(Some("first\nsecond"), AgentId::GROK);
        assert!(!draft.holds());
    }

    #[test]
    fn backspace_removes_one_unicode_character() {
        let mut draft = Draft::default();
        draft.input("hi猫".as_bytes());
        draft.input(b"\x7f!\r");
        draft.prompt_sent(Some("hi!"), AgentId::GROK);
        assert!(!draft.holds());
    }

    #[test]
    fn excess_candidates_stay_bounded_and_cannot_be_acknowledged_out_of_order() {
        let mut draft = Draft::default();
        for _ in 0..1000 {
            draft.input(b"ignored\r");
        }
        assert_eq!(draft.submissions.len(), 256);
        draft.prompt_sent(Some("ignored"), AgentId::GROK);
        assert!(draft.holds());
    }

    #[test]
    fn codex_trims_only_outer_whitespace_when_matching_a_pasted_submission() {
        let mut draft = Draft::default();
        draft.input(b"\x1b[200~\n  first\nsecond\n\n\x1b[201~\r");
        draft.prompt_sent(Some("first\nsecond"), AgentId::CODEX);
        assert!(!draft.holds());
    }

    #[test]
    fn claude_paste_wrapper_normalization_is_applied_to_candidates_too() {
        let mut draft = Draft::default();
        draft.input(b" leading space\r");
        draft.prompt_sent(Some("leading space"), AgentId::CLAUDE);
        assert!(!draft.holds());
    }
}
