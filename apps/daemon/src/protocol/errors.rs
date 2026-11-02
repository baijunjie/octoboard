//! A failure as a client is told it: the params a coded message is filled with, [`CodedError`],
//! and the one mapping from any failure to the parts of an `error` reply.

use std::collections::BTreeMap;

use super::codes::{error_code, Code};
use super::events::Event;

/// The named values a coded message is filled with. A parameter that names a console, a project or
/// a session is that record's id under the param name `console`, `project` or `session`, so the
/// client can show the record's current name; anything else is passed as text, verbatim.
pub type Params = BTreeMap<String, String>;

pub(super) fn params(pairs: &[(&str, &str)]) -> Params {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

/// An error that carries a code and its params, so a client can word it in its own language and
/// branch on it without matching prose. Travels inside `anyhow::Error` and is recovered by
/// downcasting where the reply is built.
#[derive(Debug)]
pub struct CodedError {
    pub code: Code,
    pub message: String,
    pub params: Params,
}

impl CodedError {
    /// Returns the `anyhow::Error` rather than the bare value: every caller wants it in that
    /// shape, and the one place that reads it back downcasts out of exactly this.
    pub fn raised(code: Code, message: impl Into<String>, pairs: &[(&str, &str)]) -> anyhow::Error {
        anyhow::Error::new(Self {
            code,
            message: message.into(),
            params: params(pairs),
        })
    }

    /// The `error` reply to the request `id` that failed with this.
    pub fn reply(&self, id: Option<String>) -> Event {
        Event::Error {
            id,
            code: self.code,
            params: self.params.clone(),
            message: self.message.clone(),
        }
    }

    /// An error under `code` that nests the failure `err`: `own` pairs beside its `reason`,
    /// `reason_code` and its own params (see [`nested_params`]), and the message `message` words
    /// from that reason.
    pub fn nesting(
        code: Code,
        err: &anyhow::Error,
        own: &[(&str, &str)],
        message: impl FnOnce(&str) -> String,
    ) -> anyhow::Error {
        let (params, reason) = nested_params(err, own);
        anyhow::Error::new(Self {
            code,
            message: message(&reason),
            params,
        })
    }

    pub fn unknown_console(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_CONSOLE,
            format!("unknown console {id}"),
            &[("console", id)],
        )
    }

    pub fn unknown_project(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_PROJECT,
            format!("unknown project {id}"),
            &[("project", id)],
        )
    }

    pub fn unknown_shell_terminal(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_SHELL_TERMINAL,
            format!("unknown shell terminal {id}"),
            &[("shell_terminal", id)],
        )
    }

    pub fn unknown_session(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_SESSION,
            format!("unknown session {id}"),
            &[("session", id)],
        )
    }

    pub fn unknown_account(id: &str) -> anyhow::Error {
        Self::raised(
            error_code::UNKNOWN_ACCOUNT,
            format!("unknown account {id}"),
            &[("account", id)],
        )
    }
}

/// The `code`, `params` and `message` an `error` reply carries for `err`: a [`CodedError`]'s own,
/// or, for any other failure, [`error_code::INTERNAL_ERROR`] with the whole context chain as the
/// message and as the `detail` param.
pub fn error_parts(err: &anyhow::Error) -> (Code, Params, String) {
    match err.downcast_ref::<CodedError>() {
        Some(coded) => (coded.code, coded.params.clone(), coded.message.clone()),
        None => {
            let message = format!("{err:#}");
            (
                error_code::INTERNAL_ERROR,
                params(&[("detail", &message)]),
                message,
            )
        }
    }
}

/// The params of a message that nests the failure `err`, and that failure's message as the reason:
/// the failure's own params, then `own` and the failure as `reason` and `reason_code`, each
/// replacing a param of the same name, so a client words the cause itself.
pub fn nested_params(err: &anyhow::Error, own: &[(&str, &str)]) -> (Params, String) {
    let (reason_code, mut nested, reason) = error_parts(err);
    nested.extend(params(own));
    nested.insert("reason".to_string(), reason.clone());
    nested.insert("reason_code".to_string(), reason_code.as_str().to_string());
    (nested, reason)
}

impl std::fmt::Display for CodedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CodedError {}

#[cfg(test)]
mod tests {
    use crate::protocol::{notice_code, CodedError, Event, Notice};

    /// A client words a failure and a notice from their code and params; the English message rides
    /// along for one that does not know the code.
    #[test]
    fn an_error_and_a_notice_carry_a_code_and_named_params() {
        let error = CodedError::unknown_console("console-7");
        let coded = error.downcast_ref::<CodedError>().expect("coded");
        let event = serde_json::to_value(coded.reply(Some("request-1".to_string()))).unwrap();
        assert_eq!(event["code"], "unknown_console");
        assert_eq!(event["params"]["console"], "console-7");
        assert_eq!(event["message"], "unknown console console-7");

        let event =
            serde_json::to_value(Event::from_error(None, &anyhow::anyhow!("boom"))).unwrap();
        assert_eq!(event["code"], "internal_error");
        assert_eq!(event["params"]["detail"], "boom");
        assert_eq!(event["message"], "boom");

        let notice = serde_json::to_value(
            Notice::new(notice_code::LAUNCH_FAILED, "English", &[("reason", "why")])
                .about("session-1"),
        )
        .unwrap();
        assert_eq!(notice["type"], "session_notice");
        assert_eq!(notice["session"], "session-1");
        assert_eq!(notice["code"], "launch_failed");
        assert_eq!(notice["params"]["reason"], "why");
        assert_eq!(notice["message"], "English");
    }
}
