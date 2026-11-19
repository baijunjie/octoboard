use std::io::Write;
use std::path::Path;

use serde_json::json;

use super::follow::{Follower, Read};
use super::*;
use crate::protocol::AgentId;
use crate::records::Stretch;
use crate::test_support::ScratchDir;

fn call(id: &str) -> ToolCall {
    ToolCall {
        call_id: id.to_owned(),
        name: "Bash".to_owned(),
        category: ToolCategory::Command,
        title: Some("ls\nsecond line".to_owned()),
        input: json!({"command": "ls"}),
        paths: Vec::new(),
        status: ToolStatus::Running,
        output: None,
        exit_code: None,
    }
}

fn done(output: &str) -> ToolOutcome {
    ToolOutcome {
        status: ToolStatus::Completed,
        output: Some(output.to_owned()),
        exit_code: Some(0),
    }
}

fn todo(id: &str, text: &str, status: &str) -> Todo {
    Todo {
        id: Some(id.to_owned()),
        text: text.to_owned(),
        status: status.to_owned(),
    }
}

fn kinds(out: &FeedOut) -> Vec<String> {
    out.entries()
        .iter()
        .map(|entry| {
            let turn = entry.turn;
            match &entry.body {
                ConversationBody::User { .. } => format!("user@{turn}"),
                ConversationBody::Octoboard { .. } => format!("octoboard@{turn}"),
                ConversationBody::Assistant { .. } => format!("assistant@{turn}"),
                ConversationBody::ToolCall(_) => format!("call@{turn}"),
                ConversationBody::Todos { .. } => format!("todos@{turn}"),
                ConversationBody::Compaction { .. } => format!("compaction@{turn}"),
            }
        })
        .collect()
}

/// Messages open turns, Octoboard's own are told from the user's, blank ones are no entry, and
/// what comes before the first message is turn 0.
#[test]
fn messages_open_turns_and_octoboards_are_told_from_the_users() {
    let mut out = FeedOut::default();
    out.assistant("before anything", None);
    out.user("  ", None);
    out.user("hello", Some(5));
    out.assistant("hi", None);
    out.user(crate::messaging::texts::CONTINUE_PROMPT, None);
    out.compaction(Some("  ".to_owned()), None);
    assert_eq!(
        kinds(&out),
        [
            "assistant@0",
            "user@1",
            "assistant@1",
            "octoboard@2",
            "compaction@2"
        ]
    );
    assert_eq!(out.entries()[1].at, Some(5));
    assert_eq!(out.entries()[3].seq, 3);
    assert!(matches!(
        &out.entries()[4].body,
        ConversationBody::Compaction { summary: None }
    ));
}

/// A result that arrives before the entry was taken is part of it; one that arrives after is an
/// update of the entry taken, and a call a result names that is not among the entries is passed
/// over.
#[test]
fn a_tool_result_updates_the_call_it_belongs_to() {
    let mut out = FeedOut::default();
    out.tool_call(call("a"), None);
    out.tool_call(call("b"), None);
    out.tool_result("a", done("fast"));
    out.tool_result("nothing", done("lost"));
    let changes = out.take_changes();
    assert_eq!(changes.appended.len(), 2);
    assert!(changes.updated.is_empty() && !changes.replaced);
    let ConversationBody::ToolCall(first) = &changes.appended[0].body else {
        panic!("a call");
    };
    assert_eq!(
        (first.status, first.output.as_deref(), first.exit_code),
        (ToolStatus::Completed, Some("fast"), Some(0))
    );
    assert_eq!(first.title.as_deref(), Some("ls"));

    out.tool_result("b", done("slow"));
    out.tool_result("b", done("slower"));
    let changes = out.take_changes();
    assert!(changes.appended.is_empty());
    assert_eq!(changes.updated.len(), 1);
    assert_eq!(changes.updated[0].seq, 1);
    // The same call again replaces the entry in place.
    out.tool_call(call("b"), None);
    assert_eq!(out.len(), 2);
    assert_eq!(out.take_changes().updated.len(), 1);
}

