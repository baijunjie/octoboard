//! Whether a session's conversation can still be resumed, and whether its records are there at all,
//! judged from the agent's own records in the session's account.

use std::path::Path;

use crate::agents::layout::{
    layout, CLAUDE_TRANSCRIPT_SUFFIX, CODEX, CODEX_ARCHIVED_DIR, CODEX_RECORDS_DIR,
};
use crate::hostfs::lexically_normalise;
use crate::protocol::{Account, Agent, Session};
use crate::records::codex_rollout_files::is_rollout_of;
use crate::records::locate::{account_roots, plausible_id, walk_records, Hints, Locator, Step};
use crate::records::removal::removable_record_paths;

/// Whether all known account locations can prove the absence of the native records deleting this
/// session would remove ([`removable_record_paths`]). Missing ids and unreadable or linked
/// directories cannot establish absence.
pub fn presence(
    hints: &Hints,
    locator: &mut Locator,
    accounts: &[Account],
    session: &Session,
) -> crate::protocol::SessionRecordPresence {
    use crate::protocol::SessionRecordPresence::{Absent, Present, Unknown};
    if !session
        .agent_session_id
        .as_deref()
        .is_some_and(plausible_id)
    {
        return Unknown;
    }
    let (roots, mut complete) = account_roots(locator, accounts, session);
    for root in &roots {
        complete &= locator.index(session.agent, root).complete;
    }
    for path in removable_record_paths(hints, locator, accounts, session) {
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_file() || meta.file_type().is_dir() => return Present,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            _ => complete = false,
        }
    }
    if complete {
        Absent
    } else {
        Unknown
    }
}

/// Whether a session's conversation can still be resumed, judged from the agent's own records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resumability {
    Resumable,
    /// Proven gone: the record does not exist in the account that holds it. Only this answer may
    /// be acted on destructively.
    Unresumable,
    /// Not provable either way: a location that cannot be resolved or read (a pinned account
    /// directory included), a linked records directory, or a Grok session folder missing locally,
    /// which Grok restores from its remote registry.
    Unknown,
}

/// What [`judge`] found for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Judgement {
    pub resumability: Resumability,
    /// Resuming first needs `codex unarchive`: the rollout sits in the account's
    /// `archived_sessions/`, which a resume refuses, and `sessions/` does not show it present.
    pub needs_unarchive: bool,
}

/// Where a session's record sits in its own account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Located {
    Present,
    /// Codex's `archived_sessions/`.
    Archived,
    Missing,
    Uncertain,
}

/// Whether `session`'s conversation can be resumed, looking only at the session's own account
/// (`Session.config_dir`, the agent's default account when none) and its current agent session id.
/// A copy left in another account by an earlier switch does not count. A session with no
/// conversation always is: it relaunches into a fresh one.
///
/// Claude Code needs `<id>.jsonl` in some project directory (the working directory's slug is not
/// required); a subagent directory alone is not a record. Codex needs a rollout, plain or `.zst`,
/// under `sessions/` or `archived_sessions/`; any file of the thread's chain (its first rollout or
/// a later history segment) counts, so a thread is never judged gone while a part of it is there.
/// Grok needs its session directory, and its absence is only ever `Unknown`. File names are
/// matched whatever the case of the id.
pub fn resumability(locator: &Locator, session: &Session) -> Resumability {
    judge(locator, session).resumability
}

/// [`resumability`], with whether resuming first needs `codex unarchive`, from one look at the
/// account's records.
///
/// An account sharing the default configuration reaches its Codex rollouts through linked
/// `sessions/` and `archived_sessions/`, which leave the verdict `Unknown`, so that nothing is
/// deleted on it. Whether a rollout Codex archived has to be brought back before a resume is
/// still decided, from where those links lead.
pub fn judge(locator: &Locator, session: &Session) -> Judgement {
    let (resumability, needs_unarchive) = match locate_own_record(locator, session) {
        Err(answer) => (answer, false),
        Ok(Located::Present) => (Resumability::Resumable, false),
        Ok(Located::Archived) => (Resumability::Resumable, true),
        Ok(Located::Missing) if session.agent == Agent::Grok => (Resumability::Unknown, false),
        Ok(Located::Missing) => (Resumability::Unresumable, false),
        Ok(Located::Uncertain) => (
            Resumability::Unknown,
            archived_behind_links(locator, session),
        ),
    };
    Judgement {
        resumability,
        needs_unarchive,
    }
}

