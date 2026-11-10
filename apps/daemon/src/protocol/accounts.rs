//! An account's runtime state as the daemon reports it: its login, its sharing and a change of it,
//! and what deleting its config directory would do. See "Account login" in
//! `apps/daemon/PROTOCOL.md`.

use serde::Serialize;

use super::agents::AgentId;
use super::codes::Code;
use super::errors::{params, Params};
use super::fields::is_zero;

/// What `delete_account` with `trash_directory` would do with an account's config directory, as
/// `inspect_account_trash` answers.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AccountTrashOutcome {
    /// The directory is there and may be moved to the Trash.
    Ok,
    /// Nothing is at the path: there is nothing to move, and the removal goes ahead without it.
    Missing,
    /// The home directory, the root of a disk or a directory holding Octoboard's data
    /// (`account_directory_protected`).
    Protected,
    /// Something else's directory is, holds or lies inside it (`account_directory_shared`): by
    /// `kind` another account (`name`, `agent`), an agent's default directory (`agent`) or a
    /// project's folder (`name`).
    Shared {
        kind: AccountTrashSharing,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        agent: Option<String>,
    },
    /// A path that decides it could not be looked at at once (`account_directory_trash_failed`):
    /// the directory, the home directory or Octoboard's data directory, with `cause` `unreadable`
    /// where that is known, or another account's, an agent's default or a project's path, with
    /// `cause` `other_unlooked`.
    Unlookable {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        #[cfg_attr(
            test,
            ts(type = "\"unreadable\" | \"other_unlooked\""),
            schemars(extend("enum" = ["unreadable", "other_unlooked"]))
        )]
        cause: Option<String>,
    },
    /// A look did not answer within the short time the inspection allows: nothing is said against
    /// the directory, and `delete_account` judges it with the full time.
    Undecided,
}

/// What `AccountTrashOutcome::Shared` shares the directory with.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountTrashSharing {
    Account,
    DefaultAccount,
    Project,
}

/// Where an account's login is.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginStatus {
    /// The CLI has started and has not yet said where to authorize.
    Starting,
    /// Waiting for the user to authorize in a browser: `authorization_url` and `device_code` say
    /// where and with what.
    Waiting,
    /// Signed in, confirmed by the agent itself.
    Succeeded,
    /// Ended without signing in; `failure` says why.
    Failed,
    /// Ended by `cancel_account_login`.
    Cancelled,
    /// Ended by the daemon after waiting too long for the authorization.
    TimedOut,
}

/// An account's login as the daemon last saw it. Runtime state: never stored.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountLogin {
    pub agent: AgentId,
    /// The account's id; `None` for the agent's default account.
    pub account: Option<String>,
    pub status: LoginStatus,
    /// The page the user authorizes on, once the CLI has given it. Octoboard opens it itself unless
    /// `opened_by_agent`.
    pub authorization_url: Option<String>,
    /// The code the user confirms on that page, for a device-code flow (Grok Build).
    pub device_code: Option<String>,
    /// The agent's CLI opens the page in the browser by itself and cannot be stopped from it (Grok
    /// Build), so a client must not open it again, only offer to reopen it.
    pub opened_by_agent: bool,
    /// Why a `failed` login failed, as a coded message of the same shape as an `error`; codes in
    /// `login_failure_code`.
    pub failure: Option<LoginFailure>,
}

/// An owned account's sharing state as the daemon holds it now. Runtime state: never stored.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountSharing {
    pub account: String,
    /// What refuses a change now; absent when nothing does.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub in_use: Option<AccountInUse>,
    /// How many of the account's sessions are idle — at their prompt, waiting for the user's next
    /// message, with nothing left running in the background. They do not refuse a change: it ends
    /// their processes first and resumes them once it has ended. Absent when none is.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub idle_sessions: usize,
    /// The change running now; absent when none is.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub progress: Option<SharingProgress>,
    /// The last change that failed since the daemon started, until the next one records its
    /// choice.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub failure: Option<SharingFailure>,
}

