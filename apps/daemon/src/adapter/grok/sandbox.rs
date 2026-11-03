//! Grok's sandbox check: finding the sandbox profile a launch would run under, so the launch can be
//! refused up front.

use std::path::Path;

use anyhow::Result;

use crate::adapter::files::{read_regular_text, SMALL_FILE_LIMIT};
use crate::adapter::LaunchSpec;
use crate::protocol::{Code, CodedError};
use crate::providers::grok;
use crate::records::line_file::JSON_FILE_LIMIT;

/// One sandbox profile in effect, and where it is set.
pub(super) struct SandboxSetting {
    /// The environment variable, or the file, that sets it.
    source: String,
    pub(super) profile: String,
}

/// Refuses a launch that would run under Grok's sandbox; see the `grok` module comment for why the
/// farm cannot support one.
pub(super) fn require_unsandboxed(
    spec: &LaunchSpec<'_>,
    source_home: &Path,
    system_dir: &Path,
) -> Result<()> {
    let Some(setting) = sandbox_setting(spec, source_home, system_dir) else {
        return Ok(());
    };
    Err(CodedError::raised(
        Code::of_provider(grok::SANDBOX_UNSUPPORTED),
        format!(
            "Octoboard's per-session Grok home is incompatible with Grok's sandbox (profile `{}`, \
             set by {}): to launch this session through Octoboard, turn the sandbox off where it \
             is set, or ask whoever manages that setting to. A conversation started under the \
             sandbox keeps it, so continue that one in Grok itself",
            setting.profile, setting.source
        ),
        &[("source", &setting.source), ("profile", &setting.profile)],
    ))
}

/// The sandbox profile Grok would apply to this launch, `None` when it would run unsandboxed or
/// the profile cannot be read reliably. A non-`off` `requirements.toml` pin (the system
/// directory's; the home's is excluded from the farm) refuses, and an `off` pin stops the search.
/// On a resume, either the profile saved with the session or `GROK_SANDBOX` in the launch
/// environment being non-`off` refuses, because Grok restores a saved profile and refuses a resume
/// whose requested one differs. Otherwise, and for a resume with no saved profile found,
/// `GROK_SANDBOX` beats the user's `config.toml`, which beats the system `managed_config.toml`.
/// `GROK_SANDBOX` is read from the launch environment, not the daemon's own.
pub(super) fn sandbox_setting(
    spec: &LaunchSpec<'_>,
    source_home: &Path,
    system_dir: &Path,
) -> Option<SandboxSetting> {
    let on = |source: String, profile: String| {
        (profile != "off").then_some(SandboxSetting { source, profile })
    };
    let from_file = |dir: &Path, name: &str| {
        let file = dir.join(name);
        let profile = toml_sandbox_profile(&file)?;
        Some((file.to_string_lossy().into_owned(), profile))
    };
    let from_env = || {
        spec.shell_env
            .get("GROK_SANDBOX")
            .map(|profile| profile.trim())
            .filter(|profile| !profile.is_empty())
            .map(|profile| ("GROK_SANDBOX".to_string(), profile.to_string()))
    };

    if let Some((source, profile)) = from_file(system_dir, "requirements.toml") {
        return on(source, profile);
    }

    let saved = spec
        .resume_agent_session_id
        .and_then(|id| saved_sandbox_profile(source_home, id));
    if let Some(saved) = saved {
        return [Some(saved), from_env()]
            .into_iter()
            .flatten()
            .find_map(|(source, profile)| on(source, profile));
    }

    if let Some((source, profile)) = from_env() {
        return on(source, profile);
    }
    [
        (source_home, grok::GROK_CONFIG_FILE),
        (system_dir, "managed_config.toml"),
    ]
    .into_iter()
    .find_map(|(dir, name)| from_file(dir, name))
    .and_then(|(source, profile)| on(source, profile))
}

/// `[sandbox] profile` of a Grok TOML file; `None` when the file is absent, unreadable within its
/// size bound, or does not set a non-empty one.
fn toml_sandbox_profile(file: &Path) -> Option<String> {
    let table = read_regular_text(file, SMALL_FILE_LIMIT)
        .ok()?
        .parse::<toml::Table>()
        .ok()?;
    let profile = table.get("sandbox")?.get("profile")?.as_str()?.trim();
    (!profile.is_empty()).then(|| profile.to_string())
}

/// The profile Grok saved in the `summary.json` of conversation `id`, found one directory below
/// `sessions/` rather than by reproducing how Grok encodes the working directory into its name.
pub(super) fn saved_sandbox_profile(source_home: &Path, id: &str) -> Option<(String, String)> {
    // The id joins a path below, so only the shape of an id Grok mints is looked up.
    if !crate::records::plausible_id(id) {
        return None;
    }
    for entry in std::fs::read_dir(source_home.join(grok::GROK_RECORDS_DIR))
        .ok()?
        .flatten()
    {
        let summary = entry.path().join(id).join(grok::GROK_TITLE_SUMMARY);
        if !std::fs::symlink_metadata(&summary).is_ok_and(|meta| meta.is_file()) {
            continue;
        }
        let Ok(text) = read_regular_text(&summary, JSON_FILE_LIMIT as usize) else {
            continue;
        };
        let document: serde_json::Value = serde_json::from_str(&text).ok()?;
        let profile = document.get("sandbox_profile")?.as_str()?.trim();
        return (!profile.is_empty())
            .then(|| (summary.to_string_lossy().into_owned(), profile.to_string()));
    }
    None
}
