//! The session's composer queue: the user's messages waiting to be sent into a session, as every
//! client sees them. See "The composer queue" in `apps/daemon/PROTOCOL.md`.

use serde::{Deserialize, Serialize};

use super::errors::Params;

/// How a new message of the session's composer is sent: the session's default (`Session.send_mode`)
/// and the mode `create_queued_message` takes.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendMode {
    /// Handed to the session's message delivery at once.
    #[default]
    Now,
    /// Sent by the daemon once the session is idle, one message per idle, in queue order.
    WhenIdle,
    /// Never sent automatically: it waits in the queue until the user sends it.
    Manual,
}

/// The mode a queued message waits in. Sending now is an action (`send_queued_message`), not a
/// stored mode.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuedMode {
    WhenIdle,
    Manual,
}

/// Where a queued message stands.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuedMessageState {
    /// In the queue, waiting by its mode.
    Waiting,
    /// Handed to the session's message delivery and not yet written, or waiting for the session a
    /// send now resumed to be able to take it. It cannot be edited. It can be withdrawn while it
    /// waits on the resume or delivery holds it queued, not while it is being handed over or
    /// written.
    Sending,
}

/// Why a queued message is back in the queue.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuedMessageFailureKind {
    /// The session's delivery wrote the message and reported it as not carried out: the message
    /// is back in its place and switched to `manual`, since the agent may have taken it after all.
    NotDelivered,
    /// Sending it was refused, nothing of it sent. Refused before delivery held it (a send now
    /// refused at the hand-over), it keeps its mode; refused once delivery held it, or a message
    /// the daemon sent at an idle refused for good, it is switched to `manual`.
    Refused,
}

/// What became of the last attempt to send a queued message, until it is edited or sent again.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueuedMessageFailure {
    pub kind: QueuedMessageFailureKind,
    /// The code of the delivery's notice (`not_delivered`) or of the refusal (`refused`), read as
    /// the `session_notice` or `error` of that code is.
    pub code: String,
    pub params: Params,
    /// The daemon's English text, for the log and an alert's details.
    pub message: String,
}

/// One of the user's messages waiting in a session's composer queue.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueuedMessage {
    pub id: String,
    /// The message's text; empty only for a message of images.
    pub text: String,
    /// The images to attach, each an absolute path in the session's composer directory, for a
    /// provider that declares `message_images`.
    pub images: Vec<String>,
    /// Files given to the agent as paths, written after the text when the message is sent.
    pub files: Vec<String>,
    /// Whether `text` is a slash command for the session's agent to run as one of its own,
    /// sent as `send_command` sends one.
    pub command: bool,
    pub mode: QueuedMode,
    pub state: QueuedMessageState,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub failure: Option<QueuedMessageFailure>,
    pub created_at: i64,
}

/// A session's composer queue, in queue order.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionMessageQueue {
    pub session: String,
    pub messages: Vec<QueuedMessage>,
}
