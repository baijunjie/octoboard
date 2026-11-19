//! A session's conversation as a live feed of structured entries (`protocol::ConversationEntry`),
//! for a client that follows the session. How one provider's lines read as entries is its
//! [`FeedReader`] (`RecordStore::feed_reader`); what every provider shares is here: the entries as
//! they stand ([`FeedOut`]), which tells a user's message from one Octoboard wrote, counts the
//! turns, folds the todo list, matches tool results to their calls and cuts what is too long.
//!
//! The entries are held only while someone follows the session; nothing is stored, and the
//! record collector (`records::collector`) keeps no conversation content of its own.

pub mod follow;

use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

use crate::adapter::provider;
use crate::protocol::{
    AgentId, ConversationBody, ConversationEntry, PendingEntry, TodoItem, ToolCallEntry,
    ToolCategory, ToolStatus,
};
use crate::protocol::{
    CONVERSATION_PAGE_BYTES, CONVERSATION_PAGE_DEFAULT, CONVERSATION_PAGE_LIMIT,
};
pub(crate) use crate::records::conversation::at_of;
use crate::records::conversation::{Entry, Todo};

/// The longest string of a tool call's input published whole, in bytes.
pub const INPUT_STRING_LIMIT: usize = 16 * 1024;
/// How much of a tool's output is kept from each end when it is longer than twice this, in bytes.
pub const OUTPUT_END_LIMIT: usize = 8 * 1024;

/// A tool call as a provider's reader reads it from the record.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub category: ToolCategory,
    pub title: Option<String>,
    pub input: Value,
    pub paths: Vec<String>,
    /// [`ToolStatus::Running`] unless the line that holds the call also says how it ended.
    pub status: ToolStatus,
    pub output: Option<String>,
    pub exit_code: Option<i64>,
}

/// How a tool call ended, from a line after the call's own.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    pub status: ToolStatus,
    pub output: Option<String>,
    pub exit_code: Option<i64>,
}

/// Something the agent is still producing, which the record does not hold yet.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    Assistant(String),
    ToolCall(ToolCall),
}

/// How one provider's record reads as feed entries: a reader is made for one pass over a
/// conversation from its start and is handed its lines in order, keeping whatever state it needs
/// across them. Made by `RecordStore::feed_reader`.
pub trait FeedReader: Send {
    /// One parsed line of the conversation's record.
    fn line(&mut self, line: &Value, out: &mut FeedOut);

    /// One parsed line of the file beside the record that shows what the agent is still producing
    /// (`RecordStore::feed_companion`). Lines of the record and of this file are handed over in
    /// the order they were read, the companion's after the record's of the same pass.
    fn companion_line(&mut self, _line: &Value, _out: &mut FeedOut) {}
}

/// The reader of a provider that has none of its own: the common entries its record store reads
/// from each line (`RecordStore::entries`), which by default are none.
pub struct EntriesReader {
    agent: AgentId,
}

impl EntriesReader {
    pub fn new(agent: AgentId) -> Self {
        Self { agent }
    }
}

impl FeedReader for EntriesReader {
    fn line(&mut self, line: &Value, out: &mut FeedOut) {
        let store = provider(self.agent).records();
        let at = at_of(line);
        for entry in store.entries(line) {
            match entry {
                Entry::User(text) => {
                    if let Some(text) = store.user_text(&text) {
                        out.user(&text, at);
                    }
                }
                Entry::Assistant(text) => out.assistant(&text, at),
                Entry::Summary(text) => out.compaction(Some(text), at),
                Entry::Todos { items, merge } => out.todos(items, merge, at),
                Entry::Changed(_) => {}
            }
        }
    }
}

/// The reader for `agent`'s records: its store's own, or the common entries.
pub fn reader_for(agent: AgentId) -> Box<dyn FeedReader> {
    provider(agent)
        .records()
        .feed_reader()
        .unwrap_or_else(|| Box::new(EntriesReader::new(agent)))
}

/// What changed in a [`FeedOut`] since the changes were last taken.
#[derive(Debug, Default, PartialEq)]
pub struct Changes {
    /// Entries already taken were dropped: whoever holds them takes the conversation again.
    pub replaced: bool,
    pub appended: Vec<ConversationEntry>,
    pub updated: Vec<ConversationEntry>,
    /// The pending entries, whole, when they changed.
    pub pending: Option<Vec<PendingEntry>>,
}

/// A run of consecutive entries, in order.
#[derive(Debug, Default, PartialEq)]
pub struct Page {
    pub entries: Vec<ConversationEntry>,
    /// Whether the conversation has entries before the first of these.
    pub has_earlier: bool,
}

impl Page {
    /// How many entries a page holds for the `limit` a request named: the default when it named
    /// none, and never more than [`CONVERSATION_PAGE_LIMIT`] or fewer than one.
    pub fn size(limit: Option<u32>) -> usize {
        limit.map_or(CONVERSATION_PAGE_DEFAULT, |limit| {
            (limit as usize).clamp(1, CONVERSATION_PAGE_LIMIT)
        })
    }
}

