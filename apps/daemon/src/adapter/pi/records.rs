//! Pi keeps each conversation in one JSONL file. Entries name their parent; the shared record
//! readers follow that ancestry before asking the provider to interpret the entries.

use std::fs::FileType;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::hooks::FILE_WRITING_TOOLS;
use crate::protocol::{AgentId, ToolCategory, ToolStatus};
use crate::providers::pi::LAYOUT;
use crate::records::branch::{Branching, Link};
use crate::records::context::Context;
use crate::records::conversation::{at_of, text_of, Entry};
use crate::records::feed::{EntriesReader, FeedOut, FeedReader, ToolCall, ToolOutcome};
use crate::records::observed::Observed;
use crate::records::RecordStore;

pub(super) struct PiRecords;

impl Branching for PiRecords {
    fn link(&self, line: &Value) -> Option<Link> {
        if line.get("type")?.as_str()? == "session" {
            return None;
        }
        Some(Link {
            id: line["id"].as_str()?.to_owned(),
            parent: line["parentId"].as_str().map(str::to_owned),
        })
    }
}

const ALTERNATE_RECORD_LEVEL: usize = 1;

impl RecordStore for PiRecords {
    fn record_root_override(&self) -> Option<crate::records::store::RecordRootOverride> {
        Some(crate::records::store::RecordRootOverride {
            variable: "PI_CODING_AGENT_SESSION_DIR",
            search_depth: ALTERNATE_RECORD_LEVEL,
        })
    }

    fn indexed(
        &self,
        path: &Path,
        name: &str,
        kind: FileType,
        leaf: bool,
    ) -> Option<(String, PathBuf)> {
        if !kind.is_file() || !leaf {
            return None;
        }
        Some((record_id(name)?.to_ascii_lowercase(), path.to_owned()))
    }

    fn own_paths(&self, agent_id: &str, record: &Path, levels_below_dir: usize) -> Vec<PathBuf> {
        let id = record
            .file_name()
            .and_then(|name| record_id(name.to_str()?));
        if (levels_below_dir == LAYOUT.record_level || levels_below_dir == ALTERNATE_RECORD_LEVEL)
            && id.is_some_and(|id| id.eq_ignore_ascii_case(agent_id))
        {
            vec![record.to_owned()]
        } else {
            Vec::new()
        }
    }

    fn names_record(&self, name: &str, kind: FileType, id: &str) -> bool {
        kind.is_file() && record_id(name) == Some(id)
    }

    fn recognises_records(&self) -> bool {
        true
    }

    fn absence_is_conclusive(&self) -> bool {
        true
    }

    fn branching(&self) -> Option<&dyn Branching> {
        Some(self)
    }

    fn entries(&self, line: &Value) -> Vec<Entry> {
        // Pi retains the messages a compaction summarized. History and handoffs read those
        // messages, while the live feed also marks where the context was compacted.
        if line["type"] == "message" {
            message_entries(&line["message"])
        } else {
            Vec::new()
        }
    }

    fn is_user_prompt(&self, line: &str) -> Option<bool> {
        let line: Value = serde_json::from_str(line).ok()?;
        match line["type"].as_str()? {
            "message" => match line["message"]["role"].as_str()? {
                "user" => Some(true),
                "assistant" | "system" | "toolResult" | "bashExecution" | "custom"
                | "branchSummary" | "compactionSummary" => Some(false),
                _ => None,
            },
            "session"
            | "model_change"
            | "thinking_level_change"
            | "compaction"
            | "context_edit"
            | "branch_summary"
            | "custom"
            | "custom_message"
            | "label"
            | "session_info" => Some(false),
            _ => None,
        }
    }

