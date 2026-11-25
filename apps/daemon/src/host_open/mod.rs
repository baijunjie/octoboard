//! Starting the host's own launchers: the system file manager and default applications
//! (`paths`), the system terminal (`terminal`) and an agent's desktop application (`native_app`).
//! A launcher is either waited on, for one that exits once the host has accepted the request, or
//! started in its own session and reaped in the background, for one that lives as long as the
//! application it opens. What each launcher is asked is the caller's; this module owns the start,
//! the bound on a waited one and the reading of a failed launch.

use std::collections::HashMap;
use std::ffi::OsString;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::platform::proc::Reach;
use crate::processes::subprocess::{run_bounded, Bounds};

pub mod native_app;
pub mod paths;
pub mod terminal;

/// What a waited command may take and say (a launcher or the `mdls` type query): the host answers in
/// moments, so this only bounds a wedged one, not the application a launcher then opens.
const OPEN_BOUNDS: Bounds = Bounds {
    timeout: Duration::from_secs(15),
    stdout_limit: 16 * 1024,
    stderr_limit: 16 * 1024,
    reach: Reach::Group,
    term_grace: Duration::ZERO,
};

/// The environment a launcher is started with: `inherited`, the daemon's own, as it stands
/// outside the agent session the daemon may have been started in
/// (`crate::env_shell::outside_session`). What a launcher opens (a terminal, an editor) is the
/// user's, and a daemon run from a session's shell would otherwise hand it that session's account
/// directory and git report variables. A variable whose name or value is not Unicode cannot be
/// one a launch recorded, and is passed on as it is.
fn launcher_environment(
    inherited: impl Iterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, OsString)> {
    let mut text = HashMap::new();
    let mut other = Vec::new();
    for (name, value) in inherited {
        match (name.into_string(), value.into_string()) {
            (Ok(name), Ok(value)) => {
                text.insert(name, value);
            }
            (name, value) => other.push((
                name.map_or_else(|name| name, OsString::from),
                value.map_or_else(|value| value, OsString::from),
            )),
        }
    }
    other.extend(
        crate::env_shell::outside_session(text)
            .into_iter()
            .map(|(name, value)| (name.into(), value.into())),
    );
    other
}

/// Starts `command` in [`launcher_environment`], with what the command itself sets laid over it.
/// A detached one is handed to [`crate::processes::subprocess::spawn_detached`]; any other is run
/// to its end within [`OPEN_BOUNDS`] and judged by its exit status.
fn start(detached: bool, mut command: Command) -> Result<(), String> {
    let own: Vec<(OsString, Option<OsString>)> = command
        .get_envs()
        .map(|(name, value)| (name.to_owned(), value.map(ToOwned::to_owned)))
        .collect();
    command
        .env_clear()
        .envs(launcher_environment(std::env::vars_os()));
    for (name, value) in own {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    if detached {
        return crate::processes::subprocess::spawn_detached(command);
    }
    let output = run_bounded(&mut command, &OPEN_BOUNDS, &AtomicBool::new(false))
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    for bytes in [&output.stderr, &output.stdout] {
        let message = String::from_utf8_lossy(bytes).trim().to_string();
        if !message.is_empty() {
            return Err(message);
        }
    }
    Err(format!("the launcher exited with {}", output.status))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A launcher started by a daemon that was itself started inside an agent session gets none
    /// of what that session's launch set, and no marker of Octoboard's: the user's own values, and
    /// what cannot be read as text passed on untouched.
    #[test]
    fn a_launcher_gets_the_daemons_environment_without_an_outer_sessions_values() {
        let pair = |name: &str, value: &str| (name.to_string(), value.to_string());
        let user = HashMap::from([pair("PATH", "/bin"), pair("AGENT_HOME", "/users/own")]);
        let session = crate::env_shell::session_environment(
            &user,
            &[
                pair("AGENT_HOME", "/accounts/work"),
                pair("OCTOBOARD_SESSION_ID", "outer"),
                pair("SESSION_ONLY", "1"),
            ],
        );
        #[cfg(unix)]
        let unreadable: OsString = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![0xff])
        };
        #[cfg(not(unix))]
        let unreadable = OsString::from("plain");
        let inherited = session
            .into_iter()
            .map(|(name, value)| (OsString::from(name), OsString::from(value)))
            .chain([(OsString::from("ODD"), unreadable.clone())]);

        let mut environment = launcher_environment(inherited);
        environment.sort();
        assert_eq!(
            environment,
            [
                (OsString::from("AGENT_HOME"), OsString::from("/users/own")),
                (OsString::from("ODD"), unreadable),
                (OsString::from("PATH"), OsString::from("/bin")),
            ]
        );
    }
}
