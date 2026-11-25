//! Copilot Free's monthly AI-credit budget from the CLI's headless SDK server.

use serde_json::{json, Value};

use super::{accounts, models};
use crate::accounts::quotas::probe::{ProbeLaunch, ProbeRun, QuotaProbe, Reading, ReplyProblem};
use crate::agents::json_rpc;
use crate::processes::subprocess::Framing;
use crate::protocol::{QuotaWindow, QuotaWindowKind};

const STATUS: i64 = 1;
const CREATE: i64 = 2;
const REFRESH: i64 = 3;

pub(super) static QUOTA: CopilotQuota = CopilotQuota { run: None };

pub(super) struct CopilotQuota {
    run: Option<Run>,
}

struct Run {
    home: std::path::PathBuf,
    login: Option<String>,
    session: uuid::Uuid,
}

impl QuotaProbe for CopilotQuota {
    fn args(&self) -> &'static [&'static str] {
        models::SERVER_ARGS
    }

    fn prepare(&self, context: &ProbeRun<'_>) -> Result<Option<Box<dyn QuotaProbe>>, String> {
        let home = context
            .own_dir
            .ok_or("Copilot's quota home was not prepared")?;
        let state = accounts::state_of(home).ok_or("Copilot's quota home could not be read")?;
        Ok(Some(Box::new(Self {
            run: Some(Run {
                home: home.to_owned(),
                login: accounts::named_login(&state),
                session: uuid::Uuid::new_v4(),
            }),
        })))
    }

    fn launch(&self, _: &ProbeRun<'_>) -> Result<ProbeLaunch, String> {
        let run = self
            .run
            .as_ref()
            .ok_or("Copilot's quota directory was not prepared")?;
        let mut launch = ProbeLaunch::as_account(self.args());
        launch.args.extend([
            "--log-dir".to_owned(),
            run.home.join("logs").to_string_lossy().into_owned(),
        ]);
        Ok(launch)
    }

    fn input(&self) -> Vec<String> {
        vec![request(STATUS, "auth.getStatus", json!({}))]
    }

    fn framing(&self) -> Framing {
        Framing::ContentLength
    }

    fn steps(&self) -> Vec<Vec<String>> {
        let mut steps = vec![self.input()];
        if let Some(run) = &self.run {
            steps.extend([
                vec![request(
                    CREATE,
                    "session.create",
                    json!({
                        "sessionId": run.session,
                        "workingDirectory": run.home,
                    }),
                )],
                vec![request(
                    REFRESH,
                    "session.quota.refresh",
                    json!({"sessionId": run.session}),
                )],
            ]);
        }
        steps
    }

    fn advances(&self, value: &Value) -> bool {
        (json_rpc::is_reply_to(value, STATUS) && self.authorization(value).is_ok())
            || (json_rpc::is_reply_to(value, CREATE) && created(value))
    }

    fn is_reply(&self, value: &Value) -> bool {
        // A wrong login is a terminal reply. Sending any later request could read another
        // account's quota through Copilot's silent GitHub CLI fallback.
        json_rpc::is_reply_to(value, REFRESH)
            || (json_rpc::is_reply_to(value, STATUS) && self.authorization(value).is_err())
            || (json_rpc::is_reply_to(value, CREATE) && !created(value))
    }

    fn read(&self, reply: &Value, _: Option<&Value>) -> Result<Reading, ReplyProblem> {
        if json_rpc::is_reply_to(reply, STATUS) {
            self.authorization(reply)?;
            return Err(ReplyProblem::Unreadable);
        }
        if json_rpc::is_reply_to(reply, CREATE) {
            return Err(ReplyProblem::Error(
                "Copilot could not open its quota session".into(),
            ));
        }
        if json_rpc::error_code(reply).is_some() {
            return Err(ReplyProblem::Error(
                "Copilot could not refresh its plan quota".into(),
            ));
        }
        monthly_credits(reply.get("result").ok_or(ReplyProblem::Unreadable)?)
    }

    fn signed_out_reason(&self, _: Option<&Value>, _: Option<&Value>) -> String {
        "Copilot is not signed in as the account for this quota reading".to_owned()
    }

    #[cfg(test)]
    fn test_fixture(&self) -> crate::accounts::quotas::probe::ProbeFixture {
        crate::accounts::quotas::probe::ProbeFixture {
            request_marker: Some("session.quota.refresh"),
            reply: include_str!("../../../testdata/copilot_quota_free_1.0.114.json").trim_end(),
            unreadable_reply: r#"{"jsonrpc":"2.0","id":3,"result":{}}"#,
            error_line: None,
        }
    }

    #[cfg(test)]
    fn test_start_replies(&self) -> &'static [(&'static str, &'static str)] {
        &[
            (
                "auth.getStatus",
                r#"{"jsonrpc":"2.0","id":1,"result":{"isAuthenticated":true,"authType":"user","login":"octocat"}}"#,
            ),
            (
                "session.create",
                r#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"a conversation"}}"#,
            ),
        ]
    }
}

