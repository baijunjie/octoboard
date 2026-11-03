//! The interface one provider's own records are read through ([`RecordStore`], reached as
//! `crate::adapter::AgentProvider::records`). The code in `crate::records` that works for any
//! provider (walking an account's directories, holding a record to the one rule of what may be
//! removed, reading a growing file, folding what the lines said) asks the session's provider's
//! store everything that depends on how that provider names, files and writes its records.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::FileType;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::hooks::RecordWatch;
use crate::protocol::Session;
use crate::records::context::{Context, Figure};
use crate::records::conversation::Entry;
use crate::records::locate::{modified, newest, Branch, Locator};
use crate::records::observed::Observed;
use crate::records::Stretch;
use crate::state::AppState;

/// The hint a hook payload carrying the session's `session_id` and `transcript_path` gives, as
/// Claude Code's and Codex's hooks do ([`RecordStore::hinted_record`]).
pub(crate) fn transcript_path_hint(payload: &Value) -> Option<(String, PathBuf)> {
    let path = payload.get("transcript_path")?.as_str()?;
    let agent_id = payload.get("session_id")?.as_str()?;
    Some((agent_id.to_owned(), PathBuf::from(path)))
}

/// How one provider's own records of a session are found, read and watched
/// (`crate::adapter::AgentProvider::records`). Names and depths come from the provider's records
/// layout (`crate::providers::layout`); what is answered here is how a record is recognised among
/// them and what its lines say. Every method has a default, so a provider without records
/// implements nothing ([`NoRecords`]).
pub trait RecordStore: Send + Sync {
    // -- Finding a record ------------------------------------------------------------------------

    /// The agent session id and the path of the session's record a hook event of the session's
    /// own reports, taken as a hint of where the record is; `None` when the event reports none, or
    /// the provider's hooks report no path that names the record (the default).
    fn hinted_record(&self, _payload: &Value) -> Option<(String, PathBuf)> {
        None
    }

    /// The agent session id a directory entry of the records directory is a record of, with the
    /// path the record is indexed under: `path`, `name` and `kind` are the entry's, and `leaf` says
    /// it sits on the last level the search goes to. `None` when the entry is not a record (it is
    /// then descended into, when it is a directory with levels left).
    fn indexed(
        &self,
        _path: &Path,
        _name: &str,
        _kind: FileType,
        _leaf: bool,
    ) -> Option<(String, PathBuf)> {
        None
    }

    /// Of the indexed records `paths` of one conversation, the one that holds it as it is now. The
    /// newest by modification time by default.
    fn current_of(&self, paths: &[PathBuf]) -> Option<PathBuf> {
        newest(paths)
    }

    /// What the indexed `record` stands for, when it is named for `agent_id` the way the agent
    /// names its own and sits exactly where the agent puts it: `levels_below_dir` levels below the
    /// account's records directory, counting its own name. Empty for anything else, so a name that
    /// merely equals a directory's never names that directory.
    fn own_paths(&self, _agent_id: &str, _record: &Path, _levels_below_dir: usize) -> Vec<PathBuf> {
        Vec::new()
    }

    /// The file a conversation is read from, given what [`Self::own_paths`] answered for it.
    fn conversation_file(&self, record: PathBuf) -> PathBuf {
        record
    }

    /// The conversation's [`Branch`] among `files`, its conversation files that exist. By default
    /// the one written to last, read whole.
    fn branch_among(&self, files: Vec<PathBuf>) -> Option<Branch> {
        let record = files.into_iter().max_by_key(|path| modified(path))?;
        let stretches = Some(vec![Stretch {
            path: record.clone(),
            cut: None,
        }]);
        Some(Branch { record, stretches })
    }

    /// The conversation `agent_id` (a canonical id) kept where the agent archives records, in the
    /// accounts at `roots`, for reading its history once it is found nowhere else.
    fn archived_branch(&self, _roots: &[PathBuf], _agent_id: &str) -> Option<Branch> {
        None
    }

