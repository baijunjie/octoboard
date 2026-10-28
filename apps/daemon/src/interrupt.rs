//! Interrupting the turn a session's agent is running, for the `interrupt_session` control
//! request and the MCP tool of the same name: the agent's own interrupt key is sent into its
//! terminal, and the session keeps its process and conversation and goes back to awaiting
//! instructions.
//!
//! The key differs per agent, and the wrong one does harm. The measurements are in
//! `docs/agent-cli-reference.md`, sections "## Claude Code", "## Codex" and "## Grok Build"
//! (each one's "Keys that interrupt a turn"):
//!
//! - Claude Code and Codex stop on one Esc. Ctrl-C is never sent to either: at an idle Codex it
//!   exits, and a single one arms Claude Code's "press again to exit". A second Esc is never sent
//!   either: at an idle Codex two open its transcript rewind.
//! - Grok Build ignores Esc and stops on Ctrl-C, which with text in its composer clears the text
//!   instead. So the composer is emptied first: a draft found there is cleared with Ctrl-U, a press
//!   at a time, each seen on the screen, and an empty or unread composer gets one Ctrl-U. Ctrl-C at
//!   an idle Grok asks to be pressed again to quit.
//!
//! So a key goes in only while the session reads `Working` by its hooks and its terminal title
//! shows the turn running ([`crate::title_activity::TitleActivity::shows_running`]), checked under
//! the input lock right before each key: a turn that ended in the meantime gets no key at all.
//! Each key is a write of its own, never joined to a message.
//!
//! Each agent's way out of `Working` afterwards is the one it takes when the person presses the key:
//! Codex's `Interrupt` hook and Grok's `StopCancelled`, and for what fires no hook, Claude Code's
//! transcript and title watches and Grok's title watch (`crate::title_interrupt`). Claude Code and
//! Grok Build put the prompt back in their input box when the key lands before the model has shown
//! anything; that text would be sent together with the next message, so this clears it with
//! Ctrl-U, which both take the same way: one line a press, a press at the start of a line removing
//! the line break before it, and a wrapped line going in one. It does so only while the box was
//! empty before the interrupt key and nobody else has written since, and the session's queue is
//! paused from the first key on so no message of Octoboard's lands in the restored text. Text in
//! Claude Code's box before the Esc is the user's or a message left unsent and is never touched,
//! only reported. A cancel the person makes themselves is left as the agent leaves it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use crate::protocol::{error_code, Agent, CodedError, Session, SessionStatus};
use crate::session::LiveSession;
use crate::state::{AppState, ClaimKind, WriteHold};
use crate::trust::{wait_for, wait_quiet};

use input_box::{input_box, input_box_rows};

const ESC: &[u8] = b"\x1b";
const CTRL_U: &[u8] = b"\x15";
const CTRL_C: &[u8] = b"\x03";

/// How long the session is given to leave `Working` after the key. Codex's and Grok's hooks come
/// within a fifth of a second; the title watches act about a second and a quarter after the title
/// turns.
const SETTLE: Duration = Duration::from_secs(5);

/// The pause between Grok's Ctrl-U and its Ctrl-C, so the two are read as two keys.
const BETWEEN_KEYS: Duration = Duration::from_millis(100);

/// How long the screen may take to settle before the agent's input box is read.
const QUIET_TIMEOUT: Duration = Duration::from_secs(1);

/// How long a Ctrl-U has to show on the screen.
const PRESS_TIMEOUT: Duration = Duration::from_millis(500);

/// The most Ctrl-U presses spent on an input box, however many rows it takes up.
const MAX_PRESSES: usize = 40;

const POLL: Duration = Duration::from_millis(25);

/// What an interrupt came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interrupted {
    /// A running turn was stopped: the key went in and the turn ended.
    pub interrupted: bool,
    /// The session's status once the interrupt had settled.
    pub status: SessionStatus,
    /// What the caller is to know, in prose.
    pub note: String,
}

