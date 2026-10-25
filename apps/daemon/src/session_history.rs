//! A session's history as the user recalls it: what they said, in turns, and the agent's final reply
//! to each, read from the agent's own conversation record. A message Octoboard wrote into the
//! session (a brief, a report, shared information, a notice) is kept and told apart from what the
//! user typed. Tool calls, reasoning, and every reply but the last of a turn are left out.
//!
//! The lines come from the handoff reader (`handoff::record`), which already drops what an agent
//! injects on its own; what it keeps for the package and this does not want is filtered here.
//! Nothing is written. A session that has had no conversation has an empty history; a record that
//! cannot be found or read is `None`, which the caller reports as unavailable.

use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;

use crate::agent_records::{history_branch, Branch, Hints, Locator};
use crate::handoff::record::{for_each_line, line_entries, Entry};
use crate::hooks::without_paste_wrapper;
use crate::protocol::{Account, Agent, HistoryMessage, HistorySpeaker, Session};
use crate::state::AppState;
use crate::{handoff, mcp, reporting, rfc3339, sharing};

/// How the messages Octoboard writes into a session begin, by what wrote them. The notices are
/// worded at several places and all begin alike.
const OCTOBOARD_OPENINGS: [&str; 8] = [
    reporting::NOTICE_PREFIX,
    mcp::exec::REQUEST_OPENING,
    handoff::operation::TAKEOVER_OPENING,
    reporting::BRIEF_OPENING,
    reporting::REPORT_OPENING,
    reporting::PAGE_SUBMISSION_OPENING,
    sharing::INFO_OPENING,
    sharing::INFO_COPY_OPENING,
];

/// A user line Claude Code writes that starts no turn: the note left when a turn was interrupted.
const NOT_TYPED: [&str; 1] = ["[Request interrupted by user"];

/// The slash command the user invoked, as typed (`/name args`), from the line Claude Code writes
/// for it (`<command-message>…</command-message>\n<command-name>/x</command-name>\n
/// <command-args>…</command-args>`). The prompt it expands to is a separate line the reader drops.
fn slash_command(text: &str) -> Option<String> {
    let name = tagged(text, "command-name")?;
    let args = tagged(text, "command-args").unwrap_or_default();
    Some(format!("{name} {args}").trim().to_owned())
}

/// What sits between `<tag>` and `</tag>` in `text`, trimmed, when that is not nothing.
fn tagged<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let after = &text[text.find(&format!("<{tag}>"))? + tag.len() + 2..];
    let inner = after[..after.find(&format!("</{tag}>"))?].trim();
    (!inner.is_empty()).then_some(inner)
}

/// Whether `text` is a message Octoboard wrote into a session. The branches prompt begins like
/// something a user could type, so it is told by the fixed phrase after the title too.
fn from_octoboard(text: &str) -> bool {
    OCTOBOARD_OPENINGS
        .iter()
        .any(|opening| text.starts_with(opening))
        || (text.starts_with(mcp::exec::BRANCHES_OPENING)
            && text
                .lines()
                .next()
                .is_some_and(|line| line.contains(mcp::exec::BRANCHES_MARKER)))
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
    agent: Agent,
    stretches: &[crate::agent_records::Stretch],
) -> std::io::Result<Vec<HistoryMessage>> {
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
            .and_then(rfc3339::parse_rfc3339_millis);
        if agent == Agent::Codex && commentary(line) {
            return;
        }
        for entry in line_entries(agent, line) {
            match entry {
                Entry::User(text) => {
                    let text = without_paste_wrapper(&text).trim();
                    let text = if text.starts_with("<command-") {
                        match slash_command(text) {
                            Some(command) => command,
                            None => continue,
                        }
                    } else {
                        text.to_owned()
                    };
                    let text = text.as_str();
                    if text.is_empty() || NOT_TYPED.iter().any(|lead| text.starts_with(lead)) {
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

/// Whether a Codex line is a progress note the model wrote on the way (`commentary`), as opposed to
/// its answer (`final_answer`; older Codex wrote no phase at all). The phase sits on the message
/// itself, or on the item an `item_completed` event carries.
fn commentary(line: &Value) -> bool {
    let Some(payload) = line.get("payload") else {
        return false;
    };
    let phase = payload
        .get("phase")
        .or_else(|| payload.get("item")?.get("phase"));
    phase.and_then(Value::as_str) == Some("commentary")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::agent_records::Stretch;

    fn history(agent: Agent, fixture: &str) -> Vec<HistoryMessage> {
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
        let messages = history(crate::protocol::Agent::Claude, "history_claude.jsonl");
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
        let messages = history(crate::protocol::Agent::Codex, "history_codex_rollout.jsonl");
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

    /// A paste wrapper is taken off, and the reply to a turn is the last text of the agent's lines.
    #[test]
    fn grok_history_unwraps_pastes_and_prompts() {
        use HistorySpeaker::{Agent, User};
        let messages = history(
            crate::protocol::Agent::Grok,
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
        use crate::branch_origin::handover::Handover;
        use crate::protocol::SessionStatus;
        let caller =
            crate::test_support::project_session("s-1", Agent::Claude, SessionStatus::Idle);
        let handover = Handover {
            repository: "/repo/.git".into(),
            branches: vec!["feat/a".to_owned()],
        };
        let prompts = [
            mcp::exec::request_opening_prompt(&caller, None, false, "the brief"),
            mcp::exec::request_opening_prompt(&caller, Some(&handover), true, "the brief"),
            mcp::exec::branches_opening_prompt(&caller, &handover, "the brief"),
            handoff::operation::brief(&caller, Path::new("/tmp/package.md"), Some("a note")),
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
            Agent::Claude,
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
    fn a_slash_command_reads_as_typed() {
        assert_eq!(
            slash_command(
                "<command-message>x</command-message>\n<command-name>/x</command-name>\n\
                 <command-args>a b</command-args>"
            )
            .as_deref(),
            Some("/x a b")
        );
        assert_eq!(
            slash_command("<command-name>/clear</command-name>\n<command-args></command-args>")
                .as_deref(),
            Some("/clear")
        );
        assert_eq!(slash_command("<command-message>x</command-message>"), None);
    }

    #[test]
    fn a_record_that_is_missing_is_an_error_not_an_empty_history() {
        let missing = Stretch {
            path: Path::new("/nonexistent/history.jsonl").to_path_buf(),
            cut: None,
        };
        assert!(read(Agent::Claude, &[missing]).is_err());
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
        assert!(read(Agent::Claude, &[Stretch { path, cut: None }])
            .unwrap()
            .is_empty());
    }
}
