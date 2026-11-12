//! Native composer stashes keep editor state inside the CLI while messages use its input.

use super::*;
use crate::protocol::{notice_code, Code, Notice};
use std::collections::VecDeque;

const MAX_BUFFERED_INPUT: usize = 1024 * 1024;
const SETTLE: Duration = Duration::from_secs(3);

/// Tests give the stand-in up to PATIENCE to show each handshake; timeout proofs override the
/// window and drive its clock. Read on the writer's or recovery scheduler's thread, before work
/// moves to another thread.
fn swap_settle() -> Duration {
    #[cfg(test)]
    {
        SETTLE_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    SETTLE
}

#[cfg(test)]
thread_local! {
    static SETTLE_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
struct SwapClock {
    now: std::sync::Mutex<Instant>,
    reads: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
thread_local! {
    static SWAP_CLOCK_OVERRIDE: std::cell::RefCell<Option<Arc<SwapClock>>> = const { std::cell::RefCell::new(None) };
}

fn swap_now() -> Instant {
    #[cfg(test)]
    if let Some(clock) = SWAP_CLOCK_OVERRIDE.with(|value| value.borrow().clone()) {
        let now = *clock.now.lock().expect("swap clock lock poisoned");
        clock.reads.fetch_add(1, Ordering::Release);
        return now;
    }
    Instant::now()
}

/// How long the screen may still show a message written into an empty composer as if it were
/// typed there: Claude Code draws a pasted message in its box before it takes the Enter after it
/// (some 20 ms in 2.1.316).
#[cfg_attr(test, allow(dead_code))]
const OWN_MESSAGE_SETTLE: Duration = Duration::from_millis(500);

/// How long the composer is waited on after a message went into it: [`OWN_MESSAGE_SETTLE`], except
/// under test, where a stand-in's screen is fed by a test thread that a loaded machine can hold up
/// past half a second, so tests wait up to `PATIENCE` (the wait ends as soon as the box empties) and
/// a test proving the window runs out lowers it with `OWN_MESSAGE_SETTLE_OVERRIDE`. Read on the
/// writer's thread.
fn own_message_settle() -> Duration {
    #[cfg(test)]
    {
        OWN_MESSAGE_SETTLE_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    OWN_MESSAGE_SETTLE
}

#[cfg(test)]
thread_local! {
    static OWN_MESSAGE_SETTLE_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryPhase {
    StashRequested,
    Stashed,
    Submitted,
    RestoreRequested,
    ReleasePending,
    /// Only part of a message reached the input after the draft was stashed. What the input holds
    /// is unknown: a paste left open, a cut-off escape sequence, or the message's text without its
    /// Enter, with the draft perhaps still stashed and a dialog perhaps up. Keys retained while the
    /// message went in were typed for the draft and never reach the agent: replayed into that
    /// input they could join the fragment, submit it or answer a dialog.
    Fragmented,
    /// The user typed again after a fragment, so their keys go straight in while Octoboard's own
    /// writes stay held until the agent reports a prompt submitted after the fragment: once the
    /// user has sent the input line, nothing written after it can run together with the fragment.
    /// A prompt submitted before it does not count, whether its hook comes late or the agent sends
    /// it again by itself (`Draft::sent_after_fragment`).
    TakenOver,
    Replaying,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InputRoute {
    Direct,
    Buffered,
    Dialog,
}

#[derive(Default)]
pub(super) struct InputSwap {
    phase: Option<RecoveryPhase>,
    buffered: VecDeque<Vec<u8>>,
    bytes: usize,
    pending: bool,
    reported: bool,
    allow_dialog_input: bool,
    pending_since: Option<Instant>,
    ended_reported: bool,
    /// When `write_empty_message_input` last wrote a message, until the composer is next read for a
    /// message. An `allow_mixed` write sets it too, since its paste can still be on screen when the
    /// next message is decided on; a message written after a stash does not, the restored draft
    /// being what the box then holds. Only a write that checks the composer waits on it.
    message_written: Option<Instant>,
}

/// The error of a message only partly written while a draft was stashed. The session then holds
/// every later write of Octoboard's until the user has sent a prompt after it, so unlike another
/// partial write nothing written next runs together with the fragment.
#[derive(Debug)]
pub(crate) struct HeldFragment(pub(crate) String);

impl std::fmt::Display for HeldFragment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for HeldFragment {}

/// The refusal of a write into a session whose output has been archived: gone for good, so unlike
/// [`blocked`] it is never retried.
pub(super) fn archived_refusal() -> PartialWrite {
    PartialWrite {
        written: 0,
        error: io::Error::new(
            io::ErrorKind::BrokenPipe,
            "the session output has been archived",
        ),
    }
}

pub(super) fn blocked(message: &str) -> PartialWrite {
    PartialWrite {
        written: 0,
        error: io::Error::new(io::ErrorKind::WouldBlock, message.to_string()),
    }
}

/// Unwinding retains queued user keys for bounded recovery instead of releasing them into a dialog.
struct Transaction<'a>(&'a LiveSession);

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        let mut swap = self.0.swap.lock().expect("input swap lock poisoned");
        // A takeover that came in before the write returned has nothing left to recover.
        if swap.phase.is_some() && swap.phase != Some(RecoveryPhase::TakenOver) {
            swap.pending = true;
            swap.pending_since.get_or_insert_with(Instant::now);
        }
        drop(swap);
        self.0.schedule_input_recovery();
    }
}

impl LiveSession {
    pub(crate) fn enable_native_stash(&self, enabled: bool) {
        self.native_stash.store(enabled, Ordering::Release);
    }

    pub(crate) fn input_recovery_pending(&self) -> Option<super::RecoveryPhase> {
        let swap = self.swap.lock().expect("input swap lock poisoned");
        swap.pending.then_some(swap.phase).flatten()
    }

    pub(crate) fn input_swap_active(&self) -> bool {
        self.swap
            .lock()
            .expect("input swap lock poisoned")
            .phase
            .is_some()
    }

    pub(super) fn buffer_terminal_input(&self, data: &[u8]) -> Result<bool, PartialWrite> {
        self.route_terminal_input(data)
            .map(|route| route == InputRoute::Buffered)
    }

    pub(super) fn route_terminal_input(&self, data: &[u8]) -> Result<InputRoute, PartialWrite> {
        let mut swap = self.swap.lock().expect("input swap lock poisoned");
        if swap.phase.is_none() {
            return Ok(InputRoute::Direct);
        }
        if self.is_frozen() {
            return Err(archived_refusal());
        }
        if data.is_empty() {
            return Ok(InputRoute::Buffered);
        }
        match swap.phase {
            Some(RecoveryPhase::TakenOver) => return Ok(InputRoute::Direct),
            // What an attached terminal sends by itself is no sign of the user taking over.
            Some(RecoveryPhase::Fragmented) if !is_terminal_protocol(data) => {
                let discarded = swap.bytes;
                swap.buffered.clear();
                swap.bytes = 0;
                swap.allow_dialog_input = false;
                // Nothing is left for a recovery pass to replay or time out.
                swap.pending = false;
                swap.pending_since = None;
                swap.phase = Some(RecoveryPhase::TakenOver);
                drop(swap);
                tracing::warn!(session = %self.id, discarded, "the user typed after a partly written message; retained keyboard input was discarded");
                return Ok(InputRoute::Direct);
            }
            _ => {}
        }
        // A terminal frame can exceed the usual queue budget. Accept it alone so backpressure
        // cannot permanently refuse a large paste; the WebSocket bounds each frame separately.
        // Only new, deliberate user input may answer a dialog while old edits await replay.
        // Earlier buffered edits stay retained rather than selecting a newly appeared option.
        if swap.allow_dialog_input
            && (!self.allows_automatic_input()
                || self.trust.holds_writes()
                || self.look_for_dialog().current.is_some())
        {
            return Ok(InputRoute::Dialog);
        }
        if swap.bytes > 0 && swap.bytes.saturating_add(data.len()) > MAX_BUFFERED_INPUT {
            return Err(blocked("the session's temporary input buffer is full"));
        }
        swap.bytes += data.len();
        swap.buffered.push_back(data.to_vec());
        Ok(InputRoute::Buffered)
    }

    fn composer(&self) -> Option<String> {
        self.input_box(&self.screen_text_undimmed())
    }

    pub(super) fn note_message_written(&self) {
        self.swap
            .lock()
            .expect("input swap lock poisoned")
            .message_written = Some(Instant::now());
    }

    /// [`Self::composer`], read once a message just written into an empty composer has had time
    /// to leave it: until then the screen can show that message where a draft would be, and
    /// stashing it would hold the message back instead of a draft.
    fn settled_composer(&self) -> Option<String> {
        let written = self
            .swap
            .lock()
            .expect("input swap lock poisoned")
            .message_written
            .take();
        let settle = own_message_settle();
        loop {
            let composer = self.composer();
            if composer.as_deref() == Some("")
                || written.is_none_or(|at| at.elapsed() >= settle)
                || self.is_frozen()
                || self.poll_exit()
            {
                return composer;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(super) fn stash_shown(&self) -> bool {
        stash_marker(self.agent, &self.screen_text())
    }

    fn set_swap_phase(&self, phase: RecoveryPhase) {
        self.swap.lock().expect("input swap lock poisoned").phase = Some(phase);
    }

    fn wait_for_swap(&self, restored: bool, deadline: Instant) -> bool {
        loop {
            if self.is_frozen() || self.poll_exit() {
                return false;
            }
            let shown = self.stash_shown();
            let composer = self.composer();
            if if restored {
                !shown && composer.as_ref().is_some_and(|text| !text.is_empty())
            } else {
                shown && composer.as_deref() == Some("")
            } {
                return true;
            }
            if swap_now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn swap_key(&self) -> Result<(), PartialWrite> {
        let input = self.pty.lock_input();
        if self.is_frozen()
            || self.poll_exit()
            || !self.allows_automatic_input()
            || self.trust.holds_writes()
            || self.look_for_dialog().current.is_some()
        {
            return Err(blocked(
                "the session cannot accept a stash key while ended or showing a dialog",
            ));
        }
        self.input_writes.fetch_add(1, Ordering::AcqRel);
        // Ctrl-S: the key that stashes the draft, and restores it, where the agent has a stash.
        self.pty.write_input_locked(&input, &[Input::Ctrl('s')])
    }

    /// `Ok` means the entire payload reached the CLI, even when restoration is still pending.
    /// An error counts only payload bytes, so callers never retry an already submitted message.
    pub(crate) fn write_message_input(
        &self,
        message: &[Input],
        prompt: &str,
        allow_mixed: bool,
    ) -> Result<(), PartialWrite> {
        // Past the draft's grace too, text the box shows is stashed where the agent can stash it:
        // only where it cannot, or does not confirm the stash, does the message join that text.
        if !self.checks_composer || self.settled_composer().as_deref() == Some("") {
            return self.write_empty_message_input(message, prompt, allow_mixed);
        }
        let native = {
            let _input = self.pty.lock_input();
            let mut swap = self.swap.lock().expect("input swap lock poisoned");
            if swap.phase.is_some() {
                return Err(blocked("an input replacement is in progress"));
            }
            let native = self.native_stash.load(Ordering::Acquire)
                && self
                    .draft
                    .lock()
                    .expect("draft lock poisoned")
                    .at_input_boundary(Instant::now())
                && !self.stash_shown()
                && self.composer().is_some_and(|text| !text.is_empty())
                && !self.trust.holds_writes()
                && self.allows_automatic_input()
                && self.look_for_dialog().current.is_none();
            if native {
                swap.phase = Some(RecoveryPhase::StashRequested);
            }
            native
        };
        if !native {
            return self.write_empty_message_input(message, prompt, allow_mixed);
        }
        let transaction = Transaction(self);
        let deadline = swap_now() + swap_settle();
        if self.swap_key().is_err() {
            self.release_input_protection();
            return Err(blocked("the session could not stash its composer"));
        }
        if !self.wait_for_swap(false, deadline) {
            self.report_input_recovery(notice_code::DRAFT_SAVE_UNCONFIRMED, "The agent did not confirm saving the draft in time. The message will be written into its current input and may include unfinished user text.");
            self.native_stash.store(false, Ordering::Release);
            // The stash key may already have taken effect. Do not toggle it again; submission
            // itself can restore a native stash, and the current editor is the fallback target.
            let result = self.write_swap_payload_or_hold(message, prompt);
            if !result.as_ref().err().is_some_and(|error| error.written > 0) {
                self.release_input_protection();
            }
            return result;
        }
        self.set_swap_phase(RecoveryPhase::Stashed);
        let result = self.write_swap_payload_or_hold(message, prompt);
        if !self.settle_stashed_payload(&result) {
            return result;
        }
        if self.wait_for_swap(true, deadline) {
            if !self.finish_input_swap() {
                self.report_input_recovery(notice_code::TYPING_HELD, "The draft was restored, but replaying retained keyboard input has not completed.");
            }
        } else {
            self.report_input_recovery(notice_code::DRAFT_RESTORE_UNCONFIRMED, "The agent did not confirm restoring the draft in time. Retained keyboard input will be replayed into its current input; a written message will not be sent again.");
            self.release_input_protection();
        }
        drop(transaction);
        result
    }

    /// Moves a confirmed stash on once its payload's write `result` is in, and says whether the
    /// write then waits for the agent to restore the draft.
    fn settle_stashed_payload(&self, result: &Result<(), PartialWrite>) -> bool {
        match result {
            Ok(()) => self.set_swap_phase(RecoveryPhase::Submitted),
            // The message is in the box but unsent, so the agent restores the draft only once the
            // user sends it; a recovery pass recognises that restore as it does a submitted one's.
            Err(error)
                if error
                    .error
                    .get_ref()
                    .is_some_and(|error| error.is::<super::NotSubmitted>()) =>
            {
                self.set_swap_phase(RecoveryPhase::Submitted);
                return false;
            }
            Err(error) if error.written > 0 => return false,
            Err(_) => {
                // Only this confirmed owned stash may be restored with a key, and a dialog may
                // have appeared since the payload was refused.
                self.set_swap_phase(RecoveryPhase::RestoreRequested);
                let _ = self.swap_key();
            }
        }
        true
    }

    fn write_swap_payload(&self, message: &[Input], prompt: &str) -> Result<(), PartialWrite> {
        let (now, rest) = super::paste_apart::split(message, self.submits_paste_apart());
        {
            let input = self.pty.lock_input();
            if self.is_frozen()
                || self.poll_exit()
                || !self.allows_automatic_input()
                || self.trust.holds_writes()
                || self.look_for_dialog().current.is_some()
            {
                return Err(blocked(
                    "the session cannot take a message while a dialog or trust confirmation is open",
                ));
            }
            let mut draft = self.draft.lock().expect("draft lock poisoned");
            draft.submission(None, prompt.to_string());
            self.input_writes.fetch_add(1, Ordering::AcqRel);
            self.pty.write_input_locked(&input, now)?;
            draft.message_written(Instant::now());
        }
        // Not required to show the message alone: a stash the agent never confirmed leaves the
        // draft in the box.
        self.submit_rest(now, rest, false)
    }

    /// [`Self::write_swap_payload`], which after writing only part of the message leaves the input
    /// held ([`RecoveryPhase::Fragmented`]) and marks the error [`HeldFragment`].
    fn write_swap_payload_or_hold(
        &self,
        message: &[Input],
        prompt: &str,
    ) -> Result<(), PartialWrite> {
        self.write_swap_payload(message, prompt).map_err(|partial| {
            // A message written in full and left in the box is no fragment: nothing is missing
            // from it for what comes next to join.
            if partial.written == 0
                || partial
                    .error
                    .get_ref()
                    .is_some_and(|error| error.is::<super::NotSubmitted>())
            {
                return partial;
            }
            self.mark_input_fragmented(prompt);
            PartialWrite {
                written: partial.written,
                error: io::Error::new(
                    partial.error.kind(),
                    HeldFragment(partial.error.to_string()),
                ),
            }
        })
    }

    fn mark_input_fragmented(&self, prompt: &str) {
        self.native_stash.store(false, Ordering::Release);
        self.draft
            .lock()
            .expect("draft lock poisoned")
            .mark_fragment(prompt);
        {
            let mut swap = self.swap.lock().expect("input swap lock poisoned");
            swap.phase = Some(RecoveryPhase::Fragmented);
            swap.reported = false;
        }
        self.report_input_recovery(notice_code::TYPING_BLOCKED, "Only part of an automatic message reached the input. It will not be sent again. Keyboard input retained while it was written is discarded when the user next types, and later keys go straight in; automatic input, the queue behind the message included, waits until the agent reports a prompt submitted after that.");
    }

    /// Ends a takeover after a fragment once the agent reports a prompt submitted after it. Only a
    /// takeover: before it, no key of the user's has reached the agent, so a prompt sent then is
    /// one from before the fragment, and the input may still hold the fragment.
    pub(super) fn end_input_takeover(&self) {
        let mut swap = self.swap.lock().expect("input swap lock poisoned");
        if swap.phase != Some(RecoveryPhase::TakenOver) {
            return;
        }
        swap.phase = None;
        swap.pending = false;
        swap.reported = false;
        swap.allow_dialog_input = false;
        swap.pending_since = None;
    }

    fn release_input_protection(&self) {
        self.native_stash.store(false, Ordering::Release);
        {
            let mut swap = self.swap.lock().expect("input swap lock poisoned");
            swap.phase = Some(RecoveryPhase::ReleasePending);
            swap.allow_dialog_input = true;
        }
        if !self.finish_input_swap() {
            self.report_input_recovery(notice_code::TYPING_HELD, "Retained keyboard input is waiting for a dialog to close or for the terminal to accept it; new user keys can answer the dialog.");
        }
    }

    /// Tells every client about this session's input recovery, once per recovery: `code` is one of
    /// the draft and typing notices, `detail` its English text.
    fn report_input_recovery(&self, code: Code, detail: &str) {
        {
            let mut swap = self.swap.lock().expect("input swap lock poisoned");
            if swap.reported {
                return;
            }
            swap.reported = true;
        }
        self.broadcast_notice(Notice::new(code, detail, &[("agent", self.agent.label())]));
        tracing::error!(session = %self.id, %code, %detail, "input replacement recovery is pending");
    }

    fn broadcast_notice(&self, notice: Notice) {
        let observer = self.title.lock().expect("title observer lock poisoned");
        if let Some(state) = observer.state.upgrade() {
            state.broadcast(notice.about(&self.id));
        }
    }

    pub(crate) fn note_input_recovery_ended(&self) {
        let bytes = {
            let mut swap = self.swap.lock().expect("input swap lock poisoned");
            if matches!(swap.phase, None | Some(RecoveryPhase::TakenOver)) || swap.ended_reported {
                return;
            }
            swap.ended_reported = true;
            swap.bytes
        };
        let message = format!("The session ended before draft restoration was confirmed. Its retained keyboard input ({bytes} bytes) cannot be replayed into a resumed session.");
        self.broadcast_notice(Notice::new(
            notice_code::TYPING_LOST,
            message.clone(),
            &[("agent", self.agent.label()), ("bytes", &bytes.to_string())],
        ));
        tracing::error!(session = %self.id, bytes, "{message}");
    }

    fn finish_input_swap(&self) -> bool {
        let input = self.pty.lock_input();
        if self.is_frozen() || self.poll_exit() {
            return false;
        }
        {
            let mut swap = self.swap.lock().expect("input swap lock poisoned");
            swap.phase = Some(RecoveryPhase::Replaying);
            swap.allow_dialog_input = true;
        }
        loop {
            if !self.allows_automatic_input()
                || self.trust.holds_writes()
                || self.look_for_dialog().current.is_some()
            {
                return false;
            }
            let data = {
                let mut swap = self.swap.lock().expect("input swap lock poisoned");
                let Some(data) = swap.buffered.pop_front() else {
                    swap.phase = None;
                    swap.pending = false;
                    swap.reported = false;
                    swap.allow_dialog_input = false;
                    swap.pending_since = None;
                    return true;
                };
                data
            };
            let written = match self.pty.write_locked(&input, &data) {
                Ok(()) => data.len(),
                Err(error) => {
                    let mut swap = self.swap.lock().expect("input swap lock poisoned");
                    swap.buffered.push_front(data[error.written..].to_vec());
                    swap.pending = true;
                    error.written
                }
            };
            self.swap.lock().expect("input swap lock poisoned").bytes -= written;
            if written > 0 {
                let mut draft = self.draft.lock().expect("draft lock poisoned");
                draft.typed(&data[..written], self.output_total(), Instant::now());
                // When these keys were typed is past knowing.
                draft.typed_mid_turn();
                drop(draft);
                self.input_writes.fetch_add(1, Ordering::AcqRel);
            }
            if written != data.len() {
                return false;
            }
        }
    }

    pub(crate) fn schedule_input_recovery(&self) {
        if self.input_recovery_pending().is_none()
            || self.recovery_scheduled.swap(true, Ordering::AcqRel)
        {
            return;
        }
        let observer = self.title.lock().expect("title observer lock poisoned");
        let state = observer.state.upgrade();
        let runtime = observer.runtime.clone();
        drop(observer);
        let (Some(state), Some(runtime)) = (state, runtime) else {
            self.recovery_scheduled.store(false, Ordering::Release);
            return;
        };
        let id = self.id.clone();
        let launch = self.launch_id.clone();
        let settle = swap_settle();
        runtime.spawn_blocking(move || {
            let Some(live) = state.live_session(&id).filter(|live| live.launch_id == launch) else { return; };
            let output_before = live.output_total();
            if live.is_frozen() || live.poll_exit() {
                live.note_input_recovery_ended();
            } else {
                // One look under the lock: a takeover between reading the window and the phase
                // would otherwise read as an expired window with nothing pending.
                let expired = {
                    let swap = live.swap.lock().expect("input swap lock poisoned");
                    swap.pending
                        && swap.pending_since.is_some_and(|since| since.elapsed() >= settle)
                        && !matches!(swap.phase, Some(RecoveryPhase::Replaying | RecoveryPhase::ReleasePending | RecoveryPhase::Fragmented | RecoveryPhase::TakenOver))
                };
                if expired {
                    live.report_input_recovery(notice_code::TYPING_HELD, "Draft protection timed out. Retained keyboard input will be replayed into the current input once dialogs clear.");
                    live.release_input_protection();
                }
                let phase = live.input_recovery_pending();
                if matches!(phase, Some(RecoveryPhase::Replaying | RecoveryPhase::ReleasePending))
                    || (matches!(phase, Some(RecoveryPhase::Submitted | RecoveryPhase::RestoreRequested))
                        && !live.stash_shown()
                        && live.composer().is_some_and(|text| !text.is_empty()))
                {
                    if !live.finish_input_swap() {
                        live.report_input_recovery(notice_code::TYPING_HELD, "Replaying retained keyboard input has not completed; new user keys can answer any visible dialog.");
                    }
                    if let Ok(record) = state.session_record(&id) {
                        state.flush_outbox(&id, state.agent_status(&record));
                    }
                }
            }
            live.finish_recovery_pass(output_before);
        });
    }

    fn finish_recovery_pass(&self, output_before: u64) {
        self.recovery_scheduled.store(false, Ordering::Release);
        // Output arriving after the pass looked at a dialog may have skipped scheduling while
        // this pass still held the claim. Re-check after releasing it so that wakeup is not lost.
        if self.output_total() != output_before {
            self.schedule_input_recovery();
        }
    }
}

fn stash_marker(agent: AgentId, screen: &str) -> bool {
    crate::adapter::provider(agent).screen().stash_shown(screen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_session_record(state: &crate::state::AppState, dir: &std::path::Path, id: &str) {
        state
            .store
            .insert_console(&crate::test_support::console(dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::test_support::bare_session(
                id,
                crate::protocol::SessionKind::Project,
            ))
            .unwrap();
    }

    fn draw(text: &str, stashed: bool, agent: AgentId) -> Vec<u8> {
        let rule = "─".repeat(60);
        let screen = match agent {
            AgentId::CLAUDE => format!(
                "{rule}\r\n❯ {text}\r\n{rule}\r\n{}",
                if stashed { "› stashed" } else { "" }
            ),
            AgentId::GROK => format!(
                "╭{}╮\r\n│ ❯ {text:50} │\r\n╰{rule}╯",
                if stashed {
                    format!("{} Stashed {}", "─".repeat(25), "─".repeat(25))
                } else {
                    rule.clone()
                }
            ),
            _ => unreachable!(),
        };
        format!("\x1b[2J\x1b[34;1H{screen}").into_bytes()
    }

    #[test]
    fn measured_native_stash_markers_require_an_empty_composer() {
        for (agent, bytes) in [
            (
                AgentId::CLAUDE,
                &include_bytes!("../../testdata/claude_native_stashed.bin")[..],
            ),
            (
                AgentId::GROK,
                &include_bytes!("../../testdata/grok_native_stashed.bin")[..],
            ),
        ] {
            let mut terminal = crate::terminal::snapshot::TerminalSnapshot::new(40, 120);
            terminal.process(bytes);
            assert!(
                stash_marker(agent, &terminal.visible_text()),
                "{}",
                terminal.visible_text()
            );
            assert_eq!(
                super::input_box::input_box(agent, &terminal.visible_text_undimmed()).as_deref(),
                Some("")
            );
        }
    }

    fn wait_until(condition: impl FnMut() -> bool) {
        wait_until_because(
            condition,
            "input transaction did not reach the expected phase",
        );
    }

    fn wait_until_because(mut condition: impl FnMut() -> bool, message: &str) {
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while !condition() {
            assert!(Instant::now() < deadline, "{message}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_native_swap_buffers_typing_and_enter_until_the_original_editor_is_restored() {
        for agent in [AgentId::CLAUDE, AgentId::GROK] {
            let live =
                crate::test_support::fake_live_session("native-swap", agent, 120, 40, "sleep 30")
                    .with_composer();
            live.enable_native_stash(true);
            live.on_output(&draw("draft", false, agent));
            live.write_terminal_input(b"draft").unwrap();
            let writer = live.clone();
            let send =
                std::thread::spawn(move || crate::session::term::send_message(&writer, "peer"));
            wait_until(|| live.input_swap_active());
            live.write_terminal_input(b"X\r").unwrap();
            assert_eq!(live.swap.lock().unwrap().bytes, 2);
            assert!(!live
                .write_keys_if(live.input_writes(), || true, &[Input::Ctrl('c')])
                .unwrap());
            // A protocol response remains independent while the user's input is buffered.
            live.write_terminal_reply(b"\x1b[1;1R").unwrap();
            assert_eq!(live.swap.lock().unwrap().bytes, 2);
            live.on_output(&draw("", true, agent));
            wait_until(|| live.swap.lock().unwrap().phase == Some(RecoveryPhase::Submitted));
            assert_eq!(live.swap.lock().unwrap().bytes, 2);
            live.on_output(&draw("draft", false, agent));
            send.join().unwrap().unwrap();
            assert!(!live.input_swap_active());
            assert_eq!(live.swap.lock().unwrap().bytes, 0);
            live.note_prompt_sent(Some("draftX"));
            assert!(
                !live.draft.lock().unwrap().holds(),
                "the Enter belongs to the restored draft"
            );
        }
    }

    /// Claude Code's empty box shows a hint while an earlier message waits for the turn to end;
    /// a message written then goes straight in, with no stash key.
    #[test]
    fn a_queued_message_hint_takes_the_empty_input_path() {
        let live = crate::test_support::fake_live_session(
            "queued-hint",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.enable_native_stash(true);
        live.on_output(include_bytes!("../../testdata/claude_input_queued.bin"));
        crate::session::term::send_message(&live, "peer").unwrap();
        assert_eq!(live.input_writes(), 1, "the payload alone");
        assert!(!live.input_swap_active());
    }

    /// Right after a message goes into an empty box the screen can still show it there; the
    /// next message waits for the box to empty rather than stashing the first one.
    #[test]
    fn a_message_just_written_is_not_stashed_as_a_draft() {
        let live = crate::test_support::fake_live_session(
            "own-message",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.enable_native_stash(true);
        live.on_output(&draw("", false, AgentId::CLAUDE));
        crate::session::term::send_message(&live, "first").unwrap();
        live.on_output(&draw("[Pasted text #1 +24 lines]", false, AgentId::CLAUDE));
        let writer = live.clone();
        let send =
            std::thread::spawn(move || crate::session::term::send_message(&writer, "second"));
        wait_until(|| live.swap.lock().unwrap().message_written.is_none());
        live.on_output(&draw("", false, AgentId::CLAUDE));
        send.join().unwrap().unwrap();
        assert_eq!(live.input_writes(), 2, "two payloads and no stash key");
        assert!(!live.input_swap_active());
    }

    /// The wait for a message just written to leave the box is bounded: a draft typed after it is
    /// stashed once the window runs out.
    #[test]
    fn a_draft_after_a_message_just_written_is_stashed_once_the_window_ends() {
        OWN_MESSAGE_SETTLE_OVERRIDE.with(|settle| settle.set(Some(Duration::from_millis(50))));
        let live = crate::test_support::fake_live_session(
            "own-message-window",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.enable_native_stash(true);
        live.on_output(&draw("", false, AgentId::CLAUDE));
        crate::session::term::send_message(&live, "first").unwrap();
        live.write_terminal_input(b"draft").unwrap();
        live.on_output(&draw("draft", false, AgentId::CLAUDE));
        let agent = live.clone();
        let screen = std::thread::spawn(move || {
            wait_until(|| agent.input_swap_active());
            agent.on_output(&draw("", true, AgentId::CLAUDE));
            wait_until(|| agent.swap.lock().unwrap().phase == Some(RecoveryPhase::Submitted));
            agent.on_output(&draw("draft", false, AgentId::CLAUDE));
        });
        let started = Instant::now();
        crate::session::term::send_message(&live, "second").unwrap();
        assert!(
            started.elapsed() < crate::test_support::PATIENCE,
            "the shortened window was waited out in full"
        );
        screen.join().unwrap();
        OWN_MESSAGE_SETTLE_OVERRIDE.with(|settle| settle.set(None));
        assert_eq!(
            live.input_writes(),
            4,
            "first message, draft, one stash key, second message"
        );
        assert!(!live.input_swap_active());
    }

    /// Claude Code's screen with a prompt brought back from its history in the box, its label
    /// written faint as Claude Code writes it.
    fn history_screen() -> Vec<u8> {
        let mut bytes = b"\x1b[2J\x1b[H".to_vec();
        bytes.extend(
            super::super::input_box::captures::HISTORY
                .trim_end()
                .replace('\n', "\r\n")
                .replace("History 100/100", "\x1b[2mHistory 100/100\x1b[22m")
                .into_bytes(),
        );
        bytes
    }

    /// Past the draft's grace a box that shows text is still stashed around the message where the
    /// agent can stash it: a prompt brought back from history is never joined to the message.
    #[test]
    fn past_the_grace_text_in_the_box_is_stashed_rather_than_joined() {
        let history = history_screen();
        let live = crate::test_support::fake_live_session(
            "grace-stash",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.enable_native_stash(true);
        live.on_output(&draw("", false, AgentId::CLAUDE));
        live.write_terminal_input(b"\x1b[A").unwrap();
        live.on_output(&history);
        let before = live.input_writes();
        let agent = live.clone();
        let screen = std::thread::spawn(move || {
            wait_until(|| agent.input_swap_active());
            agent.on_output(&draw("", true, AgentId::CLAUDE));
            wait_until(|| agent.swap.lock().unwrap().phase == Some(RecoveryPhase::Submitted));
            agent.on_output(&history);
        });
        let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
        live.write_message_input(&framed, "peer", true).unwrap();
        screen.join().unwrap();
        assert_eq!(
            live.input_writes(),
            before + 2,
            "one stash key and the message"
        );
        assert!(!live.input_swap_active());
    }

    /// Past the draft's grace a box read empty right after Up holds a message: the prompt it
    /// brings back may not be drawn yet. Once the key has settled and the prompt shows, the
    /// message goes in with the prompt stashed, alone.
    #[test]
    fn past_the_grace_a_box_read_empty_right_after_up_holds_until_the_prompt_shows() {
        let history = history_screen();
        let live = crate::test_support::fake_live_session(
            "grace-just-up",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.enable_native_stash(true);
        live.on_output(&draw("", false, AgentId::CLAUDE));
        live.settle_input();
        live.write_terminal_input(b"\x1b[A").unwrap();
        // However slowly the test runs, the Up stays younger than the time its prompt may take.
        live.draft.lock().unwrap().fresh_typing();
        let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
        let before = live.input_writes();
        assert_eq!(
            live.write_message_input(&framed, "peer", true)
                .unwrap_err()
                .written,
            0
        );
        assert_eq!(live.input_writes(), before);
        live.on_output(&history);
        live.settle_input();
        let agent = live.clone();
        let screen = std::thread::spawn(move || {
            wait_until(|| agent.input_swap_active());
            agent.on_output(&draw("", true, AgentId::CLAUDE));
            wait_until(|| agent.swap.lock().unwrap().phase == Some(RecoveryPhase::Submitted));
            agent.on_output(&history);
        });
        live.write_message_input(&framed, "peer", true).unwrap();
        screen.join().unwrap();
        assert_eq!(
            live.input_writes(),
            before + 2,
            "one stash key and the message"
        );
        assert!(!live.input_swap_active());
    }

    /// A prompt Claude Code brought back with Up, under its labelled history rule, is a draft the
    /// box can be read to hold: a message is written with it stashed, never joined to it.
    #[test]
    fn a_prompt_brought_back_from_history_is_stashed_around_a_message() {
        let history = history_screen();
        let live = crate::test_support::fake_live_session(
            "history-stash",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.enable_native_stash(true);
        live.on_output(&draw("", false, AgentId::CLAUDE));
        live.write_terminal_input(b"\x1b[A").unwrap();
        live.on_output(&history);
        assert_eq!(live.composer().as_deref(), Some("say only: pong-f"));
        let before = live.input_writes();
        let agent = live.clone();
        let screen = std::thread::spawn(move || {
            wait_until(|| agent.input_swap_active());
            agent.on_output(&draw("", true, AgentId::CLAUDE));
            wait_until(|| agent.swap.lock().unwrap().phase == Some(RecoveryPhase::Submitted));
            agent.on_output(&history);
        });
        crate::session::term::send_message(&live, "peer").unwrap();
        screen.join().unwrap();
        assert_eq!(
            live.input_writes(),
            before + 2,
            "one stash key and the message"
        );
        assert!(!live.input_swap_active());
    }

    #[test]
    fn an_existing_native_stash_is_never_overwritten() {
        for text in ["draft", ""] {
            let live = crate::test_support::fake_live_session(
                "owned-stash",
                AgentId::GROK,
                120,
                40,
                "sleep 30",
            )
            .with_composer();
            live.enable_native_stash(true);
            live.on_output(&draw(text, true, AgentId::GROK));
            let before = live.input_writes();
            assert_eq!(
                crate::session::term::send_message(&live, "peer")
                    .unwrap_err()
                    .written,
                0
            );
            assert_eq!(live.input_writes(), before);
            assert!(!live.input_swap_active());
        }
    }
    /// Holds the protection clock still until the fixture has shown each handshake phase.
    /// Install both overrides on the writer thread, where the production getters read them.
    fn controlled_send(
        live: &Arc<LiveSession>,
    ) -> (
        Arc<SwapClock>,
        std::thread::JoinHandle<std::result::Result<(), PartialWrite>>,
    ) {
        let clock = Arc::new(SwapClock {
            now: std::sync::Mutex::new(Instant::now()),
            reads: std::sync::atomic::AtomicUsize::new(0),
        });
        let writer = live.clone();
        let writer_clock = clock.clone();
        let send = std::thread::spawn(move || {
            SETTLE_OVERRIDE.with(|value| value.set(Some(SETTLE)));
            SWAP_CLOCK_OVERRIDE.with(|value| *value.borrow_mut() = Some(writer_clock));
            let result = crate::session::term::send_message(&writer, "peer");
            SWAP_CLOCK_OVERRIDE.with(|value| *value.borrow_mut() = None);
            SETTLE_OVERRIDE.with(|value| value.set(None));
            result
        });
        wait_until(|| clock.reads.load(Ordering::Acquire) >= 2);
        (clock, send)
    }

    #[test]
    fn a_restore_timeout_releases_keys_without_retrying_the_written_payload() {
        let live = crate::test_support::fake_live_session(
            "native-timeout",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.enable_native_stash(true);
        live.on_output(&draw("draft", false, AgentId::CLAUDE));
        let (clock, send) = controlled_send(&live);
        // Saving and restoring share one protection window; a slow save cannot buy another one.
        *clock.now.lock().unwrap() += SETTLE * 2 / 3;
        live.on_output(&draw("", true, AgentId::CLAUDE));
        wait_until(|| live.swap.lock().unwrap().phase == Some(RecoveryPhase::Submitted));
        live.write_terminal_input(b"user typing\r").unwrap();
        *clock.now.lock().unwrap() += SETTLE - SETTLE * 2 / 3;
        wait_until_because(
            || send.is_finished(),
            "restoration restarted the protection window",
        );
        assert!(
            send.join().unwrap().is_ok(),
            "the written payload must never be retried"
        );
        assert!(!live.input_swap_active());
        assert_eq!(live.swap.lock().unwrap().bytes, 0);
        assert!(!live.native_stash.load(Ordering::Acquire));
        assert_eq!(
            live.input_writes(),
            3,
            "one stash key, one payload, one replay"
        );
    }

    #[test]
    fn an_unconfirmed_stash_times_out_to_one_payload_without_a_second_toggle() {
        let live = crate::test_support::fake_live_session(
            "missing-stash",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.enable_native_stash(true);
        live.on_output(&draw("draft", false, AgentId::CLAUDE));
        live.write_terminal_input(b"draft").unwrap();
        let (clock, send) = controlled_send(&live);
        live.write_terminal_input(b"X\r").unwrap();
        *clock.now.lock().unwrap() += SETTLE;
        wait_until(|| send.is_finished());
        assert!(send.join().unwrap().is_ok());
        assert!(!live.input_swap_active());
        assert!(!live.native_stash.load(Ordering::Acquire));
        assert_eq!(live.swap.lock().unwrap().bytes, 0);
        assert_eq!(
            live.input_writes(),
            4,
            "draft, exactly one Ctrl-S, one payload, one replay"
        );
    }

    /// The user is told about the recovery as a notice naming the session, not as an error.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_unconfirmed_stash_is_announced_as_a_notice_about_the_session() {
        let (state, dir) = crate::test_support::app_state("missing-stash-notice");
        insert_session_record(&state, &dir, "missing-stash-notice");
        let live = crate::test_support::fake_live_session(
            "missing-stash-notice",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        state.register_live(live.clone());
        let mut events = state.subscribe();
        live.enable_native_stash(true);
        live.on_output(&draw("draft", false, AgentId::CLAUDE));
        live.write_terminal_input(b"draft").unwrap();
        let (clock, send) = controlled_send(&live);
        *clock.now.lock().unwrap() += SETTLE;
        wait_until(|| send.is_finished());
        send.join().unwrap().unwrap();
        let mut notices = Vec::new();
        while let Ok(event) = events.try_recv() {
            match event {
                crate::protocol::Event::SessionNotice {
                    session,
                    code,
                    params,
                    ..
                } => notices.push((session, code, params.get("agent").cloned())),
                crate::protocol::Event::Error { code, .. } => panic!("an error event {code}"),
                _ => {}
            }
        }
        assert_eq!(
            notices,
            [(
                "missing-stash-notice".to_string(),
                notice_code::DRAFT_SAVE_UNCONFIRMED,
                Some("Claude Code".to_string())
            )]
        );
    }

    /// Past the draft's grace, where no stash can be taken (none offered, one already shown, a key
    /// left unfinished) or the box cannot be read, a message goes into the input as it is; never
    /// into a dialog or an archived process.
    #[test]
    fn an_expired_draft_wait_bypasses_only_editor_protection() {
        // Each write past the grace looks at the box first, waiting for the message written just
        // before to leave it; no screen here ever shows it leave, and that wait is not tested.
        OWN_MESSAGE_SETTLE_OVERRIDE.with(|settle| settle.set(Some(Duration::ZERO)));
        for (screen, input, native, blocked_first) in [
            (
                draw("draft", false, AgentId::CLAUDE),
                &b"draft"[..],
                false,
                true,
            ),
            (b"unknown composer".to_vec(), &b"draft"[..], false, true),
            (
                draw("draft", true, AgentId::CLAUDE),
                &b"draft"[..],
                true,
                true,
            ),
            (
                draw("draft", false, AgentId::CLAUDE),
                &b"\x1b[200~draft"[..],
                true,
                true,
            ),
        ] {
            let live = crate::test_support::fake_live_session(
                "expired-editor",
                AgentId::CLAUDE,
                120,
                40,
                "sleep 30",
            )
            .with_composer();
            live.enable_native_stash(native);
            live.on_output(&screen);
            live.write_terminal_input(input).unwrap();
            live.settle_input();
            let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
            if blocked_first {
                assert_eq!(
                    live.write_message_input(&framed, "peer", false)
                        .unwrap_err()
                        .written,
                    0
                );
            }
            let before = live.input_writes();
            live.write_message_input(&framed, "peer", true).unwrap();
            assert_eq!(live.input_writes(), before + 1);
            assert!(!live.input_swap_active());
            live.on_output(b"\x1b[2J\x1b[HResume this conversation?\r\nEnter to confirm \xc2\xb7 Esc to dismiss\r\n");
            assert!(
                live.write_message_input(&framed, "peer", true).is_err(),
                "a deadline never answers a modal"
            );
            live.on_output(&draw("draft", false, AgentId::CLAUDE));
            live.freeze_output();
            assert!(
                live.write_message_input(&framed, "peer", true).is_err(),
                "a deadline never writes into an archived process"
            );
        }
        OWN_MESSAGE_SETTLE_OVERRIDE.with(|settle| settle.set(None));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_restore_timeout_keeps_old_keys_off_a_modal_and_releases_them_without_a_marker() {
        let (state, dir) = crate::test_support::app_state("timeout-modal");
        insert_session_record(&state, &dir, "timeout-modal");
        let live = crate::test_support::fake_live_session(
            "timeout-modal",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        state.register_live(live.clone());
        live.enable_native_stash(true);
        live.on_output(&draw("draft", false, AgentId::CLAUDE));
        let (clock, send) = controlled_send(&live);
        live.on_output(&draw("", true, AgentId::CLAUDE));
        wait_until(|| live.swap.lock().unwrap().phase == Some(RecoveryPhase::Submitted));
        live.write_terminal_input(b"old edit\r").unwrap();
        live.on_output(b"\x1b[2J\x1b[HResume this conversation?\r\nEnter to confirm \xc2\xb7 Esc to dismiss\r\n");
        *clock.now.lock().unwrap() += SETTLE;
        wait_until(|| send.is_finished());
        assert!(send.join().unwrap().is_ok());
        assert_eq!(live.swap.lock().unwrap().bytes, b"old edit\r".len());
        let before = live.input_writes();
        live.write_terminal_input(b"\x1b").unwrap();
        assert_eq!(
            live.input_writes(),
            before + 1,
            "new keys can answer the modal"
        );
        live.on_output(b"\x1b[2J\x1b[Hunknown composer after the dialog");
        wait_until(|| !live.input_swap_active());
        assert_eq!(live.swap.lock().unwrap().bytes, 0);
        assert_eq!(
            live.input_writes(),
            before + 2,
            "only the retained edit is replayed"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dialog_output_arriving_during_a_recovery_pass_is_not_lost() {
        let (state, dir) = crate::test_support::app_state("recovery-output-handoff");
        insert_session_record(&state, &dir, "recovery-output-handoff");
        let live = crate::test_support::fake_live_session(
            "recovery-output-handoff",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        state.register_live(live.clone());
        live.on_output(b"\x1b[2J\x1b[HResume this conversation?\r\nEnter to confirm \xc2\xb7 Esc to dismiss\r\n");
        live.set_swap_phase(RecoveryPhase::Replaying);
        live.write_terminal_input(b"retained edit").unwrap();
        live.swap.lock().unwrap().pending = true;
        // A pass has read the old modal but still owns its scheduling claim when it disappears.
        live.recovery_scheduled.store(true, Ordering::Release);
        let output_before = live.output_total();
        live.on_output(b"\x1b[2J\x1b[Hunknown composer after the dialog");
        assert!(live.input_swap_active());
        assert_eq!(live.swap.lock().unwrap().bytes, b"retained edit".len());
        live.finish_recovery_pass(output_before);
        wait_until(|| !live.input_swap_active());
        assert_eq!(live.swap.lock().unwrap().bytes, 0);
        assert_eq!(
            live.input_writes(),
            1,
            "the retained bytes are replayed exactly once"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn hook_permission_holds_automatic_input_but_accepts_new_user_answers() {
        let (state, dir) = crate::test_support::app_state("hook-permission-input");
        let id = "hook-permission-input";
        insert_session_record(&state, &dir, id);
        let live = crate::test_support::fake_live_session(id, AgentId::CLAUDE, 120, 40, "sleep 30")
            .with_composer();
        state.register_live(live.clone());
        live.on_output(&draw("", false, AgentId::CLAUDE));
        state
            .apply_hook_status(id, crate::protocol::SessionStatus::WaitingUser)
            .unwrap();
        assert!(!live.allows_automatic_input());
        assert!(live
            .write_empty_message_input(&[Input::Text("peer".into()), Input::Enter], "peer", true)
            .is_err());
        assert!(live
            .write_swap_payload(&[Input::Text("peer".into()), Input::Enter], "peer")
            .is_err());
        assert!(live.swap_key().is_err());
        live.set_swap_phase(RecoveryPhase::Submitted);
        live.write_terminal_input(b"old edit\r").unwrap();
        assert!(!live.finish_input_swap());
        assert_eq!(live.swap.lock().unwrap().bytes, b"old edit\r".len());
        assert_eq!(
            live.input_writes(),
            0,
            "unrecognized approval screens still block old keys"
        );
        live.write_terminal_input(b"y\r").unwrap();
        assert_eq!(
            live.input_writes(),
            1,
            "new user keys may answer hook-level permission"
        );
        assert_eq!(live.swap.lock().unwrap().bytes, b"old edit\r".len());
        state
            .apply_hook_status(id, crate::protocol::SessionStatus::Idle)
            .unwrap();
        assert!(live.allows_automatic_input());
        assert!(live.finish_input_swap());
        assert_eq!(live.swap.lock().unwrap().bytes, 0);
        assert_eq!(
            live.input_writes(),
            2,
            "old keys replay once after permission clears"
        );
    }

    /// A stand-in session left holding a fragment of "the whole message", with keys retained while
    /// it went in, and three prompts from before it the agent may still report sending: one the
    /// user typed, a message Octoboard wrote, and one already reported sent.
    fn fragmented_session(
        id: &str,
    ) -> (
        Arc<crate::state::AppState>,
        crate::test_support::ScratchDir,
        crate::test_support::StandIn,
    ) {
        let (state, dir) = crate::test_support::app_state(id);
        insert_session_record(&state, &dir, id);
        let live = crate::test_support::fake_live_session(id, AgentId::CLAUDE, 120, 40, "sleep 30")
            .with_composer();
        state.register_live(live.clone());
        live.on_output(&draw("", false, AgentId::CLAUDE));
        live.write_terminal_input(b"user prompt\r").unwrap();
        live.note_launch_prompt("earlier message".to_string());
        live.note_prompt_sent(Some("sent before"));
        // As `write_swap_payload` records the message it writes.
        live.draft
            .lock()
            .unwrap()
            .submission(None, "the whole message".to_string());
        live.set_swap_phase(RecoveryPhase::Submitted);
        live.write_terminal_input(b"old edit\r").unwrap();
        live.mark_input_fragmented("the whole message");
        // As the write's transaction leaves it when it ends.
        {
            let mut swap = live.swap.lock().unwrap();
            swap.pending = true;
            swap.pending_since = Some(Instant::now());
        }
        (state, dir, live)
    }

    fn wait_for_recovery_pass(live: &LiveSession) {
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while live.recovery_scheduled.load(Ordering::Acquire) {
            assert!(
                Instant::now() < deadline,
                "the recovery pass did not finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// After a partly written message, the user's next key takes over: what was retained is
    /// dropped, never replayed, and Octoboard's own writes wait for a prompt submitted after the
    /// fragment.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_user_takes_over_after_a_partly_written_message() {
        let (_state, _dir, live) = fragmented_session("fragment-takeover");
        let before = live.input_writes();
        live.write_terminal_input(b"\x1b[I").unwrap();
        assert_eq!(live.input_writes(), before, "a focus report is no takeover");
        live.write_terminal_input(b"typed\r").unwrap();
        assert_eq!(live.input_writes(), before + 1, "the new keys alone go in");
        {
            let swap = live.swap.lock().unwrap();
            assert_eq!(swap.bytes, 0);
            assert!(!swap.pending && swap.pending_since.is_none());
        }
        live.on_output(&draw("", false, AgentId::CLAUDE));
        assert!(
            !live.recovery_scheduled.load(Ordering::Acquire),
            "output schedules no pass for a takeover"
        );
        // A pass that finds the protection window expired, as one already under way at the
        // takeover can, leaves the takeover in place.
        {
            let mut swap = live.swap.lock().unwrap();
            swap.pending = true;
            swap.pending_since = Some(Instant::now());
        }
        SETTLE_OVERRIDE.set(Some(Duration::ZERO));
        live.on_output(&draw("", false, AgentId::CLAUDE));
        SETTLE_OVERRIDE.set(None);
        wait_for_recovery_pass(&live);
        assert_eq!(
            live.swap.lock().unwrap().phase,
            Some(RecoveryPhase::TakenOver)
        );
        assert_eq!(
            crate::session::term::send_message(&live, "peer")
                .unwrap_err()
                .written,
            0
        );
        for late in ["user prompt", "earlier message", "sent before"] {
            live.note_prompt_sent(Some(late));
            assert!(
                live.input_swap_active(),
                "{late} was submitted before the fragment"
            );
        }
        // Only the user's Enter can send what reached the input of the message itself.
        live.note_prompt_sent(Some("the whole message"));
        assert!(!live.input_swap_active());
        live.note_prompt_sent(Some("typed"));
        crate::session::term::send_message(&live, "peer").unwrap();
        assert_eq!(live.input_writes(), before + 2);
    }

    /// A write's transaction that ends only after the user took over leaves nothing pending for a
    /// recovery pass.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_transaction_ending_after_a_takeover_leaves_nothing_pending() {
        let (_state, _dir, live) = fragmented_session("fragment-transaction");
        live.write_terminal_input(b"x").unwrap();
        drop(Transaction(&live));
        let swap = live.swap.lock().unwrap();
        assert_eq!(swap.phase, Some(RecoveryPhase::TakenOver));
        assert!(!swap.pending && swap.pending_since.is_none());
    }

    /// Before a takeover no key of the user's has reached the agent, so a prompt it reports then
    /// was submitted before the fragment, which the input may still hold: the hold stays.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_prompt_sent_before_a_takeover_keeps_the_fragment_held() {
        let (_state, _dir, live) = fragmented_session("fragment-prompt");
        let before = live.input_writes();
        live.note_prompt_sent(Some("a prompt the agent sends by itself"));
        let swap = live.swap.lock().unwrap();
        assert_eq!(swap.phase, Some(RecoveryPhase::Fragmented));
        assert_eq!(swap.bytes, b"old edit\r".len());
        drop(swap);
        assert_eq!(live.input_writes(), before);
    }

    /// A message written in full over a stashed draft but left unsent in the box waits for no
    /// restore, and the restore that comes once the user sends it ends the protection. No agent
    /// both stashes natively and has its submit key checked today, so the result is made here.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_left_unsent_over_a_stash_ends_with_the_restore_after_the_users_send() {
        let id = "stash-not-submitted";
        let (state, dir) = crate::test_support::app_state(id);
        insert_session_record(&state, &dir, id);
        let live = crate::test_support::fake_live_session(id, AgentId::CLAUDE, 120, 40, "sleep 30")
            .with_composer();
        state.register_live(live.clone());
        live.enable_native_stash(true);
        live.set_swap_phase(RecoveryPhase::Stashed);
        live.on_output(&draw("peer", true, AgentId::CLAUDE));
        let not_submitted = Err(PartialWrite {
            written: "peer".len(),
            error: io::Error::other(super::super::NotSubmitted),
        });
        assert!(
            !live.settle_stashed_payload(&not_submitted),
            "nothing comes back to wait for"
        );
        assert_eq!(
            live.swap.lock().unwrap().phase,
            Some(RecoveryPhase::Submitted)
        );
        drop(Transaction(&live));
        assert!(
            live.input_swap_active(),
            "the unsent message holds the draft"
        );
        // The user sends the message, and the agent puts the draft back.
        live.on_output(&draw("draft", false, AgentId::CLAUDE));
        wait_until(|| !live.input_swap_active());
    }

    #[test]
    fn one_large_terminal_frame_makes_progress_without_dropping_the_next_frame() {
        let live = crate::test_support::fake_live_session(
            "large-input",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        );
        live.set_swap_phase(RecoveryPhase::Submitted);
        let large = vec![b'x'; MAX_BUFFERED_INPUT + 1];
        assert!(live.buffer_terminal_input(&large).unwrap());
        let error = live.buffer_terminal_input(b"next").unwrap_err();
        assert_eq!(error.written, 0);
        assert_eq!(error.error.kind(), io::ErrorKind::WouldBlock);
        let swap = live.swap.lock().unwrap();
        assert_eq!(swap.bytes, large.len());
        assert_eq!(swap.buffered.front(), Some(&large));
        assert_eq!(swap.buffered.len(), 1);
    }

    #[test]
    fn retained_edits_wait_for_a_dialog_but_new_user_keys_can_answer_it() {
        let live = crate::test_support::fake_live_session(
            "restore-dialog",
            AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.write_terminal_input(b"draft").unwrap();
        live.set_swap_phase(RecoveryPhase::Submitted);
        assert!(live.buffer_terminal_input(b" edit\r").unwrap());
        let mut screen = draw("draft", false, AgentId::CLAUDE);
        screen.extend_from_slice(
            b"\x1b[1;1HResume this conversation?\r\nEnter to confirm \xc2\xb7 Esc to dismiss\r\n",
        );
        live.on_output(&screen);
        assert!(live.look_for_dialog().current.is_some());
        let before = live.input_writes();
        assert!(!live.finish_input_swap());
        assert_eq!(live.input_writes(), before);
        assert_eq!(live.swap.lock().unwrap().bytes, b" edit\r".len());
        live.write_terminal_input(b"\x1b").unwrap();
        assert_eq!(
            live.input_writes(),
            before + 1,
            "new user keys answer the visible dialog"
        );
        live.on_output(&draw("draft", false, AgentId::CLAUDE));
        assert!(live.finish_input_swap());
        assert!(!live.input_swap_active());
        assert_eq!(live.swap.lock().unwrap().bytes, 0);
        live.note_prompt_sent(Some("draft edit"));
        assert!(!live.draft.lock().unwrap().holds());
    }
}