const NOTHING_TO_INTERRUPT: &str = "Nothing to interrupt: the session is not in the middle of a \
     turn. It is awaiting instructions; send it the next step with `instruct_session`.";

const NOT_SHOWN_RUNNING: &str = "No key was sent: the session reads as working, but its terminal \
     does not show a turn running (its title shows none, or has stopped changing), so a key could \
     land on something other than a turn. Check it with `get_session`.";

const WAITING_FOR_USER: &str = "No key was sent: the session's agent is showing something only \
     the user can answer (a dialog of its own, or Codex's `Action Required`), which a key would \
     answer in their place. It is theirs to clear in the terminal.";

const DRAFT_UNREAD: &str = "No key was sent: Grok Build's composer held a draft that could not \
     be read back off the screen to clear it, and a Ctrl-C would only have cleared it. Check the \
     session with `get_session`.";

const TOUCHED: &str = "Nothing was sent: something else wrote into the session's terminal just \
     then. Check it with `get_session` before trying again.";

/// Stops the turn session `id` is running. Refused, with nothing sent, for a session with no
/// process or one still starting, one being switched, handed off or restarted, and one waiting
/// for the user; a session not in a turn is answered with `interrupted: false` and nothing sent.
pub async fn interrupt_session(state: &Arc<AppState>, id: &str) -> Result<Interrupted> {
    // An event the agent posted before this call may already have ended the turn.
    state.hook_queues.settled(id).await;
    let session = state.session_record(id)?;
    let Some(_claim) = state.claim_interrupt(id) else {
        return Ok(Interrupted {
            interrupted: false,
            status: state.agent_status(&session),
            note: "An interrupt of this session is already under way; check it with \
                   `get_session`."
                .to_string(),
        });
    };
    let Some(live) = judge(state, &session)? else {
        return Ok(Interrupted {
            interrupted: false,
            status: state.agent_status(&session),
            note: NOTHING_TO_INTERRUPT.to_string(),
        });
    };
    let agent = live.agent;
    let written = state.written_mid_turn(id);
    let pressed = {
        let state = state.clone();
        tokio::task::spawn_blocking(move || press(&state, &live)).await?
    }?;
    let status = state
        .session_record(id)
        .map(|session| state.agent_status(&session))
        .unwrap_or(SessionStatus::Working);
    let (interrupted, note) = match pressed {
        Pressed::Nothing(why) => (false, nothing_sent(why).to_string()),
        Pressed::OnlyCtrlU { draft, why } => (false, only_ctrl_u(draft, why)),
        Pressed::Keys {
            draft,
            ended: false,
            ..
        } => (
            false,
            format!(
                "The interrupt key was sent, but the session still reads as working {} seconds \
                 later. Check it with `get_session` before sending it anything.{}",
                SETTLE.as_secs(),
                draft_note(draft)
            ),
        ),
        Pressed::Keys {
            draft,
            prompt,
            ended: true,
        } => (true, interrupted_note(agent, written, draft, prompt)),
    };
    Ok(Interrupted {
        interrupted,
        status,
        note,
    })
}

fn nothing_sent(why: Held) -> &'static str {
    match why {
        Held::TurnOver => NOTHING_TO_INTERRUPT,
        Held::NotShownRunning => NOT_SHOWN_RUNNING,
        Held::Touched => TOUCHED,
        Held::WaitingForUser => WAITING_FOR_USER,
        Held::DraftUnread => DRAFT_UNREAD,
    }
}

