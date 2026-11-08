//! The latest version published for an agent's CLI, read from the source its install method
//! names: the CLI's own check where it has one, the npm registry for an npm install and for a CLI
//! whose own installer is published there too, Homebrew's own record for a Homebrew install. An
//! install the daemon does not recognise has no source.
//!
//! The lookup is best-effort: it is a read of something outside the machine, so a failure is told
//! as the [`Failure`] it was and never raised as an error of the request that asked. The registry
//! is read the way the user's own network reaches it: through the proxy their shell environment
//! names and trusting the system's certificate store, not through the daemon's own environment.

use std::collections::HashMap;
use std::time::Duration;

use crate::adapter::{account_cli, CliPackaging};
use crate::protocol::{agent_cli_code, AgentId, CliInstallMethod, Code};

use super::install::{Install, ToolCommand};
use super::{bounded, failure_line, last_line, run_tool};

/// How long the lookup of one version may take, whichever source it is.
pub(super) const LOOKUP_TIMEOUT: Duration = Duration::from_secs(20);

/// The most of a source's reply that is read; a version answer is far smaller.
const REPLY_LIMIT: usize = 1024 * 1024;

/// Why the latest version is not known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// The source could not be reached, did not answer in time or reported an error.
    Failed(String),
    /// The source answered with no version that could be read.
    Unreadable,
}

impl Failure {
    pub fn code(&self) -> Code {
        match self {
            Failure::Failed(_) => agent_cli_code::LATEST_FAILED,
            Failure::Unreadable => agent_cli_code::LATEST_UNREADABLE,
        }
    }

    pub fn detail(&self) -> Option<String> {
        match self {
            Failure::Failed(detail) => Some(bounded(detail)),
            Failure::Unreadable => None,
        }
    }
}

/// The npm registry, as far as the daemon reads it: the latest version of a package. A seam so
/// that a test makes no request.
pub(crate) trait Registry: Sync {
    /// The latest version of `package`, asked as `shell_env` would reach the network.
    fn latest_version(
        &self,
        package: &str,
        shell_env: &HashMap<String, String>,
    ) -> Result<String, Failure>;
}

/// The host of the registry, which `NO_PROXY` is matched against.
const REGISTRY_HOST: &str = "registry.npmjs.org";

/// The environment variables that name the proxy for an HTTPS request, in the order they apply.
const PROXY_VARIABLES: [&str; 6] = [
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "HTTP_PROXY",
    "http_proxy",
];

/// The public npm registry, over HTTPS. A custom registry in the user's `.npmrc` is not read.
#[cfg_attr(
    test,
    allow(dead_code, reason = "the tests use a registry of their own")
)]
pub(crate) struct NpmRegistry;

impl Registry for NpmRegistry {
    fn latest_version(
        &self,
        package: &str,
        shell_env: &HashMap<String, String>,
    ) -> Result<String, Failure> {
        let url = format!("https://{REGISTRY_HOST}/{package}/latest");
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(LOOKUP_TIMEOUT))
            .proxy(proxy_of(shell_env)?)
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .into();
        let mut response = agent
            .get(&url)
            .header("Accept", "application/json")
            .call()
            .map_err(|err| Failure::Failed(err.to_string()))?;
        let body = response
            .body_mut()
            .with_config()
            .limit(REPLY_LIMIT as u64)
            .read_to_string()
            .map_err(|err| Failure::Failed(err.to_string()))?;
        version_field(&body, "version")
    }
}

/// The proxy `shell_env` names for the registry, `None` when it names none or `NO_PROXY` exempts
/// the registry's host. Never the daemon's own environment, which is not the user's network.
pub(super) fn proxy_of(
    shell_env: &HashMap<String, String>,
) -> Result<Option<ureq::Proxy>, Failure> {
    let value = |name: &str| shell_env.get(name).map(|value| value.trim());
    let no_proxy = value("NO_PROXY")
        .or_else(|| value("no_proxy"))
        .unwrap_or("");
    if bypasses(no_proxy, REGISTRY_HOST) {
        return Ok(None);
    }
    for name in PROXY_VARIABLES {
        let Some(url) = value(name).filter(|url| !url.is_empty()) else {
            continue;
        };
        return ureq::Proxy::new(url)
            .map(Some)
            .map_err(|_| Failure::Failed(format!("{name} is not a proxy address")));
    }
    Ok(None)
}

