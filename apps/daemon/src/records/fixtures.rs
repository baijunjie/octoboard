//! Fixtures the records' tests share.

use std::path::{Path, PathBuf};

use crate::protocol::{AgentId, Session};

pub(crate) const CLAUDE_ID: &str = "11111111-2222-3333-4444-555555555555";
pub(crate) const CODEX_ID: &str = "019a0000-0000-7000-8000-000000000001";

pub(crate) fn session(agent: AgentId, agent_id: &str, config_dir: Option<&Path>) -> Session {
    let mut session: Session = serde_json::from_value(serde_json::json!({
        "id":"s","agent":"claude","agent_session_id":agent_id,"console_id":"console",
        "host_id":"local","kind":"project","origin":"user","title":"Session",
        "status":"archived","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0}))
    .unwrap();
    session.agent = agent;
    session.config_dir = config_dir.map(|dir| dir.to_string_lossy().into_owned());
    session
}

pub(crate) fn touch(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "{}\n").unwrap();
}

/// A record of the test-only provider's tree-shaped variant
/// (`crate::providers::test_provider::with_tree_records`) as it stands after two turns: a line
/// outside the tree, then a prompt and its reply twice over.
pub(crate) const TREE_TRUNK: &str = concat!(
    r#"{"header":true}"#,
    "\n",
    r#"{"id":"a","parent":null,"user":"first"}"#,
    "\n",
    r#"{"id":"b","parent":"a","assistant":"reply one","tokens":100}"#,
    "\n",
    r#"{"id":"c","parent":"b","user":"abandoned"}"#,
    "\n",
    r#"{"id":"d","parent":"c","assistant":"abandoned reply","tokens":900}"#,
    "\n",
);

/// What going back to the first reply of [`TREE_TRUNK`] and carrying on from there appends: the
/// second turn stays in the file, off the conversation.
pub(crate) const TREE_BRANCH: &str = concat!(
    r#"{"id":"e","parent":"b","user":"second"}"#,
    "\n",
    r#"{"id":"f","parent":"e","assistant":"reply two","tokens":200}"#,
    "\n",
);

/// Writes `lines` as the record of conversation `agent_id` of the test-only provider in the
/// account directory `root`, and answers its path.
pub(crate) fn test_record(root: &Path, agent_id: &str, lines: &str) -> std::path::PathBuf {
    let record = root.join("sessions").join(format!("{agent_id}.jsonl"));
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, lines).unwrap();
    record
}

/// The id of a conversation of the test-only provider's row-shaped variant
/// (`crate::providers::test_provider::with_row_records`): not a UUID, as an agent's own ids need
/// not be.
pub(crate) const ROW_ID: &str = "row_0a1b2c";

/// The rows of a conversation of that variant after two turns, as the elements of a JSON array:
/// lines of the shape the tree-shaped variant's record has.
pub(crate) const ROW_TURNS: &str = concat!(
    r#"{"id":"a","parent":null,"user":"first"},"#,
    r#"{"id":"b","parent":"a","assistant":"reply one","tokens":100},"#,
    r#"{"id":"c","parent":"b","user":"second"},"#,
    r#"{"id":"d","parent":"c","assistant":"reply two","tokens":900}"#,
);

/// What going back to the first reply of [`ROW_TURNS`] and carrying on from there adds: the
/// second turn's rows stay in the database, off the conversation.
pub(crate) const ROW_FORK: &str = concat!(
    r#"{"id":"e","parent":"b","user":"third"},"#,
    r#"{"id":"f","parent":"e","assistant":"reply three","tokens":200}"#,
);

/// Writes the database of the test-only provider's row-shaped variant in the account directory
/// `root`, whole: each of `conversations` is an id and its rows, as the elements of a JSON array.
pub(crate) fn row_database(root: &Path, conversations: &[(&str, &str)]) {
    let held: Vec<String> = conversations
        .iter()
        .map(|(id, rows)| format!("{}:[{rows}]", serde_json::json!(id)))
        .collect();
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join(crate::adapter::test_provider::ROW_DATABASE),
        format!("{{{}}}", held.join(",")),
    )
    .unwrap();
}

/// One agent's record of a session whose hook reported no prompt: where its conversation file
/// sits in an account of the agent's own, what the file holds while no prompt has been sent, and
/// a line that is a prompt the user sent.
pub(crate) struct Unreported {
    pub agent: AgentId,
    pub agent_id: &'static str,
    pub account: PathBuf,
    pub file: PathBuf,
    pub unprompted: String,
    pub prompt: String,
}

