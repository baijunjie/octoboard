//! A session's conversation as a live feed of structured entries: what the user and Octoboard
//! said, what the agent replied, the tools it called with their inputs and results, its todo list
//! and its compactions, read from the agent's own records while a client follows the session
//! (`subscribe_session_conversation`).

use serde::Serialize;

/// The most entries a page of a conversation holds, and how many it holds when its request names
/// no number.
pub const CONVERSATION_PAGE_LIMIT: usize = 200;
pub const CONVERSATION_PAGE_DEFAULT: usize = 50;
/// The most a page's entries take serialized, in bytes: a page ends sooner than its limit when
/// they would exceed it, but holds one entry at least.
pub const CONVERSATION_PAGE_BYTES: usize = 1024 * 1024;

/// The `type`s of the requests a control connection serves itself, because they act on what the
/// connection follows (`crate::coordinator::conversation_feed`).
pub const CONVERSATION_REQUEST_TYPES: &[&str] = &[
    "subscribe_session_conversation",
    "unsubscribe_session_conversation",
    "read_session_conversation",
];

/// One entry of a session's conversation, as the feed publishes it.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConversationEntry {
    /// The entry's position in the conversation under the current `epoch`, counted from 0. An
    /// update names the entry it replaces by it.
    pub seq: u64,
    /// The turn it belongs to, counted from 1: a `user` or `octoboard` entry opens a turn, and
    /// what follows it shares its number. Entries before the first such entry are turn 0.
    pub turn: u32,
    /// When the record dated it, in milliseconds since the Unix epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub at: Option<i64>,
    #[serde(flatten)]
    #[cfg_attr(test, schemars(with = "super::schema::Flattened<ConversationBody>"))]
    pub body: ConversationBody,
}

/// An entry the agent is still producing, shown after the conversation's entries until the
/// record holds it; it has no position yet.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingEntry {
    /// The turn it belongs to, as for a [`ConversationEntry`].
    pub turn: u32,
    /// When the record dated it, in milliseconds since the Unix epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub at: Option<i64>,
    #[serde(flatten)]
    #[cfg_attr(test, schemars(with = "super::schema::Flattened<ConversationBody>"))]
    pub body: ConversationBody,
}

/// The kind of an entry and its own fields. A client ignores an entry whose `kind` it does not
/// know: later daemons of the same generation may add kinds.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConversationBody {
    /// What the user typed (a slash command as typed, `/name args`).
    User { text: String },
    /// A message Octoboard wrote into the session: a brief, a parent session's instruction, a
    /// report, shared information, a notice.
    Octoboard { text: String },
    /// Text the agent wrote: a progress note on the way or its answer.
    Assistant { text: String },
    /// A tool the agent called.
    ToolCall(ToolCallEntry),
    /// The agent's todo list as it stands after this entry, whole.
    Todos { items: Vec<TodoItem> },
    /// The agent compacted its context here, with the summary it wrote when the record keeps one.
    Compaction {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        summary: Option<String>,
    },
}

/// A tool call and, once the record says so, how it ended.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolCallEntry {
    /// The agent's own id of the call.
    pub call_id: String,
    /// The tool's name as the agent calls it.
    pub name: String,
    pub category: ToolCategory,
    /// One line saying what the call does: the command it runs, the file it reads or edits, what
    /// it searches for.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub title: Option<String>,
    /// The call's input as the agent wrote it, with each string longer than 16 KiB cut.
    #[cfg_attr(test, ts(type = "unknown"))]
    pub input: serde_json::Value,
    /// Whether a string of `input` was cut.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub input_truncated: bool,
    /// The files the call changed, each as the agent spells it: not checked, and possibly relative
    /// to the session's working directory. Empty for a call that changed none.
    pub paths: Vec<String>,
    pub status: ToolStatus,
    /// What the call gave back, its first and last 8 KiB when it is longer.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub output: Option<String>,
    /// Whether `output` was cut.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub output_truncated: bool,
    /// The exit code of a command.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub exit_code: Option<i64>,
}

