//! Qwen Code's conversation tree and its title and runtime sidecars, and the watch for turn
//! endings its hooks do not report.
//!
//! A turn cancelled and a prompt declined fire no hook (0.28.3). Qwen Code's own OSC 7501 reports
//! show both. A cancel is `idle` straight after `working`, which the provider declares
//! (`AgentProvider::reported_signals`), so the report itself closes the turn at once. A declined
//! prompt is `done`, which is also what Qwen Code reports between two model calls of one turn, so
//! it closes nothing by itself: `crate::session::title_interrupt` watches the terminal's activity
//! (the reports, else the glyph the title starts with, `super::screen`) stay at the prompt for
//! its settle, and closes the turn then. The same watch stands behind a cancel where no report
//! was made.

use std::fs::{FileType, Metadata};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::protocol::{ToolCategory, ToolStatus};
use crate::providers::qwen::LAYOUT;
use crate::records::branch::{Branching, Link};
use crate::records::context::Context;
use crate::records::conversation::Entry;
use crate::records::feed::{at_of, FeedOut, FeedReader, ToolCall, ToolOutcome};
use crate::records::observed::Observed;
use crate::records::store::{newest_existing, transcript_path_hint};

use crate::hooks::RecordWatch;
use crate::records::RecordStore;
use crate::state::AppState;

pub(super) struct QwenRecords;

const CHAT: &str = ".jsonl";
const TITLE: &str = ".title.json";
const RUNTIME: &str = ".runtime.json";

impl RecordStore for QwenRecords {
    fn hinted_record(&self, payload: &Value) -> Option<(String, PathBuf)> {
        transcript_path_hint(payload)
    }

    fn indexed(
        &self,
        path: &Path,
        name: &str,
        kind: FileType,
        leaf: bool,
    ) -> Option<(String, PathBuf)> {
        if !kind.is_file() || !leaf || !in_chats(path) {
            return None;
        }
        // A sidecar can outlive the conversation or be all a turnless launch wrote.
        let id = [CHAT, TITLE, RUNTIME]
            .into_iter()
            .find_map(|suffix| name.strip_suffix(suffix))?;
        crate::records::plausible_id(id)
            .then(|| (id.to_owned(), path.with_file_name(format!("{id}{CHAT}"))))
    }

    fn own_paths(&self, id: &str, record: &Path, depth: usize) -> Vec<PathBuf> {
        if depth != LAYOUT.record_level
            || !in_chats(record)
            || record.file_name().and_then(|name| name.to_str()) != Some(&format!("{id}{CHAT}"))
        {
            return Vec::new();
        }
        vec![
            record.to_path_buf(),
            record.with_extension(&TITLE[1..]),
            record.with_extension(&RUNTIME[1..]),
        ]
    }

    fn names_record(&self, name: &str, kind: FileType, id: &str) -> bool {
        kind.is_file() && name == format!("{id}{CHAT}")
    }

    fn recognises_records(&self) -> bool {
        true
    }

    fn absence_is_conclusive(&self) -> bool {
        true
    }

