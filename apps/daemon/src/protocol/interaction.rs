//! What a session's agent is asking the user while it waits on them, as a client can show and
//! answer it without the terminal: a tool permission prompt, or a question from the agent's own
//! ask-the-user tool (`Session.interaction`, `answer_interaction`).

use serde::{Deserialize, Serialize};

use super::ToolCategory;

/// The most bytes a string of an interaction holds: a longer one is cut at a character boundary,
/// and a cut `input` says so.
pub const INTERACTION_TEXT_LIMIT: usize = 4 * 1024;
/// The most bytes an interaction takes serialized. One that would take more is not published, and
/// the session is answered in the terminal.
pub const INTERACTION_LIMIT: usize = 32 * 1024;

/// A prompt the session's agent is showing the user, recognised from its hooks and its screen
/// together, which a client can answer with `answer_interaction`.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingInteraction {
    /// This sighting of the prompt, new each time the daemon recognises one: an answer names it,
    /// so that it cannot land on the prompt after.
    pub id: String,
    /// The tool call the prompt belongs to, as the conversation feed's `tool_call.call_id` names
    /// it, where the agent says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub call_id: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(test, schemars(with = "super::schema::Flattened<InteractionPrompt>"))]
    pub prompt: InteractionPrompt,
}

/// What the prompt asks, by its `kind`. A client shows the terminal for a `kind` it does not
/// know: later daemons of the same generation may add kinds.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionPrompt {
    /// The agent asks whether it may use a tool.
    Permission(PermissionPrompt),
    /// The agent's own ask-the-user tool puts one or more questions.
    Question(QuestionPrompt),
}

/// A tool permission prompt.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionPrompt {
    /// The tool's name as the agent calls it.
    pub tool: String,
    /// What kind of tool it is, as the conversation feed classes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub category: Option<ToolCategory>,
    /// One line saying what the call would do: the command, the file, the address, the server and
    /// tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub title: Option<String>,
    /// The agent's own explanation of the call, where it gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub detail: Option<String>,
    /// The call's input as the agent wrote it, each string cut at [`INTERACTION_TEXT_LIMIT`].
    #[cfg_attr(test, ts(type = "unknown"))]
    pub input: serde_json::Value,
    /// Whether a string of `input` was cut.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub input_truncated: bool,
    /// The answers offered, in the order the agent shows them. Only those the daemon can tell the
    /// effect of are listed; the others are left to the terminal.
    pub choices: Vec<PermissionChoice>,
}

/// One answer a permission prompt offers.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionChoice {
    /// What `answer_interaction`'s `choice` names it by.
    pub id: String,
    /// Its label as the agent shows it.
    pub label: String,
    pub effect: ChoiceEffect,
    /// Whether text for the agent may go with it (`answer_interaction`'s `text`): absent where
    /// none can.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub text: Option<ChoiceText>,
}

/// What choosing an answer does. A value a client does not know reads as `other`.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceEffect {
    /// Lets this call run.
    Allow,
    /// Lets this call run and records a rule that lets calls like it run without asking.
    AllowRemember,
    /// Refuses this call.
    Deny,
    /// Refuses this call and records a rule that refuses calls like it without asking.
    DenyRemember,
    Other,
}

/// How text goes with a choice. A value a client does not know reads as `optional`.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceText {
    /// The choice may carry text for the agent, or none.
    Optional,
}

/// Questions from the agent's own ask-the-user tool, answered together.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionPrompt {
    pub questions: Vec<Question>,
    /// Whether the questions can be declined without an answer (`answer_interaction`'s
    /// `dismiss`).
    pub dismissible: bool,
}

/// One question.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub text: String,
    /// A short label the agent gives the question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub header: Option<String>,
    /// Whether more than one option may be chosen.
    pub multi_select: bool,
    pub options: Vec<QuestionOption>,
    /// Whether the user may write an answer of their own, and how it goes with the options: absent
    /// where they may not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub free_text: Option<FreeText>,
}

/// One option of a question.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub description: Option<String>,
}

/// How a written answer goes with a question's options. A client that does not know the value
/// offers no written answer.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreeText {
    /// In place of the options: an answer has either selected options or text.
    Instead,
    /// Beside them: an answer may have both.
    With,
}

/// The answer to one question of a `question` interaction, in `answer_interaction`.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionAnswer {
    /// The options chosen, by their index in the question's `options`; none when left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub selected: Vec<u32>,
    /// The user's own answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub text: Option<String>,
}