impl CopilotQuota {
    fn authorization(&self, value: &Value) -> Result<(), ReplyProblem> {
        match value
            .pointer("/result/isAuthenticated")
            .and_then(Value::as_bool)
        {
            Some(true) => {}
            Some(false) => return Err(ReplyProblem::NotSignedIn),
            None => return Err(ReplyProblem::Unreadable),
        }
        let login = value.pointer("/result/login").and_then(Value::as_str);
        if self
            .run
            .as_ref()
            .and_then(|run| run.login.as_ref())
            .is_some_and(|expected| {
                !login.is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
            })
        {
            return Err(ReplyProblem::NotSignedIn);
        }
        Ok(())
    }
}

fn request(id: i64, method: &str, params: Value) -> String {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string()
}

fn created(value: &Value) -> bool {
    value
        .pointer("/result/sessionId")
        .and_then(Value::as_str)
        .is_some()
}

fn monthly_credits(result: &Value) -> Result<Reading, ReplyProblem> {
    let free = result["isFreeUser"]
        .as_bool()
        .ok_or(ReplyProblem::Unreadable)?;
    // The hosted paid plans were not available to measure.
    if !free {
        return Err(ReplyProblem::NoWindows);
    }
    let quota = result["providerQuotas"]
        .as_array()
        .ok_or(ReplyProblem::Unreadable)?
        .iter()
        .find(|quota| quota["quotaId"] == "chat" && quota["unit"] == "ai_credits")
        .ok_or(ReplyProblem::NoWindows)?;
    let budget = &quota["budgetMetadata"];
    let remaining = budget["remainingPercentage"]
        .as_f64()
        .ok_or(ReplyProblem::Unreadable)?;
    // The SDK serializes even integer millisecond epochs as JSON numbers with a decimal point.
    let reset = budget["resetAtEpochMs"].as_f64().and_then(|value| {
        let epoch = value as i64;
        jiff::Timestamp::from_millisecond(epoch).ok()
    });
    let window_minutes = reset.and_then(|reset| {
        let preceding =
            jiff::Timestamp::from_millisecond(reset.as_millisecond().checked_sub(1)?).ok()?;
        let days = u32::try_from(
            preceding
                .to_zoned(jiff::tz::TimeZone::UTC)
                .date()
                .days_in_month(),
        )
        .ok()?;
        Some(days * 24 * 60)
    });
    Ok(Reading {
        plan: result["planTier"].as_str().map(str::to_owned),
        windows: vec![QuotaWindow {
            kind: QuotaWindowKind::CreditPeriod,
            label: None,
            used_percent: 100.0 - remaining,
            resets_at: reset.map(|reset| reset.as_millisecond()),
            window_minutes,
        }],
        reset_credits: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::quotas::diagnostics::ProbeTrace;
    use crate::accounts::quotas::probe::test_support::{answering_agent, run_probe};
    use crate::protocol::{quota_unavailable_code, AgentId};
    use crate::test_support::{ScratchDir, PATIENCE};

    #[test]
    fn a_quota_session_is_created_only_for_the_named_login() {
        let scratch = ScratchDir::new("copilot-quota");
        let account = scratch.join("account");
        std::fs::create_dir(&account).unwrap();
        let state = account.join("config.json");
        let config = |login| {
            json!({"lastLoggedInUser":{"host":"https://github.com","login":login}}).to_string()
        };
        let original = config("octocat");
        std::fs::write(&state, &original).unwrap();
        let fixture = QUOTA.test_fixture();
        let env = answering_agent(&scratch, AgentId::COPILOT, fixture.reply);
        let read = || {
            run_probe(
                AgentId::COPILOT,
                &env,
                Some(&account),
                PATIENCE,
                &mut ProbeTrace::default(),
            )
        };
        let reading = read().unwrap();
        let [window] = reading.windows.as_slice() else {
            panic!("one window");
        };
        assert_eq!(reading.plan.as_deref(), Some("free"));
        assert_eq!(window.kind, QuotaWindowKind::CreditPeriod);
        assert_eq!(window.resets_at, Some(1796083200000));
        assert_eq!(window.window_minutes, Some(43200));
        assert_eq!(std::fs::read_to_string(&state).unwrap(), original);
        let actual = std::fs::read_to_string(scratch.join("input")).unwrap();
        assert!(actual.contains("session.create"));
        assert!(actual.contains("session.quota.refresh"));

        std::fs::write(&state, config("another-login")).unwrap();
        std::fs::write(scratch.join("input"), "").unwrap();
        let unavailable = read().unwrap_err();
        assert_eq!(unavailable.code, quota_unavailable_code::NOT_SIGNED_IN);
        let actual = std::fs::read_to_string(scratch.join("input")).unwrap();
        assert!(actual.contains("auth.getStatus"));
        assert!(!actual.contains("session.create"));
    }
}