    fn carried_head(&self, root: &Path, records: &[PathBuf]) -> Option<PathBuf> {
        let chats: Vec<_> = records
            .iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == &CHAT[1..]))
            .cloned()
            .collect();
        newest_existing(root, &chats, Metadata::is_file)
    }

    fn carried_companions(&self, files: Vec<PathBuf>, record: &Path) -> Vec<PathBuf> {
        let title = record.with_extension(&TITLE[1..]);
        let runtime = record.with_extension(&RUNTIME[1..]);
        files
            .into_iter()
            .filter(|path| *path == title || *path == runtime)
            .collect()
    }

    fn summary_file(&self, record: &Path) -> Option<PathBuf> {
        Some(record.with_extension(&TITLE[1..]))
    }

    fn branching(&self) -> Option<&dyn Branching> {
        Some(self)
    }

    fn entries(&self, line: &Value) -> Vec<Entry> {
        let entry = match line.get("type").and_then(Value::as_str) {
            Some("user") if is_user(line) => message_text(line).map(Entry::User),
            Some("assistant") => {
                let mut entries: Vec<_> = message_text(line)
                    .map(Entry::Assistant)
                    .into_iter()
                    .collect();
                let paths: Vec<_> = calls(line)
                    .filter_map(|call| {
                        let (category, path) = super::interaction::describe_tool(
                            call["name"].as_str()?,
                            &call["args"],
                        );
                        if category == ToolCategory::FileEdit {
                            path
                        } else {
                            None
                        }
                    })
                    .collect();
                if !paths.is_empty() {
                    entries.push(Entry::Changed(paths));
                }
                return entries;
            }
            _ => None,
        };
        entry.into_iter().collect()
    }

    fn is_user_prompt(&self, line: &str) -> Option<bool> {
        let record = serde_json::from_str::<Value>(line).ok()?;
        match record.get("type")?.as_str()? {
            "user" => Some(is_user(&record)),
            "assistant" | "system" | "tool_result" => Some(false),
            _ => None,
        }
    }

    fn feed_reader(&self) -> Option<Box<dyn FeedReader>> {
        Some(Box::new(QwenFeed))
    }

    fn observes_record_lines(&self) -> bool {
        false
    }

    fn fold_line(
        &self,
        line: &str,
        _past_boundary: bool,
        context: &mut Context,
        _observed: &mut Observed,
    ) {
        if !line.contains("\"usageMetadata\"") && !line.contains("\"chat_compression\"") {
            return;
        }
        let Ok(line) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let tokens = match (line["type"].as_str(), line["subtype"].as_str()) {
            (Some("assistant"), _) => line["usageMetadata"]["promptTokenCount"].as_u64(),
            (Some("system"), Some("chat_compression")) => {
                line["systemPayload"]["info"]["newTokenCount"].as_u64()
            }
            _ => None,
        };
        if let Some(tokens) = tokens {
            context.observe(tokens);
        }
    }

    fn arm_watch(&self, state: &Arc<AppState>, session_id: &str, watch: RecordWatch<'_>) {
        crate::session::title_interrupt::arm(state, session_id, &watch);
    }
}

impl Branching for QwenRecords {
    fn link(&self, line: &Value) -> Option<Link> {
        match line.get("type")?.as_str()? {
            "system" => {
                // Qwen Code excludes these from leaf selection and writes their children
                // against the last conversation record.
                if matches!(
                    line["subtype"].as_str(),
                    Some(
                        "session_artifact_event"
                            | "session_artifact_snapshot"
                            | "session_sources_snapshot"
                            | "generated_ui_state"
                            | "managed_session_header_v1"
                            | "managed_session_event_v1"
                            | "managed_session_commit_v1"
                            | "managed_session_snapshot_base_v1"
                    )
                ) {
                    return None;
                }
            }
            "user" | "assistant" | "tool_result" => {}
            _ => return None,
        }
        Some(Link {
            id: line.get("uuid")?.as_str()?.to_owned(),
            parent: line["parentUuid"].as_str().map(str::to_owned),
        })
    }
}

struct QwenFeed;

impl FeedReader for QwenFeed {
    fn line(&mut self, line: &Value, out: &mut FeedOut) {
        let at = at_of(line);
        match line["type"].as_str() {
            Some("user") if is_user(line) => {
                if let Some(text) = message_text(line) {
                    out.user(&text, at);
                }
            }
            Some("assistant") => {
                if let Some(text) = message_text(line) {
                    out.assistant(&text, at);
                }
                for call in calls(line).filter_map(describe_call) {
                    out.tool_call(call, at);
                }
            }
            Some("tool_result") => {
                if let Some((id, result)) = tool_result(line) {
                    out.tool_result(id, result);
                }
            }
            _ => {}
        }
    }
}

fn calls(line: &Value) -> impl Iterator<Item = &Value> {
    line["message"]["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part.get("functionCall"))
}

fn describe_call(call: &Value) -> Option<ToolCall> {
    let call_id = call["id"].as_str()?;
    let name = call["name"].as_str()?;
    let input = &call["args"];
    let (category, title) = super::interaction::describe_tool(name, input);
    let paths = if category == ToolCategory::FileEdit {
        title.clone().into_iter().collect()
    } else {
        Vec::new()
    };
    Some(ToolCall {
        call_id: call_id.to_owned(),
        name: name.to_owned(),
        category,
        title,
        input: input.clone(),
        paths,
        status: ToolStatus::Running,
        output: None,
        exit_code: None,
    })
}

fn tool_result(line: &Value) -> Option<(&str, ToolOutcome)> {
    let result = &line["toolCallResult"];
    let id = result["callId"].as_str()?;
    let status = match result["status"].as_str()? {
        "success" => ToolStatus::Completed,
        "error" => ToolStatus::Failed,
        "cancelled" => ToolStatus::Declined,
        _ => return None,
    };
    Some((
        id,
        ToolOutcome {
            status,
            output: result["resultDisplay"]
                .as_str()
                .or_else(|| result["resultDisplay"]["fileDiff"].as_str())
                .map(str::to_owned),
            exit_code: None,
        },
    ))
}

fn in_chats(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "chats")
}