/// What kind of thing a tool call does, whichever agent made it. A value a client does not know
/// reads as `other`.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    /// Runs a shell command.
    Command,
    /// Writes or edits files.
    FileEdit,
    /// Reads a file or views an image.
    FileRead,
    /// Searches file names or contents.
    Search,
    /// Fetches or searches the web.
    Web,
    /// Calls a tool of an MCP server.
    Mcp,
    /// Starts or talks to a subagent.
    Subagent,
    Other,
}

/// Where a tool call stands. A value a client does not know reads as `running`: a later daemon of
/// the same generation may add a state in which the call has not ended (waiting for approval).
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    /// Called, with no result recorded yet.
    Running,
    Completed,
    /// It ended in an error.
    Failed,
    /// It was not allowed to run: the user declined it, or a permission rule did.
    Declined,
}

/// One item of a todo list.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TodoItem {
    pub text: String,
    /// The agent's own word: `pending`, `in_progress`, `completed` or `cancelled`.
    pub status: String,
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::protocol::{Event, Request, RequestBody};

    fn call(status: ToolStatus) -> ToolCallEntry {
        ToolCallEntry {
            call_id: "c1".to_owned(),
            name: "Bash".to_owned(),
            category: ToolCategory::Command,
            title: None,
            input: json!({"command": "ls"}),
            input_truncated: false,
            paths: Vec::new(),
            status,
            output: None,
            output_truncated: false,
            exit_code: None,
        }
    }

    fn entry(body: ConversationBody) -> ConversationEntry {
        ConversationEntry {
            seq: 3,
            turn: 2,
            at: None,
            body,
        }
    }

    /// The wire shape of an entry: `kind` beside `seq` and `turn`, with what is absent left out.
    #[test]
    fn an_entry_is_flat_with_its_kind_and_leaves_out_what_is_absent() {
        assert_eq!(
            serde_json::to_value(entry(ConversationBody::User {
                text: "hi".to_owned()
            }))
            .unwrap(),
            json!({"seq": 3, "turn": 2, "kind": "user", "text": "hi"})
        );
        let mut dated = entry(ConversationBody::Octoboard {
            text: "note".to_owned(),
        });
        dated.at = Some(1_700_000_000_000);
        assert_eq!(
            serde_json::to_value(dated).unwrap(),
            json!({"seq": 3, "turn": 2, "at": 1_700_000_000_000_i64, "kind": "octoboard",
                   "text": "note"})
        );
        assert_eq!(
            serde_json::to_value(entry(ConversationBody::Compaction { summary: None })).unwrap(),
            json!({"seq": 3, "turn": 2, "kind": "compaction"})
        );
        assert_eq!(
            serde_json::to_value(entry(ConversationBody::Todos {
                items: vec![TodoItem {
                    text: "a".to_owned(),
                    status: "in_progress".to_owned()
                }]
            }))
            .unwrap(),
            json!({"seq": 3, "turn": 2, "kind": "todos",
                   "items": [{"text": "a", "status": "in_progress"}]})
        );
    }

    #[test]
    fn a_tool_call_names_every_field_the_protocol_lists_and_marks_cuts_only_when_made() {
        let running =
            serde_json::to_value(entry(ConversationBody::ToolCall(call(ToolStatus::Running))))
                .unwrap();
        assert_eq!(
            running,
            json!({"seq": 3, "turn": 2, "kind": "tool_call", "call_id": "c1", "name": "Bash",
                   "category": "command", "input": {"command": "ls"}, "paths": [],
                   "status": "running"})
        );
        let mut full = call(ToolStatus::Declined);
        full.title = Some("ls".to_owned());
        full.input_truncated = true;
        full.output = Some("out".to_owned());
        full.output_truncated = true;
        full.exit_code = Some(2);
        full.paths = vec!["/a".to_owned()];
        let value = serde_json::to_value(entry(ConversationBody::ToolCall(full))).unwrap();
        assert_eq!(value["status"], "declined");
        assert_eq!(value["title"], "ls");
        assert_eq!(value["input_truncated"], true);
        assert_eq!(value["output"], "out");
        assert_eq!(value["output_truncated"], true);
        assert_eq!(value["exit_code"], 2);
        assert_eq!(value["paths"], json!(["/a"]));
    }

    #[test]
    fn categories_and_statuses_go_out_in_the_words_the_protocol_names() {
        let categories = [
            (ToolCategory::Command, "command"),
            (ToolCategory::FileEdit, "file_edit"),
            (ToolCategory::FileRead, "file_read"),
            (ToolCategory::Search, "search"),
            (ToolCategory::Web, "web"),
            (ToolCategory::Mcp, "mcp"),
            (ToolCategory::Subagent, "subagent"),
            (ToolCategory::Other, "other"),
        ];
        for (category, word) in categories {
            assert_eq!(serde_json::to_value(category).unwrap(), word);
        }
        let statuses = [
            (ToolStatus::Running, "running"),
            (ToolStatus::Completed, "completed"),
            (ToolStatus::Failed, "failed"),
            (ToolStatus::Declined, "declined"),
        ];
        for (status, word) in statuses {
            assert_eq!(serde_json::to_value(status).unwrap(), word);
        }
    }

    /// A pending entry is the shape of an entry without a `seq`.
    #[test]
    fn a_pending_entry_has_no_seq() {
        let pending = PendingEntry {
            turn: 4,
            at: None,
            body: ConversationBody::Assistant {
                text: "typing".to_owned(),
            },
        };
        assert_eq!(
            serde_json::to_value(pending).unwrap(),
            json!({"turn": 4, "kind": "assistant", "text": "typing"})
        );
    }

    fn changed(pending: Option<Vec<PendingEntry>>) -> Value {
        serde_json::to_value(Event::SessionConversationChanged {
            session: "s".to_owned(),
            epoch: 7,
            appended: Vec::new(),
            updated: Vec::new(),
            pending,
        })
        .unwrap()
    }

    /// `pending` absent means unchanged and `[]` means cleared, and `appended` and `updated` are
    /// left out when empty.
    #[test]
    fn a_change_event_tells_unchanged_pending_from_cleared_pending() {
        assert_eq!(
            changed(None),
            json!({"type": "session_conversation_changed", "session": "s", "epoch": 7})
        );
        assert_eq!(
            changed(Some(Vec::new())),
            json!({"type": "session_conversation_changed", "session": "s", "epoch": 7,
                   "pending": []})
        );
        let value = serde_json::to_value(Event::SessionConversationChanged {
            session: "s".to_owned(),
            epoch: 7,
            appended: vec![entry(ConversationBody::Assistant {
                text: "a".to_owned(),
            })],
            updated: vec![entry(ConversationBody::ToolCall(call(
                ToolStatus::Completed,
            )))],
            pending: None,
        })
        .unwrap();
        assert_eq!(value["appended"][0]["text"], "a");
        assert_eq!(value["updated"][0]["status"], "completed");
        assert!(value.get("pending").is_none());
        // Conversation events are never broadcast, so none carries an `id`.
        assert!(value.get("id").is_none());
    }

    #[test]
    fn the_conversation_replies_and_the_replacement_carry_what_the_protocol_lists() {
        let subscribed = serde_json::to_value(Event::SessionConversationSubscribed {
            id: Some("r".to_owned()),
            session: "s".to_owned(),
            epoch: 1,
            entries: Vec::new(),
            has_earlier: true,
            pending: Vec::new(),
        })
        .unwrap();
        assert_eq!(
            subscribed,
            json!({"type": "session_conversation_subscribed", "id": "r", "session": "s",
                   "epoch": 1, "entries": [], "has_earlier": true, "pending": []})
        );
        let page = serde_json::to_value(Event::SessionConversation {
            id: Some("r".to_owned()),
            session: "s".to_owned(),
            epoch: 1,
            entries: Vec::new(),
            has_earlier: false,
        })
        .unwrap();
        assert_eq!(
            page,
            json!({"type": "session_conversation", "id": "r", "session": "s", "epoch": 1,
                   "entries": [], "has_earlier": false})
        );
        let replaced = serde_json::to_value(Event::SessionConversationReplaced {
            session: "s".to_owned(),
            epoch: 2,
            entries: Vec::new(),
            has_earlier: false,
            pending: Vec::new(),
        })
        .unwrap();
        assert_eq!(
            replaced,
            json!({"type": "session_conversation_replaced", "session": "s", "epoch": 2,
                   "entries": [], "has_earlier": false, "pending": []})
        );
    }

    fn request(text: &str) -> Request {
        serde_json::from_str(text).expect("a request")
    }

    #[test]
    fn the_three_requests_read_with_and_without_a_limit() {
        let subscribe =
            request(r#"{"type":"subscribe_session_conversation","id":"a","session":"s"}"#);
        assert_eq!(subscribe.id.as_deref(), Some("a"));
        assert!(matches!(
            subscribe.body,
            RequestBody::SubscribeSessionConversation { ref session, limit: None } if session == "s"
        ));
        assert!(matches!(
            request(r#"{"type":"subscribe_session_conversation","session":"s","limit":20}"#).body,
            RequestBody::SubscribeSessionConversation {
                limit: Some(20),
                ..
            }
        ));
        assert!(matches!(
            request(r#"{"type":"subscribe_session_conversation","session":"s","limit":null}"#).body,
            RequestBody::SubscribeSessionConversation { limit: None, .. }
        ));
        assert!(matches!(
            request(r#"{"type":"unsubscribe_session_conversation","session":"s"}"#).body,
            RequestBody::UnsubscribeSessionConversation { ref session } if session == "s"
        ));
        assert!(matches!(
            request(r#"{"type":"read_session_conversation","session":"s","epoch":4,"before":9}"#)
                .body,
            RequestBody::ReadSessionConversation {
                epoch: 4,
                before: 9,
                limit: None,
                ..
            }
        ));
        assert!(matches!(
            request(
                r#"{"type":"read_session_conversation","session":"s","epoch":4,"before":9,"limit":5}"#
            )
            .body,
            RequestBody::ReadSessionConversation { limit: Some(5), .. }
        ));
        // `epoch` and `before` are required.
        assert!(serde_json::from_str::<Request>(
            r#"{"type":"read_session_conversation","session":"s","before":9}"#
        )
        .is_err());
    }

    /// The connection routes by this list, so each type it names must be one the daemon's
    /// requests read, and each conversation request must be in it.
    #[test]
    fn the_routed_types_are_the_three_conversation_requests() {
        let bodies = [
            r#"{"type":"subscribe_session_conversation","session":"s"}"#,
            r#"{"type":"unsubscribe_session_conversation","session":"s"}"#,
            r#"{"type":"read_session_conversation","session":"s","epoch":1,"before":1}"#,
        ];
        assert_eq!(CONVERSATION_REQUEST_TYPES.len(), bodies.len());
        for (kind, body) in CONVERSATION_REQUEST_TYPES.iter().zip(bodies) {
            let value: Value = serde_json::from_str(body).unwrap();
            assert_eq!(value["type"], *kind);
            request(body);
        }
    }

    #[test]
    fn the_page_limits_are_the_protocols() {
        assert_eq!(CONVERSATION_PAGE_DEFAULT, 50);
        assert_eq!(CONVERSATION_PAGE_LIMIT, 200);
        assert_eq!(CONVERSATION_PAGE_BYTES, 1 << 20);
    }
}