/// What the caller is told when Grok's composer was cleared and its Ctrl-C did not go in.
fn only_ctrl_u(draft: Draft, why: CtrlCHeld) -> String {
    let sent = match draft {
        Draft::None => "Only a Ctrl-U was sent, into Grok Build's empty composer",
        Draft::Cleared => "Only Ctrl-U was sent, which cleared the draft in Grok Build's composer",
        Draft::Partly => {
            "Only Ctrl-U was sent, which cleared part of the draft in Grok Build's composer"
        }
    };
    let why = match why {
        CtrlCHeld::Held(Held::TurnOver) => "the turn ended first",
        CtrlCHeld::Held(Held::NotShownRunning) => "its terminal stopped showing the turn running",
        CtrlCHeld::Held(Held::Touched) => "something else wrote into its terminal first",
        CtrlCHeld::Held(Held::WaitingForUser) => {
            "its agent began showing something only the user can answer"
        }
        CtrlCHeld::Held(Held::DraftUnread) => "its composer could no longer be read",
        CtrlCHeld::DraftLeft => {
            "the draft would not clear, and a Ctrl-C would only have cleared it"
        }
    };
    format!(
        "{sent}; the Ctrl-C that cancels the turn was not sent, because {why}. Check the session \
         with `get_session`.{}",
        if draft == Draft::None {
            ""
        } else {
            " Grok Build may let you undo the clearing with ctrl+z."
        }
    )
}

fn draft_note(draft: Draft) -> &'static str {
    match draft {
        Draft::None => "",
        Draft::Cleared | Draft::Partly => {
            " Grok Build's composer held a draft, which was cleared first so the Ctrl-C would \
             cancel the turn; Grok Build may let you undo the clearing with ctrl+z."
        }
    }
}

/// What the caller of an interrupt that stopped a turn is told.
fn interrupted_note(agent: Agent, written: usize, draft: Draft, prompt: Restored) -> String {
    let mut note = String::from(
        "Interrupted: the turn was stopped. The session keeps its conversation and is awaiting \
         instructions; send it the next step with `instruct_session`.",
    );
    if written > 0 {
        // Counted by the status read before each write, so a message written just as the turn
        // began or ended can fall on the wrong side; hence "about".
        note.push_str(&match agent {
            // A message written into a Codex turn is steered into that turn, which the cancel
            // throws away with it, unless the turn had acted on it already.
            Agent::Codex => format!(
                " Octoboard wrote about {written} message(s) into the session during that turn. \
                 Codex steers such messages into the turn the cancel threw away, so unless the \
                 turn had already acted on them they were not acted on: resend whatever still \
                 matters."
            ),
            // Claude Code may take a message queued mid-turn into the turn it is running, and
            // otherwise submits it once the turn is over; Grok Build keeps it queued until then.
            Agent::Claude => format!(
                " Octoboard wrote about {written} message(s) into the session during that turn. \
                 Claude Code may already have taken some into the turn it cancelled; the rest stay \
                 queued in it and run next, as a new turn. Check it with `get_session` before \
                 sending the next step, and resend what was not acted on."
            ),
            Agent::Grok => format!(
                " Octoboard wrote about {written} message(s) into the session during that turn; \
                 Grok Build keeps them queued and runs them next, as a new turn. Check it with \
                 `get_session` before sending the next step."
            ),
        });
    }
    note.push_str(draft_note(draft));
    if prompt == Restored::Left {
        note.push_str(&format!(
            " {}'s input box still holds text after the interrupt (the interrupted prompt it put \
             back, or text that was there before) and it could not be cleared, so a message \
             written now would be sent together with it: ask the user to clear the session's \
             input box first.",
            agent.label()
        ));
    }
    note
}