impl Unreported {
    /// Writes the conversation file as it is with no prompt sent, or with the prompt after that.
    pub(crate) fn write(&self, prompted: bool) {
        std::fs::create_dir_all(self.file.parent().unwrap()).unwrap();
        let mut content = self.unprompted.clone();
        if prompted {
            content.push_str(&self.prompt);
            content.push('\n');
        }
        std::fs::write(&self.file, content).unwrap();
    }
}

/// An [`Unreported`] for every provider that says which lines are the user's prompts
/// (`RecordStore::is_user_prompt`), each in an account directory of its own under `dir`, built
/// from the committed captures (`testdata/unprompted-captures.md`). A provider added without one
/// fails here, so that it gets its own.
pub(crate) fn unreported(dir: &Path) -> Vec<Unreported> {
    let line = |capture: &str, at: usize| capture.lines().nth(at).unwrap().to_owned();
    let mut all = Vec::new();
    for agent in AgentId::ALL {
        let account = dir.join(agent.descriptor().id);
        if agent == AgentId::CLAUDE {
            // A `/clear` in a session never prompted, then a typed prompt.
            all.push(Unreported {
                agent,
                agent_id: CLAUDE_ID,
                file: account
                    .join("projects/project")
                    .join(format!("{CLAUDE_ID}.jsonl")),
                account,
                unprompted: include_str!("../../testdata/unprompted_claude_2.1.330_clear.jsonl")
                    .to_owned(),
                prompt: line(
                    include_str!("../../testdata/handoff_claude_2.1.302.jsonl"),
                    1,
                ),
            });
        } else if agent == AgentId::CODEX {
            // Codex writes no rollout until the first prompt, so there is no capture of one with
            // none: these are the lines a rollout opens with, ahead of the prompt's own event
            // (the session's metadata and the instructions Codex adds as a user-role item).
            let rollout = include_str!("../../testdata/titles_codex_0.163.0_rollout.jsonl");
            all.push(Unreported {
                agent,
                agent_id: CODEX_ID,
                file: account
                    .join("sessions/2026/10/16")
                    .join(format!("rollout-2026-10-16T00-00-00-{CODEX_ID}.jsonl")),
                account,
                unprompted: format!("{}\n{}\n", line(rollout, 0), line(rollout, 1)),
                prompt: line(rollout, 3),
            });
        } else if agent == AgentId::GROK {
            // What Grok Build writes on launch, then a prompt. The marker its session directory
            // is found by is written here, since the fixture stands for a session Octoboard can
            // find.
            let session = account.join("sessions/project").join(CLAUDE_ID);
            touch(&session.join("usage.json"));
            all.push(Unreported {
                agent,
                agent_id: CLAUDE_ID,
                file: session.join("chat_history.jsonl"),
                account,
                unprompted: include_str!(
                    "../../testdata/unprompted_grok_1.0.92_chat_history.jsonl"
                )
                .to_owned(),
                prompt: line(
                    include_str!("../../testdata/feed_grok_1.0.89_chat_history.jsonl"),
                    2,
                ),
            });
        } else {
            assert!(
                crate::adapter::provider(agent)
                    .records()
                    .is_user_prompt("{}")
                    .is_none(),
                "{agent:?} has no fixture for its prompt recognition"
            );
        }
    }
    all
}

/// The version [`witnessed`] says every agent has installed.
pub(crate) const WITNESSED_VERSION: crate::adapter::AgentVersion = (1, 0, 0);

/// Witnesses under which every agent's installed version was seen writing records the reader
/// recognises, so a record missing is proof: for the tests of what a proven absence does.
pub(crate) fn witnessed() -> &'static crate::records::Witnesses {
    static ALL: std::sync::LazyLock<crate::records::Witnesses> = std::sync::LazyLock::new(|| {
        let witnesses = crate::records::Witnesses::default();
        witness_every_agent(&witnesses);
        witnesses
    });
    &ALL
}

/// Says every agent's installed version, [`WITNESSED_VERSION`], was seen writing records, in
/// `witnesses` (an `AppState`'s, for the coordinator's tests of what a proven absence does).
pub(crate) fn witness_every_agent(witnesses: &crate::records::Witnesses) {
    for agent in AgentId::ALL {
        witnesses.witness_installed(agent, WITNESSED_VERSION);
    }
}