    fn fold_line(
        &self,
        line: &str,
        past_boundary: bool,
        context: &mut Context,
        observed: &mut Observed,
    ) {
        let Ok(line) = serde_json::from_str::<Value>(line) else {
            return;
        };
        match line["type"].as_str() {
            Some("model_change") => {
                context.compacted();
                if past_boundary {
                    observed.fold(model_of(&line, "modelId"), None);
                }
            }
            Some("thinking_level_change") if past_boundary => {
                observed.fold(None, text_of(&line, "thinkingLevel"));
            }
            // Pi discards usage preceding a context edit and has no token reading immediately
            // after compaction. A later successful reply supplies a fresh figure.
            Some("compaction" | "context_edit") => context.compacted(),
            Some("message") if line["message"]["role"] == "assistant" => {
                let message = &line["message"];
                if let Some(tokens) = assistant_usage(message) {
                    context.observe(tokens);
                }
                if past_boundary {
                    observed.fold(model_of(message, "model"), None);
                }
            }
            _ => {}
        }
    }

    fn feed_reader(&self) -> Option<Box<dyn FeedReader>> {
        Some(Box::new(PiFeed(EntriesReader::new(AgentId::PI))))
    }
}

/// The records directory among `kept`, the ones the session's earlier launches selected through
/// `PI_CODING_AGENT_SESSION_DIR` (`LaunchSpec::record_roots`), that holds the record of
/// conversation `id` among its own entries, where Pi puts a record in such a directory; the first
/// in their order. `None` where none does, or none can be listed: a resume then goes by the
/// directory the current environment selects, as a plain terminal would. Measured on 1.5.2:
/// `--session <id>` looks for the record only in the directory the environment selects, and
/// `--session-dir <dir>` makes it look in `<dir>`, whatever the environment selects
/// (`docs/agent-cli-reference.md`, "Pi", "A record kept in an earlier records directory").
pub(super) fn kept_record_dir<'a>(kept: &'a [PathBuf], id: &str) -> Option<&'a Path> {
    kept.iter()
        .filter(|dir| dir.is_absolute())
        .find(|dir| {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return false;
            };
            entries.flatten().any(|entry| {
                entry.file_type().is_ok_and(|kind| kind.is_file())
                    && entry
                        .file_name()
                        .to_str()
                        .and_then(record_id)
                        .is_some_and(|found| found.eq_ignore_ascii_case(id))
            })
        })
        .map(PathBuf::as_path)
}

fn record_id(name: &str) -> Option<&str> {
    let (timestamp, id) = name.strip_suffix(".jsonl")?.rsplit_once('_')?;
    if !timestamp.ends_with(['Z', 'z']) || !crate::records::locate::plausible_id(id) {
        return None;
    }
    Some(id)
}

fn model_of(line: &Value, key: &str) -> Option<String> {
    Some(format!(
        "{}/{}",
        line["provider"].as_str()?,
        line[key].as_str()?
    ))
}

fn assistant_usage(message: &Value) -> Option<u64> {
    if matches!(message["stopReason"].as_str(), Some("aborted" | "error")) {
        return None;
    }
    let usage = &message["usage"];
    let total = usage["totalTokens"].as_u64().filter(|&total| total != 0);
    total.or_else(|| {
        let total = ["input", "output", "cacheRead", "cacheWrite"]
            .into_iter()
            .filter_map(|key| usage[key].as_u64())
            .fold(0, u64::saturating_add);
        (total != 0).then_some(total)
    })
}

fn blocks(message: &Value) -> impl Iterator<Item = &Value> {
    message["content"].as_array().into_iter().flatten()
}

fn message_text(message: &Value) -> String {
    if let Some(text) = message["content"].as_str() {
        return text.to_owned();
    }
    blocks(message)
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("")
}

fn tool_paths(block: &Value) -> Vec<String> {
    match block["name"].as_str() {
        Some(name) if FILE_WRITING_TOOLS.contains(&name) || name == "read" => {
            text_of(&block["arguments"], "path").into_iter().collect()
        }
        _ => Vec::new(),
    }
}