/// Applies the refusals, and says which process to interrupt, or `None` for a session not in a
/// turn.
fn judge(state: &Arc<AppState>, session: &Session) -> Result<Option<Arc<LiveSession>>> {
    let id = session.id.as_str();
    if session.status.is_dormant() {
        return Err(not_running(id, "this session is not running"));
    }
    match state.claim_kind(id) {
        Some(ClaimKind::Launch) => return Err(not_running(id, STILL_STARTING)),
        Some(ClaimKind::Switch) => return Err(busy(id)),
        None => {}
    }
    let Some(live) = state.live_session(id) else {
        return Err(not_running(id, "this session is not running"));
    };
    // Looks at the screen afresh for the agent's own dialogs, and covers Codex's `Action
    // Required` title.
    match state.write_hold(id) {
        Some(WriteHold::Starting) => return Err(not_running(id, STILL_STARTING)),
        Some(WriteHold::TrustConfirmation) => return Err(trust_pending(id)),
        Some(WriteHold::AgentDialog(_) | WriteHold::AgentAction) => return Err(waiting(id)),
        Some(WriteHold::Handoff | WriteHold::SharingChange) => return Err(busy(id)),
        // A session held only for these is at its prompt or in a turn like any other.
        Some(WriteHold::AgentError | WriteHold::UnsentMessage) | None => {}
    }
    // A trust screen a failed press or a cursor key left up keeps its hand raised after the write
    // hold has let go of it.
    if live.trust.needs_user() {
        return Err(trust_pending(id));
    }
    // The record carries the hand of a hook (the agent's own prompt or question) and of a session
    // request alike; the agent's status reads through a request's hand to what is under it.
    if session.status == SessionStatus::WaitingUser {
        return Err(waiting(id));
    }
    Ok((state.agent_status(session) == SessionStatus::Working).then_some(live))
}

const STILL_STARTING: &str = "this session is still starting, so it has no turn to interrupt yet";

fn not_running(id: &str, message: &str) -> anyhow::Error {
    CodedError::raised(error_code::SESSION_NOT_RUNNING, message, &[("session", id)])
}

fn trust_pending(id: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::SESSION_TRUST_PENDING,
        "this session's agent is showing its folder-trust confirmation, so it is not in a turn; it \
         can be interrupted once that has been answered",
        &[("session", id)],
    )
}

fn busy(id: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::SESSION_BUSY,
        "this session is being switched to another account, handed off, having its branches \
         handed over, given another owner or restarted; interrupt it once that has finished",
        &[("session", id)],
    )
}

fn waiting(id: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::SESSION_WAITING_FOR_USER,
        "this session is waiting for the user, who alone can answer what it is showing; an \
         interrupt key would answer it in their place",
        &[("session", id)],
    )
}

/// Why a key was not sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    /// The turn was over.
    TurnOver,
    /// The hooks still say the session works, but its title does not show a turn running.
    NotShownRunning,
    /// Someone else wrote into the terminal between the look and the key.
    Touched,
    /// The agent is showing a dialog of its own, or Codex's title says it waits on the user.
    WaitingForUser,
    /// Grok's composer held a draft that could not be read to clear it.
    DraftUnread,
}

/// Why Grok's Ctrl-C was not sent after its composer was cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CtrlCHeld {
    Held(Held),
    /// A draft in the composer did not clear.
    DraftLeft,
}

/// What became of a draft in Grok's composer before its Ctrl-C.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Draft {
    /// There was none, or the composer was not read.
    None,
    Cleared,
    /// Some of it went before the presses stopped.
    Partly,
}

/// What [`press`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pressed {
    /// No key was sent.
    Nothing(Held),
    /// Grok's composer was cleared with Ctrl-U and its Ctrl-C was not sent.
    OnlyCtrlU { draft: Draft, why: CtrlCHeld },
    /// The interrupt key went in; `ended` says whether the turn ended within [`SETTLE`].
    Keys {
        draft: Draft,
        prompt: Restored,
        ended: bool,
    },
}

/// What the agent's input box was left holding after the interrupt key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Restored {
    /// Nothing, as far as the screen shows, or the agent's box is not read.
    None,
    /// The prompt the agent put back, which was cleared.
    Cleared,
    /// Text: a prompt put back that could not be cleared, or what was there before the key.
    Left,
}

