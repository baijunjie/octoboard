//! JSON message shapes for `/ws/control`, exactly as specced in `prototype/PROTOCOL.md`.
//!
//! Kept as two plain enums (client->daemon, daemon->client) tagged on `type`, which is the
//! simplest serde mapping for the protocol's "one object per frame" wire format.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Hub,
    Worker,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Agent {
    Claude,
    Grok,
    Codex,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Working,
    WaitingUser,
    Idle,
    Exited,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusSource {
    Hook,
    Process,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientToControl {
    StartSession {
        agent: Agent,
        cwd: String,
        role: Role,
        task: Option<String>,
    },
    SendMessage {
        session: Uuid,
        text: String,
    },
    KillSession {
        session: Uuid,
    },
    ListSessions,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlToClient {
    SessionStarted {
        session: Uuid,
        agent: Agent,
        pid: u32,
        agent_session_id: Option<String>,
    },
    Status {
        session: Uuid,
        state: SessionState,
        source: StatusSource,
        raw: serde_json::Value,
    },
    ToolCall {
        session: Uuid,
        tool: String,
        args: serde_json::Value,
    },
    MessageQueued {
        session: Uuid,
        reason: String,
    },
    Error {
        message: String,
    },
}

/// Client-sent control frame on `/ws/term/:session` (text frames only; binary frames on that
/// socket are raw PTY input and never reach this type).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TermControl {
    Resize { cols: u16, rows: u16 },
}