/// The conversation's entries as read so far, and what changed in them since the changes were
/// last taken ([`Self::take_changes`]).
#[derive(Default)]
pub struct FeedOut {
    entries: Vec<ConversationEntry>,
    turn: u32,
    /// The list each todos entry left, by the entry's index, with the ids a merge matches by; the
    /// last is the list as it stands, and dropping entries puts back the one before.
    todo_lists: Vec<(usize, Vec<Todo>)>,
    /// The entry of each tool call by its id.
    calls: HashMap<String, usize>,
    pending: Vec<PendingEntry>,
    /// The entries from here on were added since the changes were last taken.
    taken: usize,
    updated: BTreeSet<usize>,
    replaced: bool,
    pending_changed: bool,
}

impl FeedOut {
    /// How many entries there are: what a reader notes to [`Self::truncate`] back to later.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn entries(&self) -> &[ConversationEntry] {
        &self.entries
    }

    pub fn pending(&self) -> &[PendingEntry] {
        &self.pending
    }

    /// The page of at most `limit` entries (at least one: see [`Page::size`] for what a request's
    /// limit gives) that ends just before the entry `before`, or at the last entry when there is
    /// none, also kept to [`CONVERSATION_PAGE_BYTES`] serialized.
    pub fn page(&self, before: Option<u64>, limit: usize) -> Page {
        let end = before.map_or(self.entries.len(), |seq| {
            usize::try_from(seq).map_or(self.entries.len(), |seq| seq.min(self.entries.len()))
        });
        let limit = limit.max(1);
        let mut start = end;
        let mut bytes = 0;
        while start > 0 && end - start < limit {
            bytes += serde_json::to_vec(&self.entries[start - 1]).map_or(0, |json| json.len());
            if bytes > CONVERSATION_PAGE_BYTES && start < end {
                break;
            }
            start -= 1;
        }
        Page {
            entries: self.entries[start..end].to_vec(),
            has_earlier: start > 0,
        }
    }

    /// A message on the user's side, as the provider reads what the user typed: Octoboard's own
    /// messages are told apart here. A blank one is no entry.
    pub fn user(&mut self, text: &str, at: Option<i64>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.turn += 1;
        let body = if crate::records::history::from_octoboard(text) {
            ConversationBody::Octoboard {
                text: text.to_owned(),
            }
        } else {
            ConversationBody::User {
                text: text.to_owned(),
            }
        };
        self.push(body, at);
    }

    /// Text the agent wrote. A blank one is no entry.
    pub fn assistant(&mut self, text: &str, at: Option<i64>) {
        let text = text.trim();
        if !text.is_empty() {
            self.push(
                ConversationBody::Assistant {
                    text: text.to_owned(),
                },
                at,
            );
        }
    }

    /// A tool call. A second call with the same id replaces the first in place.
    pub fn tool_call(&mut self, call: ToolCall, at: Option<i64>) {
        let id = call.call_id.clone();
        let body = ConversationBody::ToolCall(published(call));
        if let Some(&index) = self.calls.get(&id) {
            self.entries[index].body = body;
            self.mark_updated(index);
            return;
        }
        self.calls.insert(id, self.entries.len());
        self.push(body, at);
    }

    /// How the call `call_id` ended. A call not among the entries is passed over.
    pub fn tool_result(&mut self, call_id: &str, outcome: ToolOutcome) {
        let Some(&index) = self.calls.get(call_id) else {
            return;
        };
        let ConversationBody::ToolCall(call) = &mut self.entries[index].body else {
            return;
        };
        call.status = outcome.status;
        if outcome.output.is_some() {
            (call.output, call.output_truncated) = bounded_output(outcome.output);
        }
        if outcome.exit_code.is_some() {
            call.exit_code = outcome.exit_code;
        }
        self.mark_updated(index);
    }

    /// A todo list; with `merge`, its items update those of the list before it by id.
    pub fn todos(&mut self, items: Vec<Todo>, merge: bool, at: Option<i64>) {
        let previous = self.todo_lists.last().map(|(_, list)| list.as_slice());
        let list = fold_todos(previous, items, merge);
        let items = list
            .iter()
            .map(|todo| TodoItem {
                text: todo.text.clone(),
                status: todo.status.clone(),
            })
            .collect();
        self.todo_lists.push((self.entries.len(), list));
        self.push(ConversationBody::Todos { items }, at);
    }

    /// The agent compacted its context, with the summary it wrote when there is one.
    pub fn compaction(&mut self, summary: Option<String>, at: Option<i64>) {
        let summary = summary
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty());
        self.push(ConversationBody::Compaction { summary }, at);
    }

    /// Drops the entries from `len` on: the agent abandoned them (it went back to an earlier
    /// point of the conversation). Dropping one already taken replaces the conversation.
    pub fn truncate(&mut self, len: usize) {
        if len >= self.entries.len() {
            return;
        }
        if len < self.taken {
            self.replaced = true;
            self.taken = len;
        }
        self.entries.truncate(len);
        self.updated.retain(|index| *index < len);
        self.calls.retain(|_, index| *index < len);
        self.turn = self.entries.last().map_or(0, |entry| entry.turn);
        self.todo_lists.retain(|(index, _)| *index < len);
    }

    /// What the agent is still producing, whole: it replaces what was pending before.
    pub fn set_pending(&mut self, pending: Vec<Pending>) {
        let pending: Vec<PendingEntry> = pending
            .into_iter()
            .filter_map(|item| {
                let body = match item {
                    Pending::Assistant(text) => {
                        let text = text.trim();
                        if text.is_empty() {
                            return None;
                        }
                        ConversationBody::Assistant {
                            text: text.to_owned(),
                        }
                    }
                    Pending::ToolCall(call) => ConversationBody::ToolCall(published(call)),
                };
                Some(PendingEntry {
                    turn: self.turn,
                    at: None,
                    body,
                })
            })
            .collect();
        if pending != self.pending {
            self.pending = pending;
            self.pending_changed = true;
        }
    }

    /// What changed since the last call. After a replace, `appended` is empty and the holder
    /// takes the conversation again.
    pub fn take_changes(&mut self) -> Changes {
        let replaced = std::mem::take(&mut self.replaced);
        let appended = if replaced {
            Vec::new()
        } else {
            self.entries[self.taken..].to_vec()
        };
        let updated = if replaced {
            Vec::new()
        } else {
            self.updated
                .iter()
                .filter(|index| **index < self.taken)
                .map(|index| self.entries[*index].clone())
                .collect()
        };
        self.updated.clear();
        self.taken = self.entries.len();
        let pending = std::mem::take(&mut self.pending_changed).then(|| self.pending.clone());
        Changes {
            replaced,
            appended,
            updated,
            pending,
        }
    }

    fn push(&mut self, body: ConversationBody, at: Option<i64>) {
        self.entries.push(ConversationEntry {
            seq: self.entries.len() as u64,
            turn: self.turn,
            at,
            body,
        });
    }

    fn mark_updated(&mut self, index: usize) {
        if index < self.taken {
            self.updated.insert(index);
        }
    }
}

