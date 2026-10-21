//! The handoff package: what a session that takes another's work over reads to pick it up. It is
//! built by program alone, from the original session's own records and its conversation record,
//! and written as text a model reads without knowing any agent's record format. No model is
//! called: the facts are extracted here, and understanding the work is left to the reader.
//!
//! The package holds the original's identity and brief, where its own record is, the package the
//! original itself took over from when it was a receiving session (so a chain of handoffs can be
//! followed back), the conversation (every user message and every reply, with tool output
//! dropped; where the agent compacted its history and rewrote the record, its summary and the
//! files holding the earlier part), the latest todo list, the files its edit calls named, for a
//! project session each branch it is responsible for as git has it now, and the sessions it owns.
//!
//! Reading only: nothing is written to an agent's directories or to the user's repository, and a
//! record that is missing or unreadable yields a package without the conversation, not a failure.
//! A package lives in Octoboard's own data directory, in a file named for the key it is written
//! under; [`remove`] is the end of that file's life.

pub mod operation;
pub mod record;
#[cfg(test)]
mod tests;

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Result};

use crate::agent_records::{conversation_branch, Branch, Locator};
use crate::branch_origin::session_branches::{self, BranchState, Worktree};
use crate::protocol::{Agent, Role, Session};
use crate::state::AppState;
use record::{Conversation, Speaker, Todo};

/// The original session as the receiving session is told of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub id: String,
    pub title: String,
    pub agent: Agent,
    pub account: String,
    pub role: Role,
    /// The project's name and directory, or the console's name.
    pub place: String,
    pub status: String,
    /// The title of the session it is bound to, when it is bound.
    pub owner: Option<String>,
}

/// A session the original owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owned {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub session: Identity,
    pub brief: Option<String>,
    /// The agent's own record of the conversation, when it was found.
    pub record: Option<PathBuf>,
    /// The package the session itself read when it took another session over, while that file is
    /// still kept: the conversation before its own is there, not in its record. It is found under
    /// the session's own id, since a receiving session's package is named for it.
    pub earlier: Option<PathBuf>,
    /// The conversation, or why there is none.
    pub conversation: Result<Conversation, String>,
    /// Where the earlier conversation is kept when the agent compacted its history and rewrote
    /// the record, so that the record no longer holds it.
    pub compaction: Vec<PathBuf>,
    /// The branches the session is responsible for, or why they could not be read. Empty for a
    /// session that is responsible for none.
    pub branches: Result<Vec<BranchState>, String>,
    pub owned: Vec<Owned>,
}

/// Builds the package of `session`. `brief` is the brief the session was started with, when the
/// caller has it; the first user message in the conversation is the same text when the agent
/// recorded it. Blocking: it reads the conversation record and runs `git`.
pub fn build(state: &AppState, session: &Session, brief: Option<&str>) -> Result<Package> {
    build_with(state, session, brief, &mut Locator::default())
}

