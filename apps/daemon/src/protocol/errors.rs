//! A failure as a client is told it: the params a coded message is filled with, [`CodedError`],
//! and the one mapping from any failure to the parts of an `error` reply.

use std::collections::BTreeMap;

use super::codes::{error_code, Code};
use super::events::Event;
use super::AgentId;

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
#[derive(Debug, PartialEq)]
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

    /// `err` as a client is told it ([`error_parts`]).
    pub fn of(err: &anyhow::Error) -> Self {
        let (code, params, message) = error_parts(err);
        Self {
            code,
            message,
            params,
        }
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

    /// The refusal of `operation` for a provider that does not offer it
    /// ([`error_code::PROVIDER_UNSUPPORTED`]).
    pub fn provider_unsupported(agent: AgentId, operation: ProviderOperation) -> anyhow::Error {
        Self::raised(
            error_code::PROVIDER_UNSUPPORTED,
            format!(
                "{} does not offer {}",
                agent.label(),
                operation.description()
            ),
            &[
                ("agent", agent.wire_name()),
                ("operation", operation.wire_name()),
            ],
        )
    }

    /// `offered`, or the refusal of `operation` when the provider offers nothing for it.
    pub fn require_offered<T>(
        agent: AgentId,
        operation: ProviderOperation,
        offered: Option<T>,
    ) -> anyhow::Result<T> {
        offered.ok_or_else(|| Self::provider_unsupported(agent, operation))
    }
}

/// What a provider may not offer, as [`error_code::PROVIDER_UNSUPPORTED`] names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderOperation {
    /// Signing an account in without a terminal.
    SignIn,
    /// Starting a usage window that has reset but not begun counting.
    Kick,
    /// Using a rate limit reset credit.
    ResetCredit,
    /// An account sharing the default account's configuration.
    ConfigSharing,
    /// Updating the agent's installed CLI.
    CliUpdate,
}

impl ProviderOperation {
    /// The value of the `operation` param.
    pub fn wire_name(self) -> &'static str {
        match self {
            ProviderOperation::SignIn => "sign_in",
            ProviderOperation::Kick => "kick",
            ProviderOperation::ResetCredit => "reset_credit",
            ProviderOperation::ConfigSharing => "config_sharing",
            ProviderOperation::CliUpdate => "cli_update",
        }
    }

    fn description(self) -> &'static str {
        match self {
            ProviderOperation::SignIn => "a sign-in flow",
            ProviderOperation::Kick => "a usage window to start",
            ProviderOperation::ResetCredit => "reset credits",
            ProviderOperation::ConfigSharing => "sharing the default account's configuration",
            ProviderOperation::CliUpdate => "an update of its CLI",
        }
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
    use crate::protocol::{notice_code, AgentId, CodedError, Event, Notice, ProviderOperation};

    /// A provider that does not offer an operation is refused with the agent's wire id and the
    /// operation's name, and one that does is let through.
    #[test]
    fn a_provider_without_an_operation_is_refused_with_its_id_and_the_operation() {
        for (operation, name) in [
            (ProviderOperation::SignIn, "sign_in"),
            (ProviderOperation::Kick, "kick"),
            (ProviderOperation::ResetCredit, "reset_credit"),
            (ProviderOperation::ConfigSharing, "config_sharing"),
        ] {
            let err =
                CodedError::require_offered::<()>(AgentId::GROK, operation, None).unwrap_err();
            let coded = err.downcast_ref::<CodedError>().expect("coded");
            assert_eq!(coded.code, "provider_unsupported");
            assert_eq!(coded.params["agent"], "grok");
            assert_eq!(coded.params["operation"], name);
            assert_eq!(coded.params.len(), 2);
            assert_eq!(
                CodedError::require_offered(AgentId::GROK, operation, Some(1)).unwrap(),
                1
            );
        }
    }

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
