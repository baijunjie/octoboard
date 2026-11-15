//! The release of the hold the keys typed into a session keep on Octoboard's messages, and the
//! checks a message written past the draft's grace still waits on: both read the agent's input box
//! off the screen, off a screen that shows a whole frame (`output_pace`) and can no longer predate
//! the keys (`draft`'s key timing); past the grace, an empty box read off a frame not drawn whole is
//! waited on for a bounded time only, and a box that shows text is never written into. The rules
//! are in `docs/product/draft-preservation.md`, sections "Keys that leave the composer empty are
//! confirmed by the screen" and "The draft grace period".

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::draft::Draft;
use super::output_pace::OutputPace;
use super::LiveSession;

/// What the release of the draft hold works from, kept per session.
#[derive(Default)]
pub(super) struct DraftHold {
    /// Set while a [`LiveSession::schedule_draft_release`] task is under way.
    release_scheduled: AtomicBool,
    /// The output total and the count of typed bytes at the latest reading of a whole frame that
    /// left the hold in place: until either moves, another reading would read the same.
    release_tried: Mutex<Option<(u64, u64)>>,
    /// When the agent last printed something, and since when a synchronized update has been open:
    /// whether the parsed screen shows a whole frame ([`OutputPace::whole_frame`]).
    output_pace: Mutex<OutputPace>,
    /// When a message past the draft's grace was first refused for a box read empty off a frame not
    /// drawn whole, in the run of such refusals going on now ([`TORN_FRAME_WAIT`]), and when it was
    /// last refused for it. The run ends once a message is written, a box is read off a whole
    /// frame, or no refusal has come for [`TORN_RUN_GAP`].
    torn_since: Mutex<Option<(Instant, Instant)>>,
}

/// How long a message past the draft's grace waits, retry after retry, on a box read empty off a
/// frame not drawn whole before it goes in on that reading, where the typing record knows of no text
/// typed into the box that nothing since removed ([`Draft::knows_text`]). An agent that keeps printing, as one
/// redrawing its spinner mid-turn many times a second can, is never quiet long enough for a whole
/// frame ([`OutputPace::whole_frame`]), and past the grace delivery takes priority over the text a
/// frame read in part may hide.
const TORN_FRAME_WAIT: Duration = Duration::from_secs(2);

/// How long after the last refusal for a frame not drawn whole a refusal still continues its run
/// ([`TORN_FRAME_WAIT`]); one later starts a run of its own, as the message was not being retried
/// in between. Several times the period of the session watch's retry of a held queue (two seconds),
/// so that retries on that period, late by however much a busy host makes them, keep the run going.
const TORN_RUN_GAP: Duration = Duration::from_secs(10);

/// How long a write that is to act on text in the agent's input box (stash it, or clear it) waits
/// for the screen to show a whole frame before it reads the box ([`LiveSession::wait_for_whole_frame`]).
/// An idle agent is still within the 50 ms a whole frame takes; one redrawing mid-turn is still that
/// long now and then between redraws. A write that finds no whole frame in this time is refused
/// and retried.
const WHOLE_FRAME_WAIT: Duration = Duration::from_millis(500);

/// [`WHOLE_FRAME_WAIT`], except under a test that keeps the frame torn on purpose and lowers it with
/// `WHOLE_FRAME_WAIT_OVERRIDE`, so that its own reading comes straight after it set the frame up.
/// Read on the writer's thread.
fn whole_frame_wait() -> Duration {
    #[cfg(test)]
    {
        WHOLE_FRAME_WAIT_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(WHOLE_FRAME_WAIT)
    }
    #[cfg(not(test))]
    WHOLE_FRAME_WAIT
}