/// A todo list replaces the one before it, or with `merge` updates it item by item.
#[test]
fn todos_fold_into_the_whole_list() {
    let mut out = FeedOut::default();
    out.todos(
        vec![todo("1", "a", "pending"), todo("2", "b", "pending")],
        false,
        None,
    );
    out.todos(
        vec![todo("2", "b", "completed"), todo("3", "c", "pending")],
        true,
        None,
    );
    let items = |out: &FeedOut, index: usize| match &out.entries()[index].body {
        ConversationBody::Todos { items } => items
            .iter()
            .map(|item| format!("{}:{}", item.text, item.status))
            .collect::<Vec<_>>(),
        _ => panic!("todos"),
    };
    assert_eq!(items(&out, 0), ["a:pending", "b:pending"]);
    assert_eq!(items(&out, 1), ["a:pending", "b:completed", "c:pending"]);
    out.todos(vec![todo("9", "z", "pending")], false, None);
    assert_eq!(items(&out, 2), ["z:pending"]);
}

/// Dropping entries nobody was given is no change to anyone; dropping ones taken replaces the
/// conversation, and the list the todos fold into goes back to what it was.
#[test]
fn truncating_below_what_was_taken_replaces_the_conversation() {
    let mut out = FeedOut::default();
    out.user("one", None);
    out.todos(vec![todo("1", "a", "pending")], false, None);
    out.take_changes();
    out.user("two", None);
    out.todos(vec![todo("1", "a", "completed")], false, None);
    out.truncate(2);
    let changes = out.take_changes();
    assert!(!changes.replaced);
    assert_eq!(changes.appended.len(), 0);
    assert_eq!(out.len(), 2);

    out.user("three", None);
    out.truncate(1);
    let changes = out.take_changes();
    assert!(changes.replaced);
    assert!(changes.appended.is_empty() && changes.updated.is_empty());
    assert_eq!(kinds(&out), ["user@1"]);
    // Entries after a replace are appended again from where it stands.
    out.assistant("again", None);
    let changes = out.take_changes();
    assert_eq!((changes.replaced, changes.appended.len()), (false, 1));
    assert_eq!(changes.appended[0].seq, 1);
}

/// The list the last remaining todos entry left keeps its ids, so a merge after a truncation
/// still matches the items it names.
#[test]
fn truncating_keeps_the_ids_of_the_todo_list_that_remains() {
    let mut out = FeedOut::default();
    out.todos(
        vec![todo("1", "a", "pending"), todo("2", "b", "pending")],
        false,
        None,
    );
    out.user("then", None);
    out.todos(vec![todo("9", "z", "pending")], false, None);
    out.truncate(2);
    out.todos(vec![todo("2", "b", "completed")], true, None);
    let ConversationBody::Todos { items } = &out.entries()[2].body else {
        panic!("todos");
    };
    let shown: Vec<String> = items
        .iter()
        .map(|item| format!("{}:{}", item.text, item.status))
        .collect();
    assert_eq!(shown, ["a:pending", "b:completed"]);
}

#[test]
fn pending_entries_are_reported_when_they_change() {
    let mut out = FeedOut::default();
    out.user("go", None);
    out.take_changes();
    out.set_pending(vec![
        Pending::Assistant("typing".to_owned()),
        Pending::Assistant(" ".to_owned()),
    ]);
    let changes = out.take_changes();
    let pending = changes.pending.expect("changed");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].turn, 1);
    out.set_pending(vec![Pending::Assistant("typing".to_owned())]);
    assert_eq!(out.take_changes().pending, None);
    out.set_pending(Vec::new());
    assert_eq!(out.take_changes().pending, Some(Vec::new()));
}

