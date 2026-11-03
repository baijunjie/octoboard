//! Where an agent keeps its own on-disk records of a session, and what they say: Claude Code's
//! transcript (and the directory of subagent transcripts beside it), Codex's rollout files
//! (compressed or not; one per history segment of a thread), Grok Build's session directory
//! (marked by its `usage.json`). Shared by the record collector, which reads them
//! (`collector.rs`), and by a session's deletion, which moves them to the Trash
//! (`removable_record_paths`). The names and depths are each agent's [`crate::providers::layout`];
//! how a record is recognised among them, and how its lines read, is each provider's
//! [`RecordStore`] (`crate::adapter::AgentProvider::records`, in `crate::adapter::<id>::records`).
//! This module holds what is common to every provider and asks the session's provider's store the
//! rest; it never branches on which provider it is.
//!
//! A record is found by its name, which carries the agent session id, in one account's own
//! directory (the path a hook reported is only a hint). Both the id and the hint come from the
//! agent's hook payload, which nothing authenticates, so everything that names a path to remove
//! holds to one rule: the id is a canonical UUID, and every path is built from the account's records
//! directory and that id, never from the payload's own path.
//!
//! The submodules read what an agent's records say: [`collector`] persists it for each session,
//! from [`context`] (the context size), [`titles`] (the titles) and [`observed`] (the model and
//! effort), with [`default_effort`] standing in for an effort the records do not name. [`history`]
//! is a session's history for the user and [`conversation`] the common sequence both it and the
//! handoff read, whichever agent wrote the record. [`line_file`] reads a growing file
//! incrementally, and [`watch`] is the mechanism behind the watchers a provider's store arms
//! (`RecordStore::arm_watch`) for what the agents' hooks never report.
//!
//! Where a record is found is [`locate`] (the hook's hint, the account directories walked), the
//! record a switch of accounts carries and what travels with it is [`carried`], which paths a
//! deletion removes are [`removal`], whether a conversation can be resumed is
//! [`mod@resumability`], and whether a relaunched session is sent a prompt to carry on is
//! [`switch_continue`]. [`store`] is the interface each provider answers them through.

pub(crate) mod carried;
pub(crate) mod collector;
pub(crate) mod context;
pub(crate) mod conversation;
pub(crate) mod default_effort;
#[cfg(test)]
pub(crate) mod fixtures;
pub(crate) mod history;
pub(crate) mod line_file;
pub(crate) mod locate;
pub(crate) mod observed;
pub(crate) mod removal;
pub(crate) mod resumability;
pub(crate) mod store;
pub(crate) mod switch_continue;
pub(crate) mod titles;
pub(crate) mod watch;

pub(crate) use locate::{
    conversation_branch, hinted_path, history_branch, on_hook, plausible_id, Branch, Hints,
    Locator, Stretch,
};
pub(crate) use removal::{conversation_files_in, removable_record_paths};
pub(crate) use resumability::{judge, presence, resumability, Judgement, Resumability};
pub(crate) use store::{NoRecords, RecordStore};
