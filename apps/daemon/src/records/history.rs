//! A session's history as the user recalls it: what they said, in turns, and the agent's final reply
//! to each, read from the agent's own conversation record. A message Octoboard wrote into the
//! session (a brief, a report, shared information, a notice) is kept and told apart from what the
//! user typed. Tool calls, reasoning, and every reply but the last of a turn are left out.
//!
//! The lines come from the conversation reader (`records::conversation`), which already drops what
//! an agent injects on its own; what it keeps for the package and this does not want is filtered
//! here, with the provider's store saying which lines a history leaves out
//! (`RecordStore::hidden_from_history`) and how a user line reads as typed, if it reads as typed at
//! all (`RecordStore::user_text`). Nothing is written. A session that has had no conversation has
//! an empty history; a record that cannot be found or read is `None`, which the caller reports as
//! unavailable.

use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;

use crate::adapter::provider;
use crate::messaging::{sharing, texts};
use crate::protocol::{Account, AgentId, HistoryMessage, HistorySpeaker, Session};
use crate::records::conversation::{for_each_line, Entry};
use crate::records::{history_branch, Branch, Hints, Locator};
use crate::state::AppState;
use crate::time;

/// How the messages Octoboard writes into a session begin, by what wrote them. The notices are
/// worded at several places and all begin alike.
const OCTOBOARD_OPENINGS: [&str; 8] = [
    texts::NOTICE_PREFIX,
    texts::REQUEST_OPENING,
    texts::TAKEOVER_OPENING,
    texts::BRIEF_OPENING,
    texts::REPORT_OPENING,
    texts::PAGE_SUBMISSION_OPENING,
    sharing::INFO_OPENING,
    sharing::INFO_COPY_OPENING,
];

/// Whether `text` is a message Octoboard wrote into a session. The branches prompt begins like
/// something a user could type, so it is told by the fixed phrase after the title too.
fn from_octoboard(text: &str) -> bool {
    OCTOBOARD_OPENINGS
        .iter()
        .any(|opening| text.starts_with(opening))
        || (text.starts_with(texts::BRANCHES_OPENING)
            && text
                .lines()
                .next()
                .is_some_and(|line| line.contains(texts::BRANCHES_MARKER)))
}

/// The history of `session`: empty for one that has had no conversation, `None` when its record is
/// not found or cannot be read (a conversation with no agent session id has none to find).
pub fn of_session(
    hints: &Hints,
    locator: &mut Locator,
    accounts: &[Account],
    session: &Session,
) -> Option<Vec<HistoryMessage>> {
    if !session.has_conversation {
        return Some(Vec::new());
    }
    let Branch { stretches, .. } = history_branch(hints, locator, accounts, session)?;
    read(session.agent, &stretches?).ok()
}

/// [`of_session`] for `session` as the daemon holds it, for what answers a request: the record can
/// run to tens of megabytes, so it is parsed on the blocking pool, not on the async threads.
pub async fn of_session_blocking(
    state: &Arc<AppState>,
    session: Session,
) -> Result<Option<Vec<HistoryMessage>>> {
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || {
        let accounts = state.store.list_accounts()?;
        Ok(of_session(
            &state.record_hints,
            &mut Locator::default(),
            &accounts,
            &session,
        ))
    })
    .await?
}

