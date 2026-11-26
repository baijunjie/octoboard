//! How Antigravity CLI's hook events are read (1.5.14; the payloads are in
//! `docs/agent-cli-reference.md`, section "Antigravity CLI", and in
//! `testdata/antigravity_hooks.jsonl`).
//!
//! Its status comes from its own OSC 7501 reports (`crate::session::program_status`), so the
//! hooks give only what the reports do not: the conversation's id, which no launch chooses, and
//! the tool calls that write a file, for the cross-project guard, and a turn's whole reply, which
//! the `done` report cuts to its first 199 characters: `Stop` names the conversation's transcript
//! and the message its turn answered, and the reply is read from there ([`reply_of_turn`]). A
//! payload is camelCase and names its event in `hookEventName`; the launch tells each event's
//! command its name as well. The session's background work, its commands left running, its timers
//! and cron jobs and its subagents, whose events come through the same hooks, is read from them
//! too ([`super::background`]).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::home::LAUNCH_DIR;
use crate::hooks::{BackgroundTasksChange, HookInterpreter, HookSession, TurnMessage};
use crate::providers::antigravity::APP_DATA_DIR;
use crate::records::locate::plausible_id;

/// The event of a new conversation's first turn.
pub(super) const SESSION_START: &str = "SessionStart";

/// The event of each model request of a turn, its first one included (`invocationNum` 0), in a
/// resumed conversation as in a new one. A resumed conversation fires no `SessionStart`, so this is
/// the hook its first turn sends as it starts, whether or not it calls a tool or ends by itself:
/// without it, a first turn of a resume with no tool call that ran past the hook bound, or was
/// cancelled (no `Stop`), read as hooks absent. One more hook run per model request is the cost.
pub(super) const PRE_INVOCATION: &str = "PreInvocation";

/// The event of a turn that ended by itself, finished or failed. A cancelled turn fires none.
pub(super) const STOP: &str = "Stop";

/// The events of a tool call: before it runs (and before its permission prompt is drawn), and
/// once it has ended.
pub(super) const PRE_TOOL_USE: &str = "PreToolUse";
pub(super) const POST_TOOL_USE: &str = "PostToolUse";

/// Every event something of the provider reads: this interpreter, the background work
/// ([`super::background`]) and the pending interactions (`super::interaction`). The launch
/// registers Octoboard's hook for each of them (`super::home`), which its test holds it to; only
/// that test reads the list. `PreInvocation` is read for what every payload carries, the
/// conversation's id, and for its arrival, which tells the daemon the launch's hooks run.
#[cfg(test)]
pub(super) const EVENTS_READ: [&str; 5] = [
    SESSION_START,
    PRE_INVOCATION,
    STOP,
    PRE_TOOL_USE,
    POST_TOOL_USE,
];

/// The tools whose calls are read by name (1.5.14): the one that runs a command, and the two that
/// write a file, which the cross-project guard is asked about.
pub(super) const RUN_COMMAND: &str = "run_command";
pub(super) const WRITE_FILE: &str = "write_file";
pub(super) const EDIT_FILE: &str = "edit_file";
pub(super) const FILE_WRITING_TOOLS: [&str; 2] = [WRITE_FILE, EDIT_FILE];

/// How `Stop` says a turn failed. Such a turn has no reply: its error is the report's own `msg`.
const FAILED: &str = "ERROR";

/// Where a conversation's transcript is, under the CLI's data directory: `brain/<conversation
/// id>/` and then these.
pub(super) const TRANSCRIPT_IN_CONVERSATION: [&str; 3] =
    [".system_generated", "logs", "transcript_full.jsonl"];

/// The most of a transcript's end that is read for a turn's reply. A transcript grows with its
/// conversation, and the turn that just ended is at its end; a reply longer than this is not
/// found, and the report's cut one stands.
const TRANSCRIPT_TAIL: u64 = 4 * 1024 * 1024;

/// What the transcript wraps a message of the user's in (1.5.14): the text `Stop` gives as
/// `lastUserInput` stands between these, at the start of the entry's `content`, with blocks of
/// metadata after them.
pub(super) const REQUEST_OPEN: &str = "<USER_REQUEST>\n";
pub(super) const REQUEST_CLOSE: &str = "\n</USER_REQUEST>";

pub(super) struct AntigravityHooks;

impl HookInterpreter for AntigravityHooks {
    /// The file a `write_file` or an `edit_file` call names, which is always absolute (the CLI
    /// refuses a relative one), with the directory the payload gives for a path that is not.
    fn edit_targets(&self, payload: &Value) -> (Vec<String>, Option<PathBuf>) {
        let Some(call) = payload.get("toolCall") else {
            return (Vec::new(), None);
        };
        let writes = call
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| FILE_WRITING_TOOLS.contains(&name));
        if !writes {
            return (Vec::new(), None);
        }
        let targets = call
            .pointer("/args/Path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .map(|path| vec![path.to_string()])
            .unwrap_or_default();
        let cwd = payload
            .get("cwd")
            .and_then(Value::as_str)
            .filter(|cwd| !cwd.is_empty())
            .map(PathBuf::from);
        (targets, cwd)
    }