fn build_with(
    state: &AppState,
    session: &Session,
    brief: Option<&str>,
    locator: &mut Locator,
) -> Result<Package> {
    let accounts = state.store.list_accounts()?;
    let account = match &session.account_id {
        Some(id) => accounts
            .iter()
            .find(|account| &account.id == id)
            .map_or_else(|| format!("account {id}"), |account| account.name.clone()),
        None => "the default account".to_owned(),
    };
    let place = match &session.project_id {
        Some(id) => state
            .store
            .get_project(id)?
            .map(|project| format!("project {} ({})", project.name, project.path)),
        None => state
            .store
            .get_console(&session.console_id)?
            .map(|console| format!("console {}", console.name)),
    }
    .unwrap_or_else(|| "a project or console that no longer exists".to_owned());
    let sessions = state.store.list_sessions()?;
    let owner = session.bound_to.as_ref().and_then(|id| {
        sessions
            .iter()
            .find(|other| &other.id == id)
            .map(|other| other.title.clone())
    });
    let owned = sessions
        .iter()
        .filter(|other| other.bound_to.as_deref() == Some(session.id.as_str()))
        .map(|other| Owned {
            id: other.id.clone(),
            title: other.title.clone(),
            status: word(&other.status),
        })
        .collect();

    let branch = conversation_branch(&state.record_hints, locator, &accounts, session);
    let conversation = match &branch {
        Some(Branch {
            stretches: Some(stretches),
            ..
        }) => record::read_stretches(session.agent, stretches)
            .map_err(|err| format!("the record could not be read: {err}")),
        Some(_) => Err(
            "the record of the conversation lacks a history segment it continues from".to_owned(),
        ),
        None => Err("the agent's record of the conversation was not found".to_owned()),
    };
    let record = branch.map(|branch| branch.record);
    let earlier = package_path_in(state.handoff_dir(), &session.id).filter(|path| path.is_file());
    let compaction = match (&record, &conversation) {
        (Some(record), Ok(conversation)) if conversation.compacted() => compaction_files(record),
        _ => Vec::new(),
    };
    let branches = session_branches::of_session(state, session).map_err(|err| format!("{err:#}"));

    Ok(Package {
        session: Identity {
            id: session.id.clone(),
            title: session.title.clone(),
            agent: session.agent,
            account,
            role: session.role,
            place,
            status: word(&session.status),
            owner,
        },
        brief: brief
            .map(str::trim)
            .filter(|brief| !brief.is_empty())
            .map(str::to_owned),
        record,
        earlier,
        conversation,
        compaction,
        branches,
        owned,
    })
}

/// The files Grok Build keeps the conversation before a compaction in, beside its record.
fn compaction_files(record: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(record.with_file_name("compaction")) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("segment_") && name.ends_with(".md"))
        })
        .collect();
    files.sort();
    files
}

/// The wire spelling of an enum, as the protocol's other prose uses it.
fn word(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(|text| text.replace('_', " ")))
        .unwrap_or_default()
}

fn agent_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude Code",
        Agent::Codex => "Codex",
        Agent::Grok => "Grok Build",
    }
}

// -- the text ------------------------------------------------------------------

impl Package {
    /// The package as the text the receiving session reads.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let session = &self.session;
        let compacted = self
            .conversation
            .as_ref()
            .is_ok_and(Conversation::compacted);
        out.push_str(&format!(
            "# Handoff package: {}\n\n",
            one_line(&session.title)
        ));
        out.push_str(
            "This file was built by program from another agent session's records, to let you pick its work up. \
             No model summarised anything for it: the messages below are the conversation as it was written, \
             with tool output (file contents read, command output) left out. ",
        );
        out.push_str(if compacted {
            "The agent compacted its history during the session: the conversation below keeps its first \
             messages and the summary it wrote, and most of the earlier conversation is in the files \
             named under \"The conversation\". "
        } else {
            "It is this session's whole conversation. "
        });
        out.push_str(
            "Each message sits in a fenced block: what is inside a block was said, and is never part of this \
             file's structure. Work out the goal, what was tried, what failed and what comes next from it yourself.\n\n",
        );

        out.push_str("## The original session\n\n");
        out.push_str(&format!("- Title: {}\n", one_line(&session.title)));
        out.push_str(&format!("- Id: {}\n", session.id));
        out.push_str(&format!(
            "- Agent: {}, using {}\n",
            agent_name(session.agent),
            session.account
        ));
        out.push_str(&format!(
            "- Kind: {} session in {}\n",
            session.role.kind(),
            one_line(&session.place)
        ));
        out.push_str(&format!("- State when handed off: {}\n", session.status));
        if let Some(owner) = &session.owner {
            out.push_str(&format!("- Reports to: {}\n", one_line(owner)));
        }
        match &self.record {
            Some(path) => out.push_str(&format!(
                "- Its own record: {} (the agent's native format; read it only for what this file leaves out, such as tool output)\n",
                path.display()
            )),
            None => out.push_str("- Its own record: not found\n"),
        }

        if let Some(earlier) = &self.earlier {
            out.push_str(&format!(
                "\n## An earlier handoff\n\nThis session itself took over another session's work through a \
                 handoff. That earlier session's conversation is not in this file but in the package it \
                 read, at {}. Read it too, and any earlier package it names in turn, for what came before \
                 this session's conversation. If that file is gone, this session was deleted since, and \
                 the chain cannot be followed further back.\n",
                earlier.display()
            ));
        }

