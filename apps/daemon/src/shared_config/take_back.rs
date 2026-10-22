//! Taking an account's own conversations back out of the default directory when it stops sharing,
//! so its sessions, archived ones included, stay resumable under the account. Every file the
//! agent keeps of a conversation goes to the same path relative to the account's directory; a
//! conversation another session records too, outside the account, is copied and left where it is
//! for that session.

use std::path::{Component, Path, PathBuf};

use anyhow::Result;

use super::codex_state;
use super::link::{Cause, Dirs, Failure, Left};
use super::moves::{backup_path, is_link, lstat, merge_copy, merge_move, resolve};
use crate::account_dirs::account_dir_key;
use crate::protocol::{Account, Agent};
use crate::relocate::is_state_database;
use crate::shared_items::shared_items;
use crate::state::AppState;

/// A conversation of the account's own sessions, by the agent's id of it.
pub(super) struct Conversation {
    pub agent_session_id: String,
    /// The Octoboard sessions on record in the account's directory that record it.
    pub sessions: Vec<String>,
    /// Whether a session outside the account records it too, which reaches it in the default
    /// directory: it is then copied back rather than moved.
    pub held_elsewhere: bool,
}

/// The conversations of every session on record in `account`'s directory, archived ones
/// included. A session that holds another directory resumes from that one, and is left out.
pub(super) fn own_conversations(state: &AppState, account: &Account) -> Result<Vec<Conversation>> {
    let key = account_dir_key(&account.config_dir);
    let sessions = state.store.list_sessions()?;
    let in_account = |session: &crate::protocol::Session| {
        session
            .config_dir
            .as_deref()
            .is_some_and(|dir| account_dir_key(dir) == key)
    };
    let mut conversations: Vec<Conversation> = Vec::new();
    for session in sessions
        .iter()
        .filter(|session| session.agent == account.agent && in_account(session))
    {
        let Some(id) = session.agent_session_id.clone() else {
            continue;
        };
        if let Some(conversation) = conversations
            .iter_mut()
            .find(|conversation| conversation.agent_session_id == id)
        {
            conversation.sessions.push(session.id.clone());
            continue;
        }
        let held_elsewhere = sessions.iter().any(|other| {
            other.agent == account.agent
                && other.agent_session_id.as_deref() == Some(id.as_str())
                && !in_account(other)
        });
        conversations.push(Conversation {
            agent_session_id: id,
            sessions: vec![session.id.clone()],
            held_elsewhere,
        });
    }
    Ok(conversations)
}