    /// The model is told the reason: `tool call denied by pre-tool hook: <reason>`.
    fn deny_edit(&self, reason: &str) -> Option<Value> {
        Some(json!({ "decision": "deny", "reason": reason }))
    }

    /// The conversation's id, where it has the shape the CLI gives one.
    fn agent_session_id(&self, payload: &Value) -> Option<String> {
        payload
            .get("conversationId")
            .and_then(Value::as_str)
            .filter(|id| plausible_id(id))
            .map(str::to_string)
    }

    /// A subagent's conversation runs through the same hooks as the session's own, under its own
    /// conversation id (1.5.14): its events are told by their empty message
    /// ([`super::background::is_subagent`]), and only the work they start and end is read of them.
    fn is_subagent(&self, payload: &Value) -> bool {
        super::background::is_subagent(payload)
    }

    /// The commands left running at a turn's end and the subagents, read from the hooks as they
    /// start and end ([`super::background`]).
    fn background_tasks_changes_in(
        &self,
        session: &HookSession<'_>,
        event: &str,
        payload: &Value,
    ) -> Vec<BackgroundTasksChange> {
        super::background::changes(session, event, payload)
    }

    fn announces_session_start(&self, event: &str, _payload: &Value) -> bool {
        event == SESSION_START
    }

    fn supplies_turn_message(&self) -> bool {
        true
    }

    /// `Stop` of a finished turn names the transcript the turn's reply is in, to be read off the
    /// hook queue, and only through the session's own launch directory ([`transcript_of`]);
    /// `Stop` of a failed turn says there is no reply, and so does one that reaches a session
    /// with no scratch directory, so the turn end does not wait for one. No other event gives a
    /// message.
    fn turn_message_in(
        &self,
        session: &HookSession<'_>,
        event: &str,
        payload: &Value,
    ) -> Option<TurnMessage> {
        if event != STOP {
            return None;
        }
        let failed = payload.get("terminationReason").and_then(Value::as_str) == Some(FAILED);
        let named = payload
            .get("transcriptPath")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty());
        match (
            failed,
            named,
            self.agent_session_id(payload),
            session.scratch,
        ) {
            (false, Some(named), Some(id), Some(scratch)) => {
                let named = PathBuf::from(named);
                let scratch = scratch.to_path_buf();
                let request = payload
                    .get("lastUserInput")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                Some(TurnMessage::Read(Box::new(move || {
                    let transcript = transcript_of(&named, &id, &scratch)?;
                    reply_of_turn(&tail_of(&transcript)?, request.as_deref())
                })))
            }
            _ => Some(TurnMessage::Said(None)),
        }
    }

    /// Antigravity CLI runs its hooks on turns and tool calls only, and a turn needs its prompt:
    /// nothing fires while its folder-trust screen is up, unless the launch itself gave it a
    /// prompt, which no launch does (`super::AntigravityProvider::plan`).
    fn past_startup(&self, _event: &str, _payload: &Value) -> bool {
        true
    }
}

/// The transcript `named` leads to, where it is the transcript of conversation `id` named through
/// the launch's directory in `scratch`, the scratch directory the daemon made for the session the
/// payload was posted for. A payload is what a hook process posted, so the path is taken only as
/// that session's launch spells it, compared as written before anything is resolved:
/// `<scratch>/<the launch's directory>/<the CLI's data directory>/brain/<id>/…`, which is how the
/// CLI names it (1.5.14: the directory as the launch gave it, no link resolved), compared as the
/// host reads one path (`hostfs::lexical_key`). The data directory there is a link to the home's
/// own (`super::home`), the one link allowed on the way: the path is then resolved, and has to be
/// that conversation's transcript right under the resolved data directory, so a link anywhere
/// below it leads to no file that is opened. A file of the very shape anywhere else, another
/// session's launch directory included, is not opened.
fn transcript_of(named: &Path, id: &str, scratch: &Path) -> Option<PathBuf> {
    use crate::hostfs::{canonical, lexical_key, lexically_normalise};

    let mut in_data_dir = PathBuf::from("brain");
    in_data_dir.push(id);
    in_data_dir.extend(TRANSCRIPT_IN_CONVERSATION);

    let data_dir = scratch.join(LAUNCH_DIR).join(APP_DATA_DIR);
    if lexical_key(named) != lexical_key(&data_dir.join(&in_data_dir)) {
        return None;
    }

    let resolved = canonical(lexically_normalise(named)).ok()?;
    (resolved == canonical(&data_dir).ok()?.join(in_data_dir)).then_some(resolved)
}

/// The end of the transcript at `path`, at most [`TRANSCRIPT_TAIL`] of it, as text. The cut may
/// fall inside an entry, which is then no entry to its reader. The CLI may be appending to the
/// file, so the read is bounded by itself, not by the size the file had.
fn tail_of(path: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = crate::hostfs::open_regular(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(TRANSCRIPT_TAIL)))
        .ok()?;
    let mut tail = Vec::new();
    file.take(TRANSCRIPT_TAIL).read_to_end(&mut tail).ok()?;
    Some(String::from_utf8_lossy(&tail).into_owned())
}

