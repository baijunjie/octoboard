//! What a session's agent asks the user while it waits on them, published for clients to answer
//! without the terminal (`Session.interaction`), and the answer pressed into the agent's terminal
//! (`answer_interaction`).
//!
//! Every agent shows its tool permission prompts and the questions of its own ask-the-user tool
//! only in its terminal, and takes the answer only there: its hooks say what the call is (the tool,
//! its input, the questions) but not which answers the prompt offers, and none of them can be
//! answered from a hook without hiding the prompt from the person at the terminal. So the two are
//! read together, the way the folder-trust confirmation is pressed (`crate::session::trust`):
//!
//! - **The calls.** Each hook event that starts a tool call is handed to the provider's
//!   [`InteractionReader::call_started`], and the calls of the current turn are kept here, newest
//!   last, each string of their input cut ([`CALLS_KEPT`]).
//! - **The prompt.** While the session is shown waiting on the user, its screen is read every
//!   [`watch::POLL`] by the provider's [`InteractionReader::read`], which says whether a prompt is
//!   up, which of the calls it belongs to, and what it offers. A prompt is published only when the
//!   provider recognised it on the screen and matched it to a call its hooks reported, and only
//!   for a kind the provider declares (`Capabilities::interactions`). Each new prompt gets a new
//!   id, which an answer has to name ([`watch`]).
//! - **The prompt leaving.** Once a published prompt is seen gone (the provider recognises the
//!   agent's input box again, twice in a row), whoever answered it, the session's hand comes down:
//!   it is put back to working, and what the agent does next moves it on
//!   (`AppState::interaction_left`).
//! - **The answer.** [`press::answer`] presses it key by key, each key chosen by the provider from
//!   the screen as it stands ([`InteractionReader::next_key`]) and written only while the screen
//!   still calls for it and nothing else has written into the session since the prompt was
//!   published, so that a person answering in the terminal meanwhile keeps it.

pub mod press;
pub mod watch;

#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde_json::Value;

use crate::platform::term::Input;
use crate::protocol::{
    InteractionPrompt, PendingInteraction, QuestionAnswer, INTERACTION_LIMIT,
    INTERACTION_TEXT_LIMIT,
};

/// How many of the current turn's tool calls are kept for a prompt to be matched to: the oldest
/// goes once there are more. An agent asks about one call at a time, and the call it asks about is
/// among the last it started.
pub const CALLS_KEPT: usize = 16;

/// A tool call a hook reported starting, which may then wait on the user.
#[derive(Debug, Clone, PartialEq)]
pub struct CallSeen {
    /// The agent's own id of the call, where the event gives one: what the conversation feed's
    /// `tool_call.call_id` names.
    pub id: Option<String>,
    /// The tool's name as the agent calls it.
    pub tool: String,
    /// The call's input, each string cut at [`INTERACTION_TEXT_LIMIT`].
    pub input: Value,
    /// Whether a string of `input` was cut.
    pub input_truncated: bool,
}

impl CallSeen {
    /// A call of `tool` with `input`, its strings cut as kept.
    pub fn new(id: Option<String>, tool: impl Into<String>, input: &Value) -> Self {
        let mut input = input.clone();
        let input_truncated = cut_strings(&mut input);
        Self {
            id,
            tool: tool.into(),
            input,
            input_truncated,
        }
    }
}

/// What a provider reads on a session's screen about a prompt.
#[derive(Debug, Clone, PartialEq)]
pub enum Reading {
    /// A prompt is up for `calls[call]`, offering `prompt`.
    Shown {
        call: usize,
        prompt: InteractionPrompt,
    },
    /// A prompt is up that cannot be published as it stands: one the provider cannot match to a
    /// call or tell the answers of, or one in the middle of being answered (a text field open).
    Up,
    /// No prompt is up: the agent's input box is back.
    Gone,
    /// Neither a prompt nor the input box is recognised: another view, or a frame half drawn.
    Unknown,
}

