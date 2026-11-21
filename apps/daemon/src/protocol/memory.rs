//! Octoboard's memory: the entries sessions keep across sessions, whatever their agent or account.
//! There are two kinds of memory, told apart by a [`MemoryScope`]: a console's, which its console
//! sessions read and write, and a place's, which the project sessions of every project at that
//! place (a host and a directory) read and write. The daemon stores them (`Store::save_memory`),
//! sessions reach the memory their kind belongs to with the memory tools, and the index of it is
//! given to a session at launch: a console session's in its instruction file, a project session's
//! in its role description. The rules an entry is held to are in `coordinator::memory`. Clients
//! list and change entries with the requests of the same names, and hear of every change from
//! `memory_saved` and `memories_deleted`.

use serde::{Deserialize, Serialize};

use super::AgentId;

/// The most entries one memory holds.
pub const MEMORY_MAX_ENTRIES: usize = 200;
/// The most bytes of UTF-8 one entry's content holds.
pub const MEMORY_MAX_CONTENT_BYTES: usize = 16 * 1024;
/// The length of an entry's id: lowercase ASCII letters and digits, assigned by the daemon when
/// the entry is created, unique within its memory and never given to another entry of it.
pub const MEMORY_ID_LENGTH: usize = 8;
/// The most characters (Unicode scalar values) of an entry's summary, after trimming.
pub const MEMORY_MAX_SUMMARY_CHARS: usize = 200;

/// Which memory an entry belongs to.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryScope {
    /// A console's memory, which its console sessions keep; deleted with the console.
    Console { console_id: String },
    /// A place's memory, which the project sessions of every project at the place keep: the
    /// project's host and its stored path. It outlives the projects, and is deleted only when the
    /// user asks.
    Place { host_id: String, path: String },
}

/// Who wrote an entry last: the user, in a window, or an agent, through a session's memory tools.
/// An agent's writer names the agent of the session that saved it, its id as data; `agent` is left
/// out only for an entry whose agent no registered provider has any more.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryWriter {
    User,
    Agent {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        agent: Option<AgentId>,
    },
}

impl MemoryWriter {
    /// The writer of a save made through the memory tools of a session of `agent`.
    pub fn agent(agent: AgentId) -> MemoryWriter {
        MemoryWriter::Agent { agent: Some(agent) }
    }
}

/// The fields of an entry its writer gives; the rest the store keeps. `local` says whether a
/// console's entry holds only on this machine; `None` is false for a new entry and keeps the
/// entry's value on an update. A place's entries are never marked, every one holding only on this
/// machine, and `local` is ignored for them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryDraft {
    pub summary: String,
    pub content: String,
    pub local: Option<bool>,
}

/// What a save does: create an entry, which the daemon gives an id, or update the entry `id`
/// based on its version `version`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemoryTarget {
    Create,
    Update { id: String, version: i64 },
}

/// One entry of a memory as stored.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub scope: MemoryScope,
    /// The entry's id, assigned by the daemon (see [`MEMORY_ID_LENGTH`]).
    pub id: String,
    /// One sentence summing up the content, the entry's only label; not unique.
    pub summary: String,
    /// Markdown.
    pub content: String,
    /// Whether the entry holds only on this machine; always false in a place's memory, every entry
    /// of which holds only here. A console's local entries stay out of a configuration export.
    pub local: bool,
    /// Milliseconds since the epoch.
    pub created_at: i64,
    /// Milliseconds since the epoch.
    pub updated_at: i64,
    pub updated_by: MemoryWriter,
    /// Starts at 1 and is raised by every update.
    pub version: i64,
}

/// One line of a memory's index: an entry without its content.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryIndexEntry {
    pub id: String,
    pub summary: String,
    /// As in [`MemoryEntry::local`].
    pub local: bool,
    /// Milliseconds since the epoch.
    pub created_at: i64,
    /// Milliseconds since the epoch.
    pub updated_at: i64,
    pub updated_by: MemoryWriter,
    /// The entry's current version, as in [`MemoryEntry::version`].
    pub version: i64,
}