/// Sends the agent's interrupt key, and waits up to [`SETTLE`] for the turn to end, clearing a
/// prompt the agent restores meanwhile. **Blocks.**
fn press(state: &Arc<AppState>, live: &Arc<LiveSession>) -> Result<Pressed> {
    let agent = live.agent;
    // Read before anything is looked at: a write by anyone else after this point means what was
    // looked at may no longer be true.
    let mut writes = live.input_writes();
    // The turn's end is told by this count moving on rather than by the status, which a message
    // the agent had queued can take back to `Working` within milliseconds of the cancel.
    let ends = state.turn_ends(&live.id);
    let mut input_before = input_box(agent, &live.screen_text_undimmed());
    let stops = live.title_activity().stops();
    // Paused while keys may still go in, so no message of Octoboard's joins what the agent puts
    // back in its box; let go, and the queue flushed, when this returns.
    let _paused = input_before.is_some().then(|| state.pause_outbox(&live.id));
    let held = |writes: u64| {
        if live.input_writes() != writes {
            Held::Touched
        } else if live.agent_dialog().is_some() || live.needs_user_action() {
            // Before the status: Codex's `Action Required` makes the agent read as waiting.
            Held::WaitingForUser
        } else if working(state, &live.id) {
            Held::NotShownRunning
        } else {
            Held::TurnOver
        }
    };
    let mut draft = Draft::None;
    if agent == Agent::Grok {
        match input_before.as_deref() {
            Some(text) if !text.is_empty() => {
                let (cleared, presses) = clear_input_box(state, live, &mut writes, true);
                match cleared {
                    // Gone before the first press, sent or cleared by someone else: nothing of
                    // it was this call's to clear.
                    Cleared::Empty if presses == 0 => {}
                    Cleared::Empty => draft = Draft::Cleared,
                    Cleared::Refused if presses == 0 => return Ok(Pressed::Nothing(held(writes))),
                    Cleared::Left if presses == 0 => {
                        return Ok(Pressed::Nothing(Held::DraftUnread))
                    }
                    Cleared::Refused => {
                        return Ok(Pressed::OnlyCtrlU {
                            draft: Draft::Partly,
                            why: CtrlCHeld::Held(held(writes)),
                        })
                    }
                    Cleared::Left => {
                        return Ok(Pressed::OnlyCtrlU {
                            draft: Draft::Partly,
                            why: CtrlCHeld::DraftLeft,
                        })
                    }
                }
                input_before = Some(String::new());
            }
            _ => match key(state, live, writes, CTRL_U)? {
                Some(next) => writes = next,
                None => return Ok(Pressed::Nothing(held(writes))),
            },
        }
        std::thread::sleep(BETWEEN_KEYS);
        match key(state, live, writes, CTRL_C)? {
            Some(next) => writes = next,
            None => {
                return Ok(Pressed::OnlyCtrlU {
                    draft,
                    why: CtrlCHeld::Held(held(writes)),
                })
            }
        }
    } else {
        match key(state, live, writes, ESC)? {
            Some(next) => writes = next,
            None => return Ok(Pressed::Nothing(held(writes))),
        }
    }
    let deadline = Instant::now() + SETTLE;
    let mut prompt = None;
    loop {
        // Before the turn is seen to end: the clearing is quicker than the title watch that ends
        // it, and the queue is paused meanwhile anyway.
        if prompt.is_none() && live.title_activity().stopped_since(stops) {
            prompt = Some(match &input_before {
                Some(text) if text.is_empty() => clear_restored_prompt(state, live, &mut writes),
                // Whatever is there now may be the user's own, so it is only reported.
                Some(_) => left_in_input_box(live),
                None => Restored::None,
            });
        }
        let ended = state.turn_ends(&live.id) != ends || !working(state, &live.id);
        if ended || Instant::now() >= deadline {
            return Ok(Pressed::Keys {
                draft,
                prompt: prompt.unwrap_or(Restored::None),
                ended,
            });
        }
        std::thread::sleep(POLL);
    }
}

/// Writes one interrupt key while the turn is still running and nobody has written since the count
/// read `writes`, and returns the count past it, or `None` when nothing was written.
fn key(
    state: &Arc<AppState>,
    live: &Arc<LiveSession>,
    writes: u64,
    keys: &[u8],
) -> Result<Option<u64>> {
    match live.write_keys_if(writes, || turn_running(state, live), keys) {
        Ok(true) => Ok(Some(writes + 1)),
        Ok(false) => Ok(None),
        Err(err) => Err(anyhow!(
            "writing the interrupt key into the session's terminal failed: {}",
            err.error
        )),
    }
}