/// The reply of the turn that answered `request`, the message `Stop` names as `lastUserInput`, in
/// `transcript`, which holds an entry a line (1.5.14, `testdata/antigravity_transcript_full.jsonl`):
/// the user's message (`type` `USER_INPUT`), then what the model did, each of its answers a
/// `PLANNER_RESPONSE` whose `content` is the text it wrote (an answer that only calls tools has
/// none) with the tools' results (`GENERIC`) and the CLI's own notes (`SYSTEM_MESSAGE`,
/// `ERROR_MESSAGE`) between and after them. The reply is the content of the last answer between
/// the last message that is `request` and the message after it; a turn with none (cancelled,
/// failed, or not written yet) has no reply. A line that is no entry is passed over.
///
/// The turn is found by its message, not taken to be the last: a message written during the turn
/// is held by the CLI and sent as the next turn's once this one ends, 0.2 to 0.5 s after `Stop`
/// (measured), so the transcript may already hold that message, and its answer, when it is read
/// here. Where the next message has the very text of this turn's, the later of the two is taken
/// for this turn's, and its answer, if already written, for the reply.
///
/// With no `request` the turn is the last one. With one that no message of `transcript` has, there
/// is no reply, unless `transcript` has no message at all: the tail of a turn longer than what was
/// read, whose last answer is the reply.
fn reply_of_turn(transcript: &str, request: Option<&str>) -> Option<String> {
    let mut reply = None;
    let mut later_messages = false;
    for line in transcript.lines().rev() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let content = entry.get("content").and_then(Value::as_str);
        match entry.get("type").and_then(Value::as_str) {
            Some("USER_INPUT") => {
                let asked = content.unwrap_or_default();
                if request.is_none_or(|request| is_request(asked, request)) {
                    return reply;
                }
                // A later turn's message: what was found after it is that turn's.
                later_messages = true;
                reply = None;
            }
            Some("PLANNER_RESPONSE") if reply.is_none() => {
                reply = content
                    .filter(|content| !content.trim().is_empty())
                    .map(str::to_string);
            }
            _ => {}
        }
    }
    reply.filter(|_| !later_messages)
}

