//! Octoboard's memory: the entries of each console's memory and of each place's (a host and a
//! directory), and every id each has given an entry. A console's memory goes with the console, by
//! a trigger on its table; a place's stays until it is deleted. Only rows are kept here; what an
//! entry may hold is checked by `coordinator::memory` before it gets here.

use rusqlite::{params, Connection, OptionalExtension, Row};

use super::{Result, Store};
use crate::protocol::{
    AgentId, CodedError, MemoryDraft, MemoryEntry, MemoryIndexEntry, MemoryScope, MemoryTarget,
    MemoryWriter, OrphanedMemory, MEMORY_ID_LENGTH,
};

/// What [`Store::save_memory`] came to.
#[derive(Debug, PartialEq, Eq)]
pub enum MemorySave {
    /// Stored, as it now stands.
    Saved(MemoryEntry),
    /// An update named a version other than the entry's current one; it is left as it is.
    StaleVersion(MemoryEntry),
    /// An update named an entry that does not exist.
    Unknown,
    /// A create found the memory holding as many entries as it may.
    Full,
}

const MEMORY_COLUMNS: &str = "console_id, host_id, path, id, summary, content, local, created_at, \
                              updated_at, updated_by, updated_by_agent, version";

/// The condition that picks one memory's rows, its three columns bound to `?1`, `?2` and `?3`.
const IN_SCOPE: &str = "console_id = ?1 AND host_id = ?2 AND path = ?3";

/// The order of a memory's entries wherever they are listed: the most recently updated first, and
/// of two updated in the same millisecond, the one stored later.
const MEMORY_ORDER: &str = "ORDER BY updated_at DESC, rowid DESC";

/// A place's memory row no project is at: no project of that host with that stored path.
const NO_PROJECT_AT_PLACE: &str = "NOT EXISTS (
    SELECT 1 FROM projects
    WHERE projects.host_id = memories.host_id AND projects.path = memories.path)";

/// The characters of an entry's id.
const ID_ALPHABET: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// A random id of [`MEMORY_ID_LENGTH`] characters of [`ID_ALPHABET`].
fn random_memory_id() -> String {
    // A v4 UUID carries 122 random bits, more than the 8 characters take; the slight bias of
    // reducing a byte modulo 36 is of no consequence for an id that is checked for reuse.
    uuid::Uuid::new_v4().as_bytes()[..MEMORY_ID_LENGTH]
        .iter()
        .map(|byte| ID_ALPHABET[*byte as usize % ID_ALPHABET.len()] as char)
        .collect()
}

/// The three columns that name `scope`'s memory: a console's id with an empty place, or an empty
/// console with a place's host and path.
fn scope_columns(scope: &MemoryScope) -> (&str, &str, &str) {
    match scope {
        MemoryScope::Console { console_id } => (console_id, "", ""),
        MemoryScope::Place { host_id, path } => ("", host_id, path),
    }
}

fn scope_from(console_id: String, host_id: String, path: String) -> MemoryScope {
    if console_id.is_empty() {
        MemoryScope::Place { host_id, path }
    } else {
        MemoryScope::Console { console_id }
    }
}

impl Store {
    /// Makes every later read or write of the memory fail.
    #[cfg(test)]
    pub fn drop_memories(&self) -> Result<()> {
        self.lock().execute_batch("DROP TABLE memories")?;
        Ok(())
    }

    /// Creates an entry of `scope`'s memory from `draft` with a new id ([`MemoryTarget::Create`]),
    /// unless the memory already holds `max_entries`; or updates the entry of the id named, when
    /// its current version is the one named, raising the version by one and keeping its creation
    /// time and, where `draft.local` is `None`, its `local`. A place's entry is never local. The
    /// checks and the write are one transaction, so of two writers that read the same version only
    /// the first is saved, and a console deleted meanwhile is refused (`unknown_console`) rather
    /// than left with rows under its id.
    pub fn save_memory(
        &self,
        scope: &MemoryScope,
        target: &MemoryTarget,
        draft: &MemoryDraft,
        writer: &MemoryWriter,
        now: i64,
        max_entries: usize,
    ) -> Result<MemorySave> {
        self.save_memory_issuing(
            scope,
            target,
            draft,
            writer,
            now,
            max_entries,
            &mut random_memory_id,
        )
    }