fn is_user(line: &Value) -> bool {
    line["type"].as_str() == Some("user")
        && matches!(line["provenance"].as_str(), None | Some("real_user"))
}

fn message_text(line: &Value) -> Option<String> {
    if let Some(text) = line["systemPayload"]["displayText"].as_str() {
        return Some(text.to_owned());
    }
    let parts = line["message"]["parts"].as_array()?;
    let text = parts
        .iter()
        .filter(|part| part["thought"].as_bool() != Some(true))
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    (!text.trim().is_empty()).then_some(text)
}

pub(super) fn prompt_text(line: &str) -> Option<String> {
    let line = serde_json::from_str::<Value>(line).ok()?;
    if is_user(&line) {
        message_text(&line)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::provider;
    use crate::protocol::{AgentId, Session, SessionStatus};
    use crate::session::title_interrupt::{polls_settled, with_settle_held, without_settle};
    use crate::state::TurnClose;
    use crate::test_support::{
        app_state, console, console_session, idle_stand_in_of, within, ScratchDir, StandIn,
        PATIENCE,
    };

    const REWOUND: &str = include_str!("../../../testdata/qwen_record_rewind.jsonl");
    const EDIT: &str = include_str!("../../../testdata/qwen_record_tool.jsonl");
    const TITLE: &str = include_str!("../../../testdata/qwen_record_manual_title.json");

    #[test]
    fn history_feed_and_context_follow_the_rewound_branch() {
        use crate::protocol::ConversationBody;
        use crate::records::branch::for_each_line_on_branch;
        use crate::records::conversation::{read_stretches, Speaker};
        use crate::records::feed::reader_for;
        use crate::records::Stretch;

        assert_eq!(
            REWOUND
                .lines()
                .filter(|line| QwenRecords.is_user_prompt(line) == Some(true))
                .count(),
            2
        );
        let scratch = ScratchDir::new("qwen-rewound-record");
        let record = scratch.join("rewound.jsonl");
        std::fs::write(&record, REWOUND).expect("captured record");
        let stretches = [Stretch {
            path: record,
            cut: None,
        }];
        let history = read_stretches(AgentId::QWEN, &stretches).expect("history");
        assert_eq!(
            history.messages,
            [
                (Speaker::User, "First probe".into()),
                (Speaker::Assistant, "Stub response 1".into()),
            ]
        );
        let store = provider(AgentId::QWEN).records();
        let mut reader = reader_for(AgentId::QWEN);
        let mut feed = FeedOut::default();
        let mut context = Context::default();
        let mut observed = Observed::default();
        for_each_line_on_branch(store.branching(), &stretches, |line| {
            reader.line(line, &mut feed);
            store.fold_line(&line.to_string(), true, &mut context, &mut observed);
        })
        .expect("read the current branch");
        assert_eq!(feed.entries().len(), 2);
        assert!(
            matches!(&feed.entries()[0].body, ConversationBody::User { text } if text == "First probe")
        );
        assert!(
            matches!(&feed.entries()[1].body, ConversationBody::Assistant { text } if text == "Stub response 1")
        );
        assert_eq!(
            context.context(store).flatten().expect("context").tokens,
            23
        );
    }

    #[test]
    fn a_record_keeps_the_edit_and_its_result_in_the_feed() {
        use crate::protocol::ConversationBody;
        let store = provider(AgentId::QWEN).records();
        let mut reader = crate::records::feed::reader_for(AgentId::QWEN);
        let mut feed = FeedOut::default();
        let mut changed = Vec::new();
        for line in EDIT.lines() {
            let line = serde_json::from_str(line).expect("captured line");
            reader.line(&line, &mut feed);
            for entry in store.entries(&line) {
                if let Entry::Changed(paths) = entry {
                    changed.extend(paths);
                }
            }
        }
        assert_eq!(changed, ["/scratch/project/sample.txt"]);
        let ConversationBody::ToolCall(call) = &feed.entries()[1].body else {
            panic!("the recorded edit");
        };
        assert_eq!(call.paths, changed);
        assert_eq!(call.status, ToolStatus::Completed);
    }

    #[test]
    fn the_chat_and_sidecars_are_carried_and_removable_but_only_the_chat_resumes() {
        use crate::records::{carried, Locator};

        let account = ScratchDir::new("qwen-carry-record");
        let id = "4102aaf7-1c23-4db4-9db4-f2e2f700a231";
        let chats = account.join("projects/project/chats");
        std::fs::create_dir_all(&chats).expect("chats");
        let record = chats.join(format!("{id}.jsonl"));
        let title = record.with_extension("title.json");
        let runtime = record.with_extension("runtime.json");
        for (file, contents) in [(&record, REWOUND), (&title, TITLE), (&runtime, "{}")] {
            std::fs::write(file, contents).expect("session file");
        }
        let head = carried::locate_record(&mut Locator::default(), AgentId::QWEN, &account, id)
            .expect("the chat rather than its sidecars");
        assert_eq!(account.join(&head), record);
        let mut companions = carried::companions(AgentId::QWEN, &account, &head, id)
            .into_iter()
            .map(|path| account.join(path))
            .collect::<Vec<_>>();
        companions.sort();
        assert_eq!(companions, [runtime.clone(), title.clone()]);

        let mut locator = Locator::with_shell(std::collections::HashMap::new());
        let session = crate::records::fixtures::session(AgentId::QWEN, id, Some(&account));
        let mut removable = crate::records::removable_record_paths(
            &crate::records::Hints::default(),
            &mut locator,
            &[],
            &session,
        );
        removable.sort();
        assert_eq!(removable, [record.clone(), runtime, title]);

        std::fs::remove_file(record).expect("the chat was removed");
        assert_eq!(
            crate::records::resumability(&locator, &session),
            crate::records::Resumability::Unknown,
        );
        assert_eq!(
            crate::records::resumability(
                &locator.with_witnesses(crate::records::fixtures::witnessed()),
                &session
            ),
            crate::records::Resumability::Unresumable,
        );
    }

    #[test]
    fn the_title_sidecar_distinguishes_names_from_the_opening_prompt() {
        let adapter = provider(AgentId::QWEN);
        let manual = serde_json::from_str(TITLE).expect("manual title sidecar");
        let generated = serde_json::from_str(include_str!(
            "../../../testdata/qwen_record_auto_title.json"
        ))
        .expect("generated title sidecar");
        let prompt = serde_json::from_str(include_str!(
            "../../../testdata/qwen_record_prompt_title.json"
        ))
        .expect("opening prompt sidecar");
        assert_eq!(
            adapter.summary_title(&manual).as_deref(),
            Some("Explicit Probe Title")
        );
        assert_eq!(
            adapter.summary_title(&generated).as_deref(),
            Some("Stub generated title")
        );
        assert_eq!(adapter.summary_title(&prompt), None);
    }

    // Qwen Code 0.28.3, one turn each, with the OSC 7501 reports and the titles as it wrote them,
    // a report right before its title (`testdata/qwen-captures.md`).
    /// working, `◐`; idle, plain.
    const CANCEL_ESC: &[u8] = include_bytes!("../../../testdata/qwen_turn_cancel_esc.bin");
    const CANCEL_CTRL_C: &[u8] = include_bytes!("../../../testdata/qwen_turn_cancel_ctrl_c.bin");
    /// working; done (the model's call ended); working; blocked `✳`; done, plain.
    const DECLINED: &[u8] = include_bytes!("../../../testdata/qwen_turn_permission_declined.bin");
    /// working; done; working; blocked `✳`; working `◐`; done, plain.
    const APPROVED: &[u8] = include_bytes!("../../../testdata/qwen_turn_permission_approved.bin");
    /// working; done (the gap between two model calls); working; done.
    const TWO_MODEL_CALLS: &[u8] =
        include_bytes!("../../../testdata/qwen_turn_two_model_calls.bin");

    struct Fixture {
        live: StandIn,
        state: Arc<AppState>,
        _dir: ScratchDir,
    }

    /// A Qwen Code session working on a turn its prompt opened, registered live so what the
    /// terminal printed can be fed to it.
    fn fixture(name: &str) -> Fixture {
        let (state, dir) = app_state(&format!("qwen-records-{name}"));
        state.store.insert_console(&console(&dir)).expect("console");
        state
            .store
            .insert_session(&Session {
                agent: AgentId::QWEN,
                ..console_session("s", SessionStatus::Working)
            })
            .expect("session");
        let live = idle_stand_in_of("s", AgentId::QWEN);
        state.register_live(live.clone());
        state.turn_started("s", None);
        Fixture {
            live,
            state,
            _dir: dir,
        }
    }

    fn status(state: &AppState) -> Option<SessionStatus> {
        state
            .store
            .get_session("s")
            .ok()
            .flatten()
            .map(|s| s.status)
    }

    fn interrupt<'a>() -> RecordWatch<'a> {
        RecordWatch::Interrupt {
            record: None,
            turn_id: None,
        }
    }

    /// Arms `watch` with the production settle: the title, or the status the program reports, has
    /// to hold at the prompt for a second.
    fn arm(fixture: &Fixture, watch: RecordWatch<'_>) {
        provider(AgentId::QWEN)
            .records()
            .arm_watch(&fixture.state, "s", watch);
    }

    /// Arms `watch` with no settle, for a case that is about what the watch reads and not how long
    /// it holds: the clock is the test's own and its polls need no real second to pass.
    fn arm_unsettled(fixture: &Fixture, watch: RecordWatch<'_>) {
        let _settle = without_settle();
        arm(fixture, watch);
    }

    /// Where each of Qwen Code's OSC 7501 reports starts in `capture`.
    fn report_starts(capture: &[u8]) -> Vec<usize> {
        let marker = b"\x1b]7501;state=";
        capture
            .windows(marker.len())
            .enumerate()
            .filter(|(_, window)| *window == marker)
            .map(|(at, _)| at)
            .collect()
    }

    /// `bytes` cut into the reads the terminal delivered them in: each ends where an OSC sequence,
    /// a report or a title, completes, so each completes in a read of its own, as on the PTY.
    fn reads(bytes: &[u8]) -> Vec<&[u8]> {
        let mut reads = Vec::new();
        let (mut start, mut at) = (0, 0);
        while at + 1 < bytes.len() {
            if bytes[at] == 0x1b && bytes[at + 1] == b']' {
                let mut end = at + 2;
                while end < bytes.len()
                    && bytes[end] != 0x07
                    && !(bytes[end] == 0x1b && bytes.get(end + 1) == Some(&b'\\'))
                {
                    end += 1;
                }
                end += if bytes.get(end) == Some(&0x07) { 1 } else { 2 };
                let end = end.min(bytes.len());
                reads.push(&bytes[start..end]);
                start = end;
                at = end;
            } else {
                at += 1;
            }
        }
        if start < bytes.len() {
            reads.push(&bytes[start..]);
        }
        reads
    }

    /// Feeds `capture[from..to]` to the session as the terminal delivered it.
    fn play(fixture: &Fixture, capture: &[u8], from: usize, to: usize) {
        for read in reads(&capture[from..to]) {
            fixture.live.on_output(read);
        }
    }

    /// Whether the session's turn has been closed within the patience: a cancel moves the status
    /// to idle first and closes the turn after, so a test looks at the turn only once it has.
    async fn turn_closes(state: &Arc<AppState>) -> bool {
        let state = state.clone();
        within(PATIENCE, move || !state.turn_is_open("s")).await
    }

    async fn reaches_idle(state: &Arc<AppState>) -> bool {
        let state = state.clone();
        within(PATIENCE, move || {
            status(&state) == Some(SessionStatus::Idle)
        })
        .await
    }

    /// A turn cancelled with Esc or with Ctrl-C fires no hook; what Qwen Code printed is a `working`
    /// report and its `◐` title, then `idle` and the plain title. Once that has held for the
    /// settle, the session is back at its prompt and the turn is closed as cancelled, so no end is
    /// owed for it.
    #[tokio::test]
    async fn a_turn_cancelled_with_esc_or_ctrl_c_is_closed_as_cancelled() {
        for (name, capture) in [("esc", CANCEL_ESC), ("ctrl-c", CANCEL_CTRL_C)] {
            let fixture = fixture(name);
            let starts = report_starts(capture);
            assert_eq!(starts.len(), 2, "{name}: working, then idle");
            // What was typed, then the prompt's hook arms the watch.
            play(&fixture, capture, 0, starts[0]);
            arm(&fixture, interrupt());
            play(&fixture, capture, starts[0], capture.len());
            assert!(
                reaches_idle(&fixture.state).await,
                "{name}: {:?}",
                status(&fixture.state)
            );
            assert!(
                turn_closes(&fixture.state).await,
                "{name}: the turn stayed open"
            );
            assert!(fixture.state.last_turn_abnormal("s"), "{name}");
            assert_eq!(
                fixture.state.close_turn("s", true),
                TurnClose::NoTurn,
                "{name}"
            );
        }
    }

    /// The cancel is read off the report, not off the watch: with no watch armed at all, `idle`
    /// straight after `working` puts the session at its prompt and closes the turn as cancelled.
    #[tokio::test]
    async fn a_reported_cancel_closes_the_turn_with_no_watch_armed() {
        for (name, capture) in [("esc", CANCEL_ESC), ("ctrl-c", CANCEL_CTRL_C)] {
            let fixture = fixture(&format!("reported-{name}"));
            play(&fixture, capture, 0, capture.len());
            assert!(
                reaches_idle(&fixture.state).await,
                "{name}: {:?}",
                status(&fixture.state)
            );
            // The status moves before the cancel closes the turn.
            let state = fixture.state.clone();
            assert!(
                within(PATIENCE, move || state
                    .live_session("s")
                    .is_some_and(|live| live.reported_cancels_settled() == 1))
                .await,
                "{name}: the cancel was not acted on"
            );
            assert!(fixture.state.last_turn_abnormal("s"), "{name}");
            assert_eq!(
                fixture.state.close_turn("s", true),
                TurnClose::NoTurn,
                "{name}"
            );
            assert_eq!(fixture.live.reported_cancels_started(), 1, "{name}");
        }
    }

    /// With no watch armed, the `done` of a declined prompt and the `done` between two model calls
    /// close nothing: only `idle` after `working` is a cancel, and neither capture's reports
    /// started one.
    #[tokio::test]
    async fn a_reported_done_closes_no_turn_by_itself() {
        for (name, capture) in [("declined", DECLINED), ("two-calls", TWO_MODEL_CALLS)] {
            let fixture = fixture(&format!("done-{name}"));
            play(&fixture, capture, 0, capture.len());
            assert_eq!(fixture.live.reported_cancels_started(), 0, "{name}");
            polls_settled().await;
            assert_eq!(
                status(&fixture.state),
                Some(SessionStatus::Working),
                "{name}"
            );
            assert!(!fixture.state.last_turn_abnormal("s"), "{name}");
        }
    }

    /// A launch with its hooks switched off takes its status from the reports, where the `done`
    /// between the two model calls of the capture is held for Qwen Code's declared gap and passed
    /// over by the next call's `working`: one turn is opened and ended, and the session's owner is
    /// told of it once. The turn's own `done` is applied once the test ends the window it is held
    /// in, which settling then waits for.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_launch_taking_its_status_from_the_reports_ends_one_turn_of_two_model_calls() {
        /// What the owner is told of a turn that ended with no message, as Qwen Code's reports
        /// carry none.
        const ENDED: &str = "stopped without reporting";
        let (state, _dir, hub) = crate::test_support::owner_team::team_state(
            "qwen-records-reports-two-calls",
            &[("s", "hub", false, SessionStatus::Idle)],
        );
        let mut record = state.session_record("s").expect("the session");
        record.agent = AgentId::QWEN;
        record.has_conversation = false;
        state.store.update_session(&record).expect("update");
        // The stand-in is launched as one whose hooks are off: its status from its reports.
        let _hooks_off =
            crate::test_support::test_provider::declaring_launch_without_status_integration();
        let live = crate::test_support::fake_live_session("s", AgentId::QWEN, 80, 24, "sleep 30");
        state.register_live(live.clone());
        assert_eq!(report_starts(TWO_MODEL_CALLS).len(), 4);

        for read in reads(TWO_MODEL_CALLS) {
            live.on_output(read);
        }
        // The capture is all in: the `done` still held is the turn's own.
        live.end_reported_done_gaps();
        live.settle_program_status().await;

        // One end tells one turn from two: a second turn opening would have ended the first.
        // (`turn_opened` is the number the turn got from a counter all sessions share.)
        assert_eq!(state.turn_ends("s"), 1, "one turn, ended once");
        assert!(!state.turn_is_open("s"));
        assert!(!state.last_turn_abnormal("s"));
        assert_eq!(status(&state), Some(SessionStatus::Idle));
        let heard = crate::test_support::owner_team::hub_heard_of(&hub, ENDED).await;
        assert_eq!(heard.matches(ENDED).count(), 1, "{heard}");
    }

    /// A message Octoboard wrote into a running turn of a launch with its hooks on is held by Qwen
    /// Code for the next model call; the Ctrl-C of the capture drops it, and the cancel its reports
    /// declare (`idle` after `working`) has it queued again, held back for after the next message.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_a_ctrl_c_dropped_is_queued_again() {
        const TICKET: u64 = 7;
        let fixture = fixture("held-ctrl-c");
        let starts = report_starts(CANCEL_CTRL_C);
        assert_eq!(starts.len(), 2, "working, then idle");
        // The turn runs: its `working` report and its title.
        play(&fixture, CANCEL_CTRL_C, 0, starts[1]);
        fixture
            .state
            .outbox
            .note_latest("s", TICKET, "peer", false, false);
        let framed = crate::session::term::frame_message(AgentId::QWEN, "peer");
        fixture
            .live
            .write_message_input_as(&format!("launch:{TICKET}"), &framed, "peer", false)
            .expect("the message is written");
        // The Ctrl-C: `idle` and the plain title.
        play(&fixture, CANCEL_CTRL_C, starts[1], CANCEL_CTRL_C.len());

        // The message is queued again before the status moves to the cancelled turn's end, so
        // both are waited for.
        let state = fixture.state.clone();
        assert!(
            within(PATIENCE, move || state.queued_count("s") == 1
                && state.last_turn_abnormal("s"))
            .await,
            "queued again: {}, closed as cancelled: {}",
            fixture.state.queued_count("s"),
            fixture.state.last_turn_abnormal("s")
        );
        assert_eq!(fixture.state.take_queued("s"), ["peer"]);
    }

    /// The wait on the user is read off the reports: `blocked` raises the session's hand at the
    /// permission prompt, and the report that follows lowers it, whether the prompt was approved
    /// (`working`) or declined (`done`).
    #[tokio::test]
    async fn a_blocked_report_raises_the_hand_and_the_next_report_lowers_it() {
        for (name, capture) in [("approved", APPROVED), ("declined", DECLINED)] {
            let fixture = fixture(&format!("hand-{name}"));
            let starts = report_starts(capture);
            // working, done, working: no wait yet.
            play(&fixture, capture, 0, starts[3]);
            assert!(
                !fixture.live.needs_user_action(),
                "{name}: before the prompt"
            );
            // `blocked`, up to the report that answers it.
            play(&fixture, capture, starts[3], starts[4]);
            assert!(fixture.live.needs_user_action(), "{name}: at the prompt");
            play(&fixture, capture, starts[4], capture.len());
            assert!(!fixture.live.needs_user_action(), "{name}: answered");
        }
    }

    /// A permission prompt declined with Esc fires no hook either: the `blocked` report and the
    /// `✳` title, then `done` and the plain title, close the turn as cancelled.
    #[tokio::test]
    async fn a_declined_permission_prompt_closes_the_turn() {
        let fixture = fixture("declined");
        let starts = report_starts(DECLINED);
        assert_eq!(starts.len(), 5, "working, done, working, blocked, done");
        play(&fixture, DECLINED, 0, starts[0]);
        arm(&fixture, interrupt());
        play(&fixture, DECLINED, starts[0], DECLINED.len());
        assert!(
            reaches_idle(&fixture.state).await,
            "{:?}",
            status(&fixture.state)
        );
        assert!(turn_closes(&fixture.state).await, "the turn stayed open");
        assert!(fixture.state.last_turn_abnormal("s"));
        assert_eq!(fixture.state.close_turn("s", true), TurnClose::NoTurn);
    }

    /// While the prompt waits the reports read `blocked` and the title `✳`, which is no turn gone
    /// back to its prompt: nothing closes the turn, however it is read (the watch runs here with no
    /// settle, so that only what it reads keeps the turn). Approved, the turn runs on and is not
    /// closed at the approval; declined, it is closed only by the `done` after.
    #[tokio::test]
    async fn a_prompt_waiting_on_the_user_does_not_close_the_turn_and_its_approval_does_not() {
        for (name, capture) in [("approved", APPROVED), ("declined", DECLINED)] {
            let fixture = fixture(&format!("waiting-{name}"));
            let starts = report_starts(capture);
            play(&fixture, capture, 0, starts[0]);
            arm_unsettled(&fixture, interrupt());
            // working, done, working, then the prompt: blocked and `✳`. The `done` before it is
            // the model's own call ending and is passed over with no poll between.
            play(&fixture, capture, starts[0], starts[4]);
            polls_settled().await;
            assert_eq!(
                status(&fixture.state),
                Some(SessionStatus::Working),
                "{name}: waiting"
            );
            assert!(!fixture.state.last_turn_abnormal("s"), "{name}: waiting");

            if name == "approved" {
                // Enter: `working` and `◐` again.
                play(&fixture, capture, starts[4], starts[5]);
                polls_settled().await;
                assert_eq!(
                    status(&fixture.state),
                    Some(SessionStatus::Working),
                    "approved"
                );
                assert!(!fixture.state.last_turn_abnormal("s"), "approved");
                assert_eq!(fixture.state.close_turn("s", false), TurnClose::OwesReport);
            } else {
                // Esc: `done` and the plain title.
                play(&fixture, capture, starts[4], capture.len());
                assert!(reaches_idle(&fixture.state).await, "declined");
                assert!(fixture.state.last_turn_abnormal("s"), "declined");
            }
        }
    }

    /// Between two model calls of one turn Qwen Code reports `done` and writes the plain title for
    /// a moment, then `working` again: to the watch the gap is a return to the prompt like a
    /// cancel's, and only the settle tells them apart. With the settle held open the turn is still
    /// running after the gap and after the second call starts; with no settle the same gap closes
    /// it. That the settle the daemon runs with is longer than the gap is a fact about a constant
    /// (`title_interrupt::SETTLE`, one second) and a measurement (0.05 s to 0.6 s, in
    /// `docs/agent-cli-reference.md`, "Qwen Code"), which this test does not check.
    #[tokio::test]
    async fn the_settle_alone_keeps_the_gap_between_two_model_calls_from_closing_the_turn() {
        let starts = report_starts(TWO_MODEL_CALLS);
        assert_eq!(starts.len(), 4, "working, done, working, done");

        // The settle is held open, from before the watch is armed: the titles are stamped with
        // the wall clock, so a stall between two reads would otherwise run it out and close the turn.
        let _settle = with_settle_held();
        let settled = fixture("two-calls");
        play(&settled, TWO_MODEL_CALLS, 0, starts[0]);
        arm(&settled, interrupt());
        // working, then the gap: done and the plain title.
        play(&settled, TWO_MODEL_CALLS, starts[0], starts[2]);
        polls_settled().await;
        assert_eq!(status(&settled.state), Some(SessionStatus::Working));
        assert!(!settled.state.last_turn_abnormal("s"));
        // The second call starts.
        play(&settled, TWO_MODEL_CALLS, starts[2], starts[3]);
        polls_settled().await;
        assert_eq!(status(&settled.state), Some(SessionStatus::Working));
        assert!(!settled.state.last_turn_abnormal("s"));
        assert_eq!(settled.state.close_turn("s", false), TurnClose::OwesReport);

        let control = fixture("two-calls-control");
        play(&control, TWO_MODEL_CALLS, 0, starts[0]);
        arm_unsettled(&control, interrupt());
        play(&control, TWO_MODEL_CALLS, starts[0], starts[2]);
        polls_settled().await;
        assert_eq!(
            status(&control.state),
            Some(SessionStatus::Idle),
            "the control: with no settle the gap reads as a cancel"
        );
    }

    /// The fallback where Qwen Code has made no OSC 7501 report: its title alone, the running
    /// glyph then the plain title, puts the session back at its prompt. Hand-built, since a Qwen
    /// Code that reports takes this over (the captures above).
    #[tokio::test]
    async fn a_cancel_shown_only_by_the_title_puts_the_session_at_its_prompt() {
        let title = |prefix: &str| format!("\x1b]0;{prefix}Qwen - proj{:<60}\x07", "").into_bytes();
        let fixture = fixture("title-only");
        // A rendered console ignores titles before the agent draws its first row.
        fixture.live.on_output(b"Qwen prompt");
        arm_unsettled(&fixture, interrupt());
        fixture.live.on_output(&title("\u{25D0}\u{FE0E} "));
        fixture.live.on_output(&title(""));
        assert!(
            reaches_idle(&fixture.state).await,
            "{:?}",
            status(&fixture.state)
        );
        assert!(fixture.state.last_turn_abnormal("s"));
    }

    /// The watch is the interrupt watch alone: a decline or a failure watch starts nothing for Qwen
    /// Code, so the declined prompt the reports show changes nothing then, where the same decline
    /// under an interrupt watch puts the session at its prompt. (A cancel would not tell the two
    /// apart: its report closes the turn whatever watch is armed.)
    #[tokio::test]
    async fn only_an_interrupt_watch_is_started() {
        let starts = report_starts(DECLINED);
        let cancel = |fixture: &Fixture, watch: RecordWatch<'_>| {
            play(fixture, DECLINED, 0, starts[0]);
            arm_unsettled(fixture, watch);
            play(fixture, DECLINED, starts[0], DECLINED.len());
        };

        let control = fixture("control");
        cancel(&control, interrupt());
        polls_settled().await;
        assert_eq!(
            status(&control.state),
            Some(SessionStatus::Idle),
            "the control: an interrupt watch settles within the controlled clock"
        );

        for (name, watch) in [
            ("decline", RecordWatch::Decline { record: None }),
            (
                "failure",
                RecordWatch::Failure {
                    record: None,
                    turn_id: None,
                },
            ),
        ] {
            let fixture = fixture(name);
            cancel(&fixture, watch);
            polls_settled().await;
            assert_eq!(
                status(&fixture.state),
                Some(SessionStatus::Working),
                "{name}"
            );
        }
    }
}