fn read(
    agent: AgentId,
    stretches: &[crate::records::Stretch],
) -> std::io::Result<Vec<HistoryMessage>> {
    let store = provider(agent).records();
    let mut messages = Vec::new();
    let mut turn = 0;
    // The last reply seen since the turn's opening message: the final one when the turn ends.
    let mut reply: Option<(String, Option<i64>)> = None;
    let close_turn = |messages: &mut Vec<HistoryMessage>, turn, reply: &mut Option<_>| {
        if let Some((text, at)) = reply.take() {
            messages.push(HistoryMessage {
                speaker: HistorySpeaker::Agent,
                text,
                turn,
                at,
            });
        }
    };
    for_each_line(stretches, |line| {
        let at = line
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(time::parse_rfc3339_millis);
        if store.hidden_from_history(line) {
            return;
        }
        for entry in store.entries(line) {
            match entry {
                Entry::User(text) => {
                    let Some(text) = store.user_text(&text) else {
                        continue;
                    };
                    let text = text.trim();
                    if text.is_empty() {
                        continue;
                    }
                    close_turn(&mut messages, turn, &mut reply);
                    turn += 1;
                    let from_octoboard = from_octoboard(text);
                    messages.push(HistoryMessage {
                        speaker: if from_octoboard {
                            HistorySpeaker::Octoboard
                        } else {
                            HistorySpeaker::User
                        },
                        text: text.to_owned(),
                        turn,
                        at,
                    });
                }
                Entry::Assistant(text) if turn > 0 => reply = Some((text, at)),
                _ => {}
            }
        }
    })?;
    close_turn(&mut messages, turn, &mut reply);
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::records::Stretch;

    /// The prompt a restarted session is sent to carry on is Octoboard's, not the user's.
    #[test]
    fn the_prompt_to_carry_on_after_a_restart_is_octoboards() {
        assert!(from_octoboard(crate::messaging::texts::CONTINUE_PROMPT));
    }

    /// Every opening a message Octoboard writes is defined with (a constant named `*_OPENING`) is
    /// matched here: named in this file, or built on the notice prefix that is. A new kind of
    /// message left out would show in the history as the user's own words.
    #[test]
    fn every_opening_octoboard_writes_with_is_matched() {
        fn sources(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    sources(&path, found);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    found.push(path);
                }
            }
        }
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&src, &mut files);
        let this_file = include_str!("history.rs");
        let mut checked = 0;
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for line in text.lines() {
                let Some(name) = line
                    .split("const ")
                    .nth(1)
                    .and_then(|rest| rest.split(':').next())
                    .filter(|name| name.ends_with("_OPENING"))
                else {
                    continue;
                };
                checked += 1;
                // Built on the prefix only where its value begins with it.
                let on_the_prefix = line
                    .split_once('=')
                    .and_then(|(_, value)| value.trim().strip_prefix("concat!("))
                    .is_some_and(|args| {
                        args.trim_start()
                            .trim_start_matches("crate::")
                            .trim_start_matches("messaging::")
                            .trim_start_matches("texts::")
                            .starts_with("notice_prefix!()")
                    });
                assert!(
                    this_file.contains(&format!("::{name}")) || on_the_prefix,
                    "`{name}` ({}) is not matched by `from_octoboard`",
                    file.display()
                );
            }
        }
        // A floor, not a count: a scan that stops finding the declarations would pass the check
        // above without proving anything.
        assert!(checked >= 5, "found only {checked} openings");
    }

    fn history(agent: AgentId, fixture: &str) -> Vec<HistoryMessage> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join(fixture);
        read(agent, &[Stretch { path, cut: None }]).unwrap()
    }

    fn shown(messages: &[HistoryMessage]) -> Vec<(HistorySpeaker, &str, u32)> {
        messages
            .iter()
            .map(|message| (message.speaker, message.text.as_str(), message.turn))
            .collect()
    }

    /// A reply that comes between tool calls is not the turn's answer, an interruption note is not
    /// something the user said, a slash command is, and an Octoboard message is told apart from
    /// the user's.
    #[test]
    fn claude_history_keeps_turns_final_replies_and_labels_octoboard_messages() {
        use HistorySpeaker::{Agent, Octoboard, User};
        let messages = history(crate::protocol::AgentId::CLAUDE, "history_claude.jsonl");
        assert_eq!(
            shown(&messages),
            [
                (
                    Octoboard,
                    "## Goal\n\nShip the export.\n\n## Context\n\nIt is slow.",
                    1
                ),
                (Agent, "Plan agreed.", 1),
                (User, "/model sonnet", 2),
                (User, "Why is it slow?", 3),
                (Agent, "The query scans the table.", 3),
                (
                    Octoboard,
                    "Message from Octoboard: your owner has changed.",
                    4
                ),
                (Agent, "Noted.", 4),
                (User, "Add an index", 5),
            ]
        );
        assert_eq!(messages[3].at, Some(1_792_334_800_000));
        assert_eq!(messages[1].at, Some(1_792_334_760_000));
    }

    /// Only the answer of a turn is a reply: the progress notes before it are dropped, and a
    /// message written without a phase (older Codex) still counts as the answer.
    #[test]
    fn codex_history_drops_commentary_and_keeps_the_final_answer() {
        use HistorySpeaker::{Agent, Octoboard, User};
        let messages = history(
            crate::protocol::AgentId::CODEX,
            "history_codex_rollout.jsonl",
        );
        assert_eq!(
            shown(&messages),
            [
                (User, "Fix the flaky test", 1),
                (Agent, "It was a race; fixed.", 1),
                (
                    Octoboard,
                    "Report from session s-1 — Child\nStatus: Done",
                    2
                ),
                (Agent, "Thanks, closing it out.", 2),
                (User, "And mind the docs", 3),
                (Agent, "Docs updated.", 3),
            ]
        );
    }

    /// A prompt is read out of its `<user_query>` and, should it carry a paste wrapper such as
    /// Claude Code records (the fixture's first prompt, made up: none has been observed in Grok
    /// Build's records), out of that too; the reply to a turn is the last text of the agent's
    /// lines.
    #[test]
    fn grok_history_unwraps_prompts_and_a_paste_wrapper_it_is_guarded_against() {
        use HistorySpeaker::{Agent, User};
        let messages = history(
            crate::protocol::AgentId::GROK,
            "history_grok_chat_history.jsonl",
        );
        assert_eq!(
            shown(&messages),
            [
                (User, "Rename the module", 1),
                (Agent, "Renamed it.", 1),
                (User, "Run the tests", 2),
            ]
        );
    }

    /// The opening prompts the daemon builds are Octoboard's, whatever else they say; a user's
    /// message that only begins like the branches prompt is not.
    #[test]
    fn the_daemons_opening_prompts_are_labelled_octoboard() {
        use crate::branch_origin::transfer::BranchTransfer;
        use crate::protocol::SessionStatus;
        let caller =
            crate::test_support::project_session("s-1", AgentId::CLAUDE, SessionStatus::Idle);
        let transfer = BranchTransfer {
            repository: "/repo/.git".into(),
            branches: vec!["feat/a".to_owned()],
            targets: Default::default(),
        };
        let prompts = [
            texts::request_opening_prompt(&caller, None, false, "the brief"),
            texts::request_opening_prompt(&caller, Some(&transfer), true, "the brief"),
            texts::branches_opening_prompt(&caller, &transfer, "the brief"),
            texts::takeover_prompt(&caller, Path::new("/tmp/package.md"), Some("a note")),
        ];
        for prompt in prompts {
            assert!(from_octoboard(&prompt), "{prompt}");
        }
        assert!(!from_octoboard("The session `x` is broken, can you look?"));
        assert!(!from_octoboard(
            "The session `x` (\"T\")\nwhich started you, also handed the branch"
        ));
    }

    #[test]
    fn a_session_with_no_conversation_is_empty_and_one_with_no_record_is_unavailable() {
        let mut session = crate::test_support::project_session(
            "s-1",
            AgentId::CLAUDE,
            crate::protocol::SessionStatus::Idle,
        );
        let history = |session: &Session| {
            of_session(&Hints::default(), &mut Locator::default(), &[], session)
        };
        session.has_conversation = false;
        assert_eq!(history(&session), Some(Vec::new()));
        session.has_conversation = true;
        session.agent_session_id = None;
        assert_eq!(history(&session), None);
    }

    #[test]
    fn a_record_that_is_missing_is_an_error_not_an_empty_history() {
        let missing = Stretch {
            path: Path::new("/nonexistent/history.jsonl").to_path_buf(),
            cut: None,
        };
        assert!(read(AgentId::CLAUDE, &[missing]).is_err());
    }

    #[test]
    fn a_history_with_no_user_message_is_empty() {
        let dir = crate::test_support::ScratchDir::new("history-empty");
        let path = dir.join("record.jsonl");
        std::fs::write(
            &path,
            "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}\n",
        )
        .unwrap();
        assert!(read(AgentId::CLAUDE, &[Stretch { path, cut: None }])
            .unwrap()
            .is_empty());
    }
}