/// What runs under an account's directory that a change of its sharing must not move files under.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountInUse {
    /// The account's sessions working under its directory: in the middle of a turn, with
    /// background work running, waiting for the user's answer (a permission prompt, a question),
    /// holding a message of Octoboard's queued, being written or left unsent in its input box, or
    /// claimed — being started, resumed, switched or handed off, or having their owner changed.
    pub sessions: usize,
    /// Whether the account is being signed in.
    pub signing_in: bool,
}

/// How far a change of an account's sharing has got.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SharingProgress {
    /// The direction: turning sharing on, or off.
    pub share: bool,
    pub phase: SharingPhase,
    /// The entries processed so far, out of `total`: none before `changing`, all of them once
    /// `resuming`.
    pub done: usize,
    pub total: usize,
    /// The entry being processed: an item of the account's directory, or, turning sharing off,
    /// the agent's id of a conversation being taken back. Present only while `changing`, from
    /// the first entry on: absent while the change waits for short CLI reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub item: Option<String>,
}

/// What a change of an account's sharing is doing, in order: waiting, while another account of
/// the same agent changes, then ending the account's idle sessions, changing the entries, and
/// resuming those sessions. `waiting` and `stopping` are left out when there is nothing to wait
/// for or stop, and `resuming` when nothing was stopped.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SharingPhase {
    Waiting,
    Stopping,
    Changing,
    Resuming,
}

/// A change of an account's sharing that did not complete, in the form `account_sharing_failed`
/// gives it.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SharingFailure {
    /// The direction of the change.
    pub share: bool,
    /// Why, as a whole: the cause the entries share (`other` when they differ), or why the
    /// change could not start when there are none.
    pub cause: SharingCause,
    /// Each entry not brought in line, as the error's `items`.
    pub items: Vec<SharingItemFailure>,
    pub detail: String,
}

/// What an item was left as by an operation that could not finish it.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SharingLeft {
    /// As it was before the operation.
    Unchanged,
    /// Renamed to its backup, with nothing linked in its place.
    BackedUp,
    /// Some of its conversation history moved into the default directory, the rest still in the
    /// account's own directory.
    PartlyMoved,
    /// Still a link into the default directory.
    Linked,
    /// Not there at all.
    Absent,
}

/// Why something could not be brought in line, for a client to explain it in plain words; the
/// failure's `detail` keeps the system's own text.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SharingCause {
    /// Not allowed to change a file or directory, or on a read-only volume.
    PermissionDenied,
    /// A file is in use: held by another process, or a database locked by one.
    Busy,
    /// A file or directory is gone.
    NotFound,
    /// The volume or a quota is full, or a file too large for it.
    NoSpace,
    /// Something is in the way or not what it has to be: an entry already there, a link leading
    /// elsewhere, a directory where a file is expected or the other way round.
    Conflict,
    /// The agent's default directory is no longer the one the links lead into, or the one sharing
    /// was started into: turning sharing off, then on again, shares the new one.
    DefaultMoved,
    Other,
}

impl SharingCause {
    /// The one of `causes`, or `other` when they differ or there are none.
    pub fn shared(causes: impl IntoIterator<Item = Self>) -> Self {
        let mut causes = causes.into_iter();
        match causes.next() {
            Some(first) if causes.all(|cause| cause == first) => first,
            _ => Self::Other,
        }
    }

    /// The name the wire uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission_denied",
            Self::Busy => "busy",
            Self::NotFound => "not_found",
            Self::NoSpace => "no_space",
            Self::Conflict => "conflict",
            Self::DefaultMoved => "default_moved",
            Self::Other => "other",
        }
    }
}

/// An item an operation could not bring in line.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SharingItemFailure {
    /// An entry of the account's directory, or for a conversation the agent's own id of it.
    pub item: String,
    pub left: SharingLeft,
    pub cause: SharingCause,
    pub detail: String,
    /// The Octoboard sessions a conversation belongs to, for a failure about one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub sessions: Vec<String>,
}