/// The user's answer, checked against the prompt it answers ([`check_answer`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// A permission prompt's choice, by its id, with text for the agent where the choice takes it.
    Choice { id: String, text: Option<String> },
    /// One answer per question, in order.
    Questions(Vec<QuestionAnswer>),
    /// The questions declined.
    Dismiss,
}

/// The next key a press writes, as the provider reads it off the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextKey {
    Press(Input),
    /// The screen is in a state the provider cannot go on from.
    Stuck(&'static str),
}

/// How one provider's prompts are read off its hooks and its screen, and answered with keys.
/// Reached through `crate::adapter::AgentProvider::interactions`; a provider without one publishes
/// nothing and answers nothing.
pub trait InteractionReader: Send + Sync {
    /// The tool call this hook event starts, or `None` for an event that starts none.
    fn call_started(&self, event: &str, payload: &Value) -> Option<CallSeen>;

    /// Whether [`Self::read`] and [`Self::next_key`] are given the screen with its faint cells
    /// blanked (`TerminalSnapshot::visible_text_undimmed`), for a provider that draws hints faint
    /// where its prompts' text goes; by default they get every cell (`visible_text`).
    fn faint_blanked(&self) -> bool {
        false
    }

    /// What `screen` (the visible rows, faint cells blanked as [`Self::faint_blanked`] says) shows
    /// of a prompt, matched against `calls`, the current turn's calls oldest first.
    fn read(&self, screen: &str, calls: &[CallSeen]) -> Reading;

    /// The next key that takes the prompt on `screen`, for `call`, towards `answer`; called again
    /// after each key, on the screen the key left, until the prompt is gone.
    fn next_key(&self, screen: &str, call: &CallSeen, answer: &Answer) -> NextKey;
}

/// `live`'s screen as `reader` reads it ([`InteractionReader::faint_blanked`]).
pub(crate) fn screen_of(
    live: &crate::session::LiveSession,
    reader: &dyn InteractionReader,
) -> String {
    if reader.faint_blanked() {
        live.screen_text_undimmed()
    } else {
        live.screen_text()
    }
}

/// The prompt published for a session, and what it was published from.
#[derive(Debug, Clone)]
pub(crate) struct Published {
    pub interaction: PendingInteraction,
    /// The call it belongs to.
    pub call: CallSeen,
    /// How many writes had been made into the session's input when it was published: one made
    /// since is someone answering in the terminal.
    pub writes: u64,
}

/// A live session's calls and published prompt.
#[derive(Default)]
pub struct InteractionState {
    inner: Mutex<Inner>,
    /// Whether a watch of the session's screen runs ([`watch::ensure`]).
    pub(crate) watching: AtomicBool,
    /// Whether an answer is being pressed: one at a time.
    pressing: AtomicBool,
    /// Whether the session's hand was lowered by a prompt leaving the screen
    /// ([`Self::note_lowered`]).
    lowered: AtomicBool,
}

#[derive(Default)]
struct Inner {
    calls: VecDeque<CallSeen>,
    published: Option<Published>,
}