fn message_entries(message: &Value) -> Vec<Entry> {
    let entry = match message["role"].as_str() {
        Some("user") => Entry::User,
        Some("assistant") => Entry::Assistant,
        _ => return Vec::new(),
    };
    let mut entries = Vec::new();
    let text = message_text(message);
    if !text.trim().is_empty() {
        entries.push(entry(text));
    }
    if message["role"] == "assistant" {
        let paths: Vec<String> = blocks(message)
            .filter(|block| {
                block["type"] == "toolCall"
                    && block["name"]
                        .as_str()
                        .is_some_and(|name| FILE_WRITING_TOOLS.contains(&name))
            })
            .flat_map(tool_paths)
            .collect();
        if !paths.is_empty() {
            entries.push(Entry::Changed(paths));
        }
    }
    entries
}

struct PiFeed(EntriesReader);

impl FeedReader for PiFeed {
    fn line(&mut self, line: &Value, out: &mut FeedOut) {
        self.0.line(line, out);
        if line["type"] == "compaction" {
            out.compaction(text_of(line, "summary"), at_of(line));
            return;
        }
        if line["type"] != "message" {
            return;
        }
        let message = &line["message"];
        match message["role"].as_str() {
            Some("assistant") => {
                for block in blocks(message).filter(|block| block["type"] == "toolCall") {
                    if let Some(call) = tool_call(block) {
                        out.tool_call(call, at_of(line));
                    }
                }
            }
            Some("toolResult") => {
                if let Some(id) = message["toolCallId"].as_str() {
                    let status = if message["isError"] == true {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Completed
                    };
                    let output = message_text(message);
                    out.tool_result(
                        id,
                        ToolOutcome {
                            status,
                            output: (!output.is_empty()).then_some(output),
                            exit_code: message["details"]["exitCode"].as_i64(),
                        },
                    );
                }
            }
            _ => {}
        }
    }
}

