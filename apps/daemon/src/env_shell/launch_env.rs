//! The environment an agent launch runs with, which never fails for want of a snapshot: the
//! capture the launch policy allows, else the last capture however old, else the daemon's own
//! environment as it stands outside any session. A person who opens a session is never refused
//! because Octoboard could not read their login shell (P10, "Terminal first"); the launch says
//! which environment it got and why, and tells the user.

use std::collections::HashMap;

use anyhow::Result;

use crate::protocol::{error_code, notice_code, CodedError, Notice};

type Env = HashMap<String, String>;

/// What a launch runs with when the login shell's environment could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackCause {
    /// The last capture of the login shell's environment, however old.
    EarlierCapture,
    /// The daemon's own environment, outside any session it was started in.
    Daemon,
}

impl FallbackCause {
    /// Its wire name, the `cause` of `shell_environment_unavailable`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EarlierCapture => "earlier_capture",
            Self::Daemon => "daemon",
        }
    }
}

/// Why a launch runs on a fallback environment: which one, and the snapshot's failure.
#[derive(Debug, Clone)]
pub struct Fallback {
    pub cause: FallbackCause,
    /// The snapshot's failure as a client is told it.
    pub failure: CodedError,
}

impl Fallback {
    /// The failure as text, the notice's `detail`.
    pub fn detail(&self) -> &str {
        &self.failure.message
    }

    /// The standing notice of a launch that ran on this fallback (`shell_environment_unavailable`).
    pub fn notice(&self) -> Notice {
        let source = match self.cause {
            FallbackCause::EarlierCapture => "an earlier capture of it",
            FallbackCause::Daemon => "the daemon's own environment",
        };
        Notice::new(
            notice_code::SHELL_ENVIRONMENT_UNAVAILABLE,
            format!(
                "The login shell's environment could not be read, so this session runs with \
                 {source}: {}",
                self.detail()
            ),
            &[("detail", self.detail()), ("cause", self.cause.as_str())],
        )
        .standing()
    }

    /// `err`, a binary that did not resolve on this fallback's environment, as the refusal it
    /// stays, with the snapshot's failure nested in it: a daemon the desktop application started
    /// may not have the CLI on its own `PATH`, which the user is then told. Any other error is
    /// returned as it is.
    pub fn explain_not_found(&self, err: anyhow::Error) -> anyhow::Error {
        let Some(coded) = err
            .downcast_ref::<CodedError>()
            .filter(|coded| coded.code == error_code::BINARY_NOT_FOUND)
        else {
            return err;
        };
        let binary = coded.params.get("binary").cloned().unwrap_or_default();
        let failure = anyhow::Error::new(self.failure.clone());
        CodedError::nesting(
            error_code::BINARY_NOT_FOUND,
            &failure,
            &[("binary", &binary), ("cause", self.cause.as_str())],
            |reason| {
                format!(
                    "`{binary}` was not found on PATH in the environment the launch fell back to, \
                     since the login shell's could not be read: {reason}"
                )
            },
        )
    }
}

/// The environment a launch runs with, and the fallback it took where the login shell's could not
/// be read. A caller that resolved something from it beforehand hands it to the launch whole, so
/// the launch tells the fallback too.
#[derive(Debug, Clone)]
pub struct LaunchEnv {
    pub vars: Env,
    pub fallback: Option<Fallback>,
}

impl LaunchEnv {
    /// A captured environment, no fallback.
    pub fn captured(vars: Env) -> Self {
        Self {
            vars,
            fallback: None,
        }
    }
}

/// What a launch runs with given the snapshot `taken`: it, or on its failure `earlier` (the last
/// capture, however old), else `daemon()`.
pub(super) fn with_fallback(
    taken: Result<Env>,
    earlier: Option<Env>,
    daemon: impl FnOnce() -> Env,
) -> LaunchEnv {
    let err = match taken {
        Ok(vars) => return LaunchEnv::captured(vars),
        Err(err) => err,
    };
    let failure = CodedError::of(&err);
    let (vars, cause) = match earlier {
        Some(vars) => (vars, FallbackCause::EarlierCapture),
        None => (daemon(), FallbackCause::Daemon),
    };
    tracing::warn!(
        error = %format!("{err:#}"),
        cause = cause.as_str(),
        "the login shell's environment could not be read; launching on a fallback"
    );
    LaunchEnv {
        vars,
        fallback: Some(Fallback { cause, failure }),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    use super::*;

    fn env(name: &str) -> Env {
        HashMap::from([("WHO".to_string(), name.to_string())])
    }

    #[test]
    fn a_capture_is_served_as_it_is() {
        let got = with_fallback(Ok(env("shell")), Some(env("earlier")), || env("daemon"));
        assert_eq!(got.vars["WHO"], "shell");
        assert!(got.fallback.is_none());
    }

    #[test]
    fn a_failed_snapshot_falls_back_to_the_earlier_capture_then_the_daemon() {
        let got = with_fallback(Err(anyhow!("rc file hung")), Some(env("earlier")), || {
            panic!("the daemon's own environment is not read while a capture exists")
        });
        assert_eq!(got.vars["WHO"], "earlier");
        let fallback = got.fallback.expect("a fallback");
        assert_eq!(fallback.cause, FallbackCause::EarlierCapture);
        assert!(fallback.detail().contains("rc file hung"));

        let got = with_fallback(Err(anyhow!("rc file hung")), None, || env("daemon"));
        assert_eq!(got.vars["WHO"], "daemon");
        assert_eq!(got.fallback.unwrap().cause, FallbackCause::Daemon);
    }

    #[test]
    fn the_notice_is_standing_and_names_the_cause_and_the_failure() {
        let fallback = with_fallback(Err(anyhow!("rc file hung")), None, || env("daemon"))
            .fallback
            .unwrap();
        let notice = fallback.notice();
        assert_eq!(notice.code, notice_code::SHELL_ENVIRONMENT_UNAVAILABLE);
        assert!(notice.standing);
        assert_eq!(notice.params["cause"], "daemon");
        assert!(notice.params["detail"].contains("rc file hung"));
    }

    #[test]
    fn a_binary_not_found_on_a_fallback_nests_the_snapshot_failure() {
        let fallback = with_fallback(
            Err(CodedError::raised(
                error_code::SHELL_ENVIRONMENT_TIMEOUT,
                "the shell took too long",
                &[("timeout", "10s")],
            )),
            Some(env("earlier")),
            || env("daemon"),
        )
        .fallback
        .unwrap();
        let not_found = super::super::resolve_binary("octoboard-no-such-tool", &HashMap::new())
            .expect_err("nothing on an empty environment");
        let explained = fallback.explain_not_found(not_found);
        let coded = explained.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::BINARY_NOT_FOUND);
        assert_eq!(coded.params["binary"], "octoboard-no-such-tool");
        assert_eq!(coded.params["reason_code"], "shell_environment_timeout");
        assert_eq!(coded.params["cause"], "earlier_capture");
        assert!(coded.message.contains("the shell took too long"));

        // Another failure is left as it was.
        let other = fallback.explain_not_found(anyhow!("something else"));
        assert!(other.downcast_ref::<CodedError>().is_none());
    }
}