impl InteractionState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("interaction lock poisoned")
    }

    /// Keeps a call a hook reported starting.
    pub fn call_started(&self, call: CallSeen) {
        let mut inner = self.lock();
        if inner.calls.len() == CALLS_KEPT {
            inner.calls.pop_front();
        }
        inner.calls.push_back(call);
    }

    /// Forgets the calls of the turn before a new one.
    pub fn turn_started(&self) {
        self.lock().calls.clear();
        self.clear_lowered();
    }

    /// Notes that a prompt leaving the screen put the session back to working, which no hook
    /// reported: a decline the agent reports nowhere is then still to be found in its record.
    pub fn note_lowered(&self) {
        self.lowered.store(true, Ordering::Release);
    }

    /// Whether the session was put back to working by a prompt leaving, since the last
    /// [`Self::clear_lowered`] or turn.
    pub fn lowered(&self) -> bool {
        self.lowered.load(Ordering::Acquire)
    }

    /// Forgets [`Self::note_lowered`]: a hook put the session at a prompt again.
    pub fn clear_lowered(&self) {
        self.lowered.store(false, Ordering::Release);
    }

    pub(crate) fn calls(&self) -> Vec<CallSeen> {
        self.lock().calls.iter().cloned().collect()
    }

    /// The prompt published for the session, as a client receives it.
    pub fn interaction(&self) -> Option<PendingInteraction> {
        self.lock()
            .published
            .as_ref()
            .map(|published| published.interaction.clone())
    }

    pub(crate) fn published(&self) -> Option<Published> {
        self.lock().published.clone()
    }

    /// Replaces what is published, and says whether that changed what a client is shown.
    pub(crate) fn publish(&self, published: Option<Published>) -> bool {
        let mut inner = self.lock();
        let changed = inner.published.as_ref().map(|p| &p.interaction)
            != published.as_ref().map(|p| &p.interaction);
        inner.published = published;
        changed
    }

    /// Withdraws the prompt published as `id`, if it still is, and says whether it was.
    pub(crate) fn withdraw(&self, id: &str) -> bool {
        let mut inner = self.lock();
        if inner
            .published
            .as_ref()
            .is_some_and(|published| published.interaction.id == id)
        {
            inner.published = None;
            true
        } else {
            false
        }
    }

    /// Claims the press of an answer, or says one is already being pressed.
    fn claim_press(&self) -> Option<PressClaim<'_>> {
        (!self.pressing.swap(true, Ordering::AcqRel)).then_some(PressClaim(self))
    }

    pub(crate) fn pressing(&self) -> bool {
        self.pressing.load(Ordering::Acquire)
    }
}

/// The claim on a session's press, released when dropped.
struct PressClaim<'a>(&'a InteractionState);

impl Drop for PressClaim<'_> {
    fn drop(&mut self) {
        self.0.pressing.store(false, Ordering::Release);
    }
}

/// Cuts every string in `value` to [`INTERACTION_TEXT_LIMIT`] bytes, and says whether any was.
pub(crate) fn cut_strings(value: &mut Value) -> bool {
    match value {
        Value::String(text) => cut(text),
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |cut, item| cut_strings(item) | cut),
        Value::Object(map) => map
            .values_mut()
            .fold(false, |cut, item| cut_strings(item) | cut),
        _ => false,
    }
}

/// Cuts `text` to [`INTERACTION_TEXT_LIMIT`] bytes at a character boundary, and says whether it
/// was.
pub(crate) fn cut(text: &mut String) -> bool {
    if text.len() <= INTERACTION_TEXT_LIMIT {
        return false;
    }
    let mut end = INTERACTION_TEXT_LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    true
}

/// `prompt` with each of its strings cut ([`cut`]), or `None` when it would still take more than
/// [`INTERACTION_LIMIT`] serialized.
pub(crate) fn within_limits(mut interaction: PendingInteraction) -> Option<PendingInteraction> {
    let text = |text: &mut String| {
        cut(text);
    };
    let option = |text: &mut Option<String>| {
        if let Some(text) = text {
            cut(text);
        }
    };
    match &mut interaction.prompt {
        InteractionPrompt::Permission(permission) => {
            text(&mut permission.tool);
            option(&mut permission.title);
            option(&mut permission.detail);
            permission.input_truncated |= cut_strings(&mut permission.input);
            for choice in &mut permission.choices {
                text(&mut choice.label);
            }
        }
        InteractionPrompt::Question(question) => {
            for question in &mut question.questions {
                text(&mut question.text);
                option(&mut question.header);
                for choice in &mut question.options {
                    text(&mut choice.label);
                    option(&mut choice.description);
                }
            }
        }
    }
    let size = serde_json::to_vec(&interaction).map_or(usize::MAX, |bytes| bytes.len());
    (size <= INTERACTION_LIMIT).then_some(interaction)
}