/// Whether `session`'s Codex rollout sits only in `archived_sessions/` when the account's
/// `sessions/` or `archived_sessions/` is a link, as an account sharing the default configuration
/// has them: each linked one is searched where it leads, by the same rules as any records
/// directory (exact names, no link followed below it).
fn archived_behind_links(locator: &Locator, session: &Session) -> bool {
    let Some(agent_id) = session
        .agent_session_id
        .as_deref()
        .filter(|id| session.agent == Agent::Codex && session.has_conversation && plausible_id(id))
        .map(str::to_ascii_lowercase)
    else {
        return false;
    };
    let Some(root) = locator.root(Agent::Codex, session.config_dir.as_deref()) else {
        return false;
    };
    let root = lexically_normalise(&root);
    let dirs = [root.join(CODEX_RECORDS_DIR), root.join(CODEX_ARCHIVED_DIR)];
    let is_link =
        |dir: &Path| std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.file_type().is_symlink());
    if !dirs.iter().any(|dir| is_link(dir)) {
        return false;
    }
    let is_record = |name: &str, kind: std::fs::FileType| {
        is_rollout_of(kind, &name.to_ascii_lowercase(), &agent_id)
    };
    let found = |dir: &Path| {
        let dir = if is_link(dir) {
            match std::fs::canonicalize(dir) {
                Ok(resolved) => resolved,
                Err(_) => return Located::Uncertain,
            }
        } else {
            dir.to_path_buf()
        };
        scan_for_record(&dir, CODEX.search_depth, &is_record)
    };
    found(&dirs[0]) == Located::Missing && found(&dirs[1]) == Located::Present
}

/// `Err` carries the answer when the record need not, or cannot, be looked for.
fn locate_own_record(locator: &Locator, session: &Session) -> Result<Located, Resumability> {
    if !session.has_conversation {
        return Err(Resumability::Resumable);
    }
    let agent = session.agent;
    let Some(agent_id) = session
        .agent_session_id
        .as_deref()
        .filter(|id| plausible_id(id))
    else {
        return Err(Resumability::Unknown);
    };
    let Some(root) = locator.root(agent, session.config_dir.as_deref()) else {
        return Err(Resumability::Unknown);
    };
    let root = lexically_normalise(&root);
    if !root.is_absolute() {
        return Err(Resumability::Unknown);
    }
    // An account directory that cannot be read (gone, unmounted, denied) says nothing about the
    // records in it.
    if std::fs::read_dir(&root).is_err() {
        return Err(Resumability::Unknown);
    }
    let layout = layout(agent);
    let dirs: Vec<&str> = std::iter::once(layout.records_dir)
        .chain(layout.archived_records_dir)
        .collect();
    let depth = layout.search_depth;
    // An account directory with none of the agent's record directories (an unmounted mount point
    // leaves an empty one) has never shown any record to be missing from.
    if !dirs
        .iter()
        .any(|dir| std::fs::symlink_metadata(root.join(dir)).is_ok())
    {
        return Err(Resumability::Unknown);
    }
    let agent_id = agent_id.to_ascii_lowercase();
    let is_record = |name: &str, kind: std::fs::FileType| {
        let name = name.to_ascii_lowercase();
        match agent {
            Agent::Claude => {
                kind.is_file() && name == format!("{agent_id}{CLAUDE_TRANSCRIPT_SUFFIX}")
            }
            Agent::Codex => is_rollout_of(kind, &name, &agent_id),
            Agent::Grok => kind.is_dir() && name == agent_id,
        }
    };
    // The first directory is the one a resume reads; a record found only in a later one is
    // archived. A directory that cannot be searched does not stop the next from being.
    let (mut archived, mut uncertain) = (false, false);
    for (at, dir) in dirs.iter().enumerate() {
        match scan_for_record(&root.join(dir), depth, &is_record) {
            Located::Present if at == 0 => return Ok(Located::Present),
            Located::Present => archived = true,
            Located::Uncertain => uncertain = true,
            _ => {}
        }
    }
    Ok(if archived {
        Located::Archived
    } else if uncertain {
        Located::Uncertain
    } else {
        Located::Missing
    })
}

