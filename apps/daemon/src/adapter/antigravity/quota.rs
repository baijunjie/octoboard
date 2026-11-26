//! `agy -p /usage --output-format json` reports usage in one line without sending a turn.
//! Its weekly limits are per model group. Only the default account can be probed.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::accounts::quotas::probe::{
    ProbeLaunch, ProbeRun, QuotaProbe, Reading, ReplyProblem, WEEK_MINUTES,
};
use crate::protocol::{QuotaWindow, QuotaWindowKind};

const ARGS: &[&str] = &["-p", "/usage", "--output-format", "json"];
const DEFAULT_PROBE: &str = "antigravity/default";
const PROBE_HOME: &str = ".gemini";

pub(super) struct AntigravityQuota;

pub(super) fn own_home(run_homes: &Path) -> PathBuf {
    run_homes.join(DEFAULT_PROBE).join(PROBE_HOME)
}

impl QuotaProbe for AntigravityQuota {
    fn args(&self) -> &'static [&'static str] {
        ARGS
    }

    fn input(&self) -> Vec<String> {
        Vec::new()
    }

    fn launch(&self, run: &ProbeRun<'_>) -> Result<ProbeLaunch, String> {
        if run.account_dir.is_some() {
            return Err("Antigravity CLI has only its default account".to_owned());
        }
        let home = run
            .own_dir
            .ok_or("Antigravity CLI's quota home was not prepared")?;
        let flag = super::home::directory_argument(home);
        Ok(ProbeLaunch {
            args: std::iter::once(flag)
                .chain(self.args().iter().map(|arg| (*arg).to_owned()))
                .collect(),
            env: Vec::new(),
        })
    }

    fn is_reply(&self, value: &Value) -> bool {
        match value["status"].as_str() {
            Some("SUCCESS") => value["command"]["name"] == "usage",
            Some("ERROR") => value["error"].is_string(),
            _ => false,
        }
    }

    fn read(&self, reply: &Value, _: Option<&Value>) -> Result<Reading, ReplyProblem> {
        if reply["status"] == "ERROR" {
            // The CLI's error is free-form and can contain details of the authentication route.
            return Err(ReplyProblem::Error(
                "Antigravity CLI refused /usage".to_owned(),
            ));
        }
        let groups = reply["command"]["data"]["groups"]
            .as_array()
            .ok_or(ReplyProblem::Unreadable)?;
        let mut windows = Vec::new();
        for group in groups {
            let label = group["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .ok_or(ReplyProblem::Unreadable)?;
            let buckets = group["buckets"]
                .as_array()
                .ok_or(ReplyProblem::Unreadable)?;
            for bucket in buckets.iter().filter(|bucket| bucket["window"] == "weekly") {
                let remaining = bucket["remaining_fraction"]
                    .as_f64()
                    .filter(|n| (0.0..=1.0).contains(n))
                    .ok_or(ReplyProblem::Unreadable)?;
                let resets_at = bucket["reset_time"]
                    .as_str()
                    .and_then(crate::time::parse_rfc3339_millis);
                windows.push(QuotaWindow {
                    kind: QuotaWindowKind::WeeklyModel,
                    label: Some(label.to_owned()),
                    used_percent: 100.0 * (1.0 - remaining),
                    resets_at,
                    window_minutes: Some(WEEK_MINUTES),
                });
            }
        }
        if windows.is_empty() {
            return Err(ReplyProblem::NoWindows);
        }
        Ok(Reading {
            plan: None,
            windows,
            reset_credits: None,
        })
    }

    fn signed_out_reason(&self, _: Option<&Value>, _: Option<&Value>) -> String {
        "Antigravity CLI has no Google sign-in".to_owned()
    }

    #[cfg(test)]
    fn test_fixture(&self) -> crate::accounts::quotas::probe::ProbeFixture {
        crate::accounts::quotas::probe::ProbeFixture {
            request_marker: None,
            reply: include_str!("../../../testdata/antigravity_quota.jsonl"),
            unreadable_reply: r#"{"status":"SUCCESS","command":{"name":"usage","data":{}}}"#,
            error_line: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::quotas::probe::test_support::probe_of;
    use crate::protocol::AgentId;
    use crate::test_support::{fake_agent, ScratchDir};

    #[test]
    fn antigravity_quota_preserves_home_and_reads_the_two_named_weekly_groups() {
        let scratch = ScratchDir::new("antigravity-quota");
        let real_home = scratch.join("keychain-home");
        std::fs::create_dir(&real_home).unwrap();
        let fixture = AntigravityQuota.test_fixture();
        let capture = scratch.join("quota.jsonl");
        std::fs::write(&capture, fixture.reply).unwrap();
        let mut env = fake_agent(
            &scratch,
            AgentId::ANTIGRAVITY.descriptor().binary,
            &format!(
                r#"test "$HOME" -ef '{}' && test "$AGY_CLI_DISABLE_AUTO_UPDATE" = true || exit 2
                   flag=$(printf '%s' "$1" | tr '\\' /)
                   case "$flag" in
                       --gemini_dir=*/antigravity/default/.gemini) test -d "${{flag#*=}}" || exit 3 ;;
                       *) exit 4 ;;
                   esac
                   test "$2 $3 $4 $5" = "-p /usage --output-format json" || exit 5
                   cat '{}'"#,
                crate::platform::shell::posix_path(&real_home),
                crate::platform::shell::posix_path(&capture),
            ),
        );
        env.insert("HOME".to_owned(), real_home.to_string_lossy().into_owned());
        let reading = probe_of(AgentId::ANTIGRAVITY, &env).unwrap();
        let [gemini, other] = reading.windows.as_slice() else {
            panic!("expected two weekly windows");
        };
        assert_eq!(gemini.label.as_deref(), Some("Gemini Models"));
        assert_eq!(gemini.kind, QuotaWindowKind::WeeklyModel);
        assert!((gemini.remaining_percent() - 95.655_280_351_638_8).abs() < 0.00001);
        assert_eq!(
            gemini.resets_at,
            crate::time::parse_rfc3339_millis("2026-11-28T16:35:55Z")
        );
        assert_eq!(other.label.as_deref(), Some("Claude and GPT models"));
        assert_eq!(other.used_percent, 0.0);
        assert_eq!(other.window_minutes, Some(WEEK_MINUTES));
    }
}
