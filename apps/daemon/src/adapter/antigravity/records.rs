//! The CLI resumes from the conversation database, while history and context come from the full
//! transcript in `brain/<id>/`. The database's SQLite sidecars, title annotation and presence lock belong to
//! that same conversation. Shared indexes and prompt recall belong to the account.

use std::borrow::Cow;
use std::fs::FileType;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::hooks::{FILE_WRITING_TOOLS, REQUEST_CLOSE, REQUEST_OPEN, TRANSCRIPT_IN_CONVERSATION};
use crate::protocol::{ToolCategory, ToolStatus};
use crate::providers::antigravity::{APP_DATA_DIR, CONVERSATIONS_DIR, LAYOUT};
use crate::records::branch::{StepIndex, Steps};
use crate::records::context::Context;
use crate::records::conversation::{text_of, Entry};
use crate::records::feed::{FeedOut, FeedReader, ToolCall, ToolOutcome};
use crate::records::observed::Observed;
use crate::records::{plausible_id, Branch, RecordStore};

pub(super) struct AntigravityRecords;

impl RecordStore for AntigravityRecords {
    fn indexed(
        &self,
        path: &Path,
        name: &str,
        kind: FileType,
        leaf: bool,
    ) -> Option<(String, PathBuf)> {
        let id = name.strip_suffix(".db")?;
        (leaf && kind.is_file() && plausible_id(id)).then(|| (id.to_owned(), path.to_path_buf()))
    }

    fn own_paths(&self, id: &str, record: &Path, levels_below_dir: usize) -> Vec<PathBuf> {
        if levels_below_dir != LAYOUT.record_level
            || record.file_name().and_then(|name| name.to_str()) != Some(&format!("{id}.db"))
        {
            return Vec::new();
        }
        // The generic depth stops at conversations/. Check its two enclosing directories too
        // before marking a database as removable.
        let Some(data) = record.parent().and_then(Path::parent) else {
            return Vec::new();
        };
        if !std::iter::once(data)
            .chain(data.parent())
            .all(|dir| std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.file_type().is_dir()))
        {
            return Vec::new();
        }
        vec![record.to_path_buf()]
    }

    fn names_record(&self, name: &str, kind: FileType, id: &str) -> bool {
        kind.is_file() && name.strip_suffix(".db") == Some(id)
    }

    fn recognises_records(&self) -> bool {
        true
    }

    fn absence_is_conclusive(&self) -> bool {
        true
    }

    fn beside_records(&self, root: &Path, id: &str) -> Vec<PathBuf> {
        let data = root.join(APP_DATA_DIR);
        let db = root.join(CONVERSATIONS_DIR);
        vec![
            data.join("brain").join(id),
            data.join("presence").join(format!("{id}.lock")),
            db.join(format!("{id}.db-wal")),
            db.join(format!("{id}.db-shm")),
            super::title::annotation_file(&data.join(super::title::ANNOTATIONS_DIR), id),
        ]
    }

    fn removes_beside_records(&self) -> bool {
        true
    }

    fn conversation_file(&self, record: PathBuf) -> PathBuf {
        self.followed_file(&record)
    }

    // The transcript can outlive the database. It still serves history, even though the CLI can
    // no longer resume that conversation.
    fn archived_branch(&self, roots: &[PathBuf], id: &str) -> Option<Branch> {
        let files = roots
            .iter()
            .filter_map(|root| transcript_in(&root.join(APP_DATA_DIR), id))
            .collect();
        self.branch_among(files)
    }

    fn followed_file(&self, record: &Path) -> PathBuf {
        let transcript = record
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".db"))
            .zip(record.parent().and_then(Path::parent))
            .and_then(|(id, data)| transcript_in(data, id));
        // An absent transcript may appear later. Keep it unreadable until its real path is known.
        transcript.unwrap_or_else(|| record.join("transcript_full.jsonl"))
    }

    fn observes_record_lines(&self) -> bool {
        false
    }

    fn fold_line(&self, line: &str, _: bool, context: &mut Context, _: &mut Observed) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return;
        };
        if kind(&value) == Kind::Reply {
            if let Some(tokens) = value.get("input_tokens").and_then(Value::as_u64) {
                context.observe(tokens);
            }
        }
    }

    fn entries(&self, line: &Value) -> Vec<Entry> {
        let message = match kind(line) {
            Kind::User => {
                return text_of(line, "content")
                    .map(Entry::User)
                    .into_iter()
                    .collect()
            }
            Kind::Reply => text_of(line, "content").map(Entry::Assistant),
            _ => return Vec::new(),
        };
        let changed: Vec<_> = calls(line)
            .filter_map(|(name, args)| edit_path(name, args))
            .collect();
        let mut entries: Vec<_> = message.into_iter().collect();
        if !changed.is_empty() {
            entries.push(Entry::Changed(changed));
        }
        entries
    }

    fn history_step_index(&self) -> Option<StepIndex> {
        Some(step_index)
    }

    fn history_at(&self, line: &Value) -> Option<i64> {
        step_time(line)
    }

    fn user_text<'a>(&self, text: &'a str) -> Option<Cow<'a, str>> {
        request_text(text).map(Cow::Borrowed)
    }

    fn is_user_prompt(&self, line: &str) -> Option<bool> {
        let value = serde_json::from_str::<Value>(line).ok()?;
        value.get("type")?.as_str()?;
        value.get("source")?.as_str()?;
        if kind(&value) != Kind::User {
            return Some(false);
        }
        let text = value["content"].as_str()?;
        request_text(text).map(|_| true)
    }

    fn feed_reader(&self) -> Option<Box<dyn FeedReader>> {
        Some(Box::<AntigravityFeed>::default())
    }
}