/// Whether `content`, of a `USER_INPUT` entry, is the message `request`.
fn is_request(content: &str, request: &str) -> bool {
    content
        .strip_prefix(REQUEST_OPEN)
        .and_then(|rest| rest.strip_prefix(request))
        .is_some_and(|rest| rest.starts_with(REQUEST_CLOSE))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::test_support::ScratchDir;

    /// The transcript of one conversation (1.5.14, a stand-in for the model): a turn whose reply
    /// is 5,010 characters, a turn that calls a tool of an MCP server and then answers, a turn
    /// cancelled with Esc and a turn that failed.
    const TRANSCRIPT: &str = include_str!("../../../testdata/antigravity_transcript_full.jsonl");

    /// The transcript cut after its first `entries` entries, as it stood when that turn ended.
    fn transcript_to(entries: usize) -> String {
        TRANSCRIPT
            .lines()
            .take(entries)
            .map(|line| format!("{line}\n"))
            .collect()
    }

    /// The transcript of a turn with a message written during it (`held message one`, then
    /// `held message two`), which the CLI sent as one message when the turn ended (1.5.14, the
    /// stand-in): the turn, then the held messages' own turn.
    const HELD: &str = include_str!("../../../testdata/antigravity_transcript_held_messages.jsonl");

    /// Entries of the kinds a real model's conversations hold beside those of [`TRANSCRIPT`]
    /// (1.5.14: `SYSTEM_MESSAGE` and `ERROR_MESSAGE`, both of `source` `SYSTEM`), in the places
    /// they were seen in, with made-up text: `testdata/antigravity-captures.md`.
    const SYSTEM_LINES: &str =
        include_str!("../../../testdata/antigravity_transcript_system_lines.jsonl");

    fn first_entries(transcript: &str, entries: usize) -> String {
        transcript
            .lines()
            .take(entries)
            .map(|line| format!("{line}\n"))
            .collect()
    }

    /// The whole reply is read, not its first 199 characters; a turn that called a tool gives
    /// what the model wrote after the tool's result, not the result; a cancelled turn and a
    /// failed one, which end on the user's message, give none.
    #[test]
    fn the_reply_is_the_last_answer_after_the_users_last_message() {
        let long = reply_of_turn(&transcript_to(2), None).expect("a reply");
        assert_eq!(long.chars().count(), 5010);
        assert!(long.starts_with("BEGIN 0123456789"), "{}", &long[..40]);
        assert!(long.ends_with("0123456789 END"));

        assert_eq!(
            reply_of_turn(&transcript_to(6), None).as_deref(),
            Some("Stub: tool finished.")
        );
        // The turn's first answer only calls the tool, and the tool's result is no answer.
        assert_eq!(reply_of_turn(&transcript_to(4), None), None);
        assert_eq!(reply_of_turn(&transcript_to(5), None), None);
        // Cancelled, and failed: nothing follows the user's message.
        assert_eq!(reply_of_turn(&transcript_to(7), None), None);
        assert_eq!(reply_of_turn(TRANSCRIPT, None), None);
        assert_eq!(reply_of_turn("", None), None);
    }

    /// The turn is the one that answered the message `Stop` names. A message held during it is
    /// sent when it ends, so the transcript may already hold that message, and its answer, when
    /// it is read: the reply is still this turn's, not none and not the next turn's.
    #[test]
    fn the_reply_is_that_of_the_turn_stop_names_whatever_was_written_since() {
        let turn = Some("SLOW6 first");
        for (written, entries) in [
            ("nothing since", 2),
            ("the held message", 3),
            ("its answer too", 4),
        ] {
            assert_eq!(
                reply_of_turn(&first_entries(HELD, entries), turn).as_deref(),
                Some("Stub: slow answer."),
                "{written}"
            );
        }
        // Read as the last turn's, the same transcripts give nothing, then the next turn's reply.
        assert_eq!(reply_of_turn(&first_entries(HELD, 3), None), None);
        assert_eq!(reply_of_turn(HELD, None).as_deref(), Some("Stub: hello."));

        // Two held messages are sent as one, a row each, and `Stop` names them so.
        let held = Some("held message one\nheld message two");
        assert_eq!(reply_of_turn(HELD, held).as_deref(), Some("Stub: hello."));
        assert_eq!(reply_of_turn(&first_entries(HELD, 3), held), None);
        // A message that only starts as the named one, or is its start, is another.
        assert_eq!(reply_of_turn(HELD, Some("held message one")), None);
        assert_eq!(reply_of_turn(HELD, Some("SLOW6 first again")), None);
    }

    /// A message `Stop` names that the transcript does not hold gives no reply where the
    /// transcript holds other messages; where it holds none, it is the end of a turn longer than
    /// what was read, and the last answer is the reply.
    #[test]
    fn a_turn_whose_message_is_not_in_what_was_read() {
        assert_eq!(reply_of_turn(&transcript_to(6), Some("not asked")), None);
        let no_message: String = TRANSCRIPT
            .lines()
            .skip(3)
            .take(3)
            .map(|line| format!("{line}\n"))
            .collect();
        assert_eq!(
            reply_of_turn(&no_message, Some("not asked")).as_deref(),
            Some("Stub: tool finished.")
        );
    }

    /// The CLI's own notes are no answers and end no turn, wherever they stand: after the user's
    /// message, between two answers, after the answer, and before the answer of a turn whose
    /// model request failed once.
    #[test]
    fn the_clis_own_notes_are_passed_over() {
        let kinds: std::collections::BTreeSet<(String, String)> = SYSTEM_LINES
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("an entry"))
            .map(|entry| {
                let field = |name: &str| entry[name].as_str().expect("a text").to_string();
                (field("source"), field("type"))
            })
            .collect();
        for kind in [("SYSTEM", "SYSTEM_MESSAGE"), ("SYSTEM", "ERROR_MESSAGE")] {
            assert!(
                kinds.contains(&(kind.0.to_string(), kind.1.to_string())),
                "{kind:?}"
            );
        }

        let first = Some("first, with notes of the CLI's own");
        // After the user's message and nothing else yet; then the answer, a note after it.
        assert_eq!(reply_of_turn(&first_entries(SYSTEM_LINES, 2), first), None);
        assert_eq!(reply_of_turn(&first_entries(SYSTEM_LINES, 5), first), None);
        for entries in [6, 7, 12] {
            assert_eq!(
                reply_of_turn(&first_entries(SYSTEM_LINES, entries), first).as_deref(),
                Some("Synthetic: answered after two notes."),
                "{entries}"
            );
        }
        assert_eq!(
            reply_of_turn(&first_entries(SYSTEM_LINES, 7), None).as_deref(),
            Some("Synthetic: answered after two notes.")
        );
        // An error, then the answer of the same turn.
        let second = Some("second, an error before the answer");
        assert_eq!(reply_of_turn(&first_entries(SYSTEM_LINES, 9), second), None);
        assert_eq!(
            reply_of_turn(SYSTEM_LINES, second).as_deref(),
            Some("Synthetic: answered after an error.")
        );
        // A note as the last line, after the user's message.
        assert_eq!(reply_of_turn(SYSTEM_LINES, None), None);
    }

    /// A tail cut inside an entry still gives the reply of the entries that are whole, and an
    /// answer that is blank is no reply.
    #[test]
    fn a_line_that_is_no_entry_is_passed_over() {
        let whole = transcript_to(6);
        let cut = &whole[whole.find("\n").expect("a first line") - 40..];
        assert_eq!(
            reply_of_turn(cut, None).as_deref(),
            Some("Stub: tool finished.")
        );
        let blank = r#"{"type":"USER_INPUT","content":"hi"}
{"type":"PLANNER_RESPONSE","content":"  "}
"#;
        assert_eq!(reply_of_turn(blank, None), None);
    }

    /// A launch's directory in a run directory under `dir`, its data directory a link to that of
    /// a home under `dir` which holds `transcript` as the transcript of conversation [`ID`], with
    /// the conversation's cut transcript beside it. Returns the session's scratch directory and
    /// the launch's.
    #[cfg(unix)]
    fn launch_with_transcript(dir: &ScratchDir, transcript: &str) -> (PathBuf, PathBuf) {
        let data = dir.join("home").join(".gemini").join("antigravity-cli");
        let logs = in_conversation(&data, ID);
        std::fs::create_dir_all(&logs).expect("logs");
        std::fs::write(logs.join("transcript_full.jsonl"), transcript).expect("transcript");
        std::fs::write(logs.join("transcript.jsonl"), transcript).expect("the cut one");
        let scratch = dir.join("run").join("a-session");
        let launch = scratch.join("antigravity");
        std::fs::create_dir_all(&launch).expect("launch directory");
        crate::test_support::symlink(&data, launch.join("antigravity-cli")).expect("a link");
        (scratch, launch)
    }

    /// What the interpreter gives for `payload` of `event`, posted for a session whose scratch
    /// directory is `scratch`.
    fn message_in(scratch: Option<&Path>, event: &str, payload: &Value) -> Option<TurnMessage> {
        let carry = crate::hooks::HookCarry::default();
        let session = HookSession::new("a-session", scratch, None, &carry);
        AntigravityHooks.turn_message_in(&session, event, payload)
    }

    /// The directory of conversation `id`'s transcripts under the data directory `data`.
    #[cfg(unix)]
    fn in_conversation(data: &Path, id: &str) -> PathBuf {
        data.join("brain")
            .join(id)
            .join(".system_generated")
            .join("logs")
    }

    /// The path a hook names leads through the launch's directory, whose data directory is a
    /// link to the home's: it is taken only where it is written as the session's own launch
    /// directory spells the transcript of the payload's own conversation, and is then resolved.
    #[cfg(unix)]
    #[test]
    fn only_the_conversations_own_transcript_is_opened() {
        let dir = ScratchDir::new("antigravity-transcript");
        let (scratch, launch) = launch_with_transcript(&dir, &transcript_to(2));
        let through = |file: &str| in_conversation(&launch.join("antigravity-cli"), ID).join(file);

        let found =
            transcript_of(&through("transcript_full.jsonl"), ID, &scratch).expect("the transcript");
        let in_home = in_conversation(&dir.join("home/.gemini/antigravity-cli"), ID)
            .join("transcript_full.jsonl");
        assert_eq!(found, crate::hostfs::canonical(&in_home).expect("resolved"));
        assert_eq!(
            reply_of_turn(&tail_of(&found).expect("read"), None)
                .expect("a reply")
                .chars()
                .count(),
            5010
        );

        // Another conversation's id, another file of the conversation, a file elsewhere, a path
        // that leads nowhere.
        let other = "00000000-aef5-4628-b74c-10a183d622bc";
        assert_eq!(
            transcript_of(&through("transcript_full.jsonl"), other, &scratch),
            None
        );
        assert_eq!(
            transcript_of(&through("transcript.jsonl"), ID, &scratch),
            None
        );
        let elsewhere = dir.join("transcript_full.jsonl");
        std::fs::write(&elsewhere, transcript_to(2)).expect("a file elsewhere");
        assert_eq!(transcript_of(&elsewhere, ID, &scratch), None);
        assert_eq!(
            transcript_of(&dir.join("missing.jsonl"), ID, &scratch),
            None
        );

        // A path that only reaches the launch's directory by going up from elsewhere is read as
        // written once folded, so this one is the transcript; one that leaves it is not.
        let folded = scratch
            .join("other/../antigravity/antigravity-cli")
            .join("brain")
            .join(ID)
            .join(".system_generated/logs/transcript_full.jsonl");
        assert!(transcript_of(&folded, ID, &scratch).is_some());
        let leaving = launch
            .join("antigravity-cli/brain")
            .join(ID)
            .join(".system_generated/logs/../../../../../../../any/antigravity-cli/brain")
            .join(ID)
            .join(".system_generated/logs/transcript_full.jsonl");
        std::fs::create_dir_all(in_conversation(&dir.join("run/any/antigravity-cli"), ID))
            .expect("where the path leads");
        std::fs::write(
            in_conversation(&dir.join("run/any/antigravity-cli"), ID).join("transcript_full.jsonl"),
            transcript_to(2),
        )
        .expect("a file of the right shape");
        assert_eq!(transcript_of(&leaving, ID, &scratch), None);
    }

    /// A link inside the conversation's directory of the home, leading to a directory of the very
    /// shape elsewhere, is not followed: the path a hook names passes as written and resolves to a
    /// transcript of that conversation's shape, but not under the data directory the launch's own
    /// link leads to.
    #[cfg(unix)]
    #[test]
    fn a_link_below_the_data_directory_is_not_followed() {
        let dir = ScratchDir::new("antigravity-transcript-link");
        let (scratch, launch) = launch_with_transcript(&dir, "");
        let home_logs = in_conversation(&dir.join("home/.gemini/antigravity-cli"), ID);
        let conversation = home_logs
            .parent()
            .and_then(Path::parent)
            .expect("brain/<id>");
        std::fs::remove_dir_all(conversation.join(".system_generated")).expect("cleared");
        let elsewhere = in_conversation(&dir.join("elsewhere/antigravity-cli"), ID);
        std::fs::create_dir_all(&elsewhere).expect("elsewhere");
        std::fs::write(elsewhere.join("transcript_full.jsonl"), transcript_to(2)).expect("file");
        crate::test_support::symlink(
            elsewhere.parent().expect(".system_generated"),
            conversation.join(".system_generated"),
        )
        .expect("a planted link");

        let named =
            in_conversation(&launch.join("antigravity-cli"), ID).join("transcript_full.jsonl");
        assert!(named.exists(), "the planted file is reached");
        assert_eq!(transcript_of(&named, ID, &scratch), None);
    }

    /// The conversation's transcript itself, and a file of its very shape that resolves to a
    /// transcript of that conversation, are refused when named any other way than through the
    /// session's own launch directory: by the home's own path, under a directory outside the
    /// daemon's, through another session's launch directory, straight under the scratch
    /// directory, a level below the launch's directory, and through another directory of the
    /// session's.
    #[cfg(unix)]
    #[test]
    fn a_file_shaped_as_the_transcript_elsewhere_is_refused() {
        let dir = ScratchDir::new("antigravity-transcript-elsewhere");
        let (scratch, _launch) = launch_with_transcript(&dir, &transcript_to(2));
        let in_home = in_conversation(&dir.join("home/.gemini/antigravity-cli"), ID)
            .join("transcript_full.jsonl");
        assert_eq!(transcript_of(&in_home, ID, &scratch), None);

        let shaped = |data: PathBuf| {
            let logs = in_conversation(&data, ID);
            std::fs::create_dir_all(&logs).expect("logs of the right shape");
            let file = logs.join("transcript_full.jsonl");
            std::fs::write(&file, transcript_to(2)).expect("a file of the right shape");
            file
        };
        for (place, data) in [
            ("another directory", dir.join("any/antigravity-cli")),
            (
                "another directory, laid out as a launch's",
                dir.join("any/a-session/antigravity/antigravity-cli"),
            ),
            (
                "another session's launch directory",
                dir.join("run/another-session/antigravity/antigravity-cli"),
            ),
            (
                "the scratch directory itself",
                scratch.join("antigravity-cli"),
            ),
            (
                "a level below the launch's directory",
                scratch.join("antigravity/deeper/antigravity-cli"),
            ),
            (
                "another directory of the session's",
                scratch.join("other/antigravity-cli"),
            ),
        ] {
            let file = shaped(data);
            assert_eq!(transcript_of(&file, ID, &scratch), None, "{place}");
            // The same through the interpreter: the file is there and holds a reply, and none
            // is read from it.
            let payload = json!({
                "hookEventName": "Stop",
                "conversationId": ID,
                "terminationReason": "NO_TOOL_CALL",
                "transcriptPath": file.to_string_lossy(),
            });
            let Some(TurnMessage::Read(read)) = message_in(Some(&scratch), "Stop", &payload) else {
                panic!("a finished turn's message is to be read: {place}");
            };
            assert_eq!(read(), None, "{place}");
        }
    }

    /// A transcript longer than what is read gives its end, and the read is bounded by itself.
    #[cfg(unix)]
    #[test]
    fn only_the_end_of_a_long_transcript_is_read() {
        let dir = ScratchDir::new("antigravity-transcript-tail");
        let path = dir.join("transcript_full.jsonl");
        let filler = format!("{}\n", "x".repeat(1023));
        let mut long = filler.repeat((TRANSCRIPT_TAIL / 1024) as usize + 8);
        long.push_str(&transcript_to(6));
        std::fs::write(&path, &long).expect("a long transcript");

        let tail = tail_of(&path).expect("read");
        assert_eq!(tail.len() as u64, TRANSCRIPT_TAIL);
        assert_eq!(
            reply_of_turn(&tail, None).as_deref(),
            Some("Stub: tool finished.")
        );
    }

    /// Every `Stop` gives the turn's message or says there is none, so a turn end never waits
    /// out its bound: a finished turn's is to be read, a failed turn's is none, and so is that of
    /// a `Stop` naming no transcript or no conversation, or posted for a session with no scratch
    /// directory. No other event gives one.
    #[test]
    fn only_stop_gives_a_turn_message() {
        assert!(AntigravityHooks.supplies_turn_message());
        let mut stops = 0;
        for payload in payloads() {
            let event = event_of(&payload);
            let message = message_in(Some(Path::new("/scratch/run/a-session")), event, &payload);
            if event != "Stop" {
                assert!(message.is_none(), "{event}");
                continue;
            }
            stops += 1;
            let failed = payload["terminationReason"] == "ERROR";
            match message.expect("a Stop answers") {
                TurnMessage::Read(_) => assert!(!failed, "{payload}"),
                TurnMessage::Said(said) => {
                    assert!(failed, "{payload}");
                    assert_eq!(said, None);
                }
            }
        }
        assert!(stops >= 2, "{stops}");

        for payload in [
            json!({ "hookEventName": "Stop", "conversationId": ID }),
            json!({ "hookEventName": "Stop", "transcriptPath": "/scratch/t.jsonl" }),
            json!({ "hookEventName": "Stop", "conversationId": "x", "transcriptPath": "/t" }),
        ] {
            assert!(
                matches!(
                    message_in(Some(Path::new("/scratch/run/a-session")), "Stop", &payload),
                    Some(TurnMessage::Said(None))
                ),
                "{payload}"
            );
        }
        // A session with no scratch directory has no launch directory to read through.
        let finished = payloads()
            .into_iter()
            .find(|payload| event_of(payload) == "Stop" && payload["terminationReason"] != "ERROR")
            .expect("a finished turn's Stop");
        assert!(matches!(
            message_in(None, "Stop", &finished),
            Some(TurnMessage::Said(None))
        ));
    }

    /// What a `Stop` of a finished turn gives is read from the transcript it names, through the
    /// launch's directory: the whole reply of the turn that answered the message the payload
    /// names, with the next turn's message and answer already written; of the last turn for a
    /// payload that names no message; and nothing from a transcript named any other way.
    #[cfg(unix)]
    #[test]
    fn the_message_of_a_finished_turn_is_read_from_the_transcript_it_names() {
        let dir = ScratchDir::new("antigravity-turn-message");
        let (scratch, launch) = launch_with_transcript(&dir, HELD);
        let through_launch =
            in_conversation(&launch.join("antigravity-cli"), ID).join("transcript_full.jsonl");
        let in_home = in_conversation(&dir.join("home/.gemini/antigravity-cli"), ID)
            .join("transcript_full.jsonl");
        let read = |named: &Path, last: Option<&str>| {
            let mut payload = json!({
                "hookEventName": "Stop",
                "conversationId": ID,
                "terminationReason": "NO_TOOL_CALL",
                "transcriptPath": named.to_string_lossy(),
            });
            if let Some(last) = last {
                payload["lastUserInput"] = json!(last);
            }
            let Some(TurnMessage::Read(read)) = message_in(Some(&scratch), "Stop", &payload) else {
                panic!("a finished turn's message is to be read");
            };
            read()
        };
        assert_eq!(
            read(&through_launch, Some("SLOW6 first")).as_deref(),
            Some("Stub: slow answer.")
        );
        assert_eq!(
            read(&through_launch, Some("held message one\nheld message two")).as_deref(),
            Some("Stub: hello.")
        );
        assert_eq!(read(&through_launch, None).as_deref(), Some("Stub: hello."));
        assert_eq!(read(&in_home, Some("SLOW6 first")), None);
    }

    /// One payload a line, as the CLI wrote them to a command hook's stdin (1.5.14,
    /// `testdata/antigravity-captures.md`): every event, both file-writing tools, a declined
    /// command and a failed turn.
    const PAYLOADS: &str = include_str!("../../../testdata/antigravity_hooks.jsonl");

    const ID: &str = "d7372c8f-aef5-4628-b74c-10a183d622bc";

    fn payloads() -> Vec<Value> {
        PAYLOADS
            .lines()
            .map(|line| serde_json::from_str(line).expect("a JSON payload"))
            .collect()
    }

    fn event_of(payload: &Value) -> &str {
        payload["hookEventName"].as_str().expect("a named event")
    }

    fn tool_of(payload: &Value) -> Option<&str> {
        payload.pointer("/toolCall/name").and_then(Value::as_str)
    }

    #[test]
    fn the_capture_holds_every_event_and_both_file_writing_tools() {
        let all = payloads();
        let mut events: Vec<&str> = all.iter().map(event_of).collect();
        events.sort_unstable();
        events.dedup();
        assert_eq!(
            events,
            [
                "PostInvocation",
                "PostToolUse",
                "PreInvocation",
                "PreToolUse",
                "SessionStart",
                "Stop"
            ]
        );
        for tool in [
            "write_file",
            "edit_file",
            "run_command",
            "ask_question",
            "call_mcp_tool",
        ] {
            assert!(
                all.iter().any(|payload| tool_of(payload) == Some(tool)),
                "{tool}"
            );
        }
    }

    /// The conversation's id is read from every payload of the capture, the same one.
    #[test]
    fn the_conversation_id_is_read_from_every_payload() {
        for payload in payloads() {
            assert_eq!(
                AntigravityHooks.agent_session_id(&payload).as_deref(),
                Some(ID),
                "{}",
                event_of(&payload)
            );
        }
    }

    /// Only an id with the shape of a UUID is the conversation's: anything else, an empty text or
    /// a value that is no text, is not taken.
    #[test]
    fn only_an_id_with_the_shape_of_a_uuid_is_taken() {
        let with = |id: Value| json!({ "hookEventName": "Stop", "conversationId": id });
        assert_eq!(
            AntigravityHooks
                .agent_session_id(&with(json!(ID)))
                .as_deref(),
            Some(ID)
        );
        for bad in [
            json!(""),
            json!("conversation-1"),
            json!("d7372c8f-aef5-4628-b74c-10a183d622b"),
            json!("d7372c8f-aef5-4628-b74c-10a183d622bcc"),
            json!("d7372c8faef5-4628-b74c-10a183d622bc-"),
            json!("g7372c8f-aef5-4628-b74c-10a183d622bc"),
            json!(42),
            json!(null),
        ] {
            assert_eq!(
                AntigravityHooks.agent_session_id(&with(bad.clone())),
                None,
                "{bad}"
            );
        }
        assert_eq!(
            AntigravityHooks.agent_session_id(&json!({ "hookEventName": "Stop" })),
            None
        );
    }

    /// `SessionStart`, a new conversation's first turn, alone announces the start; every event
    /// of the capture shows the agent past its startup, since nothing fires while its trust screen
    /// is up.
    #[test]
    fn only_session_start_announces_the_start_and_every_event_is_past_startup() {
        let all = payloads();
        let announcing: Vec<&str> = all
            .iter()
            .filter(|payload| AntigravityHooks.announces_session_start(event_of(payload), payload))
            .map(event_of)
            .collect();
        assert_eq!(announcing, ["SessionStart"]);
        for payload in &all {
            assert!(
                AntigravityHooks.past_startup(event_of(payload), payload),
                "{}",
                event_of(payload)
            );
        }
    }

    /// The status is the CLI's own reports (`crate::session::program_status`), so the hooks say
    /// nothing of it and end no turn: not a `Stop` of a finished turn, not one of a failed turn.
    #[test]
    fn no_event_gives_a_status_or_ends_a_turn() {
        for payload in payloads() {
            let event = event_of(&payload);
            assert!(
                AntigravityHooks.status(event, &payload).is_none(),
                "{event}"
            );
            assert!(
                AntigravityHooks.turn_end(event, &payload).is_none(),
                "{event}"
            );
        }
    }

    /// A call of either tool that writes a file names it as `Path`, absolute, with the payload's
    /// working directory beside it; no other tool, and no event without a tool call, names
    /// anything.
    #[test]
    fn the_file_writing_tools_name_their_target_and_no_other_tool_does() {
        let mut writes = 0;
        for payload in payloads() {
            let (targets, cwd) = AntigravityHooks.edit_targets(&payload);
            match tool_of(&payload) {
                Some("write_file" | "edit_file") => {
                    writes += 1;
                    assert_eq!(targets, ["/scratch/project/note.txt"], "{payload}");
                    assert_eq!(cwd, Some(PathBuf::from("/scratch/project")), "{payload}");
                }
                other => {
                    assert!(targets.is_empty(), "{other:?}");
                    assert_eq!(cwd, None, "{other:?}");
                }
            }
        }
        // Each of the two tools is called in its `PreToolUse` and its `PostToolUse`.
        assert_eq!(writes, 4);
    }

    /// A target that is not absolute is passed on as written with the working directory to resolve
    /// it against; a call with no target, or no working directory, names what it can.
    #[test]
    fn a_relative_target_comes_with_the_directory_to_resolve_it_against() {
        let call = |args: Value, cwd: Option<&str>| {
            let mut payload = json!({
                "hookEventName": "PreToolUse",
                "toolCall": { "name": "edit_file", "args": args },
            });
            if let Some(cwd) = cwd {
                payload["cwd"] = json!(cwd);
            }
            AntigravityHooks.edit_targets(&payload)
        };
        assert_eq!(
            call(json!({ "Path": "note.txt" }), Some("/scratch/project")),
            (
                vec!["note.txt".to_string()],
                Some(PathBuf::from("/scratch/project"))
            )
        );
        assert_eq!(
            call(json!({ "Path": "/scratch/project/note.txt" }), None),
            (vec!["/scratch/project/note.txt".to_string()], None)
        );
        for args in [json!({}), json!({ "Path": "" }), json!({ "Path": 7 })] {
            assert!(
                call(args.clone(), Some("/scratch/project")).0.is_empty(),
                "{args}"
            );
        }
    }

    /// The refusal of an edit is the answer the CLI reads on stdout: the decision and the reason,
    /// which the model is told.
    #[test]
    fn the_refusal_of_an_edit_is_a_deny_decision_with_the_reason() {
        assert_eq!(
            AntigravityHooks.deny_edit("outside the project"),
            Some(json!({ "decision": "deny", "reason": "outside the project" }))
        );
    }
}
