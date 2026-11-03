//! Where each agent keeps its files on disk: the variable naming its configuration directory and
//! the directory it uses without one, where inside that directory it keeps a session's records,
//! and the files it keeps beside them. The one place these names are spelled: the record code, the
//! account switch's copy, the sharing catalog and the adapters read them from here. A project's own
//! `.claude/` or `.codex/` is not the agent's directory, whatever it shares with its name, so a
//! file name taken from a project's directory stays where it is read.
//!
//! [`AgentLayout`] holds what every agent has, or what code that works for any agent asks of one;
//! [`layout`] gives an agent's. Code that works for any agent reads the descriptor's fields; code
//! already inside one agent's branch reads that agent's named constant where there is one, and its
//! descriptor (`CODEX.search_depth`) for a value that has none. A name only one agent's own logic
//! uses (Codex's rollout naming, Grok Build's session marker) is a constant beside that agent's
//! descriptor, named for the agent. How a record is recognised and read stays with the code that
//! does it (`crate::records`): this module holds names and depths, not rules. The measurements
//! behind them are in `docs/agent-cli-reference.md`, mostly in "Moving a conversation to another
//! config directory" and "Sharing a configuration directory between accounts".

use crate::protocol::Agent;

/// One agent's on-disk layout. Paths are relative to the agent's configuration directory unless
/// a field says otherwise.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AgentLayout {
    /// The environment variable the agent reads its configuration directory from: the one an
    /// adapter exports for a pinned account and a shell snapshot may already carry for the
    /// default one.
    pub config_dir_var: &'static str,
    /// The configuration directory's name under the home directory when the variable is unset.
    pub default_dir: &'static str,
    /// The directory a session's records are kept under, and the one a resume reads.
    pub records_dir: &'static str,
    /// Where the agent moves the records it archives, which a resume does not read.
    pub archived_records_dir: Option<&'static str>,
    /// How many levels below [`records_dir`](Self::records_dir) a session's record, a file or a
    /// directory, sits, counting its own name: the one depth a record to be removed is taken at.
    /// An agent whose index names a file inside the record (Grok Build's session marker) matches
    /// it one level deeper.
    pub record_level: usize,
    /// How many levels below the records directory a search for records goes (1 being the
    /// directory's own entries). Exact for an agent that files a record by its working directory,
    /// generous for one that files it under dates it does not look it up by.
    pub search_depth: usize,
    /// A file directly under the configuration directory that names every conversation of the
    /// account together.
    pub title_index: Option<&'static str>,
    /// A file in a session's own record directory that names the conversation alone.
    pub title_summary: Option<&'static str>,
}

/// The layout of `agent`.
pub(crate) fn layout(agent: Agent) -> &'static AgentLayout {
    match agent {
        Agent::Claude => &CLAUDE,
        Agent::Codex => &CODEX,
        Agent::Grok => &GROK,
    }
}

/// Where Claude Code keeps its transcripts, one directory per working directory.
pub(crate) const CLAUDE_RECORDS_DIR: &str = "projects";

/// Claude Code: `~/.claude`, a session's transcript at
/// `projects/<working directory slug>/<id>.jsonl` with the directory `<id>` beside it holding its subagents' transcripts and tool results, and
/// its rewind checkpoints in `file-history/<id>`. No sign-in file of it is read; the account's
/// email is in `.claude.json`, which lies outside the directory unless one is exported
/// (`crate::adapter::claude::settings::global_config_file`).
pub(crate) const CLAUDE: AgentLayout = AgentLayout {
    config_dir_var: "CLAUDE_CONFIG_DIR",
    default_dir: ".claude",
    records_dir: CLAUDE_RECORDS_DIR,
    archived_records_dir: None,
    record_level: 2,
    search_depth: 2,
    title_index: None,
    title_summary: None,
};

/// Claude Code's user settings, inside its configuration directory.
pub(crate) const CLAUDE_SETTINGS_FILE: &str = "settings.json";

/// Claude Code's key bindings, inside its configuration directory.
pub(crate) const CLAUDE_KEY_BINDINGS: &str = "keybindings.json";

/// What follows the session id in the name of Claude Code's transcript.
pub(crate) const CLAUDE_TRANSCRIPT_SUFFIX: &str = ".jsonl";

/// Claude Code's rewind checkpoints, a directory per session id.
pub(crate) const CLAUDE_CHECKPOINTS_DIR: &str = "file-history";