/// Why a login failed.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoginFailure {
    pub code: Code,
    pub params: Params,
    pub message: String,
}

impl LoginFailure {
    pub fn new(code: Code, message: impl Into<String>, pairs: &[(&str, &str)]) -> Self {
        Self {
            code,
            params: params(pairs),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{
        AccountInUse, AccountLogin, AccountSharing, AccountTrashOutcome, AccountTrashSharing,
        AgentId, Event, LoginStatus, McpServerAuthOutcome, Request, RequestBody,
        ResetCreditOutcome, SharingCause, SharingFailure, SharingPhase, SharingProgress,
    };

    #[test]
    fn an_account_trash_inspection_travels_in_the_documented_shape() {
        let wire = |outcome| {
            serde_json::to_value(Event::AccountTrashInspected {
                id: Some("request-1".to_owned()),
                outcome,
            })
            .unwrap()
        };
        assert_eq!(
            wire(AccountTrashOutcome::Shared {
                kind: AccountTrashSharing::DefaultAccount,
                name: None,
                agent: Some("Codex".to_owned()),
            }),
            serde_json::json!({
                "type": "account_trash_inspected", "id": "request-1",
                "outcome": {"status": "shared", "kind": "default_account", "agent": "Codex"},
            })
        );
        assert_eq!(
            wire(AccountTrashOutcome::Shared {
                kind: AccountTrashSharing::Project,
                name: Some("Held".to_owned()),
                agent: None,
            })["outcome"],
            serde_json::json!({"status": "shared", "kind": "project", "name": "Held"})
        );
        assert_eq!(
            wire(AccountTrashOutcome::Unlookable {
                cause: Some("other_unlooked".to_owned())
            })["outcome"],
            serde_json::json!({"status": "unlookable", "cause": "other_unlooked"})
        );
        assert_eq!(
            wire(AccountTrashOutcome::Unlookable { cause: None })["outcome"],
            serde_json::json!({"status": "unlookable"})
        );
        for (outcome, status) in [
            (AccountTrashOutcome::Undecided, "undecided"),
            (AccountTrashOutcome::Ok, "ok"),
            (AccountTrashOutcome::Missing, "missing"),
            (AccountTrashOutcome::Protected, "protected"),
        ] {
            assert_eq!(
                wire(outcome)["outcome"],
                serde_json::json!({"status": status})
            );
        }
    }

    #[test]
    fn an_mcp_server_sign_in_is_asked_for_by_agent_account_and_server() {
        for name in [
            "sign_in_account_mcp_server",
            "sign_out_account_mcp_server",
            "cancel_account_mcp_sign_in",
        ] {
            let request: Request = serde_json::from_str(&format!(
                r#"{{"type":"{name}","id":"r","agent":"codex","account":null,"server":"s"}}"#
            ))
            .unwrap();
            assert!(matches!(
                request.body,
                RequestBody::SignInAccountMcpServer { account: None, .. }
                    | RequestBody::SignOutAccountMcpServer { account: None, .. }
                    | RequestBody::CancelAccountMcpSignIn { account: None, .. }
            ));
        }
        assert!(serde_json::from_str::<Request>(
            r#"{"type":"sign_in_account_mcp_server","id":"r","agent":"codex"}"#
        )
        .is_err());
        let event = Event::AccountMcpServerAuthEnded {
            id: Some("r".to_string()),
            outcome: McpServerAuthOutcome::failed("no".to_string()),
        };
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            serde_json::json!({"type": "account_mcp_server_auth_ended", "id": "r",
                "outcome": {"status": "failed", "detail": "no"}})
        );
    }

    #[test]
    fn an_account_login_is_asked_for_by_agent_and_account_and_travels_in_the_documented_shape() {
        let request: Request =
            serde_json::from_str(r#"{"type":"start_account_login","id":"r","agent":"grok"}"#)
                .unwrap();
        assert!(matches!(
            request.body,
            RequestBody::StartAccountLogin {
                agent: AgentId::GROK,
                account: None
            }
        ));
        let request: Request = serde_json::from_str(
            r#"{"type":"cancel_account_login","id":"r","agent":"codex","account":"a"}"#,
        )
        .unwrap();
        assert!(matches!(
            request.body,
            RequestBody::CancelAccountLogin {
                agent: AgentId::CODEX,
                account: Some(_)
            }
        ));
        assert!(
            serde_json::from_str::<Request>(r#"{"type":"use_reset_credit","id":"r"}"#).is_err()
        );
        let request: Request =
            serde_json::from_str(r#"{"type":"use_reset_credit","id":"r","agent":"codex"}"#)
                .unwrap();
        assert!(matches!(
            request.body,
            RequestBody::UseResetCredit {
                agent: AgentId::CODEX,
                account: None
            }
        ));
        let request: Request = serde_json::from_str(
            r#"{"type":"use_reset_credit","id":"r","agent":"codex","account":"a"}"#,
        )
        .unwrap();
        assert!(matches!(
            request.body,
            RequestBody::UseResetCredit {
                agent: AgentId::CODEX,
                account: Some(_)
            }
        ));
        let event = serde_json::to_value(Event::ResetCreditUsed {
            id: Some("r".to_string()),
            outcome: ResetCreditOutcome::NothingToReset,
        })
        .unwrap();
        assert_eq!(
            event,
            serde_json::json!({"type": "reset_credit_used", "id": "r",
                "outcome": "nothing_to_reset"})
        );
        let event = serde_json::to_value(Event::AccountLoginUpdated {
            login: AccountLogin {
                agent: AgentId::GROK,
                account: None,
                status: LoginStatus::Waiting,
                authorization_url: Some("https://accounts.x.ai/oauth2/device".to_string()),
                device_code: Some("ABCD-1234".to_string()),
                opened_by_agent: true,
                failure: None,
            },
        })
        .unwrap();
        assert_eq!(
            event,
            serde_json::json!({
                "type": "account_login_updated",
                "login": {
                    "agent": "grok", "account": null, "status": "waiting",
                    "authorization_url": "https://accounts.x.ai/oauth2/device",
                    "device_code": "ABCD-1234", "opened_by_agent": true, "failure": null,
                },
            })
        );
    }

    #[test]
    fn an_accounts_sharing_state_leaves_out_what_it_does_not_have() {
        let free = serde_json::to_value(Event::AccountSharing(AccountSharing {
            account: "a1".to_string(),
            in_use: None,
            idle_sessions: 0,
            progress: None,
            failure: None,
        }))
        .unwrap();
        assert_eq!(
            free,
            serde_json::json!({"type": "account_sharing", "account": "a1"})
        );
        let changing = serde_json::to_value(Event::AccountSharing(AccountSharing {
            account: "a1".to_string(),
            in_use: Some(AccountInUse {
                sessions: 2,
                signing_in: false,
            }),
            idle_sessions: 1,
            progress: Some(SharingProgress {
                share: true,
                phase: SharingPhase::Changing,
                done: 3,
                total: 31,
                item: Some("skills".to_string()),
            }),
            failure: Some(SharingFailure {
                share: false,
                cause: SharingCause::PermissionDenied,
                items: Vec::new(),
                detail: "denied".to_string(),
            }),
        }))
        .unwrap();
        assert_eq!(
            changing,
            serde_json::json!({
                "type": "account_sharing", "account": "a1",
                "in_use": {"sessions": 2, "signing_in": false},
                "idle_sessions": 1,
                "progress": {
                    "share": true, "phase": "changing", "done": 3, "total": 31, "item": "skills",
                },
                "failure": {
                    "share": false, "cause": "permission_denied", "items": [], "detail": "denied",
                },
            })
        );
    }
}