#[cfg(test)]
thread_local! {
    pub(super) static WHOLE_FRAME_WAIT_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

impl LiveSession {
    /// Notes a chunk of output, read now, before the parsed screen takes it in, so that a reading
    /// which finds the chunk on the screen also finds it noted ([`Self::composer_for_draft`]).
    pub(super) fn note_output_pace(&self, data: &[u8]) {
        self.draft_hold
            .output_pace
            .lock()
            .expect("output pace lock poisoned")
            .note(data, Instant::now());
    }

    /// Moves the latest key and the latest output back as far as the empty-box release and the
    /// writes past the draft's grace wait on them: as if the agent had been still since well
    /// after it drew what the key did.
    #[cfg(test)]
    pub(crate) fn settle_input(&self) {
        self.draft
            .lock()
            .expect("draft lock poisoned")
            .settle_typing();
        self.draft_hold
            .output_pace
            .lock()
            .expect("output pace lock poisoned")
            .quieten();
    }

    /// Whether the parsed screen can be taken to show a whole frame at `now`
    /// ([`OutputPace::whole_frame`]).
    pub(super) fn shows_whole_frame(&self, now: Instant) -> bool {
        self.draft_hold
            .output_pace
            .lock()
            .expect("output pace lock poisoned")
            .whole_frame(now)
    }

    /// The text in the agent's input box as the screen shows it now, read with the input lock
    /// held and `draft` locked, and the output total the screen was read at where it showed a
    /// whole frame. A box read empty off a whole frame that can no longer predate the keys typed
    /// last ends the hold they keep on Octoboard's messages ([`Draft::shown_empty`]): the user
    /// typed something that left it empty — an Esc, text cleared again, a prompt sent that no hook
    /// matched — and with the lock held no key can come in between. A frame read in part can show
    /// the box cleared and not yet drawn again, so it ends nothing.
    pub(super) fn composer_for_draft(&self, draft: &mut Draft) -> (Option<String>, Option<u64>) {
        let (composer, whole, now) = self.read_box();
        if let (Some(""), Some(output)) = (composer.as_deref(), whole) {
            draft.shown_empty(output, now);
        }
        (composer, whole)
    }

    /// Waits, for [`WHOLE_FRAME_WAIT`] at most and without the input lock, until the screen shows a
    /// whole frame ([`Self::shows_whole_frame`]), and says whether it did. A reading taken after it
    /// still checks the frame itself ([`Self::read_box`]).
    pub(super) fn wait_for_whole_frame(&self) -> bool {
        super::wait::wait_for(whole_frame_wait(), || {
            self.shows_whole_frame(Instant::now()) || self.is_frozen()
        })
    }

    /// The text in the agent's input box as the screen shows it now (`None` where no box is
    /// recognised), the output total the screen was read at where it showed a whole frame, and the
    /// time it was read.
    pub(super) fn read_box(&self) -> (Option<String>, Option<u64>, Instant) {
        // The total first, the pace next: a chunk counted in the total has been noted in the pace
        // already, and one taken in after the total was read changes the total read again below.
        let output = self.output_total();
        let now = Instant::now();
        let whole = self.shows_whole_frame(now);
        let composer = self.input_box(&self.screen_text_undimmed());
        let whole = (whole && self.output_total() == output).then_some(output);
        (composer, whole, now)
    }

    /// Ends the hold the keys typed into the session keep on Octoboard's messages where the
    /// agent's input box shows that nothing typed is left in it ([`Self::composer_for_draft`]),
    /// and says whether it did. **Blocks** on the input lock, which a write holds while the agent
    /// is slow to read.
    pub(crate) fn release_draft_hold(&self) -> bool {
        if !self.checks_composer {
            return false;
        }
        let _input = self.pty.lock_input();
        let mut draft = self.draft.lock().expect("draft lock poisoned");
        if !draft.holds() || self.input_swap_active() || self.stash_shown() {
            return false;
        }
        let (_, whole) = self.composer_for_draft(&mut draft);
        let released = !draft.holds();
        if let (false, Some(output)) = (released, whole) {
            *self
                .draft_hold
                .release_tried
                .lock()
                .expect("draft release lock poisoned") = Some((output, draft.edits()));
        }
        released
    }

    /// Runs [`Self::release_draft_hold`] off the runtime once the keys typed last can have been
    /// drawn ([`Draft::settled`]) and the screen shows a whole frame, and writes the session's
    /// queue out when it ends the hold: a message held for what the user typed goes in once the
    /// box is drawn empty, as no hook comes for keys that sent nothing. Not again while neither the
    /// output nor the keys have moved since a whole frame left the hold in place. For the
    /// session's watch, while something is queued.
    pub(crate) fn schedule_draft_release(&self) {
        let output = self.output_total();
        let now = Instant::now();
        let whole = self.shows_whole_frame(now);
        let tried = *self
            .draft_hold
            .release_tried
            .lock()
            .expect("draft release lock poisoned");
        // Without waiting: a write holds the draft's lock while the agent is slow to read.
        let due = whole
            && self.draft.try_lock().is_ok_and(|draft| {
                draft.holds()
                    && draft.settled(output, now)
                    && tried != Some((output, draft.edits()))
            });
        if !due
            || self
                .draft_hold
                .release_scheduled
                .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let observer = self.title.lock().expect("title observer lock poisoned");
        let state = observer.state.upgrade();
        let runtime = observer.runtime.clone();
        drop(observer);
        let (Some(state), Some(runtime)) = (state, runtime) else {
            self.draft_hold
                .release_scheduled
                .store(false, Ordering::Release);
            return;
        };
        let (id, launch) = (self.id.clone(), self.launch_id.clone());
        runtime.spawn_blocking(move || {
            let Some(live) = state
                .live_session(&id)
                .filter(|live| live.launch_id == launch)
            else {
                return;
            };
            let released = live.release_draft_hold();
            live.draft_hold
                .release_scheduled
                .store(false, Ordering::Release);
            if released {
                state.retry_pending_outbox(&id);
            }
        });
    }

    /// Why a message written past the draft's grace, which no longer waits for the keys typed to
    /// be confirmed but is never written together with text in the box, has to wait, if it does;
    /// read with the input lock held and `draft` locked. It waits, the watch's retry coming back to
    /// it, while the box shows text; while the box cannot be read and any key typed into it is not
    /// yet confirmed ([`Draft::holds`]), whatever it typed or removed, or may have brought back text
    /// the mirror cannot know, such as a prompt from history: an unreadable box cannot show that
    /// such keys left it empty; while the latest key may have put text in the box that is not drawn
    /// yet; and while a box read empty comes from a frame read in part, for [`TORN_FRAME_WAIT`] of
    /// retries at most, or with no bound while the typing record knows of text typed into it that
    /// nothing since removed ([`Draft::knows_text`]). Text that shows is stashed or cleared out of
    /// the message's way before it comes here (`Self::write_message_input`).
    pub(super) fn past_grace_refusal(&self, draft: &Draft) -> Option<&'static str> {
        let (composer, whole, now) = self.read_box();
        // Any unconfirmed key, not only known text: the box cannot be read to show what it did.
        if composer.is_none() && (draft.holds() || draft.may_hold_unknown_text()) {
            return Some(
                "the session's input box cannot be read after keys that may have put text in it",
            );
        }
        if draft.text_may_be_undrawn(now) {
            return Some("keys just typed into the session may not be drawn in its input box yet");
        }
        if composer.as_ref().is_some_and(|text| !text.is_empty()) {
            self.end_torn_run();
            return Some(
                "the session's input box holds text, which a message is never written with",
            );
        }
        if composer.is_none() || whole.is_some() {
            self.end_torn_run();
            return None;
        }
        // Text the typing record knows was typed and not since removed may be what the frame hides:
        // such a box waits for a whole frame, however long, and is no part of a run.
        if draft.knows_text() {
            return Some(
                "the session's input box was read empty off a frame not yet drawn whole, while \
                 text typed into it is not known to have left it",
            );
        }
        // The run is kept when the wait is over, until a message is written
        // ([`Self::end_torn_run`]): a write that fails for another reason waits no longer next
        // time.
        if self.note_torn_refusal(now) {
            return None;
        }
        Some("the session's input box was read empty off a frame not yet drawn whole")
    }

    /// Notes a reading at `now` of a box empty off a frame not drawn whole, which continues the run
    /// of such refusals or starts one, and says whether the run has lasted [`TORN_FRAME_WAIT`],
    /// when the reading no longer refuses the message.
    fn note_torn_refusal(&self, now: Instant) -> bool {
        let mut torn = self
            .draft_hold
            .torn_since
            .lock()
            .expect("torn frame lock poisoned");
        let since = match *torn {
            Some((since, last)) if now.saturating_duration_since(last) < TORN_RUN_GAP => since,
            _ => now,
        };
        *torn = Some((since, now));
        now.saturating_duration_since(since) >= TORN_FRAME_WAIT
    }

    /// Ends the run of refusals for a frame not drawn whole ([`Self::note_torn_refusal`]), once a
    /// box has been read off a whole frame or a message has gone in.
    pub(super) fn end_torn_run(&self) {
        *self
            .draft_hold
            .torn_since
            .lock()
            .expect("torn frame lock poisoned") = None;
    }
}

#[cfg(test)]
mod tests;
