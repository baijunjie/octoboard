//! The memory tools of the Octoboard MCP server: a session saves, lists, reads and deletes the
//! entries of the memory its kind belongs to, each named by the id the daemon gave it, through
//! `coordinator::memory`, which tells every client of each change. The memory is resolved from the
//! calling session, never from an argument: a console session's console, or a project session's
//! place (its project's host and stored path). A refusal is a tool error worded here for the
//! model, carrying the current entry where the refusal is about one, so the session can merge
//! without another call.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::coordinator::memory::{self, MemoryRefusal};
use crate::mcp::lookup::{optional_bool, optional_string, required_str};
use crate::protocol::{
    MemoryDraft, MemoryEntry, MemoryScope, MemoryTarget, MemoryWriter, Session, SessionKind,
    MEMORY_MAX_CONTENT_BYTES, MEMORY_MAX_ENTRIES, MEMORY_MAX_SUMMARY_CHARS,
};
use crate::state::AppState;

/// The memory `session` reaches: its console's for a console session, its project's place's for a
/// project session.
fn scope_of(state: &Arc<AppState>, session: &Session) -> Result<MemoryScope> {
    match session.kind {
        SessionKind::Console => Ok(MemoryScope::Console {
            console_id: session.console_id.clone(),
        }),
        SessionKind::Project => {
            let project = session
                .project_id
                .as_deref()
                .ok_or_else(|| anyhow!("this project session names no project"))?;
            Ok(memory::place_of(&state.store.require_project(project)?))
        }
    }
}

/// How the tools name the memory to the model.
fn memory_name(scope: &MemoryScope) -> &'static str {
    match scope {
        MemoryScope::Console { .. } => "this console's memory",
        MemoryScope::Place { .. } => "this project's memory",
    }
}

pub(in crate::mcp) fn save_memory(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let scope = scope_of(state, session)?;
    // Only a console's entry is marked; a project session is not offered `local`, and one sent
    // anyway is ignored, every entry of a place holding only on this machine.
    let local = match scope {
        MemoryScope::Console { .. } => optional_bool(arguments, "local")?,
        MemoryScope::Place { .. } => None,
    };
    let draft = MemoryDraft {
        summary: required_str(arguments, "summary")?.to_string(),
        content: required_raw(arguments, "content")?.to_string(),
        local,
    };
    let version = match arguments.get("version") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_i64()
                .filter(|version| *version >= 1)
                .ok_or_else(|| anyhow!("`version` must be a whole number, 1 or more"))?,
        ),
    };
    let target = match (optional_string(arguments, "id"), version) {
        (None, None) => MemoryTarget::Create,
        (Some(id), Some(version)) => MemoryTarget::Update { id, version },
        (Some(id), None) => bail!(
            "the memory was not saved: an update of the entry `{id}` needs its current \
             `version`, which `read_memory` and `list_memories` give"
        ),
        (None, Some(_)) => bail!(
            "the memory was not saved: `version` goes with the `id` of the entry to update; \
             leave both out to create an entry"
        ),
    };
    let writer = MemoryWriter::Agent;
    let id = match &target {
        MemoryTarget::Update { id, .. } => id.clone(),
        MemoryTarget::Create => String::new(),
    };
    let saved = memory::save_and_announce(state, &scope, &target, draft, &writer)
        .map_err(|err| {
            // The store refuses a save to a console deleted since the session was resolved.
            if crate::protocol::error_parts(&err).0 == crate::protocol::error_code::UNKNOWN_CONSOLE
            {
                anyhow!("the memory was not saved: this console has been deleted")
            } else {
                err
            }
        })?
        .map_err(|refusal| refused(&scope, &id, refusal))?;
    let mut answer = describe(&saved);
    answer
        .as_object_mut()
        .expect("an entry is an object")
        .remove("content");
    Ok(answer)
}