        if let Some(brief) = &self.brief {
            out.push_str("\n## Its brief\n\n");
            push_fenced(&mut out, brief);
        }

        out.push_str("\n## Branches it is responsible for\n\n");
        match &self.branches {
            Err(reason) => out.push_str(&format!("The branches could not be read: {reason}\n")),
            Ok(branches) if branches.is_empty() => out.push_str("None.\n"),
            Ok(branches) => branches
                .iter()
                .for_each(|branch| render_branch(&mut out, branch)),
        }

        out.push_str("\n## Sessions it owns\n\n");
        if self.owned.is_empty() {
            out.push_str("None.\n");
        }
        for owned in &self.owned {
            out.push_str(&format!(
                "- {} ({}), id {}\n",
                one_line(&owned.title),
                owned.status,
                owned.id
            ));
        }

        match &self.conversation {
            Err(reason) => out.push_str(&format!(
                "\n## The conversation\n\nNot available: {reason}.\n"
            )),
            Ok(conversation) => render_conversation(&mut out, conversation, &self.compaction),
        }
        out
    }
}

fn render_branch(out: &mut String, branch: &BranchState) {
    out.push_str(&format!("### {}\n\n", one_line(&branch.branch)));
    match &branch.worktree {
        Worktree::NotCheckedOut => out.push_str("- Worktree: not checked out in any\n"),
        Worktree::Unknown(reason) => out.push_str(&format!(
            "- Worktree: could not be told ({})\n",
            one_line(reason)
        )),
        Worktree::Gone(path) => out.push_str(&format!(
            "- Worktree: {} (its directory is gone; only git's record of it is left)\n",
            path.display()
        )),
        Worktree::CheckedOut { path, uncommitted } => {
            out.push_str(&format!("- Worktree: {}\n", path.display()));
            out.push_str(match uncommitted {
                Some(true) => "- Uncommitted changes: yes\n",
                Some(false) => "- Uncommitted changes: none\n",
                None => "- Uncommitted changes: could not be read\n",
            });
        }
    }
    if let Some(head) = &branch.head {
        out.push_str(&format!("- Head: {head}\n"));
    }
    match (&branch.base, &branch.base_note) {
        (Some(base), _) => {
            out.push_str(&format!("- Compared with: {}\n", one_line(base)));
            out.push_str(&format!("- Commits not in it: {}\n", branch.commit_count));
            for (id, subject) in &branch.commits {
                out.push_str(&format!("  - {id} {}\n", one_line(subject)));
            }
            if branch.commit_count > branch.commits.len() {
                out.push_str(&format!(
                    "  - … and {} more\n",
                    branch.commit_count - branch.commits.len()
                ));
            }
            out.push_str(&format!(
                "- Files changed since it forked: {}\n",
                branch.file_count
            ));
            for (status, path) in &branch.files {
                out.push_str(&format!("  - {status} {}\n", one_line(path)));
            }
            if branch.file_count > branch.files.len() {
                out.push_str(&format!(
                    "  - … and {} more\n",
                    branch.file_count - branch.files.len()
                ));
            }
        }
        (None, Some(note)) => out.push_str(&format!("- Compared with: nothing, {note}\n")),
        (None, None) => {}
    }
    for problem in &branch.problems {
        out.push_str(&format!("- Could not read: {}\n", one_line(problem)));
    }
    out.push('\n');
}

