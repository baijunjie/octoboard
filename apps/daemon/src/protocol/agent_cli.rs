//! What the daemon knows about an agent's installed CLI: its version, how it was installed, the
//! latest version published, and an update of it that runs or ran. See "Agent CLI versions" in
//! `apps/daemon/PROTOCOL.md`.

use serde::Serialize;

use super::agents::AgentId;
use super::codes::Code;

/// How an agent's CLI was installed, which decides the command that updates it.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CliInstallMethod {
    /// The CLI's own installer: it updates itself (`<cli> update`).
    Native,
    /// A package of the npm registry, installed globally.
    Npm,
    /// A Homebrew formula or cask.
    Homebrew,
    /// None of the above, or one that cannot be told: the version is shown and no update is
    /// offered.
    Unknown,
}

/// Where an update of a CLI is.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CliUpdateState {
    Running,
    Succeeded,
    Failed,
}

/// An update of an agent's CLI as the daemon last saw it. Runtime state: never stored.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentCliUpdate {
    pub state: CliUpdateState,
    /// When the update started, in milliseconds since the epoch.
    pub started_at: i64,
    /// When it ended; null while it runs.
    pub finished_at: Option<i64>,
    /// Why a `failed` update failed, one of `agent_cli_code`'s update codes; null otherwise.
    pub error: Option<Code>,
    /// The last line the update printed on stderr (else on stdout), for a `failed` one. English
    /// text of the tool, trimmed and cut to a bounded length.
    pub detail: Option<String>,
}

/// One agent's installed CLI as the daemon last checked it. Runtime state: never stored.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentCliStatus {
    pub agent: AgentId,
    /// The version of the CLI that starts a session now, as `major.minor.patch`; null when the
    /// CLI is not installed or its version could not be told.
    pub installed_version: Option<String>,
    pub install_method: CliInstallMethod,
    /// The latest version published for this install method, as the source names it; null when
    /// there is no source for it (`install_method` `unknown`) or looking it up failed, in which
    /// case `latest_error` says so.
    pub latest_version: Option<String>,
    /// Why the lookup of `latest_version` failed, one of `agent_cli_code`'s latest codes; null
    /// when it succeeded or none was made.
    pub latest_error: Option<Code>,
    /// The English text of the failure behind `latest_error`, when it has one.
    pub latest_detail: Option<String>,
    /// When the check ended, in milliseconds since the epoch.
    pub checked_at: i64,
    /// The last update of this CLI since the daemon started; null when none ran.
    pub update: Option<AgentCliUpdate>,
}