/// Whether the session is in a turn by both its hooks and its terminal title, with nothing of the
/// agent's own up that a key would answer. Called under the input lock, right before a key.
///
/// The status a hook reported is held only in the store, so the record is read here, under the
/// lock: that is what makes the check and the key one step against a hook applied in between. It
/// is a read of one row of a local database, and only the session's other writers wait on it.
fn turn_running(state: &AppState, live: &LiveSession) -> bool {
    live.title_activity().shows_running(Instant::now())
        && live.agent_dialog().is_none()
        && !live.needs_user_action()
        && state.session_record(&live.id).is_ok_and(|session| {
            session.status == SessionStatus::Working
                && state.agent_status(&session) == SessionStatus::Working
        })
}

fn working(state: &AppState, id: &str) -> bool {
    state
        .session_record(id)
        .is_ok_and(|session| state.agent_status(&session) == SessionStatus::Working)
}

/// How a run of Ctrl-U presses on an input box ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cleared {
    /// The screen shows the box empty.
    Empty,
    /// A press was not written: someone else wrote, or (for a gated run) the turn stopped running.
    Refused,
    /// The presses ran out, or the box could no longer be read, with text still in it.
    Left,
}

/// Presses Ctrl-U into the agent's input box, one at a time, each written only while nobody else
/// has written since the last and, when `gated`, while the turn still runs, until the screen shows
/// the box empty. Returns how it ended and how many presses went in; `writes` is moved past each.
fn clear_input_box(
    state: &AppState,
    live: &LiveSession,
    writes: &mut u64,
    gated: bool,
) -> (Cleared, usize) {
    let Some(text) = input_box(live.agent, &live.screen_text_undimmed()) else {
        return (Cleared::Left, 0);
    };
    // A press clears a line's text and the next its line break, so two a row and one to spare.
    let presses = (text.lines().count() * 2 + 1).min(MAX_PRESSES);
    let mut pressed = 0;
    for _ in 0..presses {
        let screen = live.screen_text_undimmed();
        match input_box(live.agent, &screen) {
            Some(text) if text.is_empty() => return (Cleared::Empty, pressed),
            Some(_) => {}
            None => return (Cleared::Left, pressed),
        }
        let before = input_box_rows(live.agent, &screen);
        match live.write_keys_if(*writes, || !gated || turn_running(state, live), CTRL_U) {
            Ok(true) => {
                *writes += 1;
                pressed += 1;
            }
            Ok(false) => return (Cleared::Refused, pressed),
            Err(_) => return (Cleared::Left, pressed),
        }
        // The box's own reading, not the screen: a spinner turning elsewhere changes the screen
        // before the press has shown.
        wait_for(PRESS_TIMEOUT, || {
            input_box_rows(live.agent, &live.screen_text_undimmed()) != before
        });
    }
    match input_box(live.agent, &live.screen_text_undimmed()) {
        Some(text) if text.is_empty() => (Cleared::Empty, pressed),
        _ => (Cleared::Left, pressed),
    }
}

/// Clears what the agent put back in its input box after the interrupt key.
fn clear_restored_prompt(state: &AppState, live: &LiveSession, writes: &mut u64) -> Restored {
    wait_quiet(live, QUIET_TIMEOUT);
    match clear_input_box(state, live, writes, false) {
        (Cleared::Empty, 0) => Restored::None,
        (Cleared::Empty, _) => Restored::Cleared,
        _ => Restored::Left,
    }
}

/// Whether the agent's input box holds text once the screen has settled after the interrupt key,
/// for a box that held text before it.
fn left_in_input_box(live: &LiveSession) -> Restored {
    wait_quiet(live, QUIET_TIMEOUT);
    match input_box(live.agent, &live.screen_text_undimmed()) {
        Some(text) if text.is_empty() => Restored::None,
        _ => Restored::Left,
    }
}

mod input_box;

#[cfg(test)]
mod tests;