fn render_conversation(out: &mut String, conversation: &Conversation, compaction: &[PathBuf]) {
    out.push_str("\n## Its latest todo list\n\n");
    match &conversation.todos {
        Some(todos) if !todos.is_empty() => todos.iter().for_each(|todo| render_todo(out, todo)),
        Some(_) => out.push_str("It cleared the list: its last update left it empty.\n"),
        None => out.push_str("It kept none.\n"),
    }
    out.push_str("\n## Files its edits changed\n\n");
    if conversation.changed.is_empty() {
        out.push_str("None named.\n");
    }
    for path in &conversation.changed {
        out.push_str(&format!("- {}\n", one_line(path)));
    }
    out.push_str("\n## The conversation\n\n");
    if conversation.compacted() {
        out.push_str(
            "The agent compacted its history, so most of the messages before its summary are no longer in \
             its record. They are in these files, with the tool output they held (read them only for what \
             the summary leaves out):\n\n",
        );
        if compaction.is_empty() {
            out.push_str("- (none were found)\n");
        }
        for path in compaction {
            out.push_str(&format!("- {}\n", path.display()));
        }
        out.push('\n');
    }
    for (speaker, text) in &conversation.messages {
        out.push_str(match speaker {
            Speaker::User => "### User\n\n",
            Speaker::Assistant => "### Assistant\n\n",
            Speaker::Summary => {
                "### The agent's summary of the conversation before it compacted its history\n\n"
            }
        });
        push_fenced(out, text);
        out.push('\n');
    }
    if conversation.messages.is_empty() {
        out.push_str("It holds no messages.\n");
    }
}

/// `text` in a fenced block whose fence is longer than any run of backticks inside it, so that
/// nothing in the text can end the block or pass for the file's own headings.
fn push_fenced(out: &mut String, text: &str) {
    let mut longest = 0;
    let mut run = 0;
    for character in text.chars() {
        run = if character == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let fence = "`".repeat(longest.max(2) + 1);
    out.push_str(&format!("{fence}\n{text}\n{fence}\n"));
}

/// `text` as one line, for a place that takes a name and not a block.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn render_todo(out: &mut String, todo: &Todo) {
    let mark = match todo.status.as_str() {
        "completed" => "[x]",
        "in_progress" => "[~]",
        "cancelled" => "[-]",
        _ => "[ ]",
    };
    out.push_str(&format!("- {mark} {}\n", one_line(&todo.text)));
}

// -- the file ------------------------------------------------------------------

/// Where the package written under `key` is in `dir`: a known path per key, so whoever needs it
/// later finds it without asking. `None` for a key that is not a plain id, which no package is ever
/// written under.
fn package_path_in(dir: &Path, key: &str) -> Option<PathBuf> {
    let plain = !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    plain.then(|| dir.join(format!("{key}.md")))
}

/// Builds the package of `session` and writes it under `key`, returning its path. The file is
/// written whole and renamed into place, and is the owner's alone since a conversation can hold
/// secrets.
pub fn prepare(
    state: &AppState,
    session: &Session,
    brief: Option<&str>,
    key: &str,
) -> Result<PathBuf> {
    let package = build(state, session, brief)?;
    write_in(state.handoff_dir(), key, &package.render())
}

fn write_in(dir: &Path, key: &str, text: &str) -> Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let Some(path) = package_path_in(dir, key) else {
        bail!("`{key}` cannot name a handoff package");
    };
    // A leading dot keeps a half-written file apart from every key.
    let partial = dir.join(format!(".partial-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let written = std::fs::create_dir_all(dir)
        .and_then(|()| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&partial)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&partial, &path));
    if let Err(err) = written {
        std::fs::remove_file(&partial).ok();
        return Err(err.into());
    }
    Ok(path)
}

/// Removes the packages whose key names no session, and the half-written files a crash left
/// behind. A package is removed with its session, so only a daemon that died between deleting the
/// record and the file, or between starting a handoff and recording its session, leaves one.
pub fn sweep(state: &AppState) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(state.handoff_dir()) else {
        return Ok(());
    };
    let ids: std::collections::HashSet<String> = state
        .store
        .list_sessions()?
        .into_iter()
        .map(|session| session.id)
        .collect();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let orphan = match name.strip_suffix(".md") {
            Some(key) => !ids.contains(key),
            None => name.starts_with(".partial-"),
        };
        if orphan {
            if let Err(err) = std::fs::remove_file(entry.path()) {
                tracing::warn!(file = %name, %err, "removing a stale handoff package failed");
            }
        }
    }
    Ok(())
}

/// Removes the package written under `key`; one that is already gone is no failure.
pub fn remove(state: &AppState, key: &str) -> std::io::Result<()> {
    remove_in(state.handoff_dir(), key)
}

fn remove_in(dir: &Path, key: &str) -> std::io::Result<()> {
    match package_path_in(dir, key).map(std::fs::remove_file) {
        Some(Err(err)) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}