/// Use the native record's location, never a path reported by an event. Require the transcript to
/// resolve exactly below that data directory, so links inside it cannot redirect a read.
fn transcript_in(data: &Path, id: &str) -> Option<PathBuf> {
    if !plausible_id(id) {
        return None;
    }
    let mut relative = PathBuf::from("brain").join(id);
    relative.extend(TRANSCRIPT_IN_CONVERSATION);
    let expected = crate::hostfs::canonical(data).ok()?.join(&relative);
    let actual = crate::hostfs::canonical(data.join(relative)).ok()?;
    (actual == expected && actual.is_file()).then_some(actual)
}

#[derive(PartialEq, Eq)]
enum Kind {
    User,
    Reply,
    ToolResult,
    Other,
}

fn kind(line: &Value) -> Kind {
    match (line["source"].as_str(), line["type"].as_str()) {
        (Some("USER_EXPLICIT"), Some("USER_INPUT")) => Kind::User,
        (Some("MODEL"), Some("PLANNER_RESPONSE")) => Kind::Reply,
        (Some("MODEL"), Some("GENERIC")) => Kind::ToolResult,
        _ => Kind::Other,
    }
}

fn request_text(text: &str) -> Option<&str> {
    let request = text.strip_prefix(REQUEST_OPEN)?;
    let (text, _) = request.rsplit_once(REQUEST_CLOSE)?;
    Some(text)
}

