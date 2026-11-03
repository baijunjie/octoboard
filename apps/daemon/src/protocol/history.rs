//! A session's conversation as the daemon reads it from the agent's records: its history, whether
//! the records are present, and where a conversation to resume is held.

use serde::Serialize;

use super::agents::AgentId;

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRecordPresence {
    Present,
    Absent,
    Unknown,
}

/// Who said a message in a session's history.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HistorySpeaker {
    /// What the user typed.
    User,
    /// A message Octoboard wrote into the session: a brief, a report, shared information, a notice.
    Octoboard,
    /// The agent's final reply to the message of its turn.
    Agent,
}

/// One message of a session's history, as `read_session_history` returns it.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryMessage {
    pub speaker: HistorySpeaker,
    pub text: String,
    /// The turn it belongs to, counted from 1: a message that opens a turn and the reply to it
    /// share one.
    pub turn: u32,
    /// When the record dated it, in milliseconds since the Unix epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub at: Option<i64>,
}

/// Where a conversation is held, as `locate_conversation_to_resume` reports it.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConversationLocation {
    /// An archived session of the asking project records it: reopening that session continues it.
    ArchivedSession { session: String },
    /// A session of the asking project that is not archived records it, running or without a
    /// process: its conversation is in use already.
    LiveSession { session: String },
    /// A session outside the asking project, a console session included, records it in any state.
    OutsideProject { session: String },
    /// No session records it, and an agent's own records do, in this account (`null` for the
    /// agent's default account).
    Record {
        agent: AgentId,
        account: Option<String>,
    },
    /// No session records it and no account of an available, added agent holds it.
    NotFound,
}

#[cfg(test)]
mod tests {
    use crate::protocol::{Event, HistoryMessage, HistorySpeaker};

    /// `at` and `unavailable` are left out when absent, and the speakers go out in snake_case.
    #[test]
    fn a_session_history_serializes_without_what_is_absent() {
        let message = |speaker, at| HistoryMessage {
            speaker,
            text: "hi".to_owned(),
            turn: 1,
            at,
        };
        let event = |unavailable, messages| {
            serde_json::to_value(Event::SessionHistory {
                id: Some("request-1".to_owned()),
                session: "s-1".to_owned(),
                messages,
                unavailable,
            })
            .unwrap()
        };
        assert_eq!(
            event(
                false,
                vec![
                    message(HistorySpeaker::User, Some(5)),
                    message(HistorySpeaker::Octoboard, None),
                    message(HistorySpeaker::Agent, None),
                ]
            ),
            serde_json::json!({
                "type": "session_history",
                "id": "request-1",
                "session": "s-1",
                "messages": [
                    {"speaker": "user", "text": "hi", "turn": 1, "at": 5},
                    {"speaker": "octoboard", "text": "hi", "turn": 1},
                    {"speaker": "agent", "text": "hi", "turn": 1},
                ],
            })
        );
        assert_eq!(event(true, Vec::new())["unavailable"], true);
    }
}