/// `list` after `items`: replaced by them, or with `merge` updated by them item by item.
fn fold_todos(list: Option<&[Todo]>, items: Vec<Todo>, merge: bool) -> Vec<Todo> {
    match list.filter(|_| merge) {
        Some(list) => {
            let mut list = list.to_vec();
            for item in items {
                let known = item
                    .id
                    .as_ref()
                    .and_then(|id| list.iter_mut().find(|old| old.id.as_ref() == Some(id)));
                match known {
                    Some(old) => *old = item,
                    None => list.push(item),
                }
            }
            list
        }
        None => items,
    }
}

/// A reader's tool call as it is published, with what is too long cut.
fn published(call: ToolCall) -> ToolCallEntry {
    let (input, input_truncated) = bounded_value(call.input);
    let (output, output_truncated) = bounded_output(call.output);
    ToolCallEntry {
        call_id: call.call_id,
        name: call.name,
        category: call.category,
        title: call.title.map(|title| first_line(&title)),
        input,
        input_truncated,
        paths: call.paths,
        status: call.status,
        output,
        output_truncated,
        exit_code: call.exit_code,
    }
}

/// The first line of `text`, trimmed and cut to a title's length.
fn first_line(text: &str) -> String {
    crate::text::truncate(text.trim().lines().next().unwrap_or_default().trim(), 200)
}

/// `value` with each string longer than [`INPUT_STRING_LIMIT`] cut, and whether one was.
fn bounded_value(value: Value) -> (Value, bool) {
    match value {
        Value::String(text) if text.len() > INPUT_STRING_LIMIT => (
            Value::String(cut_at(&text, INPUT_STRING_LIMIT).to_owned()),
            true,
        ),
        Value::Array(items) => {
            let mut cut = false;
            let items = items
                .into_iter()
                .map(|item| {
                    let (item, was) = bounded_value(item);
                    cut |= was;
                    item
                })
                .collect();
            (Value::Array(items), cut)
        }
        Value::Object(map) => {
            let mut cut = false;
            let map = map
                .into_iter()
                .map(|(key, item)| {
                    let (item, was) = bounded_value(item);
                    cut |= was;
                    (key, item)
                })
                .collect();
            (Value::Object(map), cut)
        }
        other => (other, false),
    }
}

/// `output` kept to its first and last [`OUTPUT_END_LIMIT`] bytes when it is longer than both,
/// and whether it was cut.
fn bounded_output(output: Option<String>) -> (Option<String>, bool) {
    match output {
        Some(text) if text.len() > 2 * OUTPUT_END_LIMIT => {
            let head = cut_at(&text, OUTPUT_END_LIMIT);
            let mut start = text.len() - OUTPUT_END_LIMIT;
            while !text.is_char_boundary(start) {
                start += 1;
            }
            (Some(format!("{head}\n…\n{}", &text[start..])), true)
        }
        other => (other, false),
    }
}

/// The longest start of `text` of at most `limit` bytes that ends on a character boundary.
fn cut_at(text: &str, limit: usize) -> &str {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests;
