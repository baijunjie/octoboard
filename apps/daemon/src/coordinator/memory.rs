//! Octoboard's memory: the rules an entry is held to, and its operations, for every caller (the
//! sessions' memory tools, the clients' requests of the same names, a project's removal and the
//! configuration import). A memory is a console's or a place's ([`MemoryScope`]); the rules are
//! the same for both. A refusal is a [`MemoryRefusal`] rather than an error of its own wording, so
//! that each caller words it for whom it answers: the tools in prose for the model, the requests
//! as coded failures.
//!
//! An entry is named by the id the daemon gave it when it was created; its summary is only a
//! label. A write that changes a memory goes through the `*_and_announce` functions, which tell
//! every client (`memory_saved`, `memories_deleted`), whoever made the change.

use std::sync::{Arc, Mutex};

use anyhow::Result;

use crate::protocol::{
    error_code, CodedError, Event, MemoryDraft, MemoryEntry, MemoryScope, MemoryTarget,
    MemoryWriter, Project, MEMORY_MAX_CONTENT_BYTES, MEMORY_MAX_ENTRIES, MEMORY_MAX_SUMMARY_CHARS,
};
use crate::state::AppState;
use crate::store::{MemorySave, Store};
use crate::time::now_millis;

/// Held across a memory write and its announcement, so that clients hear of the writes in the
/// order they were made.
static MEMORY_WRITES: Mutex<()> = Mutex::new(());

fn lock_memory_writes() -> std::sync::MutexGuard<'static, ()> {
    MEMORY_WRITES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Why a memory operation did not go through.
#[derive(Debug, PartialEq, Eq)]
pub enum MemoryRefusal {
    /// The summary is blank, too long, or more than one line.
    InvalidSummary,
    /// The content is blank.
    EmptyContent,
    /// The content is longer than [`MEMORY_MAX_CONTENT_BYTES`]; it has this many bytes.
    ContentTooLong(usize),
    /// An update named a version other than the entry's current one, as it stands.
    StaleVersion(MemoryEntry),
    /// No entry has that id.
    Unknown,
    /// The memory holds [`MEMORY_MAX_ENTRIES`] already.
    Full,
}

/// The memory of `project`'s place: its host and stored path, which every project there shares,
/// whatever its console, and which outlives the project.
pub fn place_of(project: &Project) -> MemoryScope {
    MemoryScope::Place {
        host_id: project.host_id.clone(),
        path: project.path.clone(),
    }
}

/// A project other than `except` at the place `host_id` and `path`, if there is one.
pub(super) fn project_at_place(
    store: &Store,
    host_id: &str,
    path: &str,
    except: Option<&str>,
) -> Result<Option<Project>> {
    Ok(store.list_projects()?.into_iter().find(|project| {
        project.host_id == host_id
            && project.path == path
            && except.is_none_or(|except| project.id != except)
    }))
}

/// `draft` as it is stored: the summary trimmed, the content as written. Refused where it breaks
/// a rule of the entry.
// The refusal carrying the current entry is the large variant, built once per refused save; boxing
// it would buy nothing on a path this cold.
#[allow(clippy::result_large_err)]
pub fn checked_draft(draft: MemoryDraft) -> Result<MemoryDraft, MemoryRefusal> {
    let summary = draft.summary.trim();
    if summary.is_empty()
        || summary.chars().count() > MEMORY_MAX_SUMMARY_CHARS
        || summary.chars().any(char::is_control)
    {
        return Err(MemoryRefusal::InvalidSummary);
    }
    if draft.content.trim().is_empty() {
        return Err(MemoryRefusal::EmptyContent);
    }
    if draft.content.len() > MEMORY_MAX_CONTENT_BYTES {
        return Err(MemoryRefusal::ContentTooLong(draft.content.len()));
    }
    Ok(MemoryDraft {
        summary: summary.to_string(),
        ..draft
    })
}