/// A long string of a call's input is cut, and an output is kept at both ends.
#[test]
fn what_is_too_long_is_cut() {
    let mut out = FeedOut::default();
    let mut long = call("a");
    long.input = json!({"nested": [{"text": "é".repeat(INPUT_STRING_LIMIT)}], "short": "x"});
    out.tool_call(long, None);
    let huge = format!(
        "{}{}{}",
        "a".repeat(OUTPUT_END_LIMIT * 2),
        "MIDDLE",
        "z".repeat(OUTPUT_END_LIMIT * 2)
    );
    out.tool_result("a", done(&huge));
    let ConversationBody::ToolCall(published) = &out.entries()[0].body else {
        panic!("a call");
    };
    assert!(published.input_truncated && published.output_truncated);
    let text = published.input["nested"][0]["text"].as_str().unwrap();
    assert!(text.len() <= INPUT_STRING_LIMIT && text.chars().all(|c| c == 'é'));
    assert_eq!(published.input["short"], "x");
    let output = published.output.as_deref().unwrap();
    assert!(output.starts_with('a') && output.ends_with('z') && !output.contains("MIDDLE"));
    assert!(output.len() <= 2 * OUTPUT_END_LIMIT + 8);
}

#[test]
fn a_page_ends_before_an_entry_and_is_kept_to_its_limits() {
    let mut out = FeedOut::default();
    for n in 0..10 {
        out.assistant(&format!("m{n}"), None);
    }
    let seqs = |page: &Page| {
        page.entries
            .iter()
            .map(|entry| entry.seq)
            .collect::<Vec<_>>()
    };
    let latest = out.page(None, 3);
    assert_eq!((seqs(&latest), latest.has_earlier), (vec![7, 8, 9], true));
    let earlier = out.page(Some(7), 5);
    assert_eq!(
        (seqs(&earlier), earlier.has_earlier),
        (vec![2, 3, 4, 5, 6], true)
    );
    let first = out.page(Some(2), 50);
    assert_eq!((seqs(&first), first.has_earlier), (vec![0, 1], false));
    assert!(out.page(Some(0), 5).entries.is_empty());
    assert_eq!(out.page(Some(1000), 2).entries.len(), 2);
    assert_eq!(out.page(None, 0).entries.len(), 1);

    // Entries that together pass the size limit end a page sooner, with one at least.
    let mut big = FeedOut::default();
    for _ in 0..4 {
        big.assistant(&"x".repeat(CONVERSATION_PAGE_BYTES / 4), None);
    }
    assert_eq!(big.page(None, 50).entries.len(), 3);
    let mut huge = FeedOut::default();
    huge.assistant(&"x".repeat(CONVERSATION_PAGE_BYTES * 2), None);
    assert_eq!(huge.page(None, 50).entries.len(), 1);
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn append(path: &Path, content: &str) {
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(content.as_bytes())
        .unwrap();
}

fn stretch(path: &Path, cut: Option<u64>) -> Stretch {
    Stretch {
        path: path.to_path_buf(),
        cut,
    }
}

fn texts(out: &FeedOut) -> Vec<String> {
    out.entries()
        .iter()
        .map(|entry| match &entry.body {
            ConversationBody::User { text } | ConversationBody::Assistant { text } => text.clone(),
            other => format!("{other:?}"),
        })
        .collect()
}

/// The conversation is read from its earlier stretches (up to their cut) and the head, a line
/// that does not parse is skipped, and what is added to the head afterwards is read alone.
#[test]
fn a_follower_reads_the_stretches_then_what_is_appended() {
    let dir = ScratchDir::new("feed-follow");
    let earlier = dir.join("earlier.jsonl");
    let head = dir.join("head.jsonl");
    let first = "{\"user\":\"one\"}\n";
    write(&earlier, &format!("{first}{{\"user\":\"abandoned\"}}\n"));
    write(
        &head,
        "{\"assistant\":\"two\"}\nnot json\n{\"user\":\"three\"}\n{\"assistant\":\"unfinis",
    );
    let built = Follower::build(
        AgentId::TEST,
        vec![
            stretch(&earlier, Some(first.len() as u64)),
            stretch(&head, None),
        ],
    );
    let (mut follower, mut out) = (built.follower, built.out);
    assert_eq!(texts(&out), ["one", "two", "three"]);
    assert_eq!(follower.read_new(), Read::Unchanged);

    append(&head, "hed\"}\n{\"assistant\":\"four\"}\n");
    let Read::Lines(lines) = follower.read_new() else {
        panic!("lines");
    };
    follower.apply(lines, &mut out);
    assert_eq!(texts(&out), ["one", "two", "three", "unfinished", "four"]);
    assert_eq!(follower.read_new(), Read::Unchanged);
}

/// A head that is shorter than it was, or gone, is not read as appended to.
#[test]
fn a_rewritten_head_restarts_the_follower_and_a_missing_one_is_reported() {
    let dir = ScratchDir::new("feed-follow-restart");
    let head = dir.join("head.jsonl");
    write(&head, "{\"user\":\"one\"}\n{\"user\":\"two\"}\n");
    let mut follower = Follower::build(AgentId::TEST, vec![stretch(&head, None)]).follower;
    write(&head, "{\"user\":\"x\"}\n");
    assert_eq!(follower.read_new(), Read::Restarted);
    let mut follower = Follower::build(AgentId::TEST, vec![stretch(&head, None)]).follower;
    std::fs::remove_file(&head).unwrap();
    assert_eq!(follower.read_new(), Read::Missing);
}

/// A read of the head that fails hands nothing on and is no restart: the next look reads the same
/// lines again, once it can.
#[cfg(unix)]
#[test]
fn a_read_that_fails_is_tried_again_at_the_next_look() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: a plain query of this process's effective user.
    if unsafe { libc::geteuid() } == 0 {
        // Permissions do not stop the superuser, so no read fails.
        return;
    }
    let dir = ScratchDir::new("feed-follow-unreadable");
    let head = dir.join("head.jsonl");
    write(&head, "{\"user\":\"one\"}\n");
    let built = Follower::build(AgentId::TEST, vec![stretch(&head, None)]);
    let (mut follower, mut out) = (built.follower, built.out);
    append(&head, "{\"assistant\":\"two\"}\n");
    let chmod = |mode| {
        std::fs::set_permissions(&head, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    chmod(0o000);
    let (first, second) = (follower.read_new(), follower.read_new());
    chmod(0o644);
    assert_eq!((first, second), (Read::Unchanged, Read::Unchanged));
    let Read::Lines(lines) = follower.read_new() else {
        panic!("the lines that could not be read");
    };
    follower.apply(lines, &mut out);
    assert_eq!(texts(&out), ["one", "two"]);
    assert_eq!(follower.read_new(), Read::Unchanged);
}

/// The file the provider declares beside the record is read too, after the record's lines, and
/// need not exist yet; when it is replaced the follower restarts.
#[test]
fn the_companion_the_provider_names_is_followed_with_the_record() {
    let dir = ScratchDir::new("feed-follow-companion");
    let head = dir.join("head.jsonl");
    let companion = head.with_extension("live");
    write(&head, "{\"user\":\"one\"}\n");
    let built = Follower::build(AgentId::TEST, vec![stretch(&head, None)]);
    let (mut follower, mut out) = (built.follower, built.out);
    assert!(out.pending().is_empty());

    write(&companion, "{\"pending\":\"typing\"}\n");
    let Read::Lines(lines) = follower.read_new() else {
        panic!("lines");
    };
    follower.apply(lines, &mut out);
    assert_eq!(out.pending().len(), 1);

    // The record's lines of a pass come before the companion's.
    append(&head, "{\"assistant\":\"typed\"}\n");
    append(&companion, "{}\n");
    let Read::Lines(lines) = follower.read_new() else {
        panic!("lines");
    };
    follower.apply(lines, &mut out);
    assert_eq!(texts(&out), ["one", "typed"]);
    assert!(out.pending().is_empty());

    write(&companion, "{}\n");
    assert_eq!(follower.read_new(), Read::Restarted);
}

/// A conversation with no stretch has nothing to read, and a companion that exists when the
/// feed starts is read at once.
#[test]
fn a_companion_present_at_the_start_is_read_with_the_build() {
    let dir = ScratchDir::new("feed-follow-companion-start");
    let head = dir.join("head.jsonl");
    write(&head, "{\"user\":\"one\"}\n");
    write(&head.with_extension("live"), "{\"pending\":\"typing\"}\n");
    let built = Follower::build(AgentId::TEST, vec![stretch(&head, None)]);
    assert_eq!(built.out.pending().len(), 1);
    let none = Follower::build(AgentId::TEST, Vec::new());
    assert!(none.out.entries().is_empty());
    let mut follower = none.follower;
    assert_eq!(follower.read_new(), Read::Unchanged);
}

/// What a message is told as follows `read_session_history`'s judgement: its notices, a brief and
/// a branches prompt are Octoboard's, a user's own words that look like them are not.
#[test]
fn octoboards_messages_are_told_as_the_history_tells_them() {
    use crate::messaging::texts;
    let branches = format!(
        "{} `x` {}\nmore",
        texts::BRANCHES_OPENING,
        texts::BRANCHES_MARKER
    );
    let mut out = FeedOut::default();
    for text in [
        format!("{} (session report): green", texts::NOTICE_PREFIX),
        format!("{} do this", texts::BRIEF_OPENING),
        branches,
        "Report from the team".to_owned(),
        format!(
            "{} but typed by hand\nnot the marker",
            texts::BRANCHES_OPENING
        ),
    ] {
        out.user(&text, None);
    }
    assert_eq!(
        kinds(&out),
        [
            "octoboard@1",
            "octoboard@2",
            "octoboard@3",
            "user@4",
            "user@5"
        ]
    );
}

/// Text is kept trimmed, and a title is one line cut to a title's length.
#[test]
fn text_is_trimmed_and_a_title_is_its_first_line_cut_short() {
    let mut out = FeedOut::default();
    out.user("\n  hello there \n", None);
    let mut long = call("a");
    long.title = Some(format!("\n  {}\nsecond", "t".repeat(500)));
    out.tool_call(long, None);
    assert_eq!(texts(&out)[0], "hello there");
    let ConversationBody::ToolCall(published) = &out.entries()[1].body else {
        panic!("a call");
    };
    let title = published.title.as_deref().unwrap();
    assert!(title.starts_with("ttt") && !title.contains('\n') && !title.contains("second"));
    assert!(title.chars().count() < 500);
}

/// Output is cut only when it is longer than both its ends together, and input only when a string
/// is longer than the limit; a cut never splits a character.
#[test]
fn the_cuts_leave_what_fits_alone_and_never_split_a_character() {
    let mut out = FeedOut::default();
    let mut fits = call("fits");
    fits.input = json!({"text": "x".repeat(INPUT_STRING_LIMIT)});
    out.tool_call(fits, None);
    out.tool_result("fits", done(&"y".repeat(2 * OUTPUT_END_LIMIT)));
    let ConversationBody::ToolCall(published) = &out.entries()[0].body else {
        panic!("a call");
    };
    assert!(!published.input_truncated && !published.output_truncated);
    assert_eq!(
        published.input["text"].as_str().unwrap().len(),
        INPUT_STRING_LIMIT
    );
    assert_eq!(
        published.output.as_deref().unwrap().len(),
        2 * OUTPUT_END_LIMIT
    );

    // Three-byte characters whose boundaries fall off the limits.
    out.tool_call(call("wide"), None);
    out.tool_result("wide", done(&"€".repeat(OUTPUT_END_LIMIT)));
    let ConversationBody::ToolCall(published) = &out.entries()[1].body else {
        panic!("a call");
    };
    assert!(published.output_truncated);
    let output = published.output.as_deref().unwrap();
    assert!(output.starts_with('€') && output.ends_with('€'));
    assert!(output.chars().all(|c| matches!(c, '€' | '\n' | '…')));
}

/// A call that was marked failed without an output keeps the output the result gives later, and a
/// result without an output or an exit code leaves what the call had.
#[test]
fn a_result_adds_to_the_call_without_wiping_what_it_has() {
    let mut out = FeedOut::default();
    out.tool_call(call("a"), None);
    out.tool_result(
        "a",
        ToolOutcome {
            status: ToolStatus::Failed,
            output: None,
            exit_code: None,
        },
    );
    out.tool_result(
        "a",
        ToolOutcome {
            status: ToolStatus::Failed,
            output: Some("boom".to_owned()),
            exit_code: Some(3),
        },
    );
    out.tool_result(
        "a",
        ToolOutcome {
            status: ToolStatus::Failed,
            output: None,
            exit_code: None,
        },
    );
    let ConversationBody::ToolCall(published) = &out.entries()[0].body else {
        panic!("a call");
    };
    assert_eq!(
        (
            published.status,
            published.output.as_deref(),
            published.exit_code
        ),
        (ToolStatus::Failed, Some("boom"), Some(3))
    );
}

/// After the agent went back, the turns count on from the last entry that is left, and the
/// entries that follow take the seqs that were freed.
#[test]
fn after_a_truncation_turns_and_seqs_go_on_from_what_is_left() {
    let mut out = FeedOut::default();
    out.user("one", None);
    out.assistant("a", None);
    out.user("two", None);
    out.assistant("b", None);
    out.truncate(2);
    out.user("three", None);
    let last = out.entries().last().unwrap();
    assert_eq!((last.seq, last.turn), (2, 2));
    out.truncate(0);
    out.assistant("fresh", None);
    out.user("again", None);
    assert_eq!(kinds(&out), ["assistant@0", "user@1"]);
}

/// A call whose result comes after the changes were taken is an update of it by `seq`; entries
/// added in the same pass are appended whole; and the pending entries are carried through a
/// replacement for the holder's fresh page.
#[test]
fn changes_name_updates_by_seq_and_pending_survives_a_replacement() {
    let mut out = FeedOut::default();
    out.user("go", None);
    out.tool_call(call("a"), None);
    out.take_changes();
    out.set_pending(vec![Pending::Assistant("typing".to_owned())]);
    out.take_changes();
    out.tool_result("a", done("ok"));
    out.tool_call(call("b"), None);
    out.tool_result("b", done("also"));
    let changes = out.take_changes();
    assert_eq!(
        changes.updated.iter().map(|e| e.seq).collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(changes.appended.len(), 1);
    assert_eq!(changes.pending, None);

    out.truncate(1);
    let changes = out.take_changes();
    assert!(changes.replaced && changes.pending.is_none());
    assert_eq!(out.pending().len(), 1);
}

#[test]
fn a_line_dates_itself_by_its_rfc_3339_timestamp() {
    assert_eq!(
        at_of(&json!({"timestamp": "2026-01-02T03:04:05.678Z"})),
        crate::time::parse_rfc3339_millis("2026-01-02T03:04:05.678Z")
    );
    assert_eq!(
        at_of(&json!({"timestamp": "2026-01-02T03:04:05.678Z"})),
        Some(1_767_323_045_678)
    );
    assert_eq!(at_of(&json!({"timestamp": "yesterday"})), None);
    assert_eq!(at_of(&json!({"timestamp": 5})), None);
    assert_eq!(at_of(&json!({})), None);
}

/// A provider with no reader of its own is read as the common entries its store gives, and what
/// the store does not declare claims nothing.
mod fallback {
    use super::*;
    use crate::adapter::provider;
    use crate::records::RecordStore;
    use std::path::PathBuf;

    /// A record store that declares nothing of the feed.
    struct Plain;

    impl RecordStore for Plain {
        fn indexed(
            &self,
            _path: &Path,
            _name: &str,
            _kind: std::fs::FileType,
            _leaf: bool,
        ) -> Option<(String, PathBuf)> {
            None
        }
    }

    #[test]
    fn a_store_declaring_nothing_has_no_reader_and_no_companion() {
        assert!(Plain.feed_reader().is_none());
        assert_eq!(Plain.feed_companion(Path::new("/a/b.jsonl")), None);
    }

    /// The common entries of a provider's record, read through `EntriesReader`: the user's text as
    /// its store reads it, what the agent said, a summary as a compaction and a todo list.
    #[test]
    fn the_common_entries_read_as_user_assistant_compaction_and_todos() {
        // Claude Code's store supplies the entries and the filter for what the user typed.
        let agent = AgentId::CLAUDE;
        let mut reader = EntriesReader::new(agent);
        let mut out = FeedOut::default();
        for line in [
            json!({"type": "user", "timestamp": "2026-01-02T03:04:05.678Z",
                   "origin": {"kind": "human"},
                   "message": {"content": "<pasted_content id=\"a\">\nhello\n</pasted_content id=\"a\">"}}),
            json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "hi"}]}}),
            json!({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "id": "t", "name": "TodoWrite",
                 "input": {"todos": [{"content": "x", "status": "pending"}]}}]}}),
            json!({"type": "user", "message": {"content": "[Request interrupted by user]"}}),
            json!({"type": "summary"}),
        ] {
            reader.line(&line, &mut out);
        }
        assert_eq!(kinds(&out), ["user@1", "assistant@1", "todos@1"]);
        assert_eq!(out.entries()[0].at, Some(1_767_323_045_678));
        assert_eq!(texts(&out)[0], "hello");
    }

    /// Every provider has a reader, whichever it is, and it takes lines that mean nothing to it
    /// without making entries.
    #[test]
    fn every_provider_reads_nonsense_as_nothing() {
        for agent in AgentId::ALL {
            let mut reader = reader_for(agent);
            let mut out = FeedOut::default();
            for line in [json!({}), json!([]), json!(null), json!({"type": 5})] {
                reader.line(&line, &mut out);
                reader.companion_line(&line, &mut out);
            }
            assert!(
                out.entries().is_empty() && out.pending().is_empty(),
                "{agent}"
            );
            // What it declares as its companion sits beside the record, or there is none.
            let record = Path::new("/a/record.jsonl");
            if let Some(companion) = provider(agent).records().feed_companion(record) {
                assert_eq!(companion.parent(), record.parent(), "{agent}");
                assert_ne!(companion, record, "{agent}");
            }
        }
    }
}