/// Takes `conversation` back from the default directory into the account's own conversation
/// directories. Where the account already holds a file of it, the one written to last stays and
/// the other is kept under `<directory>.bak-<stamp>/` in the account's directory. A Codex thread
/// moved out loses its row in the default's database, which would otherwise name a rollout no
/// longer there, also when its files left on an earlier attempt, but never while no file of it is
/// found; the account's own database, when it has one, is pointed at the head taken back. Nothing
/// is taken back while a directory it would land in is still linked into the default directory.
pub(super) fn take_back(
    locator: &mut crate::agent_records::Locator,
    agent: Agent,
    dirs: &Dirs,
    conversation: &Conversation,
    stamp: &str,
) -> Result<(), Failure> {
    let id = conversation.agent_session_id.as_str();
    let failure = |left: Left, cause: Cause, detail: String| Failure {
        item: id.to_string(),
        left,
        cause,
        detail,
        sessions: conversation.sessions.clone(),
    };
    let files: Vec<PathBuf> =
        crate::agent_records::conversation_files_in(locator, agent, &dirs.default, id)
            .into_iter()
            .filter_map(|path| path.strip_prefix(&dirs.default).ok().map(Path::to_path_buf))
            .collect();
    let mut asides: Vec<(PathBuf, PathBuf)> = Vec::new();
    for relative in &files {
        let Some(Component::Normal(root)) = relative.components().next() else {
            continue;
        };
        let root = dirs.account.join(root);
        match lstat(&root) {
            Ok(present) if !is_link(&present) && present.as_ref().is_some_and(|m| m.is_dir()) => {}
            Ok(_) => {
                return Err(failure(
                    Left::Unchanged,
                    Cause::Conflict,
                    format!(
                        "left in the default directory: `{}` is not a directory of the \
                         account's own",
                        root.display()
                    ),
                ))
            }
            Err(err) => return Err(failure(Left::Unchanged, Cause::of(&err), err.to_string())),
        }
        if asides.iter().all(|(seen, _)| *seen != root) {
            let aside = backup_path(&root, stamp);
            asides.push((root, aside));
        }
    }
    for (taken, relative) in files.iter().enumerate() {
        let (from, to) = (dirs.default.join(relative), dirs.account.join(relative));
        let aside = asides
            .iter()
            .find(|(root, _)| to.starts_with(root))
            .and_then(|(root, aside)| to.strip_prefix(root).ok().map(|rest| aside.join(rest)))
            .unwrap_or_else(|| backup_path(&to, stamp));
        let carried = if conversation.held_elsewhere {
            merge_copy(&from, &to, &aside)
        } else {
            merge_move(&from, &to, &aside)
        };
        if let Err(err) = carried {
            let left = if taken == 0 {
                Left::Unchanged
            } else {
                Left::PartlyMoved
            };
            return Err(failure(
                left,
                Cause::of(&err),
                format!("`{}` not taken back: {err}", relative.display()),
            ));
        }
    }
    if agent != Agent::Codex {
        return Ok(());
    }
    copy_thread_names(dirs, id);
    let mut problems = Vec::new();
    // A row goes only once the thread's rollouts are known to be the account's: taken back now,
    // or on an earlier attempt. With none found anywhere, the row is the only trace left of where
    // they were, and stays.
    let taken = !files.is_empty()
        || !crate::agent_records::conversation_files_in(
            &mut crate::agent_records::Locator::default(),
            agent,
            &dirs.account,
            id,
        )
        .is_empty();
    if taken && !conversation.held_elsewhere {
        for item in shared_items(agent)
            .iter()
            .filter(|item| is_state_database(item.name))
        {
            if let Ok(Some(database)) = resolve(&dirs.default.join(item.name)) {
                if let Err(err) = codex_state::forget_threads(&database, &[id]) {
                    problems.push((
                        Cause::of_database(&err),
                        format!("the default's `{}` still names it: {err}", item.name),
                    ));
                }
            }
        }
    }
    if let Ok(head) = crate::relocate::locate(agent, &dirs.account, id) {
        let env = std::collections::HashMap::from([(
            "HOME".to_string(),
            crate::paths::home_dir().to_string_lossy().into_owned(),
        )]);
        if let Err(err) = crate::relocate::repoint_thread_row(agent, &dirs.account, &head, id, &env)
        {
            let cause = err
                .downcast_ref::<rusqlite::Error>()
                .map_or(Cause::Other, Cause::of_database);
            problems.push((
                cause,
                format!("the account's own database was not pointed at it: {err}"),
            ));
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    let cause = Cause::shared(problems.iter().map(|(cause, _)| *cause));
    let detail: Vec<String> = problems.iter().map(|(_, detail)| detail.clone()).collect();
    Err(failure(Left::PartlyMoved, cause, detail.join("; ")))
}

/// Copies the default's thread-name entries of Codex thread `id` into the account's own index, so
/// a resume of the thread by name still finds it there. Best effort: a name is not needed to
/// resume by id, which is how a session is resumed.
fn copy_thread_names(dirs: &Dirs, id: &str) {
    use std::io::Write;
    let Ok(index) = std::fs::read_to_string(dirs.default.join("session_index.jsonl")) else {
        return;
    };
    let lines: String = index
        .lines()
        .filter(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .is_ok_and(|entry| entry.get("id").and_then(|value| value.as_str()) == Some(id))
        })
        .map(|line| format!("{line}\n"))
        .collect();
    if lines.is_empty() {
        return;
    }
    let target = dirs.account.join("session_index.jsonl");
    let already = std::fs::read_to_string(&target).unwrap_or_default();
    if already.contains(&lines) {
        return;
    }
    let written = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)
        .and_then(|mut file| file.write_all(lines.as_bytes()));
    if let Err(err) = written {
        tracing::warn!(thread = %id, %err, "copying a Codex thread's name into the account failed");
    }
}