fn calls(line: &Value) -> impl Iterator<Item = (&str, &Value)> {
    line["tool_calls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|call| Some((call["name"].as_str()?, call.get("args")?)))
}

fn edit_path(name: &str, args: &Value) -> Option<String> {
    if FILE_WRITING_TOOLS.contains(&name) {
        text_of(args, "Path")
    } else {
        None
    }
}

fn step_index(line: &Value) -> Option<u64> {
    line["step_index"].as_u64()
}

fn step_time(line: &Value) -> Option<i64> {
    line["created_at"]
        .as_str()
        .and_then(crate::time::parse_rfc3339_millis)
}

#[derive(Default)]
struct AntigravityFeed {
    steps: Steps,
    /// A result has no call id. Only a response with one call can be paired without guessing.
    single_call: Option<String>,
}

impl FeedReader for AntigravityFeed {
    fn line(&mut self, line: &Value, out: &mut FeedOut) {
        let step = step_index(line);
        if let Some(len) = step.and_then(|index| self.steps.follow(index, out.len())) {
            out.truncate(len);
            self.single_call = None;
        }
        let at = step_time(line);
        match kind(line) {
            Kind::User => {
                self.single_call = None;
                if let Some(text) = line["content"].as_str().and_then(request_text) {
                    out.user(text, at);
                }
            }
            Kind::Reply => {
                self.single_call = None;
                if let Some(text) = line["content"].as_str() {
                    out.assistant(text, at);
                }
                let Some(step) = step else {
                    return;
                };
                let mut calls = calls(line).enumerate().peekable();
                while let Some((index, (name, args))) = calls.next() {
                    let id = format!("{step}:{index}");
                    if index == 0 && calls.peek().is_none() {
                        self.single_call = Some(id.clone());
                    }
                    let category = match name {
                        "run_command" => ToolCategory::Command,
                        "write_file" | "edit_file" => ToolCategory::FileEdit,
                        "view_file" => ToolCategory::FileRead,
                        "call_mcp_tool" => ToolCategory::Mcp,
                        _ => ToolCategory::Other,
                    };
                    out.tool_call(
                        ToolCall {
                            call_id: id,
                            name: name.to_owned(),
                            category,
                            title: None,
                            input: args.clone(),
                            paths: edit_path(name, args).into_iter().collect(),
                            status: ToolStatus::Running,
                            output: None,
                            exit_code: None,
                        },
                        at,
                    );
                }
            }
            Kind::ToolResult => {
                if let Some(id) = self.single_call.take() {
                    out.tool_result(
                        &id,
                        ToolOutcome {
                            status: ToolStatus::Completed,
                            output: line["content"].as_str().map(str::to_owned),
                            exit_code: None,
                        },
                    );
                }
            }
            Kind::Other => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::protocol::{AgentId, ConversationBody};
    use crate::records::feed::follow::Follower;
    use crate::records::fixtures::{session, touch};
    use crate::records::{
        history, removable_record_paths, resumability, Hints, Locator, Resumability,
    };
    use crate::test_support::ScratchDir;

    const ID: &str = "11111111-2222-3333-4444-555555555555";
    const TRANSCRIPT: &str = include_str!("../../../testdata/antigravity_transcript_full.jsonl");

    #[test]
    fn antigravity_history_feed_and_context_read_the_full_transcript() {
        assert_eq!(
            TRANSCRIPT
                .lines()
                .filter(|line| AntigravityRecords.is_user_prompt(line) == Some(true))
                .count(),
            4
        );
        let home = ScratchDir::new("antigravity-record");
        let account = home.join(".gemini");
        let db = account.join(CONVERSATIONS_DIR).join(format!("{ID}.db"));
        touch(&db);
        let mut transcript = account.join(APP_DATA_DIR).join("brain").join(ID);
        transcript.extend(TRANSCRIPT_IN_CONVERSATION);
        touch(&transcript);
        std::fs::write(&transcript, TRANSCRIPT).unwrap();

        let id = AgentId::ANTIGRAVITY;
        let session = session(id, ID, None);
        let hints = Hints::default();
        let mut locator = Locator::with_shell(HashMap::from([(
            "HOME".to_owned(),
            home.to_string_lossy().into_owned(),
        )]));
        let history = history::of_session(&hints, &mut locator, &[], &session).unwrap();
        assert_eq!(history[0].text, "LONGREPLY please");
        assert_eq!(history[1].text.len(), 5010);
        assert_eq!(history[3].text, "Stub: tool finished.");

        let branch =
            crate::records::conversation_branch(&hints, &mut locator, &[], &session).unwrap();
        let feed = Follower::build(id, branch.stretches.unwrap()).out;
        assert!(
            matches!(&feed.entries()[0].body, ConversationBody::User { text } if text == "LONGREPLY please")
        );
        assert!(feed.entries()[0].at.is_some());
        assert!(feed.entries().iter().any(|entry| matches!(&entry.body,
            ConversationBody::ToolCall(call) if call.name == "call_mcp_tool" && call.status == ToolStatus::Completed)));

        let mut context = Context::default();
        for line in TRANSCRIPT.lines() {
            AntigravityRecords.fold_line(line, false, &mut context, &mut Observed::default());
        }
        let figure = context.context(&AntigravityRecords).flatten().unwrap();
        assert_eq!(figure.tokens, 5);
        assert_eq!(figure.window, None);
    }

    #[test]
    fn antigravity_archived_history_dates_the_current_steps_after_rewind() {
        use crate::protocol::HistorySpeaker;

        let home = ScratchDir::new("antigravity-rewound-history");
        let account = home.join(".gemini");
        let mut transcript = account.join(APP_DATA_DIR).join("brain").join(ID);
        transcript.extend(TRANSCRIPT_IN_CONVERSATION);
        touch(&transcript);
        std::fs::write(
            &transcript,
            include_str!("../../../testdata/antigravity_transcript_rewind.jsonl"),
        )
        .unwrap();

        let session = session(AgentId::ANTIGRAVITY, ID, None);
        let hints = Hints::default();
        let mut locator = Locator::with_shell(HashMap::from([(
            "HOME".to_owned(),
            home.to_string_lossy().into_owned(),
        )]));
        let history = history::of_session(&hints, &mut locator, &[], &session).unwrap();
        let actual = history
            .iter()
            .map(|message| {
                (
                    message.speaker,
                    message.text.as_str(),
                    message.turn,
                    message.at,
                )
            })
            .collect::<Vec<_>>();
        let first = Some(1_795_495_387_000);
        let second = Some(1_795_495_391_000);
        let replacement = Some(1_795_495_401_000);
        assert_eq!(
            actual,
            [
                (HistorySpeaker::User, "M33_ORIGINAL", 1, first),
                (HistorySpeaker::Agent, "stand-in answer", 1, first),
                (HistorySpeaker::User, "M33_SECOND", 2, second),
                (HistorySpeaker::Agent, "stand-in answer", 2, second),
                (HistorySpeaker::User, "M33_AFTER", 3, replacement),
                (HistorySpeaker::Agent, "stand-in answer", 3, replacement),
            ]
        );
    }

    #[test]
    fn antigravity_live_feed_replaces_steps_reused_after_rewind() {
        let lines = include_str!("../../../testdata/antigravity_transcript_rewind.jsonl");
        let mut out = FeedOut::default();
        let mut reader = AntigravityRecords.feed_reader().unwrap();
        for (index, line) in lines.lines().enumerate() {
            reader.line(&serde_json::from_str(line).unwrap(), &mut out);
            if index == 6 {
                assert_eq!(out.take_changes().appended.len(), 6);
            }
        }
        let changes = out.take_changes();
        assert!(changes.replaced);
        let prompts: Vec<_> = out
            .entries()
            .iter()
            .filter_map(|entry| match &entry.body {
                ConversationBody::User { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(prompts, ["M33_ORIGINAL", "M33_SECOND", "M33_AFTER"]);
    }

    #[test]
    fn antigravity_resumes_from_the_database_and_removes_only_its_conversation_files() {
        let home = ScratchDir::new("antigravity-removal");
        let account = home.join(".gemini");
        let data = account.join(APP_DATA_DIR);
        let db_dir = account.join(CONVERSATIONS_DIR);
        let db = db_dir.join(format!("{ID}.db"));
        let brain = data.join("brain").join(ID);
        let presence = data.join("presence").join(format!("{ID}.lock"));
        let wal = db_dir.join(format!("{ID}.db-wal"));
        let shm = db_dir.join(format!("{ID}.db-shm"));
        let shared = data.join("conversation_summaries.db");
        let annotation = data.join("annotations").join(format!("{ID}.pbtxt"));
        for file in [
            &db,
            &presence,
            &wal,
            &shm,
            &annotation,
            &shared,
            &brain.join("artifact"),
        ] {
            touch(file);
        }
        let locator = || {
            Locator::with_shell(HashMap::from([(
                "HOME".to_owned(),
                home.to_string_lossy().into_owned(),
            )]))
        };
        let hints = Hints::default();
        let session = session(AgentId::ANTIGRAVITY, ID, None);
        assert_eq!(resumability(&locator(), &session), Resumability::Resumable);
        let paths = removable_record_paths(&hints, &mut locator(), &[], &session);
        assert_eq!(paths, [db.clone(), brain, presence, wal, shm, annotation]);
        std::fs::remove_file(db).unwrap();
        assert_eq!(
            resumability(&locator(), &session),
            Resumability::Unresumable
        );
    }
}