/// A place whose memory no project is at any more: what removing its projects left behind.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrphanedMemory {
    pub host_id: String,
    /// The path the projects at the place were stored with.
    pub path: String,
    /// How many entries the memory holds, at least one.
    pub entries: usize,
    /// When its most recently updated entry was, in milliseconds since the epoch.
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{Event, Request, RequestBody};

    fn console() -> MemoryScope {
        MemoryScope::Console {
            console_id: "c-1".to_string(),
        }
    }

    fn place() -> MemoryScope {
        MemoryScope::Place {
            host_id: "local".to_string(),
            path: "/repo".to_string(),
        }
    }

    fn entry(scope: MemoryScope) -> MemoryEntry {
        MemoryEntry {
            scope,
            id: "a1b2c3d4".to_string(),
            summary: "Web goes to site".to_string(),
            content: "text".to_string(),
            local: true,
            created_at: 1,
            updated_at: 2,
            updated_by: MemoryWriter::agent(AgentId::CODEX),
            version: 3,
        }
    }

    /// The limits the entry rules are held to are the specified ones.
    #[test]
    fn the_limits_are_the_specified_ones() {
        assert_eq!(MEMORY_MAX_ENTRIES, 200);
        assert_eq!(MEMORY_MAX_CONTENT_BYTES, 16_384);
        assert_eq!(MEMORY_ID_LENGTH, 8);
        assert_eq!(MEMORY_MAX_SUMMARY_CHARS, 200);
    }

    /// An entry goes over the wire with its scope tagged by kind, a console's by its id and a
    /// place's by host and path, its `local`, and its writer by kind, an agent's with its agent's id
    /// (left out only where no registered provider has it).
    #[test]
    fn an_entry_is_serialized_as_clients_read_it() {
        assert_eq!(
            serde_json::to_value(entry(console())).unwrap(),
            json!({
                "scope": { "kind": "console", "console_id": "c-1" },
                "id": "a1b2c3d4",
                "summary": "Web goes to site",
                "content": "text",
                "local": true,
                "created_at": 1,
                "updated_at": 2,
                "updated_by": { "kind": "agent", "agent": "codex" },
                "version": 3,
            })
        );
        assert_eq!(
            serde_json::to_value(place()).unwrap(),
            json!({ "kind": "place", "host_id": "local", "path": "/repo" })
        );
        assert_eq!(
            serde_json::to_value(MemoryWriter::User).unwrap(),
            json!({ "kind": "user" })
        );
        assert_eq!(
            serde_json::from_value::<MemoryWriter>(json!({ "kind": "agent", "agent": "grok" }))
                .unwrap(),
            MemoryWriter::agent(AgentId::GROK)
        );
        assert_eq!(
            serde_json::to_value(MemoryWriter::Agent { agent: None }).unwrap(),
            json!({ "kind": "agent" })
        );
    }

    /// The broadcasts and replies are named as clients expect and carry the memory's scope; the
    /// index line carries no content and no scope.
    #[test]
    fn the_events_are_named_as_clients_expect() {
        let saved = serde_json::to_value(Event::MemorySaved {
            entry: entry(place()),
        })
        .unwrap();
        assert_eq!(saved["type"], "memory_saved");
        assert_eq!(saved["entry"]["scope"]["kind"], "place");
        assert_eq!(
            serde_json::to_value(Event::MemoriesDeleted {
                scope: console(),
                memories: vec!["a".to_string(), "b".to_string()],
            })
            .unwrap(),
            json!({
                "type": "memories_deleted",
                "scope": { "kind": "console", "console_id": "c-1" },
                "memories": ["a", "b"],
            })
        );
        let line = MemoryIndexEntry {
            id: "a".to_string(),
            summary: "d".to_string(),
            local: false,
            created_at: 1,
            updated_at: 2,
            updated_by: MemoryWriter::User,
            version: 1,
        };
        let list = serde_json::to_value(Event::MemoryList {
            id: None,
            scope: place(),
            memories: vec![line],
        })
        .unwrap();
        assert_eq!(list["type"], "memory_list");
        assert_eq!(list["scope"]["path"], "/repo");
        let line = &list["memories"][0];
        assert!(line.get("content").is_none() && line.get("scope").is_none());
        assert_eq!(line["local"], false);
        assert_eq!(
            serde_json::to_value(Event::OrphanedMemoryList {
                id: Some("r".to_string()),
                places: vec![OrphanedMemory {
                    host_id: "local".to_string(),
                    path: "/repo".to_string(),
                    entries: 2,
                    updated_at: 5,
                }],
            })
            .unwrap(),
            json!({
                "type": "orphaned_memory_list",
                "id": "r",
                "places": [{ "host_id": "local", "path": "/repo", "entries": 2, "updated_at": 5 }],
            })
        );
    }

    /// The requests are read from the names and fields clients send: a memory by `console` or
    /// `project`, an entry's id as `memory` (a request's own `id` is its correlation), and `local`
    /// and `version` optional.
    #[test]
    fn the_requests_are_read_as_clients_send_them() {
        let body = |json: &str| serde_json::from_str::<Request>(json).unwrap().body;
        match body(r#"{"type":"save_memory","project":"p","summary":"d","content":"x"}"#) {
            RequestBody::SaveMemory {
                console,
                project,
                memory,
                local,
                version,
                ..
            } => {
                assert_eq!((console, project.as_deref()), (None, Some("p")));
                assert_eq!((memory, local, version), (None, None, None));
            }
            other => panic!("{other:?}"),
        }
        match body(
            r#"{"type":"save_memory","console":"c","memory":"a1b2c3d4","summary":"d","content":"x","local":true,"version":4}"#,
        ) {
            RequestBody::SaveMemory {
                console,
                memory,
                local,
                version,
                ..
            } => assert_eq!(
                (console.as_deref(), memory.as_deref(), local, version),
                (Some("c"), Some("a1b2c3d4"), Some(true), Some(4))
            ),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            body(r#"{"type":"list_memories","project":"p"}"#),
            RequestBody::ListMemories { .. }
        ));
        assert!(matches!(
            body(r#"{"type":"read_memory","console":"c","memory":"a1b2c3d4"}"#),
            RequestBody::ReadMemory { .. }
        ));
        assert!(matches!(
            body(r#"{"type":"delete_memory","project":"p","memory":"a1b2c3d4"}"#),
            RequestBody::DeleteMemory { .. }
        ));
        assert!(matches!(
            body(r#"{"type":"delete_memories","console":"c"}"#),
            RequestBody::DeleteMemories { .. }
        ));
        assert!(matches!(
            body(r#"{"type":"list_orphaned_memories"}"#),
            RequestBody::ListOrphanedMemories
        ));
        assert!(matches!(
            body(r#"{"type":"delete_orphaned_memory","host_id":"local","path":"/repo"}"#),
            RequestBody::DeleteOrphanedMemory { .. }
        ));
        assert!(matches!(
            body(r#"{"type":"delete_orphaned_memories"}"#),
            RequestBody::DeleteOrphanedMemories
        ));
        assert!(matches!(
            body(r#"{"type":"delete_project","project":"p","delete_memory":true}"#),
            RequestBody::DeleteProject {
                delete_memory: true,
                ..
            }
        ));
        assert!(matches!(
            body(r#"{"type":"delete_project","project":"p"}"#),
            RequestBody::DeleteProject {
                delete_memory: false,
                ..
            }
        ));
    }
}
