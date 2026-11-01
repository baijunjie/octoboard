//! Fixtures the store's tests share.

use super::*;
use crate::protocol::{
    Account, Agent, Console, OrchestrationRole, Origin, PerAgent, Project, ProjectAccount,
    ProjectSource, Role, Session, SessionStatus,
};
use crate::test_support::ScratchFile;

pub(super) fn temp_db(name: &str) -> ScratchFile {
    ScratchFile::new(&format!("store-{name}"), "octoboard.db")
}

pub(super) fn console(claude: Option<&str>, codex: Option<&str>, grok: Option<&str>) -> Console {
    Console {
        id: "console-1".to_string(),
        name: "Console".to_string(),
        workdir: "/tmp/console-1".to_string(),
        default_agent: Agent::Claude,
        accounts: PerAgent {
            claude: claude.map(str::to_string),
            codex: codex.map(str::to_string),
            grok: grok.map(str::to_string),
        },
        launch: Default::default(),
        icon: None,
        console_session_prompt: None,
        created_at: 0,
        project_session_agent: None,
        project_session_launch: Default::default(),
    }
}

pub(super) fn account(id: &str, agent: Agent, config_dir: &str) -> Account {
    Account {
        id: id.to_string(),
        agent,
        name: format!("{id}-name"),
        config_dir: config_dir.to_string(),
        share_default_config: false,
        share_default_config_incomplete: false,
        disabled: false,
    }
}

pub(super) fn session(config_dir: Option<&str>) -> Session {
    Session {
        id: "session-1".to_string(),
        agent: Agent::Claude,
        agent_session_id: None,
        console_id: "console-1".to_string(),
        project_id: None,
        host_id: LOCAL_HOST_ID.to_string(),
        role: Role::Console,
        origin: Origin::User,
        title: "Hub".to_string(),
        status: SessionStatus::Idle,
        terminal_title: None,
        record_titles: Default::default(),
        background_tasks: Vec::new(),
        switching_account: false,
        prompt_on_launch: false,
        default_effort: None,
        orchestration_role: OrchestrationRole::Coordinator,
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
        config_dir: config_dir.map(str::to_string),
        pinned: false,
        locked: false,
        started_at: 0,
        ended_at: None,
    }
}

pub(super) fn project(consent: bool) -> Project {
    Project {
        id: "project-1".to_string(),
        console_id: "console-1".to_string(),
        host_id: LOCAL_HOST_ID.to_string(),
        name: "Project".to_string(),
        path: "/tmp/project-1".to_string(),
        default_agent: None,
        source: ProjectSource::Local,
        remote_url: None,
        accounts: Default::default(),
        launch: Default::default(),
        trust_consent: consent,
        pinned: false,
        tags: Vec::new(),
        description: None,
        git_workflow: Default::default(),
    }
}

pub(super) fn named(id: &str) -> ProjectAccount {
    ProjectAccount::Account { id: id.to_string() }
}

/// A console session to feed [`Store::insert_console_session`]: a plain `role: Console` record
/// whose colour, ordinal and title that call is going to overwrite, distinguished only by `id`.
pub(super) fn console_session(id: &str) -> Session {
    let mut built = session(None);
    built.id = id.to_string();
    built
}