/// Why an answer does not fit the prompt it answers: the field, and whether it was missing
/// rather than wrong.
#[derive(Debug, PartialEq, Eq)]
pub struct Misfit {
    pub field: &'static str,
    pub missing: bool,
    pub reason: &'static str,
}

fn missing(field: &'static str, reason: &'static str) -> Misfit {
    Misfit {
        field,
        missing: true,
        reason,
    }
}

fn wrong(field: &'static str, reason: &'static str) -> Misfit {
    Misfit {
        field,
        missing: false,
        reason,
    }
}

/// The raw fields of an `answer_interaction`, for [`check_answer`].
pub struct AnswerFields {
    pub choice: Option<String>,
    pub text: Option<String>,
    pub answers: Option<Vec<QuestionAnswer>>,
    pub dismiss: bool,
}

/// The answer `fields` give to `prompt`, or why they do not fit it. Text is taken as the agent
/// would read it, control and invisible characters stripped, and text left blank counts as none.
pub fn check_answer(prompt: &InteractionPrompt, fields: AnswerFields) -> Result<Answer, Misfit> {
    let clean = |text: Option<String>| {
        text.map(|text| crate::session::term::stripped(&text).trim().to_string())
            .filter(|text| !text.is_empty())
    };
    match prompt {
        InteractionPrompt::Permission(permission) => {
            if fields.answers.is_some() {
                return Err(wrong("answers", "a permission prompt takes a choice"));
            }
            if fields.dismiss {
                return Err(wrong("dismiss", "a permission prompt takes a choice"));
            }
            let Some(id) = fields.choice else {
                return Err(missing("choice", "a permission prompt takes a choice"));
            };
            let Some(choice) = permission.choices.iter().find(|choice| choice.id == id) else {
                return Err(wrong("choice", "the prompt offers no such choice"));
            };
            let text = clean(fields.text);
            if text.is_some() && choice.text.is_none() {
                return Err(wrong("text", "this choice takes no text"));
            }
            Ok(Answer::Choice { id, text })
        }
        InteractionPrompt::Question(prompt) => {
            if fields.choice.is_some() {
                return Err(wrong("choice", "questions take answers, not a choice"));
            }
            if fields.text.is_some() {
                return Err(wrong("text", "questions take their text in their answers"));
            }
            if fields.dismiss {
                if fields.answers.is_some() {
                    return Err(wrong("answers", "a dismissal takes no answers"));
                }
                if !prompt.dismissible {
                    return Err(wrong("dismiss", "these questions cannot be dismissed"));
                }
                return Ok(Answer::Dismiss);
            }
            let Some(answers) = fields.answers else {
                return Err(missing("answers", "questions take an answer each"));
            };
            if answers.len() != prompt.questions.len() {
                return Err(wrong("answers", "there must be one answer per question"));
            }
            let mut checked = Vec::with_capacity(answers.len());
            for (question, answer) in prompt.questions.iter().zip(answers) {
                let mut selected = answer.selected;
                let distinct = selected.len();
                selected.sort_unstable();
                selected.dedup();
                if selected.len() != distinct {
                    return Err(wrong("answers", "an option is selected twice"));
                }
                if selected
                    .iter()
                    .any(|index| *index as usize >= question.options.len())
                {
                    return Err(wrong("answers", "the question has no such option"));
                }
                if !question.multi_select && selected.len() > 1 {
                    return Err(wrong("answers", "the question takes one option"));
                }
                let text = clean(answer.text);
                match (question.free_text, text.is_some()) {
                    (None, true) => {
                        return Err(wrong("answers", "the question takes no written answer"));
                    }
                    (Some(crate::protocol::FreeText::Instead), true) if !selected.is_empty() => {
                        return Err(wrong(
                            "answers",
                            "the question takes options or a written answer, not both",
                        ));
                    }
                    _ => {}
                }
                if selected.is_empty() && text.is_none() {
                    return Err(missing("answers", "every question needs an answer"));
                }
                checked.push(QuestionAnswer { selected, text });
            }
            Ok(Answer::Questions(checked))
        }
    }
}
