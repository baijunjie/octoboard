//! Taking an account's own conversations back out of the default directory when it stops sharing,
//! so its sessions, archived ones included, stay resumable under the account. Every file the
//! agent keeps of a conversation goes to the same path relative to the account's directory; a
//! conversation another session records too, outside the account, is copied and left where it is
//! for that session.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use anyhow::Result;

use super::link::Dirs;
use super::moves::{backup_path, is_link, lstat, merge_copy, merge_move, resolve};
use crate::accounts::shared_config::items::{shared_items, ItemClass};
use crate::accounts::thread_registry::{thread_registry, RowCause, RowError};
use crate::hostfs::dir_key;
use crate::protocol::{Account, AgentId, SharingCause, SharingItemFailure, SharingLeft};
use crate::state::AppState;

/// A conversation of the account's own sessions, by the agent's id of it.
pub(crate) struct Conversation {
    pub agent_session_id: String,
    /// The Octoboard sessions on record in the account's directory that record it.
    pub sessions: Vec<String>,
    /// Whether a session outside the account records it too, which reaches it in the default
    /// directory: it is then copied back rather than moved.
    pub held_elsewhere: bool,
}

/// The conversations of every session on record in `account`'s directory, archived ones
/// included. A session that holds another directory resumes from that one, and is left out.
pub(crate) fn own_conversations(state: &AppState, account: &Account) -> Result<Vec<Conversation>> {
    let key = dir_key(&account.config_dir);
    let sessions = state.store.list_sessions()?;
    let in_account = |session: &crate::protocol::Session| {
        session
            .config_dir
            .as_deref()
            .is_some_and(|dir| dir_key(dir) == key)
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
/// directories. Where the account already holds a file of it, the one written to last stays and the
/// other is kept under `<directory>.bak-<stamp>/` in the account's directory. A thread moved out
/// loses its row in the default's thread database, which would otherwise name a record no longer
/// there, also when its files left on an earlier attempt, but never while no file of it is found;
/// the account's own database, when it has one, is pointed at the head taken back, found where
/// `launch_env` (the environment the account's sessions launch with, or why it could not be had)
/// and the account's `config.toml` put it. Nothing is taken back while a directory it would land in
/// is still linked into the default directory.
pub(super) fn take_back(
    locator: &mut crate::records::Locator,
    agent: AgentId,
    dirs: &Dirs,
    conversation: &Conversation,
    stamp: &str,
    launch_env: Result<&HashMap<String, String>, &str>,
) -> Result<(), SharingItemFailure> {
    let id = conversation.agent_session_id.as_str();
    let failure = |left: SharingLeft, cause: SharingCause, detail: String| SharingItemFailure {
        item: id.to_string(),
        left,
        cause,
        detail,
        sessions: conversation.sessions.clone(),
    };
    let files: Vec<PathBuf> =
        crate::records::conversation_files_in(locator, agent, &dirs.default, id)
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
                    SharingLeft::Unchanged,
                    SharingCause::Conflict,
                    format!(
                        "left in the default directory: `{}` is not a directory of the \
                         account's own",
                        root.display()
                    ),
                ))
            }
            Err(err) => {
                return Err(failure(
                    SharingLeft::Unchanged,
                    SharingCause::of(&err),
                    err.to_string(),
                ))
            }
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
                SharingLeft::Unchanged
            } else {
                SharingLeft::PartlyMoved
            };
            return Err(failure(
                left,
                SharingCause::of(&err),
                format!("`{}` not taken back: {err}", relative.display()),
            ));
        }
    }
    let Some(threads) = thread_registry(agent) else {
        return Ok(());
    };
    threads.after_take_back(&dirs.default, &dirs.account, id);
    let mut problems = Vec::new();
    // A row goes only once the thread's records are known to be the account's: taken back now,
    // or on an earlier attempt. With none found anywhere, the row is the only trace left of where
    // they were, and stays.
    let taken = !files.is_empty()
        || !crate::records::conversation_files_in(
            &mut crate::records::Locator::default(),
            agent,
            &dirs.account,
            id,
        )
        .is_empty();
    if taken && !conversation.held_elsewhere {
        for item in shared_items(agent)
            .iter()
            .filter(|item| item.class == ItemClass::ThreadDatabase)
        {
            if let Ok(Some(database)) = resolve(&dirs.default.join(item.name)) {
                if let Err(err) = threads.forget_threads(&database, &[id]) {
                    problems.push((
                        SharingCause::of_database(&err),
                        format!("the default's `{}` still names it: {err}", item.name),
                    ));
                }
            }
        }
    }
    if let Ok(head) = crate::accounts::relocate::locate(agent, &dirs.account, id) {
        let pointed = launch_env
            .map_err(|err| (SharingCause::Other, format!("the shell environment: {err}")))
            .and_then(|env| {
                threads
                    .point_thread_at(
                        &dirs.account,
                        &dirs.account.join(head),
                        id,
                        env,
                        threads.patience(),
                    )
                    .map_err(|err| (cause_of_row(&err), err.to_string()))
            });
        if let Err((cause, err)) = pointed {
            problems.push((
                cause,
                format!("the account's own database was not pointed at it: {err}"),
            ));
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    let cause = SharingCause::shared(problems.iter().map(|(cause, _)| *cause));
    let detail: Vec<String> = problems.iter().map(|(_, detail)| detail.clone()).collect();
    Err(failure(SharingLeft::PartlyMoved, cause, detail.join("; ")))
}

/// Why the account's own row of a thread was not pointed at the head taken back.
fn cause_of_row(err: &RowError) -> SharingCause {
    match err.cause {
        RowCause::Configuration(cause) | RowCause::Registry(cause) => cause,
        RowCause::Unlocatable => SharingCause::Other,
    }
}