/// After a first read that failed part-way, the head is read again from the start only once it
/// has changed: the same file would fail the same way, so it is not rebuilt at every look.
#[test]
fn a_head_broken_part_way_restarts_only_when_it_changes() {
    let dir = ScratchDir::new("feed-follow-broken");
    let head = dir.join("head.jsonl");
    write(&head, "{\"user\":\"one\"}\n");
    let mut follower = Follower::build(AgentId::TEST, vec![stretch(&head, None)]).follower;
    follower.break_head();
    assert_eq!(follower.read_new(), Read::Unchanged);
    assert_eq!(follower.read_new(), Read::Unchanged);
    append(&head, "{\"user\":\"two\"}\n");
    assert_eq!(follower.read_new(), Read::Restarted);
}

/// A read that fails before any line was handed on applied nothing, so it is an ordinary retry
/// at the next look: no rebuild.
#[test]
fn a_read_that_fails_before_any_line_is_retried_not_rebuilt() {
    let dir = ScratchDir::new("feed-follow-unreadable");
    // A directory where the file should be cannot be read, and holds no line.
    let head = dir.join("head.jsonl");
    std::fs::create_dir_all(&head).unwrap();
    let mut follower = Follower::build(AgentId::TEST, vec![stretch(&head, None)]).follower;
    assert_eq!(follower.read_new(), Read::Unchanged);
    assert_eq!(follower.read_new(), Read::Unchanged);
    // The file changes (the directory gains an entry) and still cannot be read: a retry again,
    // where a rebuild would be wrong, nothing having been applied.
    std::fs::write(head.join("entry"), "x").unwrap();
    assert_eq!(follower.read_new(), Read::Unchanged);
}

/// A compressed head whose later frame is corrupt gives the lines before it and then fails, so
/// lines were handed on: it is read again from the start once it changes, and not before.
#[test]
fn a_failure_after_lines_were_handed_on_rebuilds_once_the_file_changes() {
    let dir = ScratchDir::new("feed-follow-corrupt");
    let head = dir.join("head.jsonl.zst");
    let mut bytes = zstd::stream::encode_all(&b"{\"user\":\"one\"}\n"[..], 3).unwrap();
    bytes.extend_from_slice(b"not a zstd frame");
    std::fs::write(&head, &bytes).unwrap();
    let built = Follower::build(AgentId::TEST, vec![stretch(&head, None)]);
    assert_eq!(texts(&built.out), ["one"]);
    let mut follower = built.follower;
    assert_eq!(follower.read_new(), Read::Unchanged);
    assert_eq!(follower.read_new(), Read::Unchanged);
    append(&head, "more garbage");
    assert_eq!(follower.read_new(), Read::Restarted);
}