/// Claude Code's prompt history, one line per prompt.
pub(crate) const CLAUDE_PROMPT_HISTORY: &str = "history.jsonl";

/// Codex: `~/.codex`, a thread's rollouts at
/// `sessions/<year>/<month>/<day>/rollout-<start>-<thread id>.jsonl` (`.jsonl.zst` once compressed;
/// a later history segment adds `_<segment id>`), moved under `archived_sessions/` by
/// `codex archive`; the thread names in `session_index.jsonl`, the thread state in
/// `state_<generation>.sqlite`.
pub(crate) const CODEX: AgentLayout = AgentLayout {
    config_dir_var: "CODEX_HOME",
    default_dir: ".codex",
    records_dir: CODEX_RECORDS_DIR,
    archived_records_dir: Some(CODEX_ARCHIVED_DIR),
    record_level: 4,
    search_depth: 6,
    title_index: Some(CODEX_TITLE_INDEX),
    title_summary: None,
};

/// Where Codex keeps a thread's rollouts.
pub(crate) const CODEX_RECORDS_DIR: &str = "sessions";

/// Where `codex archive` moves a thread's rollouts, filed as under `sessions/`.
pub(crate) const CODEX_ARCHIVED_DIR: &str = "archived_sessions";

/// Codex's index of thread names, one line per name given.
pub(crate) const CODEX_TITLE_INDEX: &str = "session_index.jsonl";

/// The start of every Codex rollout's name.
pub(crate) const CODEX_ROLLOUT_PREFIX: &str = "rollout-";

/// The end of a Codex rollout's name.
pub(crate) const CODEX_ROLLOUT_SUFFIX: &str = ".jsonl";

/// The end of a Codex rollout's name once Codex has compressed it (it does so for rollouts older
/// than a week).
pub(crate) const CODEX_COMPRESSED_ROLLOUT_SUFFIX: &str = ".jsonl.zst";

/// Codex's prompt history, one line per prompt.
pub(crate) const CODEX_PROMPT_HISTORY: &str = "history.jsonl";

/// What a Codex thread state database's name starts and ends with, around its schema generation,
/// which a newer Codex may raise.
pub(crate) const CODEX_STATE_DATABASE: (&str, &str) = ("state_", ".sqlite");

/// Codex's sign-in.
pub(crate) const CODEX_SIGN_IN_FILE: &str = "auth.json";

/// Codex's own configuration file.
pub(crate) const CODEX_CONFIG_FILE: &str = "config.toml";

/// Codex's cached model catalogue.
pub(crate) const CODEX_MODELS_CACHE: &str = "models_cache.json";

/// Grok Build: `~/.grok`, a session's directory at
/// `sessions/<url-encoded working directory>/<id>/`, marked by its `usage.json`, holding the conversation in `chat_history.jsonl` and its title in
/// `summary.json`.
pub(crate) const GROK: AgentLayout = AgentLayout {
    config_dir_var: "GROK_HOME",
    default_dir: ".grok",
    records_dir: GROK_RECORDS_DIR,
    archived_records_dir: None,
    record_level: 2,
    search_depth: 2,
    title_index: None,
    title_summary: Some(GROK_TITLE_SUMMARY),
};

/// Where Grok Build keeps its sessions' directories.
pub(crate) const GROK_RECORDS_DIR: &str = "sessions";

/// Grok Build's trust store, inside its configuration directory.
pub(crate) const GROK_TRUST_STORE: &str = "trusted_folders.toml";

/// The file inside a Grok Build session's directory that marks it as one.
pub(crate) const GROK_SESSION_MARKER: &str = "usage.json";

/// The file inside a Grok Build session's directory that holds the conversation.
pub(crate) const GROK_CONVERSATION_FILE: &str = "chat_history.jsonl";

/// The file inside a Grok Build session's directory that holds its title and how it was
/// configured.
pub(crate) const GROK_TITLE_SUMMARY: &str = "summary.json";

/// Grok Build's sign-in.
pub(crate) const GROK_SIGN_IN_FILE: &str = "auth.json";

/// Grok Build's own configuration file.
pub(crate) const GROK_CONFIG_FILE: &str = "config.toml";

/// Grok Build's cached model catalogue.
pub(crate) const GROK_MODELS_CACHE: &str = "models_cache.json";