/// Looks for an entry `is_record` accepts in `dir`, at most `depth` levels down, without following
/// a link. A link anywhere, or anything unreadable, leaves a record that is not found `Uncertain`
/// rather than `Missing`, since it could be behind that.
fn scan_for_record(
    dir: &Path,
    depth: usize,
    is_record: &dyn Fn(&str, std::fs::FileType) -> bool,
) -> Located {
    let walked = walk_records(dir, depth, &mut |_, name, kind, _| {
        if is_record(name, kind) {
            Step::Stop
        } else {
            Step::Descend
        }
    });
    if walked.stopped {
        Located::Present
    } else if walked.complete {
        Located::Missing
    } else {
        Located::Uncertain
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::records::fixtures::*;

    #[test]
    fn native_presence_requires_a_complete_scan_and_counts_orphan_directories() {
        use crate::protocol::SessionRecordPresence::{Absent, Present, Unknown};
        let dir = crate::test_support::ScratchDir::new("agent-records-presence");
        let account = dir.join("account");
        let shell = || {
            HashMap::from([(
                "HOME".to_owned(),
                dir.join("home").to_string_lossy().into_owned(),
            )])
        };
        let hints = Hints::default();
        for agent in [Agent::Claude, Agent::Codex, Agent::Grok] {
            let mut native = session(agent, CLAUDE_ID, Some(&account));
            assert_eq!(
                presence(&hints, &mut Locator::with_shell(shell()), &[], &native),
                Absent
            );
            native.agent_session_id = None;
            assert_eq!(
                presence(&hints, &mut Locator::with_shell(shell()), &[], &native),
                Unknown
            );
            native.agent_session_id = Some("invalid".to_owned());
            assert_eq!(
                presence(&hints, &mut Locator::with_shell(shell()), &[], &native),
                Unknown
            );
        }
        let claude = session(Agent::Claude, CLAUDE_ID, Some(&account));
        let orphan = account.join("projects/work").join(CLAUDE_ID);
        std::fs::create_dir_all(orphan.join("subagents")).unwrap();
        assert_eq!(
            presence(&hints, &mut Locator::with_shell(shell()), &[], &claude),
            Present
        );
        let grok = session(Agent::Grok, CLAUDE_ID, Some(&account));
        std::fs::create_dir_all(account.join("sessions/work").join(CLAUDE_ID)).unwrap();
        assert_eq!(
            presence(&hints, &mut Locator::with_shell(shell()), &[], &grok),
            Present
        );

        std::fs::remove_dir_all(&orphan).unwrap();
        std::os::unix::fs::symlink(dir.join("elsewhere"), account.join("projects/linked")).unwrap();
        assert_eq!(
            presence(&hints, &mut Locator::with_shell(shell()), &[], &claude),
            Unknown
        );
        touch(
            &account
                .join("projects/work")
                .join(format!("{CLAUDE_ID}.jsonl")),
        );
        assert_eq!(
            presence(&hints, &mut Locator::with_shell(shell()), &[], &claude),
            Present
        );
        std::fs::remove_file(
            account
                .join("projects/work")
                .join(format!("{CLAUDE_ID}.jsonl")),
        )
        .unwrap();
        std::fs::remove_file(account.join("projects/linked")).unwrap();
        std::fs::remove_dir_all(account.join("projects")).unwrap();
        touch(&account.join("projects"));
        assert_eq!(
            presence(&hints, &mut Locator::with_shell(shell()), &[], &claude),
            Unknown
        );
    }

    fn judged(session: &Session) -> Resumability {
        resumability(&Locator::default(), session)
    }

    fn needs_unarchive(session: &Session) -> bool {
        judge(&Locator::default(), session).needs_unarchive
    }

    #[test]
    fn claude_is_resumable_only_with_its_main_transcript_in_its_own_account() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-claude");
        let (account, other) = (dir.join("account"), dir.join("other"));
        let own = session(Agent::Claude, CLAUDE_ID, Some(&account));
        std::fs::create_dir_all(account.join("projects")).unwrap();
        assert_eq!(judged(&own), Resumability::Unresumable);

        // A subagent folder left without its main transcript is not a record.
        std::fs::create_dir_all(account.join("projects/work").join(CLAUDE_ID)).unwrap();
        assert_eq!(judged(&own), Resumability::Unresumable);

        // Another account's copy, left by an earlier switch, does not count.
        touch(
            &other
                .join("projects/work")
                .join(format!("{CLAUDE_ID}.jsonl")),
        );
        assert_eq!(judged(&own), Resumability::Unresumable);

        // Any project folder of the account will do.
        touch(
            &account
                .join("projects/elsewhere")
                .join(format!("{CLAUDE_ID}.jsonl")),
        );
        assert_eq!(judged(&own), Resumability::Resumable);
    }

    #[test]
    fn a_session_without_a_conversation_is_resumable_and_an_unusable_id_is_unknown() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-fresh");
        let account = dir.join("account");
        for agent in [Agent::Claude, Agent::Codex, Agent::Grok] {
            let mut fresh = session(agent, CLAUDE_ID, Some(&account));
            fresh.has_conversation = false;
            assert_eq!(judged(&fresh), Resumability::Resumable);
            let mut no_id = session(agent, CLAUDE_ID, Some(&account));
            no_id.agent_session_id = None;
            assert_eq!(judged(&no_id), Resumability::Unknown);
            no_id.agent_session_id = Some("../x".to_owned());
            assert_eq!(judged(&no_id), Resumability::Unknown);
        }
    }

    #[test]
    fn an_account_directory_that_cannot_be_read_is_unknown_for_every_agent() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-gone");
        for agent in [Agent::Claude, Agent::Codex, Agent::Grok] {
            let gone = session(agent, CLAUDE_ID, Some(&dir.join("gone")));
            assert_eq!(judged(&gone), Resumability::Unknown);
        }
        // Present but not a directory, so it cannot be read either.
        touch(&dir.join("file"));
        let file = session(Agent::Claude, CLAUDE_ID, Some(&dir.join("file")));
        assert_eq!(judged(&file), Resumability::Unknown);
    }

    #[test]
    fn an_account_directory_without_any_records_directory_is_unknown_for_every_agent() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-empty");
        std::fs::create_dir_all(dir.join("account")).unwrap();
        for agent in [Agent::Claude, Agent::Codex, Agent::Grok] {
            let empty = session(agent, CLAUDE_ID, Some(&dir.join("account")));
            assert_eq!(judged(&empty), Resumability::Unknown);
        }
    }

    #[test]
    fn the_default_account_is_judged_through_the_shell_and_unknown_when_it_cannot_be_resolved() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-default");
        let default = session(Agent::Claude, CLAUDE_ID, None);
        let shell = HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_owned(),
            dir.join("default").to_string_lossy().into_owned(),
        )]);
        let locator = Locator::with_shell(shell);
        std::fs::create_dir_all(dir.join("default/projects")).unwrap();
        assert_eq!(resumability(&locator, &default), Resumability::Unresumable);
        touch(
            &dir.join("default/projects/work")
                .join(format!("{CLAUDE_ID}.jsonl")),
        );
        assert_eq!(resumability(&locator, &default), Resumability::Resumable);

        // A location that is not absolute is not a place to look.
        let relative = Locator::with_shell(HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_owned(),
            "relative".to_owned(),
        )]));
        assert_eq!(resumability(&relative, &default), Resumability::Unknown);
    }

    #[test]
    fn codex_counts_compressed_and_archived_rollouts_and_archived_ones_need_unarchiving() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-codex");
        let account = dir.join("account");
        let own = session(Agent::Codex, CODEX_ID, Some(&account));
        std::fs::create_dir_all(account.join("sessions")).unwrap();
        assert_eq!(judged(&own), Resumability::Unresumable);
        assert!(!needs_unarchive(&own));

        // Another thread's rollout is not this one's.
        touch(&account.join("sessions/2026/10/01/rollout-2026-10-01T00-00-00-019a0000-0000-7000-8000-000000000002.jsonl"));
        assert_eq!(judged(&own), Resumability::Unresumable);

        let name = format!("rollout-2026-10-01T00-00-00-{CODEX_ID}.jsonl");
        touch(
            &account
                .join("archived_sessions")
                .join(format!("{name}.zst")),
        );
        assert_eq!(judged(&own), Resumability::Resumable);
        assert!(needs_unarchive(&own));

        touch(
            &account
                .join("sessions/2026/10/01")
                .join(format!("{name}.zst")),
        );
        assert_eq!(judged(&own), Resumability::Resumable);
        assert!(!needs_unarchive(&own));

        let plain = dir.join("plain");
        touch(&plain.join("sessions/2026/10/01").join(&name));
        assert_eq!(
            judged(&session(Agent::Codex, CODEX_ID, Some(&plain))),
            Resumability::Resumable
        );
    }

    /// A reverted Codex thread is one record: while any file of its chain is there the thread is
    /// not proven gone, whichever of the first rollout and the segments survive, and a chain kept
    /// only in `archived_sessions/` needs unarchiving.
    #[test]
    fn a_reverted_codex_thread_is_not_gone_while_any_of_its_chain_is_there() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-chain");
        let account = dir.join("account");
        let own = session(Agent::Codex, CODEX_ID, Some(&account));
        let chain = crate::test_support::codex_chain(&account, CODEX_ID, false);
        assert_eq!(judged(&own), Resumability::Resumable);

        // Only the segments, then only the first rollout.
        std::fs::remove_file(&chain.first).unwrap();
        assert_eq!(judged(&own), Resumability::Resumable);
        std::fs::remove_file(&chain.middle).unwrap();
        assert_eq!(judged(&own), Resumability::Resumable);
        touch(&chain.first);
        std::fs::remove_file(&chain.head).unwrap();
        assert_eq!(judged(&own), Resumability::Resumable);

        // A segment on its own in the archive.
        std::fs::remove_file(&chain.first).unwrap();
        assert_eq!(judged(&own), Resumability::Unresumable);
        let archived = account
            .join("archived_sessions")
            .join(chain.middle.file_name().unwrap());
        touch(&archived);
        assert_eq!(judged(&own), Resumability::Resumable);
        assert!(needs_unarchive(&own));
    }

    #[test]
    fn an_archived_codex_rollout_is_found_although_sessions_cannot_be_searched() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-codex-uncertain");
        let account = dir.join("account");
        let own = session(Agent::Codex, CODEX_ID, Some(&account));
        std::fs::create_dir_all(dir.join("elsewhere")).unwrap();
        std::fs::create_dir_all(&account).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.join("elsewhere"), account.join("sessions")).unwrap();
        assert_eq!(judged(&own), Resumability::Unknown);
        assert!(!needs_unarchive(&own));

        touch(
            &account
                .join("archived_sessions")
                .join(format!("rollout-2026-10-01T00-00-00-{CODEX_ID}.jsonl")),
        );
        assert_eq!(judged(&own), Resumability::Resumable);
        assert!(needs_unarchive(&own));
    }

    #[test]
    fn record_names_match_the_id_whatever_its_case() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-case");
        let (claude, codex) = (dir.join("claude"), dir.join("codex"));
        touch(
            &claude
                .join("projects/work")
                .join(format!("{}.jsonl", CLAUDE_ID.to_uppercase())),
        );
        assert_eq!(
            judged(&session(Agent::Claude, CLAUDE_ID, Some(&claude))),
            Resumability::Resumable
        );
        touch(&codex.join("sessions/2026/10/01").join(format!(
            "rollout-2026-10-01T00-00-00-{}.jsonl",
            CODEX_ID.to_uppercase()
        )));
        assert_eq!(
            judged(&session(Agent::Codex, CODEX_ID, Some(&codex))),
            Resumability::Resumable
        );
        // And an id recorded in capitals finds a lower-case file.
        let upper = session(Agent::Codex, &CODEX_ID.to_uppercase(), Some(&codex));
        assert_eq!(judged(&upper), Resumability::Resumable);
    }

    #[test]
    fn a_codex_rollout_in_a_deep_date_tree_is_found() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-codex-deep");
        let account = dir.join("account");
        touch(
            &account
                .join("sessions/a/b/c/d/e")
                .join(format!("rollout-x-{CODEX_ID}.jsonl")),
        );
        assert_eq!(
            judged(&session(Agent::Codex, CODEX_ID, Some(&account))),
            Resumability::Resumable
        );
    }

    #[test]
    fn grok_is_resumable_with_its_folder_and_only_unknown_without() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-grok");
        let account = dir.join("account");
        let own = session(Agent::Grok, CLAUDE_ID, Some(&account));
        std::fs::create_dir_all(account.join("sessions")).unwrap();
        assert_eq!(judged(&own), Resumability::Unknown);
        touch(
            &account
                .join("sessions/work")
                .join(CLAUDE_ID)
                .join("usage.json"),
        );
        assert_eq!(judged(&own), Resumability::Resumable);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_records_directory_is_unknown_not_unresumable() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-link");
        let account = dir.join("account");
        std::fs::create_dir_all(dir.join("elsewhere")).unwrap();
        std::fs::create_dir_all(&account).unwrap();
        std::os::unix::fs::symlink(dir.join("elsewhere"), account.join("projects")).unwrap();
        assert_eq!(
            judged(&session(Agent::Claude, CLAUDE_ID, Some(&account))),
            Resumability::Unknown
        );
        // A link inside the records directory could hide the record as well.
        let inner = dir.join("inner");
        std::fs::create_dir_all(inner.join("projects")).unwrap();
        std::os::unix::fs::symlink(dir.join("elsewhere"), inner.join("projects/work")).unwrap();
        assert_eq!(
            judged(&session(Agent::Claude, CLAUDE_ID, Some(&inner))),
            Resumability::Unknown
        );
    }
}