pub(in crate::mcp) fn list_memories(
    state: &Arc<AppState>,
    session: &Session,
    _arguments: &Map<String, Value>,
) -> Result<Value> {
    let scope = scope_of(state, session)?;
    let index = state.store.memory_index(&scope)?;
    let console = matches!(scope, MemoryScope::Console { .. });
    let memories: Vec<Value> = index
        .iter()
        .map(|entry| {
            let mut line = json!({
                "id": entry.id,
                "summary": entry.summary,
                "updated_at": entry.updated_at,
                "updated_by": entry.updated_by,
                "version": entry.version,
            });
            if console {
                line["local"] = json!(entry.local);
            }
            line
        })
        .collect();
    Ok(json!({ "memories": memories }))
}

pub(in crate::mcp) fn read_memory(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let scope = scope_of(state, session)?;
    let id = required_str(arguments, "id")?;
    let entry = memory::read_memory(&state.store, &scope, id)?
        .map_err(|refusal| refused(&scope, id, refusal))?;
    Ok(describe(&entry))
}

pub(in crate::mcp) fn delete_memory(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let scope = scope_of(state, session)?;
    let id = required_str(arguments, "id")?;
    memory::delete_and_announce(state, &scope, id)?
        .map_err(|refusal| refused(&scope, id, refusal))?;
    Ok(json!({ "deleted": id }))
}

/// A string argument as written, refused when absent or not a string; blank is left to the rules.
fn required_raw<'a>(arguments: &'a Map<String, Value>, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("`{key}` is required"))
}

/// An entry as the tools describe it: a console's with its `local`, a place's without, since
/// every entry there holds only on this machine.
fn describe(entry: &MemoryEntry) -> Value {
    let mut described = json!({
        "id": entry.id,
        "summary": entry.summary,
        "content": entry.content,
        "version": entry.version,
        "created_at": entry.created_at,
        "updated_at": entry.updated_at,
        "updated_by": entry.updated_by,
    });
    if matches!(entry.scope, MemoryScope::Console { .. }) {
        described["local"] = json!(entry.local);
    }
    described
}

/// The tool error for `refusal` of an operation on the entry `id` (blank for a create) of
/// `scope`'s memory.
fn refused(scope: &MemoryScope, id: &str, refusal: MemoryRefusal) -> anyhow::Error {
    let id = id.trim();
    let memory = memory_name(scope);
    match refusal {
        MemoryRefusal::InvalidSummary => anyhow!(
            "the memory was not saved: `summary` must be one line of 1 to \
             {MEMORY_MAX_SUMMARY_CHARS} characters after trimming"
        ),
        MemoryRefusal::EmptyContent => anyhow!("the memory was not saved: `content` is blank"),
        MemoryRefusal::ContentTooLong(bytes) => anyhow!(
            "the memory was not saved: `content` is {bytes} bytes, and an entry holds at most \
             {MEMORY_MAX_CONTENT_BYTES}; shorten it, or split it into entries"
        ),
        MemoryRefusal::StaleVersion(entry) => {
            let current = serde_json::to_string_pretty(&describe(&entry)).unwrap_or_default();
            anyhow!(
                "the memory was not saved: the entry `{id}` is at version {} now, changed since \
                 the version you named. Merge your change into it and save again with `id` \
                 `{id}` and `version` {}.\n\nThe current entry:\n{current}",
                entry.version,
                entry.version
            )
        }
        MemoryRefusal::Unknown => anyhow!(
            "{memory} has no entry `{id}`; it may have been deleted. `list_memories` lists the \
             entries by id, and `save_memory` without `id` and `version` creates one"
        ),
        MemoryRefusal::Full => anyhow!(
            "the memory was not saved: {memory} holds {MEMORY_MAX_ENTRIES} entries, the most it \
             may. Update an entry on the same subject by its `id`, or delete one no longer \
             needed, instead"
        ),
    }
}

#[cfg(test)]
mod tests;
