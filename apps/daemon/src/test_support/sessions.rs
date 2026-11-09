//! Session, Codex rollout and page records for tests of what is stored about a session.

use std::path::{Path, PathBuf};

use super::move_modification_time_back;
use crate::protocol::{AgentId, Origin, Page, Role, Session, SessionKind, SessionStatus};

pub(crate) fn project_session(id: &str, agent: AgentId, status: SessionStatus) -> Session {
    Session {
        agent,
        project_id: Some("project-1".to_string()),
        kind: SessionKind::Project,
        status,
        ..bare_session(id, SessionKind::Project)
    }
}

pub(crate) fn bare_session(id: &str, kind: SessionKind) -> Session {
    Session {
        compatibility: Default::default(),
        id: id.to_string(),
        agent: AgentId::CLAUDE,
        agent_session_id: None,
        console_id: "console-1".to_string(),
        project_id: None,
        host_id: crate::store::LOCAL_HOST_ID.to_string(),
        kind,
        origin: Origin::User,
        title: "Session".to_string(),
        status: SessionStatus::Idle,
        terminal_title: None,
        record_titles: Default::default(),
        background_tasks: Vec::new(),
        switching_account: false,
        paused: false,
        prompt_on_launch: false,
        screen_inexact: false,
        holder_link: None,
        default_effort: None,
        role: Role::Coordinator,
        context: None,
        observed_model: None,
        observed_effort: None,
        model: None,
        effort: None,
        has_conversation: false,
        bound_to: None,
        colour: None,
        ordinal: None,
        account_id: None,
        config_dir: None,
        pinned: false,
        locked: false,
        started_at: 0,
        ended_at: None,
    }
}

pub(crate) fn console_session(id: &str, status: SessionStatus) -> Session {
    Session {
        title: id.to_string(),
        status,
        ..bare_session(id, SessionKind::Console)
    }
}

/// The rollout files of a Codex thread that had a turn reverted twice, laid out the way Codex
/// writes them under a home's `sessions/`: the first rollout and two later history segments, each
/// continuing the one before (`history_base`), in dated directories of their own.
pub(crate) struct CodexChain {
    pub first: PathBuf,
    pub middle: PathBuf,
    pub head: PathBuf,
}

impl CodexChain {
    /// The segment ids the middle and head files are named with.
    pub const MIDDLE_ID: &'static str = "019a0000-0000-7000-8000-0000000000a1";
    pub const HEAD_ID: &'static str = "019a0000-0000-7000-8000-0000000000a2";

    /// The three files, first rollout first.
    pub fn all(&self) -> [PathBuf; 3] {
        [self.first.clone(), self.middle.clone(), self.head.clone()]
    }
}

/// Writes the chain of thread `thread` into `home`, compressed the way Codex does an old rollout
/// when `compressed`. Each file holds a prompt of its own (`first prompt`, `middle prompt`, `head
/// prompt`) and the thread's context size as of its end (100, 200, 300 tokens). Each file but the
/// head goes on past the point the next continues from, with a prompt (`reverted turn A`,
/// `reverted turn B`) that is not part of the conversation as it stands. The head is backdated,
/// so a reader that took the file written last would pick another one.
pub(crate) fn codex_chain(home: &Path, thread: &str, compressed: bool) -> CodexChain {
    let suffix = if compressed { ".jsonl.zst" } else { ".jsonl" };
    let path = |day: &str, time: &str, name: &str| {
        home.join("sessions")
            .join("2026")
            .join("10")
            .join(day)
            .join(format!("rollout-2026-10-{day}T{time}-{name}{suffix}"))
    };
    let chain = CodexChain {
        first: path("01", "00-00-00", thread),
        middle: path(
            "02",
            "00-00-00",
            &format!("{thread}_{}", CodexChain::MIDDLE_ID),
        ),
        head: path(
            "03",
            "00-00-00",
            &format!("{thread}_{}", CodexChain::HEAD_ID),
        ),
    };
    // Each segment continues the file before it from just after that file's prompt line.
    let rollout = |base: Option<(&str, usize)>, prompt: &str, later: &str, total: u64| {
        let base = base.map_or(serde_json::Value::Null, |(id, cut)| {
            serde_json::json!({"thread_id": id, "end_ordinal_exclusive": 2, "end_byte_offset": cut})
        });
        let lines = [
            serde_json::json!({"type":"session_meta","payload":{"id":thread,
                "history_mode":"paginated","history_base":base}}),
            serde_json::json!({"type":"event_msg","payload":{"type":"user_message",
                "message":prompt,"images":[],"local_images":[],"text_elements":[]}}),
            serde_json::json!({"type":"event_msg","payload":{"type":"user_message",
                "message":later,"images":[],"local_images":[],"text_elements":[]}}),
            serde_json::json!({"type":"event_msg","payload":{"type":"token_count","info":{
                "total_token_usage":{"input_tokens":total,"cached_input_tokens":0,
                    "output_tokens":0,"reasoning_output_tokens":0,"total_tokens":total},
                "last_token_usage":{"input_tokens":total,"cached_input_tokens":0,
                    "output_tokens":0,"reasoning_output_tokens":0,"total_tokens":total}}}}),
        ];
        lines.map(|line| line.to_string() + "\n")
    };
    let cut = |lines: &[String; 4]| lines[0].len() + lines[1].len();
    let first = rollout(None, "first prompt", "reverted turn A", 100);
    let middle = rollout(
        Some((thread, cut(&first))),
        "middle prompt",
        "reverted turn B",
        200,
    );
    let head = rollout(
        Some((CodexChain::MIDDLE_ID, cut(&middle))),
        "head prompt",
        "last turn",
        300,
    );
    let files = [
        (&chain.first, first.concat()),
        (&chain.middle, middle.concat()),
        (&chain.head, head.concat()),
    ];
    for (file, content) in files {
        std::fs::create_dir_all(file.parent().expect("a rollout has a directory"))
            .expect("the dated directory is creatable");
        let bytes = if compressed {
            zstd::encode_all(content.as_bytes(), 0).expect("the rollout compresses")
        } else {
            content.into_bytes()
        };
        std::fs::write(file, bytes).expect("the rollout is writable");
    }
    // Writing an earlier segment again (as a resume that decompressed it would) moves its
    // modification time past the head's.
    move_modification_time_back(&chain.head);
    chain
}

/// A page of the report panel pushed by `session_id` at `created_at`.
pub(crate) fn test_page(id: &str, session_id: &str, created_at: i64) -> Page {
    Page {
        id: id.to_string(),
        session_id: session_id.to_string(),
        html: "<p>page</p>".to_string(),
        created_at,
    }
}
