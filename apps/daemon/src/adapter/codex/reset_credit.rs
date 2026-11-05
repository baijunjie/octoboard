//! Using one of a Codex account's rate limit reset credits: `account/rateLimitResetCredit/consume`
//! on `codex app-server`, measured in `docs/agent-cli-reference.md` ("Plan quota"). It is run the
//! way a probe is (same arguments, login-shell snapshot, the account's directory as `CODEX_HOME`,
//! bounded the same way), as a short conversation of `initialize`, `initialized` and the consume
//! request.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::{json, Value};

use super::app_server::{self, CODEX_APP_SERVER_ARGS};
use crate::accounts::quotas::probe::OUTPUT_LIMIT;
use crate::accounts::quotas::reset_credit::{Failure, ResetCredits};
use crate::adapter::account_cli::{self, NotRun};
use crate::agents::json_rpc;
use crate::processes::subprocess::{run_exchange, Bounds, ExchangeFailure};
use crate::protocol::{AgentId, ResetCreditOutcome};

pub(super) struct CodexResetCredits;

impl ResetCredits for CodexResetCredits {
    fn consume(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        credit_id: Option<&str>,
        idempotency_key: &str,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<ResetCreditOutcome, Failure> {
        consume(
            shell_env,
            config_dir,
            credit_id,
            idempotency_key,
            timeout,
            cancel,
        )
    }
}

/// Asks Codex to use the credit `credit_id`, or the next one available when it is `None`, under
/// `idempotency_key`. Blocking; setting `cancel` stops it, and it is refused before anything is run
/// when it is set already.
fn consume(
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    credit_id: Option<&str>,
    idempotency_key: &str,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<ResetCreditOutcome, Failure> {
    let agent = AgentId::CODEX;
    if cancel.load(Ordering::Relaxed) {
        return Err(Failure::Failed(
            "the use was cancelled before it started".to_string(),
        ));
    }
    let mut run =
        account_cli::admitted_command(agent, shell_env, config_dir, CODEX_APP_SERVER_ARGS)
            .map_err(|refused| match refused {
                NotRun::BinaryNotFound(binary) => Failure::BinaryNotFound(binary.to_string()),
                NotRun::BinaryRefused(err) => Failure::BinaryRefused(err),
                NotRun::DirMissing(_) => Failure::Failed(refused.detail()),
            })?;
    let mut params = json!({"idempotencyKey": idempotency_key});
    if let Some(credit_id) = credit_id {
        params["creditId"] = json!(credit_id);
    }
    let input = [
        app_server::initialize(),
        app_server::initialized(),
        json!({"jsonrpc": "2.0", "id": 2, "method": "account/rateLimitResetCredit/consume",
            "params": params}),
    ]
    .iter()
    .map(Value::to_string)
    .collect::<Vec<_>>();
    let bounds = Bounds::agent_query(timeout, OUTPUT_LIMIT);
    let reply: RefCell<Option<Value>> = RefCell::new(None);
    run_exchange(&mut run.command, &input, &bounds, cancel, |line| {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        if !json_rpc::is_reply_to(&value, 2) {
            return false;
        }
        *reply.borrow_mut() = Some(value);
        true
    })
    .map_err(|failure| match failure {
        ExchangeFailure::TimedOut(_) => Failure::TimedOut,
        other => Failure::Failed(other.to_string()),
    })?;
    let reply = reply.take().ok_or_else(unreadable)?;
    if let Some(detail) = json_rpc::error_detail(&reply) {
        return Err(
            if detail.to_lowercase().contains("authentication required") {
                Failure::NotSignedIn
            } else {
                Failure::Failed(detail)
            },
        );
    }
    match reply.pointer("/result/outcome").and_then(Value::as_str) {
        Some("reset") => Ok(ResetCreditOutcome::Reset),
        Some("nothingToReset") => Ok(ResetCreditOutcome::NothingToReset),
        Some("noCredit") => Ok(ResetCreditOutcome::NoCredit),
        Some("alreadyRedeemed") => Ok(ResetCreditOutcome::AlreadyRedeemed),
        _ => Err(unreadable()),
    }
}

fn unreadable() -> Failure {
    Failure::Failed(format!(
        "{} answered with a reply that could not be read",
        AgentId::CODEX.descriptor().label
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fake_agent, ScratchDir};

    const SHORT: Duration = Duration::from_millis(700);

    /// A fake Codex in `dir` that records its arguments, the directory variable and each line it
    /// is given, and answers the consume request with `reply` after a line that is not the reply.
    fn answering_codex(dir: &Path, reply: &str) -> HashMap<String, String> {
        std::fs::write(dir.join("reply"), format!("{reply}\n")).expect("reply");
        let var = crate::providers::codex::LAYOUT.config_dir_var;
        fake_agent(
            dir,
            AgentId::CODEX.descriptor().binary,
            &format!(
                "echo \"$*\" > '{d}/args'; echo \"${var}\" > '{d}/dir'\n\
                 while read line; do echo \"$line\" >> '{d}/input'\n\
                 case \"$line\" in *'rateLimitResetCredit/consume'*) echo 'not the reply'; \
                 cat '{d}/reply';; esac\n\
                 done",
                d = dir.display(),
            ),
        )
    }

    fn consume_in(
        dir: &Path,
        env: &HashMap<String, String>,
        credit: Option<&str>,
    ) -> Result<ResetCreditOutcome, Failure> {
        let account = dir.join("account");
        std::fs::create_dir_all(&account).unwrap();
        consume(
            env,
            Some(&account),
            credit,
            "key-1",
            crate::test_support::PATIENCE,
            &AtomicBool::new(false),
        )
    }

    #[test]
    fn the_chosen_credit_and_the_key_are_sent_to_the_app_server_in_the_accounts_directory() {
        let dir = ScratchDir::new("consume-sends");
        let env = answering_codex(&dir, r#"{"id":2,"result":{"outcome":"reset"}}"#);
        assert_eq!(
            consume_in(&dir, &env, Some("credit-a")),
            Ok(ResetCreditOutcome::Reset)
        );
        let args = std::fs::read_to_string(dir.join("args")).unwrap();
        assert!(args.starts_with("app-server "), "{args}");
        assert_eq!(
            std::fs::read_to_string(dir.join("dir")).unwrap().trim(),
            dir.join("account").display().to_string()
        );
        let lines: Vec<Value> = std::fs::read_to_string(dir.join("input"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines[0]["method"], "initialize");
        assert_eq!(lines[1]["method"], "initialized");
        assert_eq!(lines[2]["method"], "account/rateLimitResetCredit/consume");
        assert_eq!(
            lines[2]["params"],
            json!({"creditId": "credit-a", "idempotencyKey": "key-1"})
        );
    }

    #[test]
    fn no_credit_is_named_when_none_is_chosen() {
        let dir = ScratchDir::new("consume-unnamed");
        let env = answering_codex(&dir, r#"{"id":2,"result":{"outcome":"reset"}}"#);
        consume_in(&dir, &env, None).unwrap();
        let input = std::fs::read_to_string(dir.join("input")).unwrap();
        let consume: Value = serde_json::from_str(input.lines().nth(2).unwrap()).unwrap();
        assert_eq!(consume["params"], json!({"idempotencyKey": "key-1"}));
    }

    #[test]
    fn each_outcome_codex_names_is_reported() {
        for (wire, outcome) in [
            ("reset", ResetCreditOutcome::Reset),
            ("nothingToReset", ResetCreditOutcome::NothingToReset),
            ("noCredit", ResetCreditOutcome::NoCredit),
            ("alreadyRedeemed", ResetCreditOutcome::AlreadyRedeemed),
        ] {
            let dir = ScratchDir::new("consume-outcome");
            let env = answering_codex(
                &dir,
                &format!(r#"{{"id":2,"result":{{"outcome":"{wire}"}}}}"#),
            );
            assert_eq!(consume_in(&dir, &env, None), Ok(outcome), "{wire}");
        }
    }

    #[test]
    fn a_refusal_for_want_of_a_sign_in_is_told_from_other_errors() {
        let dir = ScratchDir::new("consume-signed-out");
        let env = answering_codex(
            &dir,
            r#"{"error":{"code":-32600,"message":"codex account authentication required for rate limit reset credits"},"id":2}"#,
        );
        assert_eq!(consume_in(&dir, &env, None), Err(Failure::NotSignedIn));

        let dir = ScratchDir::new("consume-refused");
        let env = answering_codex(
            &dir,
            r#"{"error":{"code":-32603,"message":"backend unavailable"},"id":2}"#,
        );
        assert_eq!(
            consume_in(&dir, &env, None),
            Err(Failure::Failed("backend unavailable".to_string()))
        );
    }

    #[test]
    fn a_reply_that_names_no_known_outcome_is_unreadable() {
        for reply in [
            r#"{"id":2,"result":{"outcome":"somethingNew"}}"#,
            r#"{"id":2,"result":{}}"#,
        ] {
            let dir = ScratchDir::new("consume-unreadable");
            let env = answering_codex(&dir, reply);
            assert_eq!(consume_in(&dir, &env, None), Err(unreadable()), "{reply}");
        }
    }

    #[test]
    fn a_codex_that_never_answers_times_out() {
        let dir = ScratchDir::new("consume-timeout");
        let env = fake_agent(
            &dir,
            AgentId::CODEX.descriptor().binary,
            "while read line; do :; done",
        );
        let account = dir.join("account");
        std::fs::create_dir(&account).unwrap();
        let result = consume(
            &env,
            Some(&account),
            None,
            "key-1",
            SHORT,
            &AtomicBool::new(false),
        );
        assert_eq!(result, Err(Failure::TimedOut));
    }

    #[test]
    fn a_use_cancelled_beforehand_runs_nothing() {
        let dir = ScratchDir::new("consume-cancelled");
        let ran = dir.join("ran");
        let env = fake_agent(
            &dir,
            AgentId::CODEX.descriptor().binary,
            &format!("touch '{}'", ran.display()),
        );
        let account = dir.join("account");
        std::fs::create_dir(&account).unwrap();
        let result = consume(
            &env,
            Some(&account),
            None,
            "k",
            SHORT,
            &AtomicBool::new(true),
        );
        assert!(matches!(result, Err(Failure::Failed(_))));
        assert!(!ran.exists());
    }

    #[test]
    fn a_missing_binary_and_a_missing_directory_run_nothing() {
        let dir = ScratchDir::new("consume-missing");
        let account = dir.join("account");
        let no_binary = HashMap::from([("PATH".to_string(), dir.display().to_string())]);
        std::fs::create_dir(&account).unwrap();
        assert!(matches!(
            consume(
                &no_binary,
                Some(&account),
                None,
                "k",
                SHORT,
                &AtomicBool::new(false)
            ),
            Err(Failure::BinaryNotFound(_))
        ));
        let ran = dir.join("ran");
        let env = fake_agent(
            &dir,
            AgentId::CODEX.descriptor().binary,
            &format!("touch '{}'", ran.display()),
        );
        let gone = dir.join("gone");
        let result = consume(&env, Some(&gone), None, "k", SHORT, &AtomicBool::new(false));
        assert!(matches!(result, Err(Failure::Failed(ref d)) if d.contains("does not exist")));
        assert!(!ran.exists());
    }
}
