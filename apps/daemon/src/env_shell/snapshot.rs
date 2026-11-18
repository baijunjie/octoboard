//! The snapshot of the user's login shell as the daemon uses it: what the host's shell gives
//! (`platform::shell::snapshot`), with the variables that mark an agent session or belong to the
//! snapshot shell removed and `HOME` settled.
//!
//! The snapshot shell is run with `TERM=dumb`, which keeps it from printing a prompt. That `TERM`
//! is for the snapshot shell only and must not reach the agent: `env -0` dumps it like any other
//! variable, and an agent inheriting `TERM=dumb` downgrades its own renderer, colour handling and
//! mouse reporting. `SNAPSHOT_ONLY_VARS` is dropped from the snapshot and the PTY command sets its
//! own `TERM` instead.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use crate::platform::shell::{self, SnapshotTimeout};
use crate::protocol::{error_code, CodedError};

/// Upper bound on how long the login-shell snapshot may run. An rc file waiting on a version
/// manager or a network call is routinely a second or two; this leaves a wide margin above that
/// while still turning a wedged rc file (one blocked on something other than stdin, which never
/// resolves on its own) into a reported failure within a single human-noticeable wait instead of
/// a launch that hangs forever.
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);

/// Variables that belong to the snapshot shell rather than to the agent, and are stripped from the
/// snapshot before it is handed to a PTY command.
pub const SNAPSHOT_ONLY_VARS: &[&str] = &[
    "TERM",
    "TERMINFO",
    "COLORTERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
];

/// Octoboard's own session markers, stripped when a daemon inherits a session's shell.
pub const INHERITED_SESSION_VARS: &[&str] = &[
    "OCTOBOARD_DAEMON_CREDENTIAL_PATH",
    // Earlier builds put the key itself, and then its file as `OCTOBOARD_DAEMON_KEY_FILE`, in a
    // session's environment; a daemon started from inside such a session must not pass them on.
    "OCTOBOARD_DAEMON_KEY",
    "OCTOBOARD_DAEMON_KEY_FILE",
    "OCTOBOARD_DAEMON_PORT",
    "OCTOBOARD_PROCESS_TAG",
    "OCTOBOARD_SESSION_ID",
];

/// True when a variable marks the daemon's own agent session rather than the user's environment.
pub fn is_filtered(key: &str) -> bool {
    SNAPSHOT_ONLY_VARS.contains(&key)
        || INHERITED_SESSION_VARS.contains(&key)
        || crate::providers::descriptors()
            .any(|descriptor| descriptor.inherited_session_vars.contains(&key))
}

/// Takes the user's login-shell environment, with the filtered variables already removed and
/// `HOME` settled (see `settle_home`).
pub fn take() -> Result<HashMap<String, String>> {
    settled(shell::snapshot(SNAPSHOT_TIMEOUT))
}

/// What `take()` makes of what the host's shell gave: the user-facing error for a timeout, else the
/// environment without the variables that are not the user's.
fn settled(taken: Result<HashMap<String, String>>) -> Result<HashMap<String, String>> {
    let mut env = taken.map_err(|err| match err.downcast::<SnapshotTimeout>() {
        Ok(timeout) => timeout_error(&timeout),
        Err(err) => err,
    })?;
    env.retain(|key, _| !is_filtered(key));
    settle_home(&mut env);
    Ok(env)
}

/// Gives the snapshot an absolute `HOME`: one the shell left unset or relative becomes the
/// daemon's own home directory (`crate::paths::home_dir`). Agents are launched with this
/// environment, and every directory taken from its `HOME` (an account's, a Codex database's) must
/// be one place rather than one relative to whatever directory the reader runs in.
fn settle_home(env: &mut HashMap<String, String>) {
    let absolute = env
        .get("HOME")
        .is_some_and(|home| Path::new(home).is_absolute());
    if !absolute {
        env.insert(
            "HOME".to_string(),
            crate::paths::home_dir().to_string_lossy().into_owned(),
        );
    }
}