fn tool_call(block: &Value) -> Option<ToolCall> {
    let name = block["name"].as_str()?;
    let category = match name {
        "bash" | "powershell" => ToolCategory::Command,
        "write" | "edit" => ToolCategory::FileEdit,
        "read" => ToolCategory::FileRead,
        "grep" | "find" | "ls" => ToolCategory::Search,
        name if name.starts_with("mcp__") => ToolCategory::Mcp,
        _ => ToolCategory::Other,
    };
    Some(ToolCall {
        call_id: block["id"].as_str()?.to_owned(),
        name: name.to_owned(),
        category,
        title: None,
        input: block["arguments"].clone(),
        paths: tool_paths(block),
        status: ToolStatus::Running,
        output: None,
        exit_code: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ConversationBody;
    use crate::records::conversation::{read_stretches, Speaker};
    use crate::records::feed::follow::Follower;
    use crate::records::Stretch;
    use crate::test_support::ScratchDir;

    const RECORD: &str = include_str!("../../../testdata/pi_branched_records.jsonl");

    /// The kept directory holding a conversation's record among its own entries is found, the
    /// first in order where two do; a directory that holds it deeper, under a link or not at all,
    /// a relative one and one that is gone are passed over.
    #[test]
    fn the_kept_directory_holding_the_record_is_the_one_found() {
        const ID: &str = "7e57ab1e-1111-4222-8333-944455556666";
        let dir = ScratchDir::new("pi-kept-record-dir");
        let name = format!("2026-11-26T05-00-00-000Z_{ID}.jsonl");
        let [gone, empty, deeper, holds, also] =
            ["gone", "empty", "deeper", "holds", "also"].map(|sub| dir.join(sub));
        for made in [&empty, &holds, &also] {
            std::fs::create_dir_all(made).unwrap();
        }
        std::fs::create_dir_all(deeper.join("nested")).unwrap();
        std::fs::write(deeper.join("nested").join(&name), "").unwrap();
        std::fs::write(holds.join(&name), "").unwrap();
        std::fs::write(also.join(&name), "").unwrap();
        std::fs::write(empty.join("2026-11-26T05-00-00-000Z_not-an-id.jsonl"), "").unwrap();

        let kept = [
            PathBuf::from("relative"),
            gone,
            empty.clone(),
            deeper,
            holds.clone(),
            also,
        ];
        assert_eq!(kept_record_dir(&kept, ID), Some(holds.as_path()));
        assert_eq!(
            kept_record_dir(&kept, &ID.to_ascii_uppercase()),
            Some(holds.as_path())
        );
        assert_eq!(
            kept_record_dir(&kept, "0badc0de-0000-4000-8000-000000000000"),
            None
        );
        assert_eq!(kept_record_dir(&[], ID), None);

        if crate::platform::fs::file_links_available() {
            let linked = dir.join("linked");
            std::fs::create_dir_all(&linked).unwrap();
            crate::test_support::symlink(holds.join(&name), linked.join(&name)).unwrap();
            assert_eq!(kept_record_dir(&[linked, empty], ID), None);
        }
    }

    #[test]
    fn the_pi_file_is_what_can_be_carried_removed_and_resumed() {
        use crate::records::carried::{companions, locate_record};
        use crate::records::fixtures::session;
        use crate::records::locate::{Hints, Locator};
        use crate::records::removal::removable_record_paths;
        use crate::records::resumability::{resumability, Resumability};

        let dir = ScratchDir::new("pi-file-record");
        let root = crate::hostfs::canonical(&*dir).unwrap();
        let header: Value = serde_json::from_str(RECORD.lines().next().unwrap()).unwrap();
        let id = header["id"].as_str().unwrap();
        let relative = PathBuf::from("sessions/--project--")
            .join(format!("2026-11-24T04-00-00-000Z_{id}.jsonl"));
        let path = root.join(&relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, RECORD).unwrap();

        let mut locator = Locator::with_shell(Default::default());
        let session = session(AgentId::PI, id, Some(&root));
        assert_eq!(
            locate_record(&mut locator, AgentId::PI, &root, id),
            Some(relative.clone())
        );
        assert!(companions(AgentId::PI, &root, &relative, id).is_empty());
        assert_eq!(resumability(&locator, &session), Resumability::Resumable);
        assert_eq!(
            removable_record_paths(&Hints::default(), &mut locator, &[], &session),
            vec![path.clone()]
        );

        std::fs::remove_file(path).unwrap();
        // Proven gone only once the installed version was seen writing records.
        assert_eq!(resumability(&locator, &session), Resumability::Unknown);
        assert_eq!(
            resumability(
                &locator.with_witnesses(crate::records::fixtures::witnessed()),
                &session
            ),
            Resumability::Unresumable
        );
    }

    #[test]
    fn a_pi_record_in_the_launchs_alternate_root_is_found_before_refusing_resume() {
        use crate::protocol::SessionRecordPresence;
        use crate::records::{
            history_branch, presence, removable_record_paths, resumability, Hints, Locator,
            Resumability,
        };
        use crate::test_support::{app_state, console};

        let (state, dir) = app_state("pi-alternate-record-root");
        let root = crate::hostfs::canonical(&*dir).unwrap();
        let mut console = console(&dir);
        console.workdir = root.to_str().unwrap().to_owned();
        state.store.insert_console(&console).unwrap();
        let account = root.join("account");
        std::fs::create_dir(&account).unwrap();
        let header: Value = serde_json::from_str(RECORD.lines().next().unwrap()).unwrap();
        let id = header["id"].as_str().unwrap();
        let mut session = crate::records::fixtures::session(AgentId::PI, id, Some(&account));
        session.console_id = console.id;
        let alternate = root.join("custom records");
        std::fs::create_dir(&alternate).unwrap();
        let record = alternate.join(format!("2026-11-24T04-00-00-000Z_{id}.jsonl"));
        std::fs::write(&record, RECORD).unwrap();

        let locator = || {
            Locator::with_shell(std::collections::HashMap::from([(
                "PI_CODING_AGENT_SESSION_DIR".to_owned(),
                "custom records".to_owned(),
            )]))
            .with_store(&state.store)
        };
        let hints = Hints::default();
        let mut listing = locator();
        assert_eq!(resumability(&listing, &session), Resumability::Resumable);
        assert_eq!(
            presence(&hints, &mut listing, &[], &session),
            SessionRecordPresence::Present
        );
        assert_eq!(
            history_branch(&hints, &mut listing, &[], &session)
                .unwrap()
                .record,
            record
        );
        assert_eq!(
            removable_record_paths(&hints, &mut listing, &[], &session),
            std::slice::from_ref(&record)
        );

        std::fs::remove_file(record).unwrap();
        assert_eq!(resumability(&locator(), &session), Resumability::Unknown);
        assert_eq!(
            resumability(
                &locator().with_witnesses(crate::records::fixtures::witnessed()),
                &session
            ),
            Resumability::Unresumable
        );
    }

    /// A record written where an earlier launch's `PI_CODING_AGENT_SESSION_DIR` pointed is still
    /// found once the variable is unset or points elsewhere, and is proven gone only by a complete
    /// search of that directory too.
    #[test]
    fn a_pi_record_in_a_directory_an_earlier_launch_selected_outlives_the_variable() {
        use crate::protocol::SessionRecordPresence;
        use crate::records::{
            history_branch, presence, removable_record_paths, resumability, Hints, Locator,
            Resumability,
        };
        use std::collections::HashMap;

        let dir = ScratchDir::new("pi-kept-record-root");
        let root = crate::hostfs::canonical(&*dir).unwrap();
        let account = root.join("account");
        std::fs::create_dir_all(account.join("sessions")).unwrap();
        let header: Value = serde_json::from_str(RECORD.lines().next().unwrap()).unwrap();
        let id = header["id"].as_str().unwrap();
        let kept = root.join("kept records");
        std::fs::create_dir(&kept).unwrap();
        let record = kept.join(format!("2026-11-24T04-00-00-000Z_{id}.jsonl"));
        std::fs::write(&record, RECORD).unwrap();
        let mut session = crate::records::fixtures::session(AgentId::PI, id, Some(&account));
        session.record_roots = vec![kept.clone()];
        let elsewhere = root.join("other records");
        std::fs::create_dir(&elsewhere).unwrap();
        let shells = [
            HashMap::new(),
            HashMap::from([(
                "PI_CODING_AGENT_SESSION_DIR".to_owned(),
                elsewhere.to_string_lossy().into_owned(),
            )]),
        ];
        let hints = Hints::default();

        for shell in &shells {
            let mut locator = Locator::with_shell(shell.clone());
            assert_eq!(resumability(&locator, &session), Resumability::Resumable);
            assert_eq!(
                presence(&hints, &mut locator, &[], &session),
                SessionRecordPresence::Present
            );
            assert_eq!(
                history_branch(&hints, &mut locator, &[], &session)
                    .unwrap()
                    .record,
                record
            );
            assert_eq!(
                removable_record_paths(&hints, &mut locator, &[], &session),
                std::slice::from_ref(&record)
            );
        }

        std::fs::remove_file(&record).unwrap();
        // Something in the kept directory that cannot be looked through could hide the record.
        crate::test_support::symlink(&elsewhere, kept.join("linked")).unwrap();
        for shell in &shells {
            let mut locator = Locator::with_shell(shell.clone());
            assert_eq!(resumability(&locator, &session), Resumability::Unknown);
            assert_eq!(
                presence(&hints, &mut locator, &[], &session),
                SessionRecordPresence::Unknown
            );
        }
        crate::test_support::remove_symlink(kept.join("linked")).unwrap();
        for shell in &shells {
            assert_eq!(
                resumability(&Locator::with_shell(shell.clone()), &session),
                Resumability::Unknown
            );
            let mut locator = Locator::with_shell(shell.clone())
                .with_witnesses(crate::records::fixtures::witnessed());
            assert_eq!(resumability(&locator, &session), Resumability::Unresumable);
            assert_eq!(
                presence(&hints, &mut locator, &[], &session),
                SessionRecordPresence::Absent
            );
        }
    }

    /// The directory a launch keeps is the one its environment selects: a leading `~` from the
    /// snapshot's home, a relative one from the launch's working directory; none when the variable
    /// is unset or empty.
    #[test]
    fn a_launch_keeps_the_record_directory_its_environment_selects() {
        use crate::records::locate::launch_record_root;
        use std::collections::HashMap;

        let dir = ScratchDir::new("pi-launch-record-root");
        let root = crate::hostfs::canonical(&*dir).unwrap();
        let cwd = root.join("project");
        let home = root.join("home");
        let env = |value: &str| {
            HashMap::from([
                ("PI_CODING_AGENT_SESSION_DIR".to_owned(), value.to_owned()),
                ("HOME".to_owned(), home.to_string_lossy().into_owned()),
            ])
        };

        assert_eq!(
            launch_record_root(AgentId::PI, &env("records/../pi"), &cwd),
            Some(cwd.join("pi"))
        );
        assert_eq!(
            launch_record_root(AgentId::PI, &env("~/pi"), &cwd),
            Some(home.join("pi"))
        );
        let absolute = root.join("absolute");
        assert_eq!(
            launch_record_root(AgentId::PI, &env(absolute.to_str().unwrap()), &cwd),
            Some(absolute)
        );
        assert_eq!(launch_record_root(AgentId::PI, &env(""), &cwd), None);
        assert_eq!(launch_record_root(AgentId::PI, &HashMap::new(), &cwd), None);
    }

    #[test]
    fn history_feed_and_context_follow_the_captured_pi_branch() {
        assert_eq!(
            RECORD
                .lines()
                .filter(|line| PiRecords.is_user_prompt(line) == Some(true))
                .count(),
            3
        );
        let dir = ScratchDir::new("pi-record");
        let path = dir.join("record.jsonl");
        std::fs::write(&path, RECORD).unwrap();
        let stretches = vec![Stretch { path, cut: None }];
        let history = read_stretches(AgentId::PI, &stretches).unwrap();
        assert_eq!(
            history.messages,
            vec![
                (Speaker::User, "Start the captured conversation.".to_owned()),
                (Speaker::Assistant, "Captured reply 3".to_owned()),
                (Speaker::User, "The current path.".to_owned()),
                (Speaker::Assistant, "Captured reply 6".to_owned()),
            ]
        );

        assert_eq!(history.changed, ["fixture-note.txt"]);

        let feed = Follower::build(AgentId::PI, stretches.clone());
        assert!(feed.out.entries().iter().any(|entry| {
            matches!(
                &entry.body,
                ConversationBody::ToolCall(call)
                    if call.category == ToolCategory::FileEdit && call.status == ToolStatus::Completed
            )
        }));
        let texts: Vec<&str> = feed
            .out
            .entries()
            .iter()
            .filter_map(|entry| match &entry.body {
                ConversationBody::User { text } | ConversationBody::Assistant { text } => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            [
                "Start the captured conversation.",
                "Captured reply 3",
                "The current path.",
                "Captured reply 6"
            ]
        );

        let mut context = Context::default();
        let mut observed = Observed::default();
        crate::records::branch::for_each_line_on_branch(
            PiRecords.branching(),
            &stretches,
            |line| {
                PiRecords.fold_line(&line.to_string(), true, &mut context, &mut observed);
            },
        )
        .unwrap();
        assert_eq!(context.context(&PiRecords).flatten().unwrap().tokens, 179);
        assert_eq!(observed.model.as_deref(), Some("probe/sample"));
    }
}
