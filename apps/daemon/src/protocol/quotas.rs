//! An account's plan quota and, for a provider that offers them (`Capabilities::reset_credits`,
//! Codex's), its rate limit reset credits. See "Account quotas" in `apps/daemon/PROTOCOL.md`.

use serde::Serialize;

use super::agents::AgentId;
use super::codes::Code;
use super::errors::{params, Params};

pub use crate::providers::QuotaWindowKind;

/// One window of an account's plan quota.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuotaWindow {
    pub kind: QuotaWindowKind,
    /// The model's display name for `weekly_model`; absent for the other kinds.
    pub label: Option<String>,
    /// How much of the window is used, normally 0 to 100; a value above 100 is passed on as the
    /// agent gives it.
    pub used_percent: f64,
    /// When the window resets, in milliseconds since the epoch, if the agent says.
    pub resets_at: Option<i64>,
    /// The window's length, if known.
    pub window_minutes: Option<u32>,
}

impl QuotaWindow {
    /// How much of the window is left, 0 to 100: what `used_percent` leaves, never below 0.
    pub fn remaining_percent(&self) -> f64 {
        (100.0 - self.used_percent).max(0.0)
    }
}

/// One of an account's rate limit reset credits that can still be used, as its provider describes
/// it.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResetCredit {
    pub id: String,
    /// The backend's name for the credit ("Full reset (Weekly + 5 hr)"), if it gives one.
    pub title: Option<String>,
    /// The backend's description of the credit, if it gives one.
    pub description: Option<String>,
    /// When the credit was granted, in milliseconds since the epoch, if the provider says.
    pub granted_at: Option<i64>,
    /// When the credit expires, in milliseconds since the epoch; null for one that never does.
    pub expires_at: Option<i64>,
}

/// The rate limit reset credits of an account that can be used now. Only present while there is at
/// least one.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResetCredits {
    /// How many credits can be used, whatever the list below holds.
    pub available_count: u32,
    /// The usable credits in the order they would be used: the one that expires first, a credit
    /// with no expiry last. Null when the provider gave only the count, and may be shorter than
    /// `available_count` when the provider caps its list.
    pub credits: Option<Vec<ResetCredit>>,
}

/// What the provider did with a request to use a reset credit (`reset_credit_used`).
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetCreditOutcome {
    /// The credit was used and the windows that could be reset were.
    Reset,
    /// No window of the account is eligible for a reset now. Whether the credit is kept was not
    /// measured.
    NothingToReset,
    /// The account has no credit left to use.
    NoCredit,
    /// The request's idempotency key had already succeeded.
    AlreadyRedeemed,
}

/// Why an account's quota has no windows, as a coded message of the same shape as an `error`. It
/// is part of the reading rather than a failure of anything else.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuotaUnavailable {
    pub code: Code,
    pub params: Params,
    pub message: String,
}

impl QuotaUnavailable {
    pub fn new(code: Code, message: impl Into<String>, pairs: &[(&str, &str)]) -> Self {
        Self {
            code,
            params: params(pairs),
            message: message.into(),
        }
    }
}

/// The plan quota of one account as the agent's own CLI last reported it. Runtime state of the
/// daemon: never stored and never part of an exported configuration.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountQuota {
    pub agent: AgentId,
    /// The account's id; `None` for the agent's default account.
    pub account: Option<String>,
    /// When the reading was taken, milliseconds since the epoch.
    pub fetched_at: i64,
    /// The plan as the agent names it (`max`, `plus`, `SuperGrok`), if it says.
    pub plan: Option<String>,
    /// The email the account is signed in with, if the agent's sign-in record gives one. Read
    /// whether or not the quota could be; `None` for an account that is not signed in or whose
    /// configuration directory is missing.
    pub email: Option<String>,
    /// Empty when the account has never been read. After a failed re-read, the last good windows
    /// stay (with their `fetched_at`) and `unavailable` says why the newer attempt failed.
    pub windows: Vec<QuotaWindow>,
    pub unavailable: Option<QuotaUnavailable>,
    /// The account's usable rate limit reset credits (Codex only); absent when it has none or the
    /// agent says nothing of them. Kept, like the windows, over a failed re-read.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub reset_credits: Option<ResetCredits>,
}

#[cfg(test)]
mod tests {
    use crate::protocol::{
        quota_unavailable_code, AccountQuota, AgentId, Event, QuotaUnavailable, QuotaWindow,
        QuotaWindowKind, Settings,
    };

    #[test]
    fn account_quotas_travel_in_the_snapshot_and_in_their_own_event_in_the_documented_shape() {
        let quota = |kind, label: Option<&str>| QuotaWindow {
            kind,
            label: label.map(str::to_string),
            used_percent: 27.5,
            resets_at: Some(1_792_258_799_899),
            window_minutes: Some(300),
        };
        let quotas = vec![AccountQuota {
            reset_credits: None,
            agent: AgentId::CLAUDE,
            account: None,
            fetched_at: 7,
            plan: Some("max".to_string()),
            email: Some("me@example.com".to_string()),
            windows: vec![
                quota(QuotaWindowKind::Session, None),
                quota(QuotaWindowKind::Weekly, None),
                quota(QuotaWindowKind::WeeklyModel, Some("Fable")),
                quota(QuotaWindowKind::CreditPeriod, None),
            ],
            unavailable: Some(QuotaUnavailable::new(
                quota_unavailable_code::TIMED_OUT,
                "late",
                &[("agent", "Claude Code")],
            )),
        }];
        let event = serde_json::to_value(Event::AccountQuotasUpdated {
            account_quotas: quotas.clone(),
        })
        .unwrap();
        assert_eq!(event["type"], "account_quotas_updated");
        assert_eq!(
            event["account_quotas"][0],
            serde_json::json!({
                "agent": "claude",
                "account": null,
                "fetched_at": 7,
                "plan": "max",
                "email": "me@example.com",
                "windows": [
                    {"kind": "session", "label": null, "used_percent": 27.5,
                     "resets_at": 1_792_258_799_899_i64, "window_minutes": 300},
                    {"kind": "weekly", "label": null, "used_percent": 27.5,
                     "resets_at": 1_792_258_799_899_i64, "window_minutes": 300},
                    {"kind": "weekly_model", "label": "Fable", "used_percent": 27.5,
                     "resets_at": 1_792_258_799_899_i64, "window_minutes": 300},
                    {"kind": "credit_period", "label": null, "used_percent": 27.5,
                     "resets_at": 1_792_258_799_899_i64, "window_minutes": 300},
                ],
                "unavailable": {"code": "quota_timed_out", "params": {"agent": "Claude Code"},
                                "message": "late"},
            })
        );
        let snapshot = serde_json::to_value(Event::Snapshot {
            hosts: vec![],
            consoles: vec![],
            projects: vec![],
            sessions: vec![],
            trusted_directories: vec![],
            settings: Settings::default(),
            providers: Vec::new(),
            git_statuses: vec![],
            agent_availability: vec![],
            account_quotas: quotas,
            account_logins: vec![],
            account_sharing: vec![],
            agent_cli_statuses: vec![],
            daemon_update: None,
            home_dir: None,
            host: crate::platform::HostFacts::current().into(),
            keep_awake_held: false,
            shell_terminals: vec![],
        })
        .unwrap();
        assert_eq!(snapshot["account_quotas"][0]["plan"], "max");
    }
}