/// The user-facing timeout error.
fn timeout_error(timeout: &SnapshotTimeout) -> anyhow::Error {
    let SnapshotTimeout {
        shell,
        command,
        timeout,
    } = timeout;
    CodedError::raised(
        error_code::SHELL_ENVIRONMENT_TIMEOUT,
        format!(
            "`{shell} -l -i -c '{command}'` did not finish within {timeout:?}; a shell startup file \
             is probably blocked on something other than stdin, or it left a background process \
             holding the shell's output open. {app} refuses to launch with an unknown \
             environment rather than guess at one — fix or skip the slow step in the shell's rc \
             files.",
            app = crate::APP_NAME
        ),
        &[
            ("shell", shell),
            ("command", command),
            ("timeout", &format!("{timeout:?}")),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variables_of_the_snapshot_shell_and_of_an_agent_session_are_stripped() {
        let raw = HashMap::from([
            ("FOO".to_string(), "bar".to_string()),
            ("HOME".to_string(), "/Users/snapshot".to_string()),
            ("TERM".to_string(), "dumb".to_string()),
            ("CLAUDE_PID".to_string(), "1".to_string()),
        ]);
        let env = settled(Ok(raw)).expect("settled");
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
        assert!(!env.contains_key("TERM"));
        assert!(!env.contains_key("CLAUDE_PID"));
    }

    /// A session started from inside another session must not inherit the daemon's address, the
    /// path of the first session's key file or, from an earlier build, the key itself.
    #[test]
    fn the_daemon_address_and_key_file_variables_of_a_session_are_stripped() {
        let raw = HashMap::from([
            ("FOO".to_string(), "bar".to_string()),
            ("HOME".to_string(), "/Users/snapshot".to_string()),
            (
                "OCTOBOARD_DAEMON_CREDENTIAL_PATH".to_string(),
                "/run/s/key".to_string(),
            ),
            (
                "OCTOBOARD_DAEMON_KEY_FILE".to_string(),
                "/run/s/key".to_string(),
            ),
            ("OCTOBOARD_DAEMON_KEY".to_string(), "k".to_string()),
            ("OCTOBOARD_DAEMON_PORT".to_string(), "7".to_string()),
            ("OCTOBOARD_SESSION_ID".to_string(), "s".to_string()),
        ]);
        let env = settled(Ok(raw)).expect("settled");
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
        for name in [
            "OCTOBOARD_DAEMON_CREDENTIAL_PATH",
            "OCTOBOARD_DAEMON_KEY",
            "OCTOBOARD_DAEMON_KEY_FILE",
            "OCTOBOARD_DAEMON_PORT",
            "OCTOBOARD_SESSION_ID",
        ] {
            assert!(!env.contains_key(name), "{name}");
            assert!(is_filtered(name), "{name}");
        }
    }

    #[test]
    fn every_providers_declared_session_markers_are_filtered_without_losing_user_settings() {
        let settings = [
            "CODEX_HOME",
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_CODE_USE_BEDROCK",
            "GROK_HOME",
            "GROK_CODE_XAI_API_KEY",
        ];
        let mut raw = HashMap::from_iter(
            settings
                .into_iter()
                .map(|key| (key.to_string(), "user-value".to_string())),
        );
        for descriptor in crate::providers::descriptors() {
            for key in descriptor.inherited_session_vars {
                raw.insert(key.to_string(), "session-marker".to_string());
            }
        }
        let env = settled(Ok(raw)).expect("settled");
        for descriptor in crate::providers::descriptors() {
            for key in descriptor.inherited_session_vars {
                assert!(!env.contains_key(*key), "{key}");
            }
        }
        for key in settings {
            assert_eq!(
                env.get(key).map(String::as_str),
                Some("user-value"),
                "{key}"
            );
        }
    }

    /// A timeout reaches the caller as the coded error the user is told about.
    #[test]
    fn a_timeout_is_worded_for_the_user() {
        let timeout = anyhow::Error::new(SnapshotTimeout {
            shell: "/bin/zsh".to_string(),
            command: "env -0".to_string(),
            timeout: Duration::from_secs(10),
        });
        let err = settled(Err(timeout)).expect_err("a timeout");
        assert!(err.to_string().contains("did not finish within"));
        assert!(err.to_string().contains("rc files"));
        let other = settled(Err(anyhow::anyhow!("boom"))).expect_err("another failure");
        assert_eq!(other.to_string(), "boom");
    }

    /// An absolute `HOME` is kept; one unset, empty or relative becomes the daemon's own home.
    #[test]
    fn the_snapshot_always_carries_an_absolute_home() {
        let own = crate::paths::home_dir().to_string_lossy().into_owned();
        let absolute = crate::test_support::host_path("/Users/snapshot");
        for (given, expected) in [
            (Some(absolute.as_str()), absolute.as_str()),
            (Some(""), own.as_str()),
            (Some("relative/home"), own.as_str()),
            (None, own.as_str()),
        ] {
            let mut env = HashMap::new();
            if let Some(home) = given {
                env.insert("HOME".to_string(), home.to_string());
            }
            settle_home(&mut env);
            assert_eq!(env["HOME"], expected, "{given:?}");
        }
    }
}
