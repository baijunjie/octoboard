//! The memory tools, and the schemas only they take: one set worded for a console session, which
//! reaches its console's memory, and one for a project session, coordinator or worker, which
//! reaches its project's place's. The names are the same; the daemon resolves the memory from the
//! calling session.

use serde_json::{json, Value};

use super::schemas::object_schema;
use super::ToolDef;

fn id_schema(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn version_schema() -> Value {
    json!({
        "type": "integer",
        "minimum": 1,
        "description": "The current version of the entry `id`, as `read_memory`, `list_memories` \
                        or the last save gave it; required with `id`, and left out to create an \
                        entry.",
    })
}

fn summary_schema() -> Value {
    json!({
        "type": "string",
        "description": "One sentence summing up the content, which sessions read in the index to \
                        decide whether to read the entry; its only label, one line of at most 200 \
                        characters after trimming.",
    })
}

fn content_schema() -> Value {
    json!({
        "type": "string",
        "description": "The entry's body, in Markdown; not blank, at most 16 KiB (16,384 bytes of \
                        UTF-8).",
    })
}

pub(super) const CONSOLE_SAVE_MEMORY: ToolDef = ToolDef {
    name: "save_memory",
    description: "Save an entry of this console's memory, which every console session of this \
                  console reads, whatever its agent or account. Without `id` and `version` it \
                  creates an entry, which Octoboard gives an id. With both, it replaces the \
                  summary and content of the entry `id`; a `version` other than the entry's \
                  current one is refused, and the error carries the current entry, so that one \
                  session cannot overwrite another's change unseen: merge your change into it and \
                  save again with its version. An `id` that names no entry is refused. Before \
                  creating an entry, look in the index for one on the same subject and update \
                  that instead. `local` marks an entry that holds only on this machine, which a \
                  configuration export leaves out; a new entry is not local unless it says so, and \
                  an update that leaves `local` out keeps the entry's value. A console holds at \
                  most 200 entries. Returns the entry as saved without its content: `id`, \
                  `summary`, `local`, `version` (the new one), `created_at` and `updated_at` \
                  (milliseconds since the epoch) and `updated_by`.",
    schema: || {
        object_schema(
            json!({
                "id": id_schema(
                    "The id of the entry to update, as the index, `list_memories` or the save \
                     that created it gave it; leave it out to create an entry.",
                ),
                "version": version_schema(),
                "summary": summary_schema(),
                "content": content_schema(),
                "local": {
                    "type": "boolean",
                    "description": "Whether the entry holds only on this machine (a path, a \
                                    local tool's behaviour, this machine's accounts), which keeps \
                                    it out of a configuration export. Left out: false for a new \
                                    entry, unchanged on an update.",
                },
            }),
            &["summary", "content"],
        )
    },
};

pub(super) const CONSOLE_LIST_MEMORIES: ToolDef = ToolDef {
    name: "list_memories",
    description: "The current index of this console's memory, the most recently updated first: \
                  `memories`, each with `id`, `summary`, `local`, `updated_at` (milliseconds \
                  since the epoch), `updated_by` (as `read_memory` gives it) and `version`. It \
                  includes what other console sessions saved since this session's instruction \
                  file was written. Read an entry's content with `read_memory`.",
    schema: || object_schema(json!({}), &[]),
};

pub(super) const CONSOLE_READ_MEMORY: ToolDef = ToolDef {
    name: "read_memory",
    description: concat!(
        "One entry of this console's memory in full, by its id: `id`, `summary`, `content`, \
         `local`, `version` (the one `save_memory` takes to update it), `created_at` and \
         `updated_at` (milliseconds since the epoch), and ",
        "`updated_by`, who wrote it last: `{\"kind\": \"agent\", \"agent\": <id>}` for a session's \
         memory tools, naming the session's agent by its id, `{\"kind\": \"user\"}` for the user."
    ),
    schema: || object_schema(json!({ "id": id_schema("The entry's id.") }), &["id"]),
};

pub(super) const CONSOLE_DELETE_MEMORY: ToolDef = ToolDef {
    name: "delete_memory",
    description: "Delete one entry of this console's memory by its id, for good. Returns \
                  `deleted`, its id.",
    schema: || object_schema(json!({ "id": id_schema("The entry's id.") }), &["id"]),
};

pub(super) const PROJECT_SAVE_MEMORY: ToolDef = ToolDef {
    name: "save_memory",
    description: "Save an entry of this project's memory: facts about the project that hold only \
                  on this machine, which every session in this project reads, whatever its agent \
                  or account, and every project of another console at the same directory. Without \
                  `id` and `version` it creates an entry, which Octoboard gives an id. With both, \
                  it replaces the summary and content of the entry `id`; a `version` other than \
                  the entry's current one is refused, and the error carries the current entry, so \
                  that one session cannot overwrite another's change unseen: merge your change \
                  into it and save again with its version. An `id` that names no entry is \
                  refused. Before creating an entry, look in the index for one on the same \
                  subject and update that instead. The memory holds at most 200 entries. Returns \
                  the entry as saved without its content: `id`, `summary`, `version` (the new \
                  one), `created_at` and `updated_at` (milliseconds since the epoch) and \
                  `updated_by`.",
    schema: || {
        object_schema(
            json!({
                "id": id_schema(
                    "The id of the entry to update, as the index, `list_memories` or the save \
                     that created it gave it; leave it out to create an entry.",
                ),
                "version": version_schema(),
                "summary": summary_schema(),
                "content": content_schema(),
            }),
            &["summary", "content"],
        )
    },
};

pub(super) const PROJECT_LIST_MEMORIES: ToolDef = ToolDef {
    name: "list_memories",
    description: "The current index of this project's memory, the most recently updated first: \
                  `memories`, each with `id`, `summary`, `updated_at` (milliseconds since the \
                  epoch), `updated_by` (as `read_memory` gives it) and `version`. It includes \
                  what other sessions saved since this session was launched. Read an entry's \
                  content with `read_memory`.",
    schema: || object_schema(json!({}), &[]),
};

pub(super) const PROJECT_READ_MEMORY: ToolDef = ToolDef {
    name: "read_memory",
    description: concat!(
        "One entry of this project's memory in full, by its id: `id`, `summary`, `content`, \
         `version` (the one `save_memory` takes to update it), `created_at` and `updated_at` \
         (milliseconds since the epoch), and ",
        "`updated_by`, who wrote it last: `{\"kind\": \"agent\", \"agent\": <id>}` for a session's \
         memory tools, naming the session's agent by its id, `{\"kind\": \"user\"}` for the user."
    ),
    schema: || object_schema(json!({ "id": id_schema("The entry's id.") }), &["id"]),
};

pub(super) const PROJECT_DELETE_MEMORY: ToolDef = ToolDef {
    name: "delete_memory",
    description: "Delete one entry of this project's memory by its id, for good. Returns \
                  `deleted`, its id.",
    schema: || object_schema(json!({ "id": id_schema("The entry's id.") }), &["id"]),
};
