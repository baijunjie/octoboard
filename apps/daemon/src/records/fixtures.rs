//! Fixtures the records' tests share.

use std::path::Path;

use crate::protocol::{Agent, Session};

pub(super) const CLAUDE_ID: &str = "11111111-2222-3333-4444-555555555555";
pub(super) const CODEX_ID: &str = "019a0000-0000-7000-8000-000000000001";

pub(super) fn session(agent: Agent, agent_id: &str, config_dir: Option<&Path>) -> Session {
    let mut session: Session = serde_json::from_value(serde_json::json!({
        "id":"s","agent":"claude","agent_session_id":agent_id,"console_id":"console",
        "host_id":"local","kind":"project","origin":"user","title":"Session",
        "status":"archived","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0}))
    .unwrap();
    session.agent = agent;
    session.config_dir = config_dir.map(|dir| dir.to_string_lossy().into_owned());
    session
}

pub(super) fn touch(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "{}\n").unwrap();
}
