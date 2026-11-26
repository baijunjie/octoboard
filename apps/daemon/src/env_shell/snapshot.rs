//! The snapshot of the user's login shell as the daemon uses it: what the host's shell gives
//! (`platform::shell::snapshot`), with the variables that mark an agent session or belong to the
//! snapshot shell removed and `HOME` settled.
//! It also holds the same rewrite of the daemon's own process environment
//! (`forget_inherited_session`), for a daemon started inside an agent session.
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
    super::session_exports::MARKER,
    "OCTOBOARD_SESSION_ID",
];

/// True when a variable is not the user's environment: it marks an agent session
/// ([`marks_a_session`]) or belongs to the snapshot shell (`SNAPSHOT_ONLY_VARS`).
fn is_filtered(key: &str) -> bool {
    SNAPSHOT_ONLY_VARS.contains(&key) || marks_a_session(key)
}

/// `env`, an environment that may have been inherited from an agent session (a daemon started in
/// a session's shell has one, and so has the login shell it starts), as it stands outside that
/// session: what the session's launch set for the session alone is put back as the snapshot it
/// was launched over had it (`session_exports`), and the variables that mark a session or belong
/// to the snapshot shell are removed. In that order: the filter takes the marker the first reads.
/// Every reading of an environment that is to stand for the user's goes through this.
pub fn outside_session(mut env: HashMap<String, String>) -> HashMap<String, String> {
    super::session_exports::revert_inherited(&mut env);
    env.retain(|key, _| !is_filtered(key));
    env
}

/// True when a variable says the environment holding it was inherited from an agent session: the
/// record of a launch, one of Octoboard's own session markers, or a variable a provider declares
/// as its session's. The snapshot shell's terminal variables say nothing of the kind: a daemon
/// started from a plain terminal has them too.
fn marks_a_session(key: &str) -> bool {
    INHERITED_SESSION_VARS.contains(&key)
        || crate::providers::descriptors()
            .any(|descriptor| descriptor.inherited_session_vars.contains(&key))
}

/// What turns `env`, a process's own environment, into [`outside_session`] of it: each variable's
/// name with the value it gets, or `None` for one that goes. Empty unless something in `env`
/// [`marks_a_session`]: a daemon that was not started inside a session keeps its environment
/// exactly as it is, the terminal variables of the terminal it was started from included.
fn own_environment_changes(env: &HashMap<String, String>) -> Vec<(String, Option<String>)> {
    if !env.keys().any(|key| marks_a_session(key)) {
        return Vec::new();
    }
    let outside = outside_session(env.clone());
    let mut changes: Vec<(String, Option<String>)> = env
        .iter()
        .filter(|(name, value)| outside.get(*name) != Some(*value))
        .map(|(name, _)| (name.clone(), outside.get(name).cloned()))
        .collect();
    changes.sort();
    changes
}

/// Makes this process's own environment what [`outside_session`] gives of it, where it was
/// inherited from an agent session (see [`own_environment_changes`]). For the daemon alone, which
/// stands outside any session: everything it starts that inherits its environment, and every call
/// of the host's that starts a program for it (the shell's open on Windows, which takes no
/// environment), then has the user's values and none of a session's marks. A helper mode of the
/// same executable runs inside a session, for that session, and must not call this.
///
/// Changing the environment races with any other thread reading it, so this is called while the
/// process has no other thread yet.
pub fn forget_inherited_session() {
    let own = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    for (name, value) in own_environment_changes(&own) {
        match value {
            // A value the host cannot hold is not one a launch recorded.
            Some(value) if !value.contains('\0') => std::env::set_var(&name, value),
            Some(_) => {}
            None => std::env::remove_var(&name),
        }
    }
}

/// Takes the user's login-shell environment, with the filtered variables already removed and
/// `HOME` settled (see `settle_home`).
pub fn take() -> Result<HashMap<String, String>> {
    settled(shell::snapshot(SNAPSHOT_TIMEOUT))
}

/// What `take()` makes of what the host's shell gave: the user-facing error for a timeout, else the
/// environment as it stands outside any session it was inherited from ([`outside_session`]).
fn settled(taken: Result<HashMap<String, String>>) -> Result<HashMap<String, String>> {
    let env = taken.map_err(|err| match err.downcast::<SnapshotTimeout>() {
        Ok(timeout) => timeout_error(&timeout),
        Err(err) => err,
    })?;
    let mut env = outside_session(env);
    settle_home(&mut env);
    Ok(env)
}