    // -- Resumability ----------------------------------------------------------------------------

    /// Whether a directory entry, named `lowercase_name`, is the record of the conversation
    /// `lowercase_id`, for judging whether it can be resumed.
    fn names_record(&self, _lowercase_name: &str, _kind: FileType, _lowercase_id: &str) -> bool {
        false
    }

    /// Whether the store recognises the provider's records at all ([`Self::indexed`],
    /// [`Self::own_paths`], [`Self::names_record`]), so that a record it finds nowhere in an
    /// account that could be searched whole is not there. A store that recognises none can tell
    /// nothing of a record's absence: its sessions' records are never judged absent.
    fn recognises_records(&self) -> bool {
        false
    }

    /// Whether a record found missing from its own account proves the conversation gone, so that a
    /// resume is refused and an archived session's cleanup may delete it. Only for a store that
    /// [recognises its records](Self::recognises_records), and not for a provider that restores a
    /// missing record itself; by default nothing is proven gone.
    fn absence_is_conclusive(&self) -> bool {
        false
    }

    /// Whether, in an account whose records directories are links (one sharing the default
    /// configuration), `session`'s record sits only where the agent archives records, so that a
    /// resume first needs it brought back.
    fn archived_behind_links(&self, _locator: &Locator, _session: &Session) -> bool {
        false
    }

    /// Brings the record of conversation `agent_id` back from where the agent archives records
    /// (`AgentLayout::archived_records_dir`), which a resume does not read, the way a session of
    /// the account would: `config_dir` is the account's pinned directory, `None` for the default
    /// account, and `shell_env` the environment it launches in. Blocking. Unsupported by default:
    /// an error, which refuses the resume and leaves the session as it was.
    fn unarchive(
        &self,
        _shell_env: &HashMap<String, String>,
        _config_dir: Option<&Path>,
        _agent_id: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("this agent's archived records cannot be brought back for a resume")
    }

    // -- Removal and an account switch ---------------------------------------------------------

    /// What the agent keeps of conversation `agent_id` beside its records in the account directory
    /// `root`, outside the records directory: each path spelled through `root`.
    fn beside_records(&self, _root: &Path, _agent_id: &str) -> Vec<PathBuf> {
        Vec::new()
    }

    /// The record among `records`, the conversation's files under the records directory relative
    /// to `root`, that a switch of accounts carries. By default the regular file written to last.
    fn carried_head(&self, root: &Path, records: &[PathBuf]) -> Option<PathBuf> {
        newest_existing(root, records, std::fs::Metadata::is_file)
    }

    /// Whether the record a conversation's head is in can change while a turn runs (the agent
    /// starts a new file of the conversation), so that a switch finds it again once the process is
    /// down instead of carrying the one found beforehand.
    fn head_moves_during_turn(&self) -> bool {
        false
    }

    /// What of `files`, the conversation's files relative to the account directory, travels with
    /// its carried `record` besides it.
    fn carried_companions(&self, _files: Vec<PathBuf>, _record: &Path) -> Vec<PathBuf> {
        Vec::new()
    }

    /// Whether the turn the record `record` (as `crate::records::carried::locate_record` answers
    /// it) last showed is to be carried on after a relaunch: with `usage_limit_only`, only when it
    /// stopped on the plan's usage limit. Read after the process has ended.
    fn should_continue(&self, _record: &Path, _usage_limit_only: bool) -> io::Result<bool> {
        Ok(false)
    }

    // -- What the records say --------------------------------------------------------------------

    /// The file the collector follows for `record`, the indexed record.
    fn followed_file(&self, record: &Path) -> PathBuf {
        record.to_path_buf()
    }

    /// Whether the lines of the followed file give the model and effort, which an observation
    /// boundary then has to hold.
    fn observes_record_lines(&self) -> bool {
        true
    }