/// Creates an entry of `scope`'s memory from `draft`, with a new id, or updates the entry
/// `target` names when the version it names is the entry's current one, recording `writer` and
/// now.
pub fn save_memory(
    store: &Store,
    scope: &MemoryScope,
    target: &MemoryTarget,
    draft: MemoryDraft,
    writer: &MemoryWriter,
) -> Result<Result<MemoryEntry, MemoryRefusal>> {
    let draft = match checked_draft(draft) {
        Ok(draft) => draft,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let target = match target {
        MemoryTarget::Update { id, version } => MemoryTarget::Update {
            id: id.trim().to_string(),
            version: *version,
        },
        MemoryTarget::Create => MemoryTarget::Create,
    };
    let saved = store.save_memory(
        scope,
        &target,
        &draft,
        writer,
        now_millis(),
        MEMORY_MAX_ENTRIES,
    )?;
    Ok(match saved {
        MemorySave::Saved(entry) => Ok(entry),
        MemorySave::StaleVersion(entry) => Err(MemoryRefusal::StaleVersion(entry)),
        MemorySave::Unknown => Err(MemoryRefusal::Unknown),
        MemorySave::Full => Err(MemoryRefusal::Full),
    })
}

/// The entry `id` of `scope`'s memory, in full.
pub fn read_memory(
    store: &Store,
    scope: &MemoryScope,
    id: &str,
) -> Result<Result<MemoryEntry, MemoryRefusal>> {
    Ok(store
        .get_memory(scope, id.trim())?
        .ok_or(MemoryRefusal::Unknown))
}

/// Deletes the entry `id` of `scope`'s memory.
pub fn delete_memory(
    store: &Store,
    scope: &MemoryScope,
    id: &str,
) -> Result<Result<(), MemoryRefusal>> {
    Ok(if store.delete_memory(scope, id.trim())? {
        Ok(())
    } else {
        Err(MemoryRefusal::Unknown)
    })
}

/// [`save_memory`], telling every client of the entry saved.
pub fn save_and_announce(
    state: &Arc<AppState>,
    scope: &MemoryScope,
    target: &MemoryTarget,
    draft: MemoryDraft,
    writer: &MemoryWriter,
) -> Result<Result<MemoryEntry, MemoryRefusal>> {
    let _writes = lock_memory_writes();
    let saved = save_memory(&state.store, scope, target, draft, writer)?;
    if let Ok(entry) = &saved {
        state.broadcast(Event::MemorySaved {
            entry: entry.clone(),
        });
    }
    Ok(saved)
}

/// [`delete_memory`], telling every client of the entry deleted.
pub fn delete_and_announce(
    state: &Arc<AppState>,
    scope: &MemoryScope,
    id: &str,
) -> Result<Result<(), MemoryRefusal>> {
    let _writes = lock_memory_writes();
    let deleted = delete_memory(&state.store, scope, id)?;
    if deleted.is_ok() {
        announce_deleted(state, scope, vec![id.trim().to_string()]);
    }
    Ok(deleted)
}

/// Deletes every entry of `scope`'s memory, telling every client of those deleted; returns their
/// ids.
pub fn delete_all_and_announce(state: &Arc<AppState>, scope: &MemoryScope) -> Result<Vec<String>> {
    let _writes = lock_memory_writes();
    let ids = state.store.delete_all_memories(scope)?;
    announce_deleted(state, scope, ids.clone());
    Ok(ids)
}

fn announce_deleted(state: &Arc<AppState>, scope: &MemoryScope, ids: Vec<String>) {
    if !ids.is_empty() {
        state.broadcast(Event::MemoriesDeleted {
            scope: scope.clone(),
            memories: ids,
        });
    }
}

/// Tells every client which places' memory no project is at any more, after something that may
/// have changed it: a project added or removed, a place's memory deleted. A failure to read it is
/// logged, not raised: what changed has been done, and a client asks again on its next `snapshot`.
pub fn announce_orphaned(state: &AppState) {
    match state.store.orphaned_memories() {
        Ok(places) => state.broadcast(Event::OrphanedMemoryList { id: None, places }),
        Err(err) => {
            tracing::warn!(%err, "reading the memory left behind by removed projects failed")
        }
    }
}

/// Refuses deleting the memory of the place `host_id` and `path` while a project other than
/// `except` is at it, which still reads it.
pub fn refuse_while_place_in_use(
    store: &Store,
    host_id: &str,
    path: &str,
    except: Option<&str>,
) -> Result<()> {
    match project_at_place(store, host_id, path, except)? {
        None => Ok(()),
        Some(project) => Err(CodedError::raised(
            error_code::MEMORY_PLACE_IN_USE,
            format!(
                "the project {} is at this place and still reads its memory",
                project.name
            ),
            &[("project", &project.id)],
        )),
    }
}

/// `refusal` of a request about the entry `id` (none for a create) as the coded failure a client
/// is answered with.
fn coded_refusal(refusal: MemoryRefusal, id: Option<&str>) -> anyhow::Error {
    let id = id.map(str::trim).unwrap_or_default();
    match refusal {
        MemoryRefusal::InvalidSummary => CodedError::raised(
            error_code::INVALID_MEMORY_SUMMARY,
            format!(
                "a memory entry's summary is one line of 1 to {MEMORY_MAX_SUMMARY_CHARS} \
                 characters"
            ),
            &[("limit", &MEMORY_MAX_SUMMARY_CHARS.to_string())],
        ),
        MemoryRefusal::EmptyContent => CodedError::raised(
            error_code::MEMORY_CONTENT_EMPTY,
            "a memory entry's content cannot be blank",
            &[],
        ),
        MemoryRefusal::ContentTooLong(bytes) => CodedError::raised(
            error_code::MEMORY_CONTENT_TOO_LONG,
            format!(
                "a memory entry's content is at most {MEMORY_MAX_CONTENT_BYTES} bytes; this one \
                 has {bytes}"
            ),
            &[
                ("limit", &MEMORY_MAX_CONTENT_BYTES.to_string()),
                ("bytes", &bytes.to_string()),
            ],
        ),
        MemoryRefusal::StaleVersion(current) => CodedError::raised(
            error_code::MEMORY_CHANGED,
            format!(
                "the memory entry {id} ({}) was saved by someone else since it was read",
                current.summary
            ),
            &[("memory", id), ("summary", &current.summary)],
        ),
        MemoryRefusal::Unknown => CodedError::raised(
            error_code::UNKNOWN_MEMORY,
            format!("the memory has no entry {id}"),
            &[("memory", id)],
        ),
        MemoryRefusal::Full => CodedError::raised(
            error_code::MEMORY_FULL,
            format!("the memory holds {MEMORY_MAX_ENTRIES} entries already"),
            &[("limit", &MEMORY_MAX_ENTRIES.to_string())],
        ),
    }
}

/// The client requests on a memory: each names a console, which must exist, or a project, whose
/// place's memory it reaches, and is written by the user. A refusal is answered as a coded failure.
pub(super) mod requests {
    use super::*;

    /// The memory a request names by exactly one of `console` and `project`.
    pub fn scope(
        state: &Arc<AppState>,
        console: Option<String>,
        project: Option<String>,
    ) -> Result<MemoryScope> {
        match (console, project) {
            (Some(console), None) => {
                state.store.require_console(&console)?;
                Ok(MemoryScope::Console {
                    console_id: console,
                })
            }
            (None, Some(project)) => Ok(place_of(&state.store.require_project(&project)?)),
            (None, None) => Err(super::super::validate::field_required("console")),
            (Some(_), Some(_)) => Err(CodedError::raised(
                error_code::UNREADABLE_REQUEST,
                "a memory request names a console or a project, not both",
                &[],
            )),
        }
    }

    /// Answers `list_memories` with the index of `scope`, the most recently updated first.
    pub fn list(
        state: &Arc<AppState>,
        request_id: Option<String>,
        scope: MemoryScope,
    ) -> Result<Option<Event>> {
        let memories = state.store.memory_index(&scope)?;
        Ok(Some(Event::MemoryList {
            id: request_id,
            scope,
            memories,
        }))
    }

    /// Answers `read_memory` with the entry `memory` of `scope`, content included.
    pub fn read(
        state: &Arc<AppState>,
        request_id: Option<String>,
        scope: MemoryScope,
        memory: String,
    ) -> Result<Option<Event>> {
        let entry = read_memory(&state.store, &scope, &memory)?
            .map_err(|refusal| coded_refusal(refusal, Some(&memory)))?;
        Ok(Some(Event::MemoryContent {
            id: request_id,
            entry,
        }))
    }

    /// `save_memory` from a client: written by the user, creating an entry without `memory` and
    /// `version` and updating the entry `memory` with both, checked against `version` as a
    /// session's save is.
    pub fn save(
        state: &Arc<AppState>,
        scope: &MemoryScope,
        memory: Option<String>,
        version: Option<i64>,
        draft: MemoryDraft,
    ) -> Result<()> {
        let target = match (memory, version) {
            (None, None) => MemoryTarget::Create,
            (Some(id), Some(version)) => MemoryTarget::Update { id, version },
            (Some(_), None) => return Err(super::super::validate::field_required("version")),
            (None, Some(_)) => return Err(super::super::validate::field_required("memory")),
        };
        let id = match &target {
            MemoryTarget::Update { id, .. } => Some(id.clone()),
            MemoryTarget::Create => None,
        };
        save_and_announce(state, scope, &target, draft, &MemoryWriter::User)?
            .map_err(|refusal| coded_refusal(refusal, id.as_deref()))?;
        Ok(())
    }

    /// `delete_memory` from a client.
    pub fn delete(state: &Arc<AppState>, scope: &MemoryScope, memory: &str) -> Result<()> {
        delete_and_announce(state, scope, memory)?
            .map_err(|refusal| coded_refusal(refusal, Some(memory)))?;
        Ok(())
    }

    /// `delete_memories` from a client: every entry of `scope`.
    pub fn delete_all(state: &Arc<AppState>, scope: &MemoryScope) -> Result<()> {
        delete_all_and_announce(state, scope)?;
        Ok(())
    }

    /// Answers `list_orphaned_memories` with the places whose memory no project is at any more.
    pub fn list_orphaned(
        state: &Arc<AppState>,
        request_id: Option<String>,
    ) -> Result<Option<Event>> {
        Ok(Some(Event::OrphanedMemoryList {
            id: request_id,
            places: state.store.orphaned_memories()?,
        }))
    }

    /// `delete_orphaned_memory`: deletes the memory of the place `host_id` and `path`, refused
    /// while a project is at it.
    pub fn delete_orphaned(state: &Arc<AppState>, host_id: String, path: String) -> Result<()> {
        refuse_while_place_in_use(&state.store, &host_id, &path, None)?;
        delete_all_and_announce(state, &MemoryScope::Place { host_id, path })?;
        announce_orphaned(state);
        Ok(())
    }

    /// `delete_orphaned_memories`: deletes the memory of every place no project is at, keeping a
    /// place a project was added at meanwhile; what is left behind is told once.
    pub fn delete_all_orphaned(state: &Arc<AppState>) -> Result<()> {
        {
            let _writes = lock_memory_writes();
            for (scope, ids) in state.store.delete_orphaned_memories()? {
                announce_deleted(state, &scope, ids);
            }
        }
        announce_orphaned(state);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The memory of the console the tests' state holds.
    fn console_scope() -> MemoryScope {
        MemoryScope::Console {
            console_id: "console-1".to_string(),
        }
    }

    fn draft(summary: &str, content: &str) -> MemoryDraft {
        MemoryDraft {
            summary: summary.to_string(),
            local: None,
            content: content.to_string(),
        }
    }

    fn update(id: &str, version: i64) -> MemoryTarget {
        MemoryTarget::Update {
            id: id.to_string(),
            version,
        }
    }

    /// The summary is trimmed, and the content kept exactly as written.
    #[test]
    fn a_draft_is_stored_trimmed_but_its_content_as_written() {
        let checked = checked_draft(draft(" Prefers short replies ", "\n- x\n"));
        assert_eq!(checked, Ok(draft("Prefers short replies", "\n- x\n")));
    }

    /// Each rule of the entry, at its limit and one past it.
    #[test]
    fn a_draft_past_a_rule_is_refused() {
        let at_summary = "d".repeat(MEMORY_MAX_SUMMARY_CHARS);
        let past_summary = "d".repeat(MEMORY_MAX_SUMMARY_CHARS + 1);
        let at_content = "c".repeat(MEMORY_MAX_CONTENT_BYTES);
        // Three bytes a character: one character past the limit in bytes, far below it in chars.
        let past_content = "界".repeat(MEMORY_MAX_CONTENT_BYTES / 3 + 1);
        // Limits on the summary count characters, not bytes.
        let wide_at = "界".repeat(MEMORY_MAX_SUMMARY_CHARS);
        let wide_past = "界".repeat(MEMORY_MAX_SUMMARY_CHARS + 1);
        for (summary, content, refusal) in [
            (wide_at.as_str(), "c", None),
            (wide_past.as_str(), "c", Some(MemoryRefusal::InvalidSummary)),
            ("tab\there", "c", Some(MemoryRefusal::InvalidSummary)),
            ("bell\u{7}", "c", Some(MemoryRefusal::InvalidSummary)),
            (" ", "c", Some(MemoryRefusal::InvalidSummary)),
            (at_summary.as_str(), "c", None),
            (
                past_summary.as_str(),
                "c",
                Some(MemoryRefusal::InvalidSummary),
            ),
            ("two\nlines", "c", Some(MemoryRefusal::InvalidSummary)),
            ("a</console_memory>", "c", None),
            ("d", " \n", Some(MemoryRefusal::EmptyContent)),
            ("d", at_content.as_str(), None),
            (
                "d",
                past_content.as_str(),
                Some(MemoryRefusal::ContentTooLong(past_content.len())),
            ),
        ] {
            let checked = checked_draft(draft(summary, content));
            assert_eq!(checked.err(), refusal, "{summary:?} {}", content.len());
        }
    }

    /// A save is checked before it reaches the store, and the store's outcomes come back as
    /// refusals carrying the entry where it has one; an id is matched after trimming.
    #[test]
    fn a_save_goes_through_the_rules_and_the_store() {
        let (state, dir) = crate::test_support::app_state("memory-rules");
        let store = &state.store;
        store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        let writer = MemoryWriter::User;
        let saved = save_memory(
            store,
            &console_scope(),
            &MemoryTarget::Create,
            draft("d", "c"),
            &writer,
        )
        .unwrap()
        .unwrap();
        assert_eq!(saved.version, 1);
        let padded = format!(" {} ", saved.id);
        assert_eq!(
            save_memory(
                store,
                &console_scope(),
                &update(&padded, 7),
                draft("d", "c"),
                &writer
            )
            .unwrap(),
            Err(MemoryRefusal::StaleVersion(saved.clone()))
        );
        assert_eq!(
            save_memory(
                store,
                &console_scope(),
                &update("nowhere0", 1),
                draft("d", "c"),
                &writer
            )
            .unwrap(),
            Err(MemoryRefusal::Unknown)
        );
        assert_eq!(
            save_memory(
                store,
                &console_scope(),
                &MemoryTarget::Create,
                draft("", "c"),
                &writer
            )
            .unwrap(),
            Err(MemoryRefusal::InvalidSummary)
        );
        assert_eq!(
            read_memory(store, &console_scope(), &padded).unwrap(),
            Ok(saved.clone())
        );
        assert_eq!(
            delete_memory(store, &console_scope(), &padded).unwrap(),
            Ok(())
        );
        assert_eq!(
            delete_memory(store, &console_scope(), &saved.id).unwrap(),
            Err(MemoryRefusal::Unknown)
        );
        assert_eq!(
            read_memory(store, &console_scope(), &saved.id).unwrap(),
            Err(MemoryRefusal::Unknown)
        );
    }

    mod requests {
        use super::*;
        use crate::coordinator::handle;
        use crate::protocol::{CodedError, RequestBody};
        use crate::test_support::code_of;

        fn save(memory: Option<&str>, content: &str, version: Option<i64>) -> RequestBody {
            RequestBody::SaveMemory {
                console: Some("console-1".to_string()),
                project: None,
                memory: memory.map(str::to_string),
                summary: "about it".to_string(),
                local: None,
                content: content.to_string(),
                version,
            }
        }

        fn create(content: &str) -> RequestBody {
            save(None, content, None)
        }

        fn read(memory: &str) -> RequestBody {
            RequestBody::ReadMemory {
                console: Some("console-1".to_string()),
                project: None,
                memory: memory.to_string(),
            }
        }

        fn delete(memory: &str) -> RequestBody {
            RequestBody::DeleteMemory {
                console: Some("console-1".to_string()),
                project: None,
                memory: memory.to_string(),
            }
        }

        fn session() -> MemoryWriter {
            MemoryWriter::agent(crate::protocol::AgentId::CODEX)
        }

        fn console_state(name: &str) -> (Arc<AppState>, crate::test_support::ScratchDir) {
            let (state, dir) = crate::test_support::app_state(name);
            state
                .store
                .insert_console(&crate::test_support::console(&dir))
                .unwrap();
            (state, dir)
        }

        /// The entry the next `memory_saved` on `events` carries.
        fn saved_entry(events: &mut tokio::sync::broadcast::Receiver<Event>) -> MemoryEntry {
            match events.try_recv().unwrap() {
                Event::MemorySaved { entry } => entry,
                other => panic!("a save is announced, not {other:?}"),
            }
        }

        /// The user creates, lists, reads, updates by id with the version read, and deletes an
        /// entry; each write records the user and is told to every client, by id.
        #[tokio::test]
        async fn the_user_manages_a_consoles_memory_and_every_client_hears_of_it() {
            let (state, _dir) = console_state("memory-requests");
            let mut events = state.subscribe();
            assert!(handle(&state, None, create("first"))
                .await
                .unwrap()
                .is_none());
            let entry = saved_entry(&mut events);
            assert_eq!(entry.updated_by, MemoryWriter::User);
            assert_eq!(entry.version, 1);
            assert_eq!(entry.content, "first");
            let id = entry.id;

            let listed = handle(
                &state,
                Some("r1".to_string()),
                RequestBody::ListMemories {
                    console: Some("console-1".to_string()),
                    project: None,
                },
            )
            .await
            .unwrap();
            let Some(Event::MemoryList {
                id: request,
                scope,
                memories,
            }) = listed
            else {
                panic!("a list is answered with memory_list");
            };
            assert_eq!(request.as_deref(), Some("r1"));
            assert_eq!(scope, console_scope());
            assert_eq!(memories.len(), 1);
            assert_eq!(memories[0].id, id);
            assert_eq!(memories[0].updated_by, MemoryWriter::User);
            assert_eq!(memories[0].version, 1);

            let content = handle(&state, None, read(&id)).await.unwrap();
            assert!(matches!(
                content,
                Some(Event::MemoryContent { ref entry, .. }) if entry.content == "first"
            ));

            handle(&state, None, save(Some(&id), "second", Some(1)))
                .await
                .unwrap();
            let entry = saved_entry(&mut events);
            assert_eq!(
                (entry.id.as_str(), entry.version, entry.content.as_str()),
                (id.as_str(), 2, "second")
            );

            handle(&state, None, delete(&id)).await.unwrap();
            assert!(matches!(
                events.try_recv().unwrap(),
                Event::MemoriesDeleted { ref scope, ref memories }
                    if scope == &console_scope() && memories == std::slice::from_ref(&id)
            ));
        }

        /// A user's update over a session's save in between is refused, and every refusal comes
        /// back as a coded failure naming what it is about; nothing refused is announced.
        #[tokio::test]
        async fn a_refused_request_is_a_coded_failure() {
            let (state, _dir) = console_state("memory-request-refusals");
            let mut events = state.subscribe();
            handle(&state, None, create("first")).await.unwrap();
            let id = saved_entry(&mut events).id;
            save_and_announce(
                &state,
                &console_scope(),
                &MemoryTarget::Update {
                    id: id.clone(),
                    version: 1,
                },
                MemoryDraft {
                    summary: "the session's".to_string(),
                    local: None,
                    content: "the session's".to_string(),
                },
                &session(),
            )
            .unwrap()
            .unwrap();
            saved_entry(&mut events);
            let refusal = |body| {
                let state = state.clone();
                async move {
                    let err = handle(&state, None, body).await.expect_err("refused");
                    (code_of(&err), CodedError::of(&err).params)
                }
            };
            let (code, params) = refusal(save(Some(&id), "mine", Some(1))).await;
            assert_eq!(code, error_code::MEMORY_CHANGED);
            assert_eq!(params.get("memory"), Some(&id));
            assert_eq!(
                params.get("summary").map(String::as_str),
                Some("the session's")
            );
            let (code, params) = refusal(save(Some("nowhere0"), "mine", Some(1))).await;
            assert_eq!(code, error_code::UNKNOWN_MEMORY);
            assert_eq!(params.get("memory").map(String::as_str), Some("nowhere0"));
            let (code, params) = refusal(save(Some(&id), "mine", None)).await;
            assert_eq!(code, error_code::FIELD_REQUIRED);
            assert_eq!(params.get("field").map(String::as_str), Some("version"));
            let (code, params) = refusal(save(None, "mine", Some(1))).await;
            assert_eq!(code, error_code::FIELD_REQUIRED);
            assert_eq!(params.get("field").map(String::as_str), Some("memory"));
            assert_eq!(
                refusal(create(" ")).await.0,
                error_code::MEMORY_CONTENT_EMPTY
            );
            let too_long = "c".repeat(MEMORY_MAX_CONTENT_BYTES + 1);
            let (code, params) = refusal(create(&too_long)).await;
            assert_eq!(code, error_code::MEMORY_CONTENT_TOO_LONG);
            assert_eq!(params.get("limit").map(String::as_str), Some("16384"));
            assert_eq!(
                params.get("bytes").map(String::as_str),
                Some(too_long.len().to_string().as_str())
            );
            let mut long_summary = create("c");
            if let RequestBody::SaveMemory { summary, .. } = &mut long_summary {
                *summary = "d".repeat(MEMORY_MAX_SUMMARY_CHARS + 1);
            }
            let (code, params) = refusal(long_summary).await;
            assert_eq!(code, error_code::INVALID_MEMORY_SUMMARY);
            assert_eq!(params.get("limit").map(String::as_str), Some("200"));
            let (code, params) = refusal(read("nowhere0")).await;
            assert_eq!(code, error_code::UNKNOWN_MEMORY);
            assert_eq!(params.get("memory").map(String::as_str), Some("nowhere0"));
            assert_eq!(
                refusal(delete("nowhere0")).await.0,
                error_code::UNKNOWN_MEMORY
            );
            assert!(events.try_recv().is_err(), "nothing refused is announced");
        }

        /// A full console refuses a new entry with its limit.
        #[tokio::test]
        async fn a_full_console_refuses_a_new_entry() {
            let (state, _dir) = console_state("memory-request-full");
            for _ in 0..MEMORY_MAX_ENTRIES {
                handle(&state, None, create("c")).await.unwrap();
            }
            let err = handle(&state, None, create("c")).await.expect_err("full");
            assert_eq!(code_of(&err), error_code::MEMORY_FULL);
            assert_eq!(
                CodedError::of(&err).params.get("limit").map(String::as_str),
                Some(MEMORY_MAX_ENTRIES.to_string().as_str())
            );
        }

        /// Deleting every entry announces their ids at once, and deleting none announces nothing.
        #[tokio::test]
        async fn deleting_every_entry_announces_them() {
            let (state, _dir) = console_state("memory-request-delete-all");
            let mut events = state.subscribe();
            handle(&state, None, create("one")).await.unwrap();
            let first = saved_entry(&mut events).id;
            handle(&state, None, create("two")).await.unwrap();
            let second = saved_entry(&mut events).id;
            let delete_all = || {
                handle(
                    &state,
                    None,
                    RequestBody::DeleteMemories {
                        console: Some("console-1".to_string()),
                        project: None,
                    },
                )
            };
            delete_all().await.unwrap();
            let Event::MemoriesDeleted { memories, .. } = events.try_recv().unwrap() else {
                panic!("the deletion is announced");
            };
            let mut deleted = memories.clone();
            deleted.sort();
            let mut expected = vec![first, second];
            expected.sort();
            assert_eq!(deleted, expected);
            delete_all().await.unwrap();
            assert!(events.try_recv().is_err());
        }

        /// Every write is announced in the order it was made, whoever made it.
        #[tokio::test]
        async fn writes_are_announced_in_the_order_they_were_made() {
            let (state, _dir) = console_state("memory-request-order");
            let mut events = state.subscribe();
            handle(&state, None, create("one")).await.unwrap();
            let first = saved_entry(&mut events).id;
            handle(&state, None, delete(&first)).await.unwrap();
            let second = save_and_announce(
                &state,
                &console_scope(),
                &MemoryTarget::Create,
                draft("d", "two"),
                &session(),
            )
            .unwrap()
            .unwrap()
            .id;
            handle(&state, None, save(Some(&second), "three", Some(1)))
                .await
                .unwrap();
            let heard: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
                .map(|event| match event {
                    Event::MemoriesDeleted { memories, .. } => {
                        format!("deleted {}", memories.join(","))
                    }
                    Event::MemorySaved { entry } => {
                        format!("saved {} v{} {}", entry.id, entry.version, entry.content)
                    }
                    other => panic!("unexpected {other:?}"),
                })
                .collect();
            assert_ne!(
                first, second,
                "a new entry is never given a deleted one's id"
            );
            assert_eq!(
                heard,
                [
                    format!("deleted {first}"),
                    format!("saved {second} v1 two"),
                    format!("saved {second} v2 three"),
                ]
            );
        }

        /// A request names a console that must exist, whichever it is.
        #[tokio::test]
        async fn a_request_for_an_unknown_console_is_refused() {
            let (state, _dir) = console_state("memory-request-console");
            for body in [
                RequestBody::DeleteMemories {
                    console: Some("nowhere".to_string()),
                    project: None,
                },
                RequestBody::DeleteMemory {
                    console: Some("nowhere".to_string()),
                    project: None,
                    memory: "a".to_string(),
                },
                RequestBody::ReadMemory {
                    console: Some("nowhere".to_string()),
                    project: None,
                    memory: "a".to_string(),
                },
                RequestBody::ListMemories {
                    console: Some("nowhere".to_string()),
                    project: None,
                },
                RequestBody::SaveMemory {
                    console: Some("nowhere".to_string()),
                    project: None,
                    memory: None,
                    summary: "d".to_string(),
                    local: None,
                    content: "c".to_string(),
                    version: None,
                },
            ] {
                let err = handle(&state, None, body).await.expect_err("refused");
                assert_eq!(code_of(&err), error_code::UNKNOWN_CONSOLE);
            }
        }

        /// A user's delete is not version-checked, and a user's update carries the user as
        /// writer over a session's.
        #[tokio::test]
        async fn a_users_update_replaces_the_writer_and_a_delete_needs_no_version() {
            let (state, _dir) = console_state("memory-request-writer");
            let id = save_and_announce(
                &state,
                &console_scope(),
                &MemoryTarget::Create,
                draft("d", "theirs"),
                &session(),
            )
            .unwrap()
            .unwrap()
            .id;
            handle(&state, None, save(Some(&id), "mine", Some(1)))
                .await
                .unwrap();
            let entry = read_memory(&state.store, &console_scope(), &id)
                .unwrap()
                .unwrap();
            assert_eq!(
                (entry.version, entry.updated_by, entry.content.as_str()),
                (2, MemoryWriter::User, "mine")
            );
            handle(&state, None, delete(&id)).await.unwrap();
            assert_eq!(
                read_memory(&state.store, &console_scope(), &id).unwrap(),
                Err(MemoryRefusal::Unknown)
            );
        }

        /// Two projects of two consoles at one place, `p1` in `console-1` and `p2` in
        /// `console-2`, and the place's memory scope.
        fn shared_place(
            state: &Arc<AppState>,
            dir: &crate::test_support::ScratchDir,
        ) -> MemoryScope {
            let first = crate::test_support::insert_project(state, &dir.join("repo"));
            let mut second_console = crate::test_support::console(dir);
            second_console.id = "console-2".to_string();
            state.store.insert_console(&second_console).unwrap();
            let mut second = first.clone();
            second.id = "p2".to_string();
            second.console_id = "console-2".to_string();
            state.store.insert_project(&second).unwrap();
            place_of(&first)
        }

        fn in_project(project: &str) -> (Option<String>, Option<String>) {
            (None, Some(project.to_string()))
        }

        /// A request naming a project reaches its place's memory, which every project at the
        /// place shares whatever its console, apart from each console's own; its broadcasts name
        /// the place.
        #[tokio::test]
        async fn a_project_request_reaches_the_memory_of_its_place() {
            let (state, dir) = console_state("memory-request-place");
            let place = shared_place(&state, &dir);
            let mut events = state.subscribe();
            let (console, project) = in_project("p1");
            handle(
                &state,
                None,
                RequestBody::SaveMemory {
                    console,
                    project,
                    memory: None,
                    summary: "Docker needs a prompt here".to_string(),
                    content: "x".to_string(),
                    local: Some(true),
                    version: None,
                },
            )
            .await
            .unwrap();
            let entry = saved_entry(&mut events);
            assert_eq!(entry.scope, place);
            assert!(!entry.local, "a place's entry is never marked");

            let (console, project) = in_project("p2");
            let listed = handle(&state, None, RequestBody::ListMemories { console, project })
                .await
                .unwrap();
            let Some(Event::MemoryList {
                scope, memories, ..
            }) = listed
            else {
                panic!("a list is answered with memory_list");
            };
            assert_eq!(scope, place);
            assert_eq!(memories.len(), 1);
            assert_eq!(memories[0].id, entry.id);
            assert!(state
                .store
                .memory_index(&console_scope())
                .unwrap()
                .is_empty());

            let (console, project) = in_project("p2");
            handle(
                &state,
                None,
                RequestBody::DeleteMemory {
                    console,
                    project,
                    memory: entry.id.clone(),
                },
            )
            .await
            .unwrap();
            assert!(matches!(
                events.try_recv().unwrap(),
                Event::MemoriesDeleted { ref scope, ref memories }
                    if scope == &place && memories == std::slice::from_ref(&entry.id)
            ));
            let (console, project) = in_project("nowhere");
            let err = handle(&state, None, RequestBody::ListMemories { console, project })
                .await
                .expect_err("refused");
            assert_eq!(code_of(&err), error_code::UNKNOWN_PROJECT);
        }

        /// A request names its memory by exactly one of a console and a project.
        #[tokio::test]
        async fn a_request_names_a_console_or_a_project_not_both_or_neither() {
            let (state, dir) = console_state("memory-request-owner");
            crate::test_support::insert_project(&state, &dir.join("repo"));
            let err = handle(
                &state,
                None,
                RequestBody::ListMemories {
                    console: None,
                    project: None,
                },
            )
            .await
            .expect_err("refused");
            assert_eq!(code_of(&err), error_code::FIELD_REQUIRED);
            let err = handle(
                &state,
                None,
                RequestBody::ListMemories {
                    console: Some("console-1".to_string()),
                    project: Some("p1".to_string()),
                },
            )
            .await
            .expect_err("refused");
            assert_eq!(code_of(&err), error_code::UNREADABLE_REQUEST);
        }

        /// A place's memory is listed as left behind once no project is at it, and is deleted
        /// on request only then; the deletion is told to every client, with what is left behind
        /// after it.
        #[tokio::test]
        async fn memory_left_behind_is_listed_and_deleted_only_without_a_project() {
            let (state, dir) = console_state("memory-request-orphaned");
            let project = crate::test_support::insert_project(&state, &dir.join("repo"));
            let place = place_of(&project);
            let id = save_memory(
                &state.store,
                &place,
                &MemoryTarget::Create,
                draft("d", "x"),
                &MemoryWriter::User,
            )
            .unwrap()
            .unwrap()
            .id;
            let listed = || {
                let state = state.clone();
                async move {
                    match handle(
                        &state,
                        Some("r".to_string()),
                        RequestBody::ListOrphanedMemories,
                    )
                    .await
                    .unwrap()
                    {
                        Some(Event::OrphanedMemoryList { id, places }) => {
                            assert_eq!(id.as_deref(), Some("r"));
                            places
                        }
                        other => panic!("{other:?}"),
                    }
                }
            };
            assert!(listed().await.is_empty());
            let (host_id, path) = (project.host_id.clone(), project.path.clone());
            let delete = || RequestBody::DeleteOrphanedMemory {
                host_id: host_id.clone(),
                path: path.clone(),
            };
            let err = handle(&state, None, delete()).await.expect_err("in use");
            assert_eq!(code_of(&err), error_code::MEMORY_PLACE_IN_USE);
            assert_eq!(
                CodedError::of(&err)
                    .params
                    .get("project")
                    .map(String::as_str),
                Some("p1")
            );

            state.store.delete_project("p1").unwrap();
            let places = listed().await;
            assert_eq!(places.len(), 1);
            assert_eq!(
                (&places[0].host_id, &places[0].path, places[0].entries),
                (&host_id, &path, 1)
            );

            let mut events = state.subscribe();
            handle(&state, None, delete()).await.unwrap();
            assert!(matches!(
                events.try_recv().unwrap(),
                Event::MemoriesDeleted { ref scope, ref memories }
                    if scope == &place && memories == std::slice::from_ref(&id)
            ));
            assert!(matches!(
                events.try_recv().unwrap(),
                Event::OrphanedMemoryList { id: None, ref places } if places.is_empty()
            ));
            assert!(listed().await.is_empty());
        }

        /// Deleting all the memory left behind deletes every place's no project is at, keeps a
        /// place a project is at, tells each place's deletion and then what is left behind once.
        #[tokio::test]
        async fn deleting_all_the_memory_left_behind_keeps_a_place_in_use() {
            let (state, dir) = console_state("memory-request-orphaned-all");
            let in_use = crate::test_support::insert_project(&state, &dir.join("repo"));
            let kept_place = place_of(&in_use);
            let gone = MemoryScope::Place {
                host_id: in_use.host_id.clone(),
                path: dir.join("gone").to_string_lossy().into_owned(),
            };
            let create = |scope: &MemoryScope| {
                save_memory(
                    &state.store,
                    scope,
                    &MemoryTarget::Create,
                    draft("d", "x"),
                    &MemoryWriter::User,
                )
                .unwrap()
                .unwrap()
                .id
            };
            let kept = create(&kept_place);
            let deleted = create(&gone);

            let mut events = state.subscribe();
            let answer = handle(&state, None, RequestBody::DeleteOrphanedMemories)
                .await
                .unwrap();
            assert!(answer.is_none(), "answered with ack");
            assert!(matches!(
                events.try_recv().unwrap(),
                Event::MemoriesDeleted { ref scope, ref memories }
                    if scope == &gone && memories == std::slice::from_ref(&deleted)
            ));
            assert!(matches!(
                events.try_recv().unwrap(),
                Event::OrphanedMemoryList { id: None, ref places } if places.is_empty()
            ));
            assert!(
                events.try_recv().is_err(),
                "what is left behind is told once"
            );
            assert!(state
                .store
                .get_memory(&kept_place, &kept)
                .unwrap()
                .is_some());
            assert!(state.store.memory_index(&gone).unwrap().is_empty());

            handle(&state, None, RequestBody::DeleteOrphanedMemories)
                .await
                .unwrap();
            assert!(matches!(
                events.try_recv().unwrap(),
                Event::OrphanedMemoryList { id: None, ref places } if places.is_empty()
            ));
        }
    }
}
