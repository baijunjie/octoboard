use super::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::accounts::quotas::tests::{key, quota_of};
use crate::protocol::{error_code, Agent, CodedError, Event, ResetCreditOutcome};
use crate::state::AppState;
use crate::test_support::code_of;

/// The held reading of Codex's default account, listing a credit that expires first.
fn hold_credits(state: &Arc<AppState>) {
    hold_credit_list(
        state,
        1,
        Some(&[("credit-soon", Some(i64::MAX - 1))]),
        false,
    );
}

/// The held reading of Codex's default account with `available_count` credits, of which
/// `credits` (id, expiry) are listed; `unavailable` marks the reading as a failed attempt.
fn hold_credit_list(
    state: &Arc<AppState>,
    available_count: u32,
    credits: Option<&[(&str, Option<i64>)]>,
    unavailable: bool,
) {
    let mut quota = quota_of(&key(Agent::Codex, None), unavailable);
    quota.reset_credits = Some(crate::protocol::ResetCredits {
        available_count,
        credits: credits.map(|list| {
            list.iter()
                .map(|(id, expires_at)| crate::protocol::ResetCredit {
                    id: (*id).to_string(),
                    title: None,
                    description: None,
                    granted_at: Some(1),
                    expires_at: *expires_at,
                })
                .collect()
        }),
    });
    state.account_quotas.hold_for_test(&quota);
}

/// A shell environment whose Codex records each line it is given in `input` and answers the
/// consume request with `reply`.
fn consuming_codex(dir: &Path, reply: &str) -> ShellSource {
    std::fs::write(dir.join("reply"), format!("{reply}\n")).unwrap();
    let env = crate::test_support::fake_agent(
        dir,
        crate::adapter::adapter_for(Agent::Codex).binary(),
        &format!(
            "while read line; do echo \"$line\" >> '{d}/input'\n\
             case \"$line\" in *'consume'*) cat '{d}/reply';; esac\ndone",
            d = dir.display()
        ),
    );
    Arc::new(move || Ok(env.clone()))
}

/// The params of the consume request a fake from [`consuming_codex`] was given.
fn consume_params(dir: &Path) -> serde_json::Value {
    let input = std::fs::read_to_string(dir.join("input")).unwrap();
    let consume: serde_json::Value = serde_json::from_str(input.lines().nth(2).unwrap()).unwrap();
    consume["params"].clone()
}