    /// [`Store::save_memory`], drawing the candidates for a new entry's id from `next_id` until
    /// one the memory has never given.
    #[allow(clippy::too_many_arguments)]
    fn save_memory_issuing(
        &self,
        scope: &MemoryScope,
        target: &MemoryTarget,
        draft: &MemoryDraft,
        writer: &MemoryWriter,
        now: i64,
        max_entries: usize,
        next_id: &mut dyn FnMut() -> String,
    ) -> Result<MemorySave> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let (console_id, host_id, path) = scope_columns(scope);
        let (writer_kind, writer_agent) = writer_columns(writer);
        let place = matches!(scope, MemoryScope::Place { .. });
        if !place {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM consoles WHERE id = ?1",
                    params![console_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                return Err(CodedError::unknown_console(console_id).into());
            }
        }
        let id = match target {
            MemoryTarget::Create => {
                let held: i64 = tx.query_row(
                    &format!("SELECT COUNT(*) FROM memories WHERE {IN_SCOPE}"),
                    params![console_id, host_id, path],
                    |row| row.get(0),
                )?;
                if held >= max_entries as i64 {
                    return Ok(MemorySave::Full);
                }
                let id = loop {
                    let candidate = next_id();
                    let issued = tx.execute(
                        "INSERT OR IGNORE INTO memory_ids (console_id, host_id, path, id)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![console_id, host_id, path, candidate],
                    )?;
                    if issued > 0 {
                        break candidate;
                    }
                };
                let local = !place && draft.local.unwrap_or(false);
                tx.execute(
                    "INSERT INTO memories (console_id, host_id, path, id, summary, content, local,
                         created_at, updated_at, updated_by, updated_by_agent, version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, 1)",
                    params![
                        console_id,
                        host_id,
                        path,
                        id,
                        draft.summary,
                        draft.content,
                        local,
                        now,
                        writer_kind,
                        writer_agent,
                    ],
                )?;
                id
            }
            MemoryTarget::Update { id, version } => {
                let Some(current) = memory_in(&tx, scope, id)? else {
                    return Ok(MemorySave::Unknown);
                };
                if current.version != *version {
                    return Ok(MemorySave::StaleVersion(current));
                }
                let local = !place && draft.local.unwrap_or(current.local);
                tx.execute(
                    &format!(
                        "UPDATE memories
                         SET summary = ?5, content = ?6, local = ?7, updated_at = ?8,
                             updated_by = ?9, updated_by_agent = ?10, version = ?11
                         WHERE {IN_SCOPE} AND id = ?4"
                    ),
                    params![
                        console_id,
                        host_id,
                        path,
                        id,
                        draft.summary,
                        draft.content,
                        local,
                        now,
                        writer_kind,
                        writer_agent,
                        version + 1,
                    ],
                )?;
                id.clone()
            }
        };
        let saved = memory_in(&tx, scope, &id)?.expect("the entry was written in this transaction");
        tx.commit()?;
        Ok(MemorySave::Saved(saved))
    }

    /// The entry `id` of `scope`'s memory, content included.
    pub fn get_memory(&self, scope: &MemoryScope, id: &str) -> Result<Option<MemoryEntry>> {
        Ok(memory_in(&self.lock(), scope, id)?)
    }

    /// Every entry of `scope`'s memory, content included, the most recently updated first.
    pub fn memories(&self, scope: &MemoryScope) -> Result<Vec<MemoryEntry>> {
        let conn = self.lock();
        let (console_id, host_id, path) = scope_columns(scope);
        let mut stmt = conn.prepare(&format!(
            "SELECT {MEMORY_COLUMNS} FROM memories WHERE {IN_SCOPE} {MEMORY_ORDER}"
        ))?;
        let rows = stmt
            .query_map(params![console_id, host_id, path], read_memory)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Whether `scope`'s memory holds any entry.
    pub fn has_memory(&self, scope: &MemoryScope) -> Result<bool> {
        let (console_id, host_id, path) = scope_columns(scope);
        Ok(self.lock().query_row(
            &format!("SELECT EXISTS (SELECT 1 FROM memories WHERE {IN_SCOPE})"),
            params![console_id, host_id, path],
            |row| row.get(0),
        )?)
    }

    /// The index of `scope`'s memory: every entry without its content, the most recently updated
    /// first.
    pub fn memory_index(&self, scope: &MemoryScope) -> Result<Vec<MemoryIndexEntry>> {
        let conn = self.lock();
        let (console_id, host_id, path) = scope_columns(scope);
        let mut stmt = conn.prepare(&format!(
            "SELECT id, summary, local, created_at, updated_at, updated_by, updated_by_agent,
                    version
             FROM memories WHERE {IN_SCOPE} {MEMORY_ORDER}"
        ))?;
        let rows = stmt
            .query_map(params![console_id, host_id, path], |row| {
                Ok(MemoryIndexEntry {
                    id: row.get(0)?,
                    summary: row.get(1)?,
                    local: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    updated_by: writer_from(row, 5)?,
                    version: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Deletes every entry of `scope`'s memory; returns the ids deleted, in the order they were
    /// listed. The ids stay given.
    pub fn delete_all_memories(&self, scope: &MemoryScope) -> Result<Vec<String>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let (console_id, host_id, path) = scope_columns(scope);
        let ids = {
            let mut stmt = tx.prepare(&format!(
                "SELECT id FROM memories WHERE {IN_SCOPE} {MEMORY_ORDER}"
            ))?;
            let ids = stmt
                .query_map(params![console_id, host_id, path], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids
        };
        tx.execute(
            &format!("DELETE FROM memories WHERE {IN_SCOPE}"),
            params![console_id, host_id, path],
        )?;
        tx.commit()?;
        Ok(ids)
    }

    /// Deletes the entry `id` of `scope`'s memory; returns whether there was one. Its id stays
    /// given.
    pub fn delete_memory(&self, scope: &MemoryScope, id: &str) -> Result<bool> {
        let (console_id, host_id, path) = scope_columns(scope);
        let deleted = self.lock().execute(
            &format!("DELETE FROM memories WHERE {IN_SCOPE} AND id = ?4"),
            params![console_id, host_id, path, id],
        )?;
        Ok(deleted > 0)
    }

    /// Deletes the memory of every place no project is at, in one transaction, so a place a
    /// project is added at meanwhile keeps its memory; returns each place deleted from with the
    /// ids deleted there, by host and path. The ids stay given.
    pub fn delete_orphaned_memories(&self) -> Result<Vec<(MemoryScope, Vec<String>)>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let rows = {
            let mut stmt = tx.prepare(&format!(
                "SELECT host_id, path, id FROM memories
                 WHERE console_id = '' AND {NO_PROJECT_AT_PLACE}
                 ORDER BY host_id, path, updated_at DESC, rowid DESC"
            ))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        tx.execute(
            &format!("DELETE FROM memories WHERE console_id = '' AND {NO_PROJECT_AT_PLACE}"),
            [],
        )?;
        tx.commit()?;
        let mut places: Vec<(MemoryScope, Vec<String>)> = Vec::new();
        for (host_id, path, id) in rows {
            match places.last_mut() {
                Some((
                    MemoryScope::Place {
                        host_id: h,
                        path: p,
                    },
                    ids,
                )) if *h == host_id && *p == path => ids.push(id),
                _ => places.push((MemoryScope::Place { host_id, path }, vec![id])),
            }
        }
        Ok(places)
    }

    /// The places whose memory holds entries while no project is at them (no project of that host
    /// with that stored path), the most recently updated first.
    pub fn orphaned_memories(&self) -> Result<Vec<OrphanedMemory>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT host_id, path, COUNT(*), MAX(updated_at) FROM memories
             WHERE console_id = '' AND {NO_PROJECT_AT_PLACE}
             GROUP BY host_id, path
             ORDER BY MAX(updated_at) DESC, host_id, path"
        ))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(OrphanedMemory {
                    host_id: row.get(0)?,
                    path: row.get(1)?,
                    entries: row.get::<_, i64>(2)? as usize,
                    updated_at: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

fn memory_in(
    conn: &Connection,
    scope: &MemoryScope,
    id: &str,
) -> rusqlite::Result<Option<MemoryEntry>> {
    let (console_id, host_id, path) = scope_columns(scope);
    conn.query_row(
        &format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE {IN_SCOPE} AND id = ?4"),
        params![console_id, host_id, path, id],
        read_memory,
    )
    .optional()
}

/// The writer as its two columns keep it: `updated_by`, the kind, and `updated_by_agent`, the
/// agent's id for an agent, `NULL` for the user.
fn writer_columns(writer: &MemoryWriter) -> (&'static str, Option<&'static str>) {
    match writer {
        MemoryWriter::User => ("user", None),
        MemoryWriter::Agent { agent } => ("agent", agent.map(AgentId::wire_name)),
    }
}

/// The writer kept in columns `index` (the kind) and `index + 1` (the agent's id). An agent's id
/// that no registered provider has any more is kept as text and read as an agent with none.
fn writer_from(row: &Row<'_>, index: usize) -> rusqlite::Result<MemoryWriter> {
    let writer_kind: String = row.get(index)?;
    Ok(match writer_kind.as_str() {
        "agent" => {
            let agent: Option<String> = row.get(index + 1)?;
            MemoryWriter::Agent {
                agent: agent.as_deref().and_then(AgentId::from_wire_name),
            }
        }
        _ => MemoryWriter::User,
    })
}

fn read_memory(row: &Row<'_>) -> rusqlite::Result<MemoryEntry> {
    let updated_by = writer_from(row, 9)?;
    Ok(MemoryEntry {
        scope: scope_from(row.get(0)?, row.get(1)?, row.get(2)?),
        id: row.get(3)?,
        summary: row.get(4)?,
        content: row.get(5)?,
        local: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        updated_by,
        version: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Console;
    use crate::store::test_support::{console, temp_db};
    use crate::test_support::ScratchFile;

    const MAX: usize = 3;

    /// A store at a scratch path, and the file that keeps it alive.
    fn temp_store(name: &str) -> (Store, ScratchFile) {
        let path = temp_db(name);
        (Store::open(&path).expect("store"), path)
    }

    impl Store {
        /// Inserts a console `id` and returns its memory's scope.
        fn test_console(&self, id: &str) -> MemoryScope {
            self.insert_console(&Console {
                id: id.to_string(),
                ..console(None, None, None)
            })
            .unwrap();
            MemoryScope::Console {
                console_id: id.to_string(),
            }
        }

        /// Creates an entry summarized `summary` holding `content`, as an agent at `now`.
        fn create(
            &self,
            console: &MemoryScope,
            summary: &str,
            content: &str,
            now: i64,
        ) -> Result<MemorySave> {
            self.save_memory(
                console,
                &MemoryTarget::Create,
                &draft(summary, content),
                &agent(),
                now,
                MAX,
            )
        }
    }

    fn draft(summary: &str, content: &str) -> MemoryDraft {
        MemoryDraft {
            summary: summary.to_string(),
            content: content.to_string(),
            local: None,
        }
    }

    fn update(id: &str, version: i64) -> MemoryTarget {
        MemoryTarget::Update {
            id: id.to_string(),
            version,
        }
    }

    fn agent() -> MemoryWriter {
        MemoryWriter::agent(AgentId::CODEX)
    }

    fn saved(outcome: Result<MemorySave>) -> MemoryEntry {
        match outcome.unwrap() {
            MemorySave::Saved(entry) => entry,
            other => panic!("not saved: {other:?}"),
        }
    }

    /// A create is given an id of eight lowercase letters and digits, starts at version 1 and
    /// records its writer; an update of that id naming the current version raises it, keeps the
    /// creation time and records the new writer; one naming another version, or an id that
    /// names no entry, changes nothing.
    #[test]
    fn an_update_must_name_the_current_version() {
        let (store, _dir) = temp_store("memory-versions");
        let console = store.test_console("c");
        let first = saved(store.create(&console, "about a", "one", 10));
        assert_eq!(first.id.len(), MEMORY_ID_LENGTH);
        assert!(first
            .id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit()));
        assert_eq!(
            (first.version, first.created_at, first.updated_at),
            (1, 10, 10)
        );
        assert_eq!(first.updated_by, agent());

        let second = saved(store.save_memory(
            &console,
            &update(&first.id, 1),
            &draft("about a, again", "two"),
            &MemoryWriter::User,
            20,
            MAX,
        ));
        assert_eq!(second.id, first.id);
        assert_eq!(
            (second.version, second.created_at, second.updated_at),
            (2, 10, 20)
        );
        assert_eq!(
            (
                second.summary.as_str(),
                second.content.as_str(),
                &second.updated_by
            ),
            ("about a, again", "two", &MemoryWriter::User)
        );

        assert_eq!(
            store
                .save_memory(
                    &console,
                    &update(&first.id, 1),
                    &draft("x", "three"),
                    &agent(),
                    30,
                    MAX
                )
                .unwrap(),
            MemorySave::StaleVersion(second.clone())
        );
        assert_eq!(
            store
                .save_memory(
                    &console,
                    &update("zzzzzzzz", 1),
                    &draft("x", "x"),
                    &agent(),
                    30,
                    MAX
                )
                .unwrap(),
            MemorySave::Unknown
        );
        assert_eq!(store.get_memory(&console, &first.id).unwrap(), Some(second));
        assert_eq!(store.get_memory(&console, "zzzzzzzz").unwrap(), None);
    }

    /// Summaries are no key: two entries may share one, each with an id of its own; an id of
    /// one console names nothing in another.
    #[test]
    fn entries_are_told_apart_by_id_only() {
        let (store, _dir) = temp_store("memory-ids");
        let (one, two) = (store.test_console("c1"), store.test_console("c2"));
        let a = saved(store.create(&one, "same", "a", 1));
        let b = saved(store.create(&one, "same", "b", 2));
        assert_ne!(a.id, b.id);
        assert_eq!(store.get_memory(&one, &a.id).unwrap().unwrap().content, "a");
        assert_eq!(store.get_memory(&two, &a.id).unwrap(), None);
        assert!(!store.delete_memory(&two, &a.id).unwrap());
    }

    /// An id is never given twice in a console: a candidate the console gave before, to an entry
    /// still held or to one deleted since, is passed over for the next; another console may give
    /// it.
    #[test]
    fn an_id_is_never_given_again_in_its_console() {
        let (store, _dir) = temp_store("memory-id-reuse");
        let (one, two) = (store.test_console("c1"), store.test_console("c2"));
        let mut candidates = [
            "aaaaaaaa", "aaaaaaaa", "bbbbbbbb", "aaaaaaaa", "bbbbbbbb", "cccccccc",
        ]
        .into_iter()
        .map(str::to_string);
        let mut next = || candidates.next().expect("a candidate");
        let mut create = |console: &MemoryScope| {
            saved(store.save_memory_issuing(
                console,
                &MemoryTarget::Create,
                &draft("d", "x"),
                &agent(),
                1,
                MAX,
                &mut next,
            ))
            .id
        };
        assert_eq!(create(&one), "aaaaaaaa");
        assert_eq!(create(&one), "bbbbbbbb");
        assert!(store.delete_memory(&one, "aaaaaaaa").unwrap());
        assert_eq!(create(&one), "cccccccc");
        let mut others = ["aaaaaaaa"].into_iter().map(str::to_string);
        let reused = saved(store.save_memory_issuing(
            &two,
            &MemoryTarget::Create,
            &draft("d", "x"),
            &agent(),
            1,
            MAX,
            &mut || others.next().unwrap(),
        ));
        assert_eq!(reused.id, "aaaaaaaa");
    }

    /// A console holding the most entries it may takes no new one, while its entries can still be
    /// updated, and it takes one again once one is deleted.
    #[test]
    fn a_full_console_takes_no_new_entry() {
        let (store, _dir) = temp_store("memory-full");
        let console = store.test_console("c");
        let ids: Vec<String> = (0..3)
            .map(|at| saved(store.create(&console, "d", "x", at)).id)
            .collect();
        assert_eq!(
            store.create(&console, "d", "x", 5).unwrap(),
            MemorySave::Full
        );
        saved(store.save_memory(
            &console,
            &update(&ids[0], 1),
            &draft("d", "y"),
            &agent(),
            6,
            MAX,
        ));
        assert!(store.delete_memory(&console, &ids[1]).unwrap());
        assert!(!store.delete_memory(&console, &ids[1]).unwrap());
        saved(store.create(&console, "d", "x", 7));
    }

    /// The index holds no content and lists the most recently updated first (of two saved in the
    /// same millisecond, the later stored); the full listing keeps the same order.
    #[test]
    fn the_index_lists_the_latest_first() {
        let (store, _dir) = temp_store("memory-index");
        let console = store.test_console("c");
        let old = saved(store.create(&console, "old", "x", 1)).id;
        let tie = saved(store.create(&console, "tie", "x", 1)).id;
        let new = saved(store.create(&console, "new", "x", 2)).id;
        let ids: Vec<String> = store
            .memory_index(&console)
            .unwrap()
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        assert_eq!(ids, [new.clone(), tie.clone(), old.clone()]);
        let full: Vec<String> = store
            .memories(&console)
            .unwrap()
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        assert_eq!(full, [new, tie, old]);
    }

    /// A console's entry is not local unless its create says so, and an update that leaves
    /// `local` out keeps the entry's value; a place's entry is never local, whatever is asked.
    #[test]
    fn local_defaults_to_false_and_an_update_without_it_keeps_it() {
        let (store, _dir) = temp_store("memory-local");
        let console = store.test_console("c");
        let plain = saved(store.create(&console, "plain", "x", 1));
        assert!(!plain.local);
        let marked = saved(store.save_memory(
            &console,
            &MemoryTarget::Create,
            &MemoryDraft {
                local: Some(true),
                ..draft("here", "x")
            },
            &agent(),
            2,
            MAX,
        ));
        assert!(marked.local);
        let kept = saved(store.save_memory(
            &console,
            &update(&marked.id, 1),
            &draft("here, again", "y"),
            &agent(),
            3,
            MAX,
        ));
        assert!(kept.local, "an update without `local` keeps it");
        let cleared = saved(store.save_memory(
            &console,
            &update(&marked.id, 2),
            &MemoryDraft {
                local: Some(false),
                ..draft("not only here", "y")
            },
            &agent(),
            4,
            MAX,
        ));
        assert!(!cleared.local);
        assert!(store
            .memory_index(&console)
            .unwrap()
            .iter()
            .all(|entry| !entry.local));
        let place = place("h", "/p");
        let entry = saved(store.save_memory(
            &place,
            &MemoryTarget::Create,
            &MemoryDraft {
                local: Some(true),
                ..draft("place", "x")
            },
            &agent(),
            1,
            MAX,
        ));
        assert!(!entry.local);
        assert_eq!(entry.scope, place);
    }

    fn place(host_id: &str, path: &str) -> MemoryScope {
        MemoryScope::Place {
            host_id: host_id.to_string(),
            path: path.to_string(),
        }
    }

    /// A place's memory is its host's and path's alone, apart from every console's; its ids are
    /// its own; and it is listed as left behind only while it holds entries and no project is
    /// at the place.
    #[test]
    fn a_places_memory_is_its_own_and_is_orphaned_only_without_a_project() {
        use crate::store::test_support::project;
        let (store, _dir) = temp_store("memory-place");
        let console = store.test_console("console-1");
        let here = place(crate::store::LOCAL_HOST_ID, "/tmp/project-1");
        let other_host = place("remote", "/tmp/project-1");
        let other_path = place(crate::store::LOCAL_HOST_ID, "/tmp/elsewhere");
        let entry = saved(store.create(&here, "local docker", "x", 5));
        saved(store.create(&other_host, "remote", "x", 1));
        assert_eq!(
            store.get_memory(&here, &entry.id).unwrap(),
            Some(entry.clone())
        );
        for elsewhere in [&console, &other_host, &other_path] {
            assert_eq!(store.get_memory(elsewhere, &entry.id).unwrap(), None);
            assert!(!store.delete_memory(elsewhere, &entry.id).unwrap());
        }
        assert!(store.memory_index(&console).unwrap().is_empty());
        assert!(store.memory_index(&other_path).unwrap().is_empty());

        // `project(false)` is the project `project-1` at `/tmp/project-1` on the local host.
        let orphaned = |store: &Store| -> Vec<(String, String, usize)> {
            store
                .orphaned_memories()
                .unwrap()
                .into_iter()
                .map(|place| (place.host_id, place.path, place.entries))
                .collect()
        };
        assert_eq!(
            orphaned(&store),
            [
                (
                    crate::store::LOCAL_HOST_ID.to_string(),
                    "/tmp/project-1".to_string(),
                    1
                ),
                ("remote".to_string(), "/tmp/project-1".to_string(), 1),
            ]
        );
        store.insert_project(&project(false)).unwrap();
        assert_eq!(
            orphaned(&store),
            [("remote".to_string(), "/tmp/project-1".to_string(), 1)]
        );
        assert_eq!(store.orphaned_memories().unwrap()[0].updated_at, 1);
        store.delete_project("project-1").unwrap();
        assert_eq!(
            orphaned(&store).len(),
            2,
            "removing the project leaves its memory"
        );
        assert_eq!(store.get_memory(&here, &entry.id).unwrap(), Some(entry));
        store.delete_all_memories(&here).unwrap();
        assert_eq!(
            orphaned(&store).len(),
            1,
            "a memory with no entries is not listed"
        );
    }

    /// Deleting all the memory left behind deletes every place's no project is at, newest entry
    /// first within each place, and keeps a console's memory and a place a project is at.
    #[test]
    fn deleting_the_memory_left_behind_keeps_what_a_project_or_console_reads() {
        use crate::store::test_support::project;
        let (store, _dir) = temp_store("memory-orphaned-all");
        let console = store.test_console("console-1");
        // `project(false)` is the project `project-1` at `/tmp/project-1` on the local host.
        store.insert_project(&project(false)).unwrap();
        let in_use = place(crate::store::LOCAL_HOST_ID, "/tmp/project-1");
        let gone = place(crate::store::LOCAL_HOST_ID, "/tmp/gone");
        let remote = place("remote", "/tmp/project-1");
        let kept = saved(store.create(&in_use, "kept", "x", 1)).id;
        let console_kept = saved(store.create(&console, "console", "x", 1)).id;
        let older = saved(store.create(&gone, "older", "x", 1)).id;
        let newer = saved(store.create(&gone, "newer", "x", 2)).id;
        let far = saved(store.create(&remote, "far", "x", 1)).id;

        assert_eq!(
            store.delete_orphaned_memories().unwrap(),
            [
                (gone.clone(), vec![newer, older]),
                (remote.clone(), vec![far])
            ]
        );
        assert!(store.orphaned_memories().unwrap().is_empty());
        assert!(store.memory_index(&gone).unwrap().is_empty());
        assert!(store.memory_index(&remote).unwrap().is_empty());
        assert!(store.get_memory(&in_use, &kept).unwrap().is_some());
        assert!(store.get_memory(&console, &console_kept).unwrap().is_some());
        assert!(
            store.delete_orphaned_memories().unwrap().is_empty(),
            "nothing left behind deletes nothing"
        );
    }

    /// Deleting a console deletes its memory and the ids it gave, and leaves another console's
    /// alone.
    #[test]
    fn deleting_a_console_deletes_its_entries() {
        let (store, _dir) = temp_store("memory-cascade");
        let (gone, kept) = (store.test_console("c1"), store.test_console("c2"));
        saved(store.create(&gone, "d", "x", 1));
        let kept_id = saved(store.create(&kept, "d", "x", 1)).id;
        store.delete_console("c1").unwrap();
        for table in ["memories", "memory_ids"] {
            let rows: i64 = store
                .lock()
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE console_id = ?1"),
                    params!["c1"],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(rows, 0, "{table}");
        }
        assert!(store.get_memory(&kept, &kept_id).unwrap().is_some());
    }

    /// Deleting every entry of a console names the ids it deleted and leaves another console's
    /// alone; the index carries each entry's writer and version.
    #[test]
    fn deleting_all_of_a_console_names_them_and_spares_the_others() {
        let (store, _dir) = temp_store("memory-delete-all");
        let (cleared, kept) = (store.test_console("c1"), store.test_console("c2"));
        let first = saved(store.create(&cleared, "b", "x", 1)).id;
        let second = saved(store.create(&cleared, "a", "x", 2)).id;
        saved(store.save_memory(
            &kept,
            &MemoryTarget::Create,
            &draft("a", "x"),
            &MemoryWriter::User,
            1,
            MAX,
        ));
        let index = store.memory_index(&kept).unwrap();
        assert_eq!(index[0].updated_by, MemoryWriter::User);
        assert_eq!(index[0].version, 1);
        assert_eq!(store.memory_index(&cleared).unwrap()[0].updated_by, agent());
        assert_eq!(
            store.delete_all_memories(&cleared).unwrap(),
            [second, first]
        );
        assert!(store.memory_index(&cleared).unwrap().is_empty());
        assert!(store.delete_all_memories(&cleared).unwrap().is_empty());
        assert_eq!(store.memory_index(&kept).unwrap().len(), 1);
    }

    /// A place's memory never gives an id twice either: one it gave an entry since deleted, whether
    /// by deleting the entry, every entry of the memory or the memory left behind, is passed over
    /// for the next, and an update naming it finds no entry.
    #[test]
    fn a_places_ids_are_never_given_again_either() {
        let (store, _dir) = temp_store("memory-place-id-reuse");
        let here = place("h", "/gone");
        let mut candidates = [
            "aaaaaaaa", "aaaaaaaa", "bbbbbbbb", "bbbbbbbb", "aaaaaaaa", "cccccccc", "cccccccc",
            "dddddddd",
        ]
        .into_iter()
        .map(str::to_string);
        let mut next = || candidates.next().expect("a candidate");
        let mut create = |store: &Store| {
            saved(store.save_memory_issuing(
                &here,
                &MemoryTarget::Create,
                &draft("d", "x"),
                &agent(),
                1,
                MAX,
                &mut next,
            ))
            .id
        };
        assert_eq!(create(&store), "aaaaaaaa");
        assert!(store.delete_memory(&here, "aaaaaaaa").unwrap());
        assert_eq!(create(&store), "bbbbbbbb");
        assert_eq!(
            store.delete_all_memories(&here).unwrap(),
            ["bbbbbbbb".to_string()]
        );
        assert_eq!(create(&store), "cccccccc");
        // No project is at the place, so its memory is left behind and goes with the rest of it.
        assert_eq!(store.delete_orphaned_memories().unwrap().len(), 1);
        assert_eq!(
            store
                .save_memory(
                    &here,
                    &update("cccccccc", 1),
                    &draft("d", "y"),
                    &agent(),
                    2,
                    MAX
                )
                .unwrap(),
            MemorySave::Unknown
        );
        assert_eq!(create(&store), "dddddddd");
    }

    /// A save to a console that is not there, deleted meanwhile, is refused as `unknown_console`
    /// and leaves no row under its id.
    #[test]
    fn a_save_to_a_console_that_is_gone_is_refused() {
        let (store, _dir) = temp_store("memory-console-gone");
        let gone = store.test_console("c1");
        store.delete_console("c1").unwrap();
        let err = anyhow::Error::from(store.create(&gone, "d", "x", 1).unwrap_err());
        assert_eq!(
            crate::test_support::code_of(&err),
            crate::protocol::error_code::UNKNOWN_CONSOLE
        );
        let rows: i64 = store
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM memory_ids WHERE console_id = 'c1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rows, 0);
    }

    /// The memory left behind is listed by the place updated most recently first, whatever its
    /// host and path.
    #[test]
    fn the_memory_left_behind_is_listed_latest_first() {
        let (store, _dir) = temp_store("memory-orphaned-order");
        let (older, newer) = (place("a", "/a"), place("b", "/b"));
        saved(store.create(&older, "d", "x", 1));
        saved(store.create(&newer, "d", "x", 2));
        let listed: Vec<(String, i64)> = store
            .orphaned_memories()
            .unwrap()
            .into_iter()
            .map(|place| (place.host_id, place.updated_at))
            .collect();
        assert_eq!(listed, [("b".to_string(), 2), ("a".to_string(), 1)]);
    }

    /// An agent's writer keeps its agent's id, which a later save by another agent replaces and a
    /// save by the user clears; an id no registered provider has any more reads as an agent with
    /// none, rather than losing the entry.
    #[test]
    fn the_writers_agent_is_kept_with_the_entry() {
        let (store, _dir) = temp_store("memory-writer-agent");
        let console = store.test_console("c");
        let first = saved(store.create(&console, "about a", "one", 10));
        assert_eq!(first.updated_by, MemoryWriter::agent(AgentId::CODEX));
        let by_claude = saved(store.save_memory(
            &console,
            &update(&first.id, 1),
            &draft("about a", "two"),
            &MemoryWriter::agent(AgentId::CLAUDE),
            20,
            MAX,
        ));
        assert_eq!(by_claude.updated_by, MemoryWriter::agent(AgentId::CLAUDE));
        assert_eq!(
            store.memory_index(&console).unwrap()[0].updated_by,
            MemoryWriter::agent(AgentId::CLAUDE)
        );
        saved(store.save_memory(
            &console,
            &update(&first.id, 2),
            &draft("about a", "three"),
            &MemoryWriter::User,
            30,
            MAX,
        ));
        let cleared: Option<String> = store
            .lock()
            .query_row(
                "SELECT updated_by_agent FROM memories WHERE id = ?1",
                params![first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cleared, None);

        store
            .lock()
            .execute(
                "UPDATE memories SET updated_by = 'agent', updated_by_agent = 'retired'
                 WHERE id = ?1",
                params![first.id],
            )
            .unwrap();
        let retired = MemoryWriter::Agent { agent: None };
        assert_eq!(
            store
                .get_memory(&console, &first.id)
                .unwrap()
                .unwrap()
                .updated_by,
            retired
        );
        assert_eq!(store.memory_index(&console).unwrap()[0].updated_by, retired);
    }
}