/// The daemon's own environment as it stands outside any session it was started in, with `HOME`
/// settled: what a launch runs with when no capture of the login shell's exists.
pub(super) fn daemon_environment() -> HashMap<String, String> {
    let own = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    let mut env = outside_session(own);
    settle_home(&mut env);
    env
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
    use std::path::PathBuf;

    use super::super::session_exports::session_environment;
    use super::*;
    use crate::providers::AgentId;

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

    /// A daemon started inside a session takes its snapshot through `settled`, which undoes what
    /// the session's launch set before it filters: through the test provider's own plan, a pinned
    /// account's directory does not reach the nested daemon's default launch, the user's own
    /// value does, unchanged, and no marker is passed on.
    #[test]
    fn a_nested_daemons_snapshot_holds_the_users_values_not_the_outer_sessions() {
        let agent = AgentId::TEST;
        let var = agent.descriptor().layout.config_dir_var;
        let planned = |shell: &HashMap<String, String>, config_dir: Option<&str>| {
            let mut fixture = crate::adapter::tests::spec_fixture();
            fixture.shell_env = shell.clone();
            fixture.config_dir = config_dir.map(PathBuf::from);
            let plan = crate::adapter::provider(agent)
                .plan(&fixture.spec())
                .expect("a plan");
            session_environment(shell, &plan.env)
        };
        let work = crate::test_support::host_path("/accounts/work");
        let own = crate::test_support::host_path("/users/own");
        let home = (
            "HOME".to_string(),
            crate::test_support::host_path("/Users/snapshot"),
        );
        for shell in [
            HashMap::from([home.clone()]),
            HashMap::from([home.clone(), (var.to_string(), own.clone())]),
        ] {
            let outer = planned(&shell, Some(&work));
            assert_eq!(outer[var], work);
            let nested = settled(Ok(outer)).expect("settled");
            assert_eq!(nested, shell);
            assert_eq!(planned(&nested, None), shell);
            // Nested once more, from a session pinned to another account.
            let personal = crate::test_support::host_path("/accounts/personal");
            let second = planned(&nested, Some(&personal));
            assert_eq!(second[var], personal);
            assert_eq!(settled(Ok(second)).expect("settled"), shell);
        }
    }

    /// The variable [`own_environment_child`] reports through, and the two it is started with.
    const REPORT_VAR: &str = "OCTOBOARDD_TEST_OWN_ENVIRONMENT_REPORT";
    const REPLACED_VAR: &str = "OCTOBOARDD_TEST_OWN_ENVIRONMENT_REPLACED";
    const SET_VAR: &str = "OCTOBOARDD_TEST_OWN_ENVIRONMENT_SET";
    /// One of Octoboard's own session markers, and a terminal variable.
    const MARK_VAR: &str = "OCTOBOARD_SESSION_ID";
    const TERMINAL_VAR: &str = "COLORTERM";

    /// What [`own_environment_child`] reports when started with exactly `session` for the
    /// variables this test is about.
    fn own_environment_after(session: &HashMap<String, String>) -> String {
        let dir = crate::test_support::ScratchDir::new("own-environment");
        let report = dir.join("report");
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args([
            "--exact",
            "env_shell::snapshot::tests::own_environment_child",
            "--ignored",
            "--test-threads=1",
        ]);
        // The test process may itself run inside a session; the child starts outside one.
        for (name, _) in std::env::vars_os() {
            if name.to_str().is_some_and(is_filtered) {
                command.env_remove(name);
            }
        }
        let status = command
            .env_remove(super::super::session_exports::MARKER)
            .envs(session)
            .env(REPORT_VAR, &report)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        std::fs::read_to_string(report).unwrap()
    }

    /// The daemon makes its own environment what it is outside the session it was started in: a
    /// value the launch replaced is the user's again, one it set is gone, and so are the record,
    /// Octoboard's marks and the terminal variables. A mark alone, from a session launched by a
    /// build that recorded nothing, is enough for the marks and terminal variables to go. Started
    /// from a plain terminal, with a terminal variable and no mark, it changes nothing at all.
    /// Run in a child process: the environment is the whole test process's.
    #[test]
    fn the_daemon_reads_its_own_environment_as_outside_a_session_and_leaves_a_plain_one_alone() {
        let pair = |name: &str, value: &str| (name.to_string(), value.to_string());
        let user = HashMap::from([pair(REPLACED_VAR, "the user's own")]);
        let session = session_environment(
            &user,
            &[
                pair(REPLACED_VAR, "the session's"),
                pair(SET_VAR, "1"),
                pair(MARK_VAR, "outer"),
                pair(TERMINAL_VAR, "truecolor"),
            ],
        );
        assert_eq!(
            own_environment_after(&session),
            "replaced=the user's own set=None mark=false terminal=false others_changed=false"
        );

        let marked = HashMap::from([
            pair(SET_VAR, "1"),
            pair(MARK_VAR, "outer"),
            pair(TERMINAL_VAR, "truecolor"),
        ]);
        assert_eq!(
            own_environment_after(&marked),
            "replaced= set=Some(\"1\") mark=false terminal=false others_changed=false"
        );

        let plain = HashMap::from([pair(SET_VAR, "1"), pair(TERMINAL_VAR, "truecolor")]);
        assert_eq!(own_environment_changes(&plain), []);
        assert_eq!(
            own_environment_after(&plain),
            "replaced= set=Some(\"1\") mark=false terminal=true others_changed=false"
        );
    }

    #[test]
    #[ignore = "run in a process of its own by the test above"]
    fn own_environment_child() {
        let Some(report) = std::env::var_os(REPORT_VAR) else {
            return;
        };
        let ours = [REPLACED_VAR, SET_VAR, MARK_VAR, TERMINAL_VAR];
        let others = || -> Vec<_> {
            std::env::vars_os()
                .filter(|(name, _)| {
                    name.to_str().is_some_and(|name| {
                        !ours.contains(&name) && name != super::super::session_exports::MARKER
                    })
                })
                .collect()
        };
        let before = others();
        forget_inherited_session();
        std::fs::write(
            report,
            format!(
                "replaced={} set={:?} mark={} terminal={} others_changed={}",
                std::env::var(REPLACED_VAR).unwrap_or_default(),
                std::env::var(SET_VAR).ok(),
                std::env::var_os(MARK_VAR).is_some(),
                std::env::var_os(TERMINAL_VAR).is_some(),
                before != others(),
            ),
        )
        .unwrap();
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

    /// What a launch falls back on when no capture exists is the daemon's own environment, with
    /// its values as they are and an absolute `HOME`.
    #[test]
    fn the_daemons_own_environment_carries_its_values_and_an_absolute_home() {
        let env = daemon_environment();
        assert!(Path::new(&env["HOME"]).is_absolute(), "{:?}", env["HOME"]);
        #[cfg(unix)]
        if let Some(path) = std::env::var_os("PATH").and_then(|path| path.into_string().ok()) {
            assert_eq!(env["PATH"], path);
        }
    }
}