#[tokio::test]
async fn using_a_reset_credit_names_the_first_held_one_and_reads_the_account_again() {
    let (state, dir) = crate::test_support::app_state("quota-use-reset-credit");
    hold_credits(&state);
    let shell = consuming_codex(&dir, r#"{"id":2,"result":{"outcome":"reset"}}"#);
    let outcome = use_reset_credit(&state, None, shell, crate::test_support::PATIENCE)
        .await
        .unwrap();
    assert_eq!(outcome, ResetCreditOutcome::Reset);
    let params = consume_params(&dir);
    assert_eq!(params["creditId"], "credit-soon");
    assert!(params["idempotencyKey"].as_str().is_some());
    assert!(state.account_quotas.is_due_for_test(Agent::Codex, None));
    assert!(state.account_quotas.lock().in_flight.is_empty());
}

#[test]
fn a_credit_that_worked_leaves_the_held_reading_and_the_others_stay() {
    let (state, _dir) = crate::test_support::app_state("quota-spend-held-credit");
    let key = key(Agent::Codex, None);
    let far = Some(i64::MAX - 1);
    hold_credit_list(&state, 2, Some(&[("first", far), ("second", far)]), false);
    state
        .account_quotas
        .spend_held_credit(&state, &key, Some("first"));
    let held = state.account_quotas.lock().entries[&key]
        .quota
        .reset_credits
        .clone()
        .unwrap();
    assert_eq!(held.available_count, 1);
    let ids: Vec<_> = held.credits.unwrap().into_iter().map(|c| c.id).collect();
    assert_eq!(ids, ["second"]);

    // The last one takes the field with it.
    state
        .account_quotas
        .spend_held_credit(&state, &key, Some("second"));
    assert_eq!(
        state.account_quotas.lock().entries[&key]
            .quota
            .reset_credits,
        None
    );

    // A count alone stands for the credit used.
    hold_credit_list(&state, 2, None, false);
    state.account_quotas.spend_held_credit(&state, &key, None);
    let held = state.account_quotas.lock().entries[&key]
        .quota
        .reset_credits
        .clone()
        .unwrap();
    assert_eq!((held.available_count, held.credits), (1, None));

    // A use that named none does not know which of a listed set went, so the list goes.
    hold_credit_list(&state, 2, Some(&[("first", far), ("second", far)]), false);
    state.account_quotas.spend_held_credit(&state, &key, None);
    let held = state.account_quotas.lock().entries[&key]
        .quota
        .reset_credits
        .clone()
        .unwrap();
    assert_eq!((held.available_count, held.credits), (1, None));
}

#[test]
fn spending_a_held_credit_is_broadcast_with_the_credit_gone() {
    let (state, _dir) = crate::test_support::app_state("quota-spend-broadcast");
    let key = key(Agent::Codex, None);
    let far = Some(i64::MAX - 1);
    hold_credit_list(&state, 1, Some(&[("only", far)]), false);
    let mut events = state.subscribe();
    state
        .account_quotas
        .spend_held_credit(&state, &key, Some("only"));
    let Ok(Event::AccountQuotasUpdated { account_quotas }) = events.try_recv() else {
        panic!("the spend was not broadcast");
    };
    assert_eq!(account_quotas.len(), 1);
    assert_eq!(account_quotas[0].reset_credits, None);
    // Nothing held to spend, nothing to say.
    state
        .account_quotas
        .spend_held_credit(&state, &key, Some("only"));
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn an_outcome_that_used_nothing_leaves_the_held_credits_alone() {
    let (state, dir) = crate::test_support::app_state("quota-use-reset-credit-kept");
    hold_credits(&state);
    let shell = consuming_codex(&dir, r#"{"id":2,"result":{"outcome":"nothingToReset"}}"#);
    use_reset_credit(&state, None, shell, crate::test_support::PATIENCE)
        .await
        .unwrap();
    let inner = state.account_quotas.lock();
    let held = inner.entries[&key(Agent::Codex, None)]
        .quota
        .reset_credits
        .as_ref()
        .unwrap();
    assert_eq!(held.available_count, 1);
}

#[tokio::test]
async fn an_expired_credit_is_passed_over_and_a_reading_without_details_names_none() {
    let (state, dir) = crate::test_support::app_state("quota-use-reset-credit-choice");
    let far = Some(i64::MAX - 1);
    hold_credit_list(&state, 2, Some(&[("gone", Some(1)), ("live", far)]), false);
    let shell = consuming_codex(&dir, r#"{"id":2,"result":{"outcome":"nothingToReset"}}"#);
    use_reset_credit(&state, None, shell, crate::test_support::PATIENCE)
        .await
        .unwrap();
    assert_eq!(consume_params(&dir)["creditId"], "live");

    for (label, count_only, unavailable) in [("count", true, false), ("stale", false, true)] {
        let (state, dir) =
            crate::test_support::app_state(&format!("quota-use-reset-credit-{label}"));
        let credits: &[(&str, Option<i64>)] = &[("credit", far)];
        hold_credit_list(&state, 1, (!count_only).then_some(credits), unavailable);
        let shell = consuming_codex(&dir, r#"{"id":2,"result":{"outcome":"nothingToReset"}}"#);
        use_reset_credit(&state, None, shell, crate::test_support::PATIENCE)
            .await
            .unwrap();
        assert!(consume_params(&dir).get("creditId").is_none(), "{label}");
    }
}

#[tokio::test]
async fn a_use_while_the_daemon_stops_runs_nothing() {
    let (state, dir) = crate::test_support::app_state("quota-use-reset-credit-stopping");
    hold_credits(&state);
    state.account_quotas.stop_cli_runs();
    let shell = consuming_codex(&dir, r#"{"id":2,"result":{"outcome":"reset"}}"#);
    let err = use_reset_credit(&state, None, shell, crate::test_support::PATIENCE)
        .await
        .unwrap_err();
    assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
    assert!(!dir.join("input").exists());
}

#[tokio::test]
async fn a_codex_that_is_not_found_is_told_so_and_the_account_is_not_read_again() {
    let (state, dir) = crate::test_support::app_state("quota-use-reset-credit-no-binary");
    hold_credits(&state);
    let env = HashMap::from([("PATH".to_string(), dir.display().to_string())]);
    let shell: ShellSource = Arc::new(move || Ok(env.clone()));
    let err = use_reset_credit(&state, None, shell, Duration::from_secs(1))
        .await
        .unwrap_err();
    assert_eq!(code_of(&err), error_code::BINARY_NOT_FOUND);
    assert!(!state.account_quotas.is_due_for_test(Agent::Codex, None));
}

#[tokio::test]
async fn a_refusal_for_want_of_a_sign_in_names_the_account() {
    let (state, dir) = crate::test_support::app_state("quota-use-reset-credit-signed-out");
    hold_credits(&state);
    let shell = consuming_codex(
        &dir,
        r#"{"error":{"code":-32600,"message":"codex account authentication required"},"id":2}"#,
    );
    let err = use_reset_credit(&state, None, shell, crate::test_support::PATIENCE)
        .await
        .unwrap_err();
    let coded = err.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.code, error_code::RESET_CREDIT_NOT_SIGNED_IN);
    let name = state
        .store
        .account_display_name(Agent::Codex, None)
        .unwrap();
    assert_eq!(coded.params.get("account"), Some(&name));
    assert!(state.account_quotas.is_due_for_test(Agent::Codex, None));
}

#[tokio::test]
async fn a_second_use_on_the_same_account_is_refused_while_one_runs() {
    let (state, _dir) = crate::test_support::app_state("quota-use-reset-credit-twice");
    state
        .account_quotas
        .resetting
        .lock()
        .unwrap()
        .insert(key(Agent::Codex, None).clone());
    let shell: ShellSource = Arc::new(|| anyhow::bail!("not reached"));
    let err = use_reset_credit(&state, None, shell, Duration::from_secs(1))
        .await
        .unwrap_err();
    assert_eq!(code_of(&err), error_code::RESET_CREDIT_IN_PROGRESS);
}

#[tokio::test]
async fn a_use_that_fails_is_a_coded_error_and_the_account_is_read_again_all_the_same() {
    let (state, dir) = crate::test_support::app_state("quota-use-reset-credit-fails");
    hold_credits(&state);
    let silent = crate::test_support::fake_agent(
        &dir,
        crate::adapter::adapter_for(Agent::Codex).binary(),
        "while read line; do :; done",
    );
    let shell: ShellSource = Arc::new(move || Ok(silent.clone()));
    let err = use_reset_credit(&state, None, shell, Duration::from_millis(700))
        .await
        .unwrap_err();
    assert_eq!(code_of(&err), error_code::RESET_CREDIT_TIMED_OUT);
    assert!(state.account_quotas.is_due_for_test(Agent::Codex, None));
    // The place among the uses is given back.
    assert!(state.account_quotas.resetting.lock().unwrap().is_empty());

    let shell: ShellSource = Arc::new(|| anyhow::bail!("no shell"));
    let err = use_reset_credit(&state, None, shell, Duration::from_secs(1))
        .await
        .unwrap_err();
    assert_eq!(code_of(&err), error_code::SHELL_ENVIRONMENT_TIMEOUT);
}