    /// Takes one line of the followed file into `context` and, when `past_boundary` (the line was
    /// written after the session's observation boundary), into `observed`.
    fn fold_line(
        &self,
        _line: &str,
        _past_boundary: bool,
        _context: &mut Context,
        _observed: &mut Observed,
    ) {
    }

    /// Takes the title summary file beside the record (`AgentLayout::title_summary`) into
    /// `observed`.
    fn observe_summary(&self, _summary: &Value, _observed: &mut Observed) {}

    /// A JSON file beside the record that gives the context window.
    fn window_file(&self) -> Option<&'static str> {
        None
    }

    /// The context window [`Self::window_file`]'s content gives.
    fn window_in(&self, _file: &Value) -> Option<u64> {
        None
    }

    /// How much of `window` a context of `tokens` uses, in whole percent, by the agent's own
    /// formula.
    fn used_percent(&self, _tokens: u64, _window: u64) -> Option<u8> {
        None
    }

    /// Whether `record` continues the conversation from an earlier file, holding only what came
    /// after it picked up.
    fn continues_earlier_file(&self, _record: &Path) -> bool {
        false
    }

    /// The file conversation `agent_id` opened in, in the account at `root`, for reading its
    /// opening prompt when the record [`continues an earlier file`](Self::continues_earlier_file).
    fn opening_file(
        &self,
        _locator: &mut Locator,
        _root: &Path,
        _agent_id: &str,
    ) -> Option<PathBuf> {
        None
    }

    /// The context figure conversation `agent_id` had where `head` picks it up, in the account at
    /// `root`: `Some(None)` when none was found, `None` when it could not be looked up and is to be
    /// tried again.
    fn earlier_figure(
        &self,
        _locator: &mut Locator,
        _root: &Path,
        _agent_id: &str,
        _head: &Path,
    ) -> Option<Option<Figure>> {
        Some(None)
    }

    /// The file under an account directory that gives each model's default effort, for a provider
    /// whose records can name a model without the effort it runs at.
    fn effort_defaults_file(&self) -> Option<&'static str> {
        None
    }

    /// Each model's default effort, by the id its sessions run it under, from the parsed content of
    /// [`Self::effort_defaults_file`].
    fn effort_defaults(&self, _catalogue: &Value) -> HashMap<String, String> {
        HashMap::new()
    }

    // -- Conversation and history ---------------------------------------------------------------

    /// The entries one parsed line of the conversation holds.
    fn entries(&self, _line: &Value) -> Vec<Entry> {
        Vec::new()
    }

    /// Whether a line is left out of the session's history, though the conversation keeps it.
    fn hidden_from_history(&self, _line: &Value) -> bool {
        false
    }

    /// What the user typed, from the text of a user entry, or `None` for an entry the agent writes
    /// as the user's that starts no turn. By default the text as it is.
    fn user_text<'a>(&self, text: &'a str) -> Option<Cow<'a, str>> {
        Some(Cow::Borrowed(text))
    }

    // -- Watching ---------------------------------------------------------------------------------

    /// Starts the watch a hook event asked for (`crate::hooks::HookInterpreter::record_watch`), for
    /// what the provider's hooks will not report. Spawned, not awaited.
    fn arm_watch(&self, _state: &Arc<AppState>, _session_id: &str, _watch: RecordWatch<'_>) {}
}

/// The record store of a provider that keeps no records Octoboard reads.
pub struct NoRecords;

impl RecordStore for NoRecords {}

/// Of `records`, relative to `root`, the one written to last among those whose metadata `is`
/// accepts, links not followed.
pub(crate) fn newest_existing(
    root: &Path,
    records: &[PathBuf],
    is: impl Fn(&std::fs::Metadata) -> bool,
) -> Option<PathBuf> {
    records
        .iter()
        .filter(|relative| {
            std::fs::symlink_metadata(root.join(relative)).is_ok_and(|meta| is(&meta))
        })
        .max_by_key(|relative| crate::hostfs::modified(&root.join(relative)))
        .cloned()
}