/// Whether the `NO_PROXY` list `no_proxy` names `host`: `*`, the host itself, or a domain it is
/// under, written with or without a leading `.` or `*.`.
fn bypasses(no_proxy: &str, host: &str) -> bool {
    no_proxy.split(',').map(str::trim).any(|entry| {
        let domain = entry.trim_start_matches('*').trim_start_matches('.');
        entry == "*"
            || (!domain.is_empty()
                && (host.eq_ignore_ascii_case(domain)
                    || host
                        .to_ascii_lowercase()
                        .ends_with(&format!(".{}", domain.to_ascii_lowercase()))))
    })
}

/// The latest version of the CLI of `agent` installed as `install`, `None` where this install has
/// no source to ask. Blocking.
pub(crate) fn latest_version(
    agent: AgentId,
    packaging: &CliPackaging,
    install: &Install,
    shell_env: &HashMap<String, String>,
    registry: &dyn Registry,
) -> Option<Result<String, Failure>> {
    match install.method {
        CliInstallMethod::Native => match &packaging.native_latest {
            Some(check) => Some(own_check(agent, check, shell_env)),
            None => Some(registry.latest_version(packaging.npm_package?, shell_env)),
        },
        CliInstallMethod::Npm => Some(registry.latest_version(packaging.npm_package?, shell_env)),
        CliInstallMethod::Homebrew => {
            let command = install.homebrew_info_command(shell_env)?;
            Some(homebrew_record(&command, shell_env))
        }
        CliInstallMethod::Unknown => None,
    }
}

/// The version the CLI's own check names. The check is a query of the agent's own CLI, run as the
/// default account would run it.
fn own_check(
    agent: AgentId,
    check: &crate::adapter::NativeLatest,
    shell_env: &HashMap<String, String>,
) -> Result<String, Failure> {
    let output = account_cli::query(
        agent,
        shell_env,
        None,
        check.args,
        LOOKUP_TIMEOUT,
        REPLY_LIMIT,
        None,
    )
    .map_err(|failure| match failure {
        account_cli::QueryFailure::NotRun(not_run) => Failure::Failed(not_run.detail()),
        account_cli::QueryFailure::Bounded(bounded) => Failure::Failed(bounded.to_string()),
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        // The check prints JSON, so a line of its stdout is no message: its own `error` first.
        let detail = error_field(&stdout)
            .or_else(|| last_line(&output.stderr))
            .unwrap_or_else(|| format!("the check ended with {}", output.status));
        return Err(Failure::Failed(detail));
    }
    if let Some(error) = error_field(&stdout) {
        return Err(Failure::Failed(error));
    }
    version_field(&stdout, check.version_field)
}

/// The version Homebrew's metadata gives the package: a formula's stable version, a cask's.
fn homebrew_record(
    command: &ToolCommand,
    shell_env: &HashMap<String, String>,
) -> Result<String, Failure> {
    let output = run_tool(command, shell_env, LOOKUP_TIMEOUT, REPLY_LIMIT)
        .map_err(|err| Failure::Failed(err.to_string()))?;
    if !output.status.success() {
        return Err(Failure::Failed(failure_line(
            &output,
            &format!("`brew info` ended with {}", output.status),
        )));
    }
    let record: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|_| Failure::Unreadable)?;
    let formula = record.pointer("/formulae/0/versions/stable");
    let cask = record.pointer("/casks/0/version");
    formula
        .or(cask)
        .and_then(serde_json::Value::as_str)
        .and_then(plausible)
        .ok_or(Failure::Unreadable)
}

/// `field` of the JSON object `text`, as a version.
pub(super) fn version_field(text: &str, field: &str) -> Result<String, Failure> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| Failure::Unreadable)?;
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .and_then(plausible)
        .ok_or(Failure::Unreadable)
}

/// What a CLI's own check reports as its error, when the JSON it printed has one.
fn error_field(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let error = value.get("error")?;
    match error {
        serde_json::Value::Null => None,
        serde_json::Value::String(message) => Some(message.clone()),
        other => Some(other.to_string()),
    }
}

/// `text` as a version, if it reads as one: it starts with a digit, as `0.166.0` and
/// `2.1.319-beta.1` do, and is short and printable.
fn plausible(text: &str) -> Option<String> {
    let text = text.trim();
    (text.starts_with(|c: char| c.is_ascii_digit())
        && text.len() <= 64
        && text.chars().all(|c| c.is_ascii_graphic()))
    .then(|| text.to_owned())
}
