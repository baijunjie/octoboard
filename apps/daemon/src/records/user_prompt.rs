//! Whether an agent's own record of a session holds a prompt the user sent, which is what makes a
//! conversation one the user took part in whether or not the agent's hook reported the prompt.
//! Which lines are such a prompt is each provider's answer (`RecordStore::is_user_prompt`); a
//! provider that cannot tell, a line it cannot place, a line the reader skips and a record that
//! cannot be read say nothing either way, and that is never taken as a record without one.
//!
//! The collector follows a session's conversation file with a [`Search`] while the session counts
//! as having no conversation; a deletion asks [`of_session`] once before it moves the records of
//! such a session.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use crate::adapter::provider;
use crate::protocol::{Account, AgentId, Session};
use crate::records::line_file::{self, Change, LineFile};
use crate::records::locate::{own_paths, record_paths, records_dir, Hints, Locator};
use crate::records::RecordStore;

/// What a session's records say of whether the user sent it a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UserPrompt {
    /// A record holds one.
    Held,
    /// Every record found was read, and none holds one.
    NotHeld,
    /// A record could not be read through, a line of one was skipped or could not be placed, or
    /// its provider cannot tell a prompt of the user's.
    Unknown,
}

/// Whether the agent's records of `session` hold a prompt the user sent: every copy of its
/// conversation found the way [`record_paths`] finds a session's records, each read from its start
/// in one pass that stops at the first such prompt. A session with no records holds none.
pub(crate) fn of_session(
    hints: &Hints,
    locator: &mut Locator,
    accounts: &[Account],
    session: &Session,
) -> UserPrompt {
    let store = provider(session.agent).records();
    if let Some(rows) = super::rows::read(locator, session) {
        return match rows {
            Ok(rows) => {
                let mut unknown = false;
                for row in rows {
                    match store.is_user_prompt(&row.to_string()) {
                        Some(true) => return UserPrompt::Held,
                        Some(false) => {}
                        None => unknown = true,
                    }
                }
                if unknown {
                    UserPrompt::Unknown
                } else {
                    UserPrompt::NotHeld
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => UserPrompt::NotHeld,
            Err(_) => UserPrompt::Unknown,
        };
    }
    let mut unknown = !locator.alternate_root_complete(session);
    for path in record_paths(hints, locator, accounts, session) {
        let file = store.conversation_file(path);
        match std::fs::metadata(&file) {
            Ok(meta) if meta.is_file() => {}
            // A directory beside the record, or a record with no conversation file yet.
            Ok(_) => continue,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                unknown = true;
                continue;
            }
        }
        match in_file(store, &file) {
            UserPrompt::Held => return UserPrompt::Held,
            UserPrompt::Unknown => unknown = true,
            UserPrompt::NotHeld => {}
        }
    }
    if unknown {
        UserPrompt::Unknown
    } else {
        UserPrompt::NotHeld
    }
}

/// Whether the conversation file at `file` holds a prompt the user sent, as `store` reads its
/// lines, in one pass from its start that stops at the first. A line the reader skips (one past
/// its line limit, such as a prompt with a large image pasted into it, or one that is not UTF-8)
/// may have been that prompt, so a file that shows none past such a line is unknown.
fn in_file(store: &dyn RecordStore, file: &Path) -> UserPrompt {
    let mut found = UserPrompt::NotHeld;
    let read = line_file::read_whole_until(file, None, |line| {
        if line.trim().is_empty() {
            return ControlFlow::Continue(());
        }
        found = match store.is_user_prompt(line) {
            Some(false) => return ControlFlow::Continue(()),
            Some(true) => UserPrompt::Held,
            None => UserPrompt::Unknown,
        };
        ControlFlow::Break(())
    });
    match read {
        Ok(pass) if pass.skipped_a_line && found == UserPrompt::NotHeld => UserPrompt::Unknown,
        Ok(_) => found,
        // A pass that failed part-way has not seen the whole file.
        Err(_) => UserPrompt::Unknown,
    }
}

/// The file the conversation of `record` is read from: `record` is the indexed record of the
/// conversation `agent_id` in the account at `root` (`Locator::current_record`). `None` when the
/// record is not where the agent puts its own, or holds no conversation file yet.
pub(crate) fn conversation_file_of(
    agent: AgentId,
    root: &Path,
    agent_id: &str,
    record: &Path,
) -> Option<PathBuf> {
    let dir = records_dir(agent, root)?;
    conversation_file_under(agent, &dir, agent_id, record)
}

pub(super) fn conversation_file_under(
    agent: AgentId,
    dir: &Path,
    agent_id: &str,
    record: &Path,
) -> Option<PathBuf> {
    let store = provider(agent).records();
    own_paths(agent, agent_id, record, dir)
        .into_iter()
        .map(|path| store.conversation_file(path))
        .find(|file| std::fs::metadata(file).is_ok_and(|meta| meta.is_file()))
}

/// A conversation file followed for a prompt the user sent, read incrementally: each pass reads
/// only the lines added since the last.
#[derive(Default)]
pub(crate) struct Search {
    file: LineFile,
    found: bool,
}

impl Search {
    /// Reads what the conversation file at `file` gained and says whether it has shown a prompt
    /// the user sent. A file that was replaced or rewritten is read again from its start; one that
    /// cannot be read now leaves what was found before.
    pub(crate) fn read(&mut self, store: &dyn RecordStore, file: &Path) -> bool {
        let Ok(change) = self.file.check(file) else {
            return self.found;
        };
        if change == Change::Restarted {
            self.found = false;
        }
        if change != Change::Unchanged {
            let mut found = self.found;
            let _ = self.file.read(file, |line| {
                found |= store.is_user_prompt(line) == Some(true);
            });
            self.found = found;
        }
        self.found
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::records::fixtures::*;

    fn locator() -> Locator<'static> {
        Locator::with_shell(HashMap::new())
    }

    fn asked(agent: AgentId, agent_id: &str, root: &Path) -> UserPrompt {
        of_session(
            &Hints::default(),
            &mut locator(),
            &[],
            &session(agent, agent_id, Some(root)),
        )
    }

    fn claude(dir: &Path) -> Unreported {
        unreported(dir)
            .into_iter()
            .find(|each| each.agent == AgentId::CLAUDE)
            .unwrap()
    }

    /// For each agent, a record as it is with no prompt sent holds no prompt of the user's, and
    /// holds one once a line the user sent follows.
    #[test]
    fn a_record_holds_a_prompt_once_a_line_of_it_is_what_the_user_sent() {
        let dir = crate::test_support::ScratchDir::new("user-prompt-of-session");
        for each in unreported(&dir) {
            let found = || asked(each.agent, each.agent_id, &each.account);
            each.write(false);
            assert_eq!(found(), UserPrompt::NotHeld, "{:?}", each.agent);
            each.write(true);
            assert_eq!(found(), UserPrompt::Held, "{:?}", each.agent);
        }
    }

    /// A session with no record at all (as Claude Code and Codex leave one never prompted), and a
    /// Grok Build session directory with no chat history, hold no prompt.
    #[test]
    fn a_session_with_no_conversation_file_holds_no_prompt() {
        let dir = crate::test_support::ScratchDir::new("user-prompt-no-file");
        for each in unreported(&dir) {
            std::fs::create_dir_all(each.file.parent().unwrap()).unwrap();
            assert_eq!(
                asked(each.agent, each.agent_id, &each.account),
                UserPrompt::NotHeld,
                "{:?}",
                each.agent
            );
        }
    }

    /// A copy of the conversation in another account counts: a prompt in any copy is a prompt of
    /// the session's.
    #[test]
    fn a_prompt_in_another_accounts_copy_is_found() {
        let dir = crate::test_support::ScratchDir::new("user-prompt-copy");
        let own = claude(&dir.join("own"));
        let other = claude(&dir.join("other"));
        own.write(false);
        other.write(true);
        let accounts = [Account {
            id: "a".to_owned(),
            agent: AgentId::CLAUDE,
            name: "Other".to_owned(),
            config_dir: other.account.to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
            versions: Default::default(),
        }];
        let found = of_session(
            &Hints::default(),
            &mut locator(),
            &accounts,
            &session(AgentId::CLAUDE, own.agent_id, Some(&own.account)),
        );
        assert_eq!(found, UserPrompt::Held);
    }

    /// A record that cannot be read through says nothing: a compressed rollout whose content is
    /// not a zstd stream.
    #[test]
    fn a_record_that_cannot_be_read_is_unknown() {
        let dir = crate::test_support::ScratchDir::new("user-prompt-unreadable");
        let rollout = dir
            .join("sessions/2026/10/16")
            .join(format!("rollout-2026-10-16T00-00-00-{CODEX_ID}.jsonl.zst"));
        std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
        std::fs::write(&rollout, b"not a zstd stream").unwrap();
        assert_eq!(asked(AgentId::CODEX, CODEX_ID, &dir), UserPrompt::Unknown);
    }

    /// A line the reader skips may have been the prompt: one past the line limit (a prompt with a
    /// large image pasted into it is written as one line) or not UTF-8 leaves a record that shows
    /// no prompt unknown, and takes nothing from one that shows a prompt before it.
    #[test]
    fn a_skipped_line_leaves_a_record_with_no_prompt_unknown() {
        let dir = crate::test_support::ScratchDir::new("user-prompt-skipped");
        let each = claude(&dir);
        let found = || asked(each.agent, each.agent_id, &each.account);
        let long = "x".repeat(crate::records::line_file::LINE_LIMIT);
        let write = |lines: &[&[u8]]| {
            std::fs::create_dir_all(each.file.parent().unwrap()).unwrap();
            std::fs::write(&each.file, lines.join(&b"\n"[..])).unwrap();
        };
        let unprompted = each.unprompted.trim_end().as_bytes();
        write(&[unprompted, long.as_bytes(), b""]);
        assert_eq!(found(), UserPrompt::Unknown, "past the line limit");
        write(&[unprompted, b"\xff\xfe", b""]);
        assert_eq!(found(), UserPrompt::Unknown, "not UTF-8");
        write(&[unprompted, long.as_bytes()]);
        assert_eq!(found(), UserPrompt::Unknown, "unfinished");
        write(&[each.prompt.as_bytes(), long.as_bytes(), b""]);
        assert_eq!(found(), UserPrompt::Held);
    }

    /// A line the provider cannot place leaves the record unknown: shell input Claude Code 2.1.330
    /// went on to ask the model about, and a line cut short.
    #[test]
    fn a_line_that_cannot_be_placed_leaves_the_record_unknown() {
        let dir = crate::test_support::ScratchDir::new("user-prompt-unplaced");
        let each = claude(&dir);
        let found = || asked(each.agent, each.agent_id, &each.account);
        std::fs::create_dir_all(each.file.parent().unwrap()).unwrap();
        std::fs::write(
            &each.file,
            include_str!("../../testdata/unprompted_claude_2.1.330_shell.jsonl"),
        )
        .unwrap();
        assert_eq!(found(), UserPrompt::Unknown);
        std::fs::write(&each.file, format!("{}{{\"type\":\"us", each.unprompted)).unwrap();
        assert_eq!(found(), UserPrompt::Unknown);
    }

    /// A provider that does not say which lines are the user's prompts leaves a record with lines
    /// in it unknown, never without a prompt.
    #[test]
    fn a_provider_that_cannot_tell_a_prompt_leaves_the_record_unknown() {
        let dir = crate::test_support::ScratchDir::new("user-prompt-cannot-tell");
        let record = records_dir(AgentId::TEST, &dir)
            .unwrap()
            .join(format!("{CLAUDE_ID}.jsonl"));
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(&record, "{\"user\":\"Fix the test\"}\n").unwrap();
        assert_eq!(asked(AgentId::TEST, CLAUDE_ID, &dir), UserPrompt::Unknown);
    }

    /// The followed file shows a prompt once one is appended, keeps saying so, and starts over when
    /// the file is rewritten without it.
    #[test]
    fn a_followed_file_shows_a_prompt_once_one_is_appended() {
        let dir = crate::test_support::ScratchDir::new("user-prompt-search");
        for each in unreported(&dir) {
            let store = provider(each.agent).records();
            let mut search = Search::default();
            assert!(!search.read(store, &each.file), "no file yet");
            each.write(false);
            assert!(!search.read(store, &each.file), "{:?}", each.agent);
            each.write(true);
            assert!(search.read(store, &each.file), "{:?}", each.agent);
            assert!(search.read(store, &each.file), "what was found stays found");
            each.write(false);
            assert!(!search.read(store, &each.file), "rewritten without it");
        }
    }

    /// The conversation file of an indexed record is the record itself for Claude Code and Codex,
    /// and the chat history beside the marker for Grok Build.
    #[test]
    fn the_conversation_file_of_an_indexed_record_is_each_agents_own() {
        let dir = crate::test_support::ScratchDir::new("user-prompt-conversation-file");
        for each in unreported(&dir) {
            each.write(true);
            let record = locator()
                .current_record(each.agent, &each.account, each.agent_id)
                .unwrap();
            assert_eq!(
                conversation_file_of(each.agent, &each.account, each.agent_id, &record),
                Some(each.file.clone()),
                "{:?}",
                each.agent
            );
        }
    }
}