/// Where Claude Code caches the model catalogue of each account that has used this configuration
/// directory, inside it: one `<organization>-<account hash>-cc.json` file per account.
pub(crate) const CLAUDE_MODEL_CATALOGUE_DIR: &str = "cache/model-catalog";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_agent_has_its_own_descriptor() {
        assert_eq!(layout(Agent::Claude), &CLAUDE);
        assert_eq!(layout(Agent::Codex), &CODEX);
        assert_eq!(layout(Agent::Grok), &GROK);
    }

    #[test]
    fn the_names_are_the_ones_each_agent_writes() {
        let names = |layout: &AgentLayout| {
            (
                layout.config_dir_var,
                layout.default_dir,
                layout.records_dir,
                layout.archived_records_dir,
                layout.record_level,
                layout.search_depth,
            )
        };
        assert_eq!(
            names(&CLAUDE),
            ("CLAUDE_CONFIG_DIR", ".claude", "projects", None, 2, 2)
        );
        assert_eq!(
            names(&CODEX),
            (
                "CODEX_HOME",
                ".codex",
                "sessions",
                Some("archived_sessions"),
                4,
                6
            )
        );
        assert_eq!(names(&GROK), ("GROK_HOME", ".grok", "sessions", None, 2, 2));
        let sidecars = |layout: &AgentLayout| (layout.title_index, layout.title_summary);
        assert_eq!(sidecars(&CLAUDE), (None, None));
        assert_eq!(sidecars(&CODEX), (Some("session_index.jsonl"), None));
        assert_eq!(sidecars(&GROK), (None, Some("summary.json")));
    }

    #[test]
    fn every_named_constant_is_the_name_the_agent_writes() {
        assert_eq!(CLAUDE_RECORDS_DIR, "projects");
        assert_eq!(CLAUDE_SETTINGS_FILE, "settings.json");
        assert_eq!(CLAUDE_KEY_BINDINGS, "keybindings.json");
        assert_eq!(CLAUDE_TRANSCRIPT_SUFFIX, ".jsonl");
        assert_eq!(CLAUDE_CHECKPOINTS_DIR, "file-history");
        assert_eq!(CLAUDE_PROMPT_HISTORY, "history.jsonl");
        assert_eq!(CODEX_RECORDS_DIR, "sessions");
        assert_eq!(CODEX_ARCHIVED_DIR, "archived_sessions");
        assert_eq!(CODEX_TITLE_INDEX, "session_index.jsonl");
        assert_eq!(CODEX_ROLLOUT_PREFIX, "rollout-");
        assert_eq!(CODEX_ROLLOUT_SUFFIX, ".jsonl");
        assert_eq!(CODEX_COMPRESSED_ROLLOUT_SUFFIX, ".jsonl.zst");
        assert_eq!(CODEX_PROMPT_HISTORY, "history.jsonl");
        assert_eq!(CODEX_STATE_DATABASE, ("state_", ".sqlite"));
        assert_eq!(CODEX_SIGN_IN_FILE, "auth.json");
        assert_eq!(CODEX_CONFIG_FILE, "config.toml");
        assert_eq!(CODEX_MODELS_CACHE, "models_cache.json");
        assert_eq!(GROK_RECORDS_DIR, "sessions");
        assert_eq!(GROK_TRUST_STORE, "trusted_folders.toml");
        assert_eq!(GROK_SESSION_MARKER, "usage.json");
        assert_eq!(GROK_CONVERSATION_FILE, "chat_history.jsonl");
        assert_eq!(GROK_TITLE_SUMMARY, "summary.json");
        assert_eq!(GROK_SIGN_IN_FILE, "auth.json");
        assert_eq!(GROK_CONFIG_FILE, "config.toml");
        assert_eq!(GROK_MODELS_CACHE, "models_cache.json");
        assert_eq!(CLAUDE_MODEL_CATALOGUE_DIR, "cache/model-catalog");
    }

    /// `crate::records`' `index_records_at` takes a record as the last level of its walk
    /// (`left == 1`), which is the record's own level only for an agent whose search goes exactly
    /// that deep. Claude Code and Grok Build are the agents that code matches by position.
    #[test]
    fn the_agents_matched_by_position_search_exactly_as_deep_as_a_record_sits() {
        for agent in [&CLAUDE, &GROK] {
            assert_eq!(agent.search_depth, agent.record_level);
        }
    }
}
