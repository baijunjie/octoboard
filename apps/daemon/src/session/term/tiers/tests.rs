//! The tier decision of a launch ([`super::plan_launch`]), run without a process: what refuses,
//! what falls to the bare plan and what the user is told of it.

use super::*;
use crate::adapter::tests::{spec_fixture, SpecFixture};
use crate::protocol::{error_code, CodedError, ProviderOperation};
use crate::test_support::test_provider::{
    declaring_checked_notices, declaring_failing_plan, declaring_launch_refusal,
    declaring_reduced_plan, FAILING_PLAN_CODE,
};

/// What the launch prepared for its helpers failing, as a coded error with a path.
fn unprepared() -> Result<()> {
    Err(CodedError::raised(
        error_code::CONFIG_DIR_UNREACHABLE,
        "the scratch directory could not be made",
        &[("path", "/run/scratch")],
    ))
}

/// A fixture for `agent` that its refusals let through: no account directory of its own, and the
/// version its launch features are tested at.
fn fixture_for(agent: AgentId) -> SpecFixture {
    let mut fixture = spec_fixture();
    fixture.agent_version = adapter::provider(agent).launch_fixture_version();
    fixture
}

/// Plans `agent`'s launch over `fixture` at the enhanced tier, with what the launch prepared.
fn planned(agent: AgentId, fixture: &SpecFixture, prepared: Result<()>) -> Result<PlannedLaunch> {
    let mut spec = fixture.spec();
    plan_launch(
        agent,
        adapter::provider(agent),
        &mut spec,
        LaunchTier::Enhanced,
        None,
        prepared,
        false,
    )
}

/// For every provider, an enhanced launch that cannot be prepared starts bare, with a standing
/// `launch_enhancements_unavailable` naming the agent and nesting the failure: its message as
/// `reason`, its code as `reason_code` and its own params. The bare plan carries no server of the
/// provider's and nothing to rebuild.
#[test]
fn every_provider_falls_to_its_bare_plan_with_the_failure_nested() {
    for agent in AgentId::ALL {
        let fixture = fixture_for(agent);
        let launch = planned(agent, &fixture, unprepared())
            .unwrap_or_else(|err| panic!("{agent:?}: {err:#}"));
        assert_eq!(launch.tier, LaunchTier::Bare, "{agent:?}");
        let notice = launch.notice.expect("the user is told");
        assert_eq!(
            notice.code,
            notice_code::LAUNCH_ENHANCEMENTS_UNAVAILABLE,
            "{agent:?}"
        );
        assert!(notice.standing, "{agent:?}");
        assert_eq!(notice.params["agent"], agent.label(), "{agent:?}");
        assert_eq!(
            notice.params["reason_code"],
            error_code::CONFIG_DIR_UNREACHABLE.as_str(),
            "{agent:?}"
        );
        assert_eq!(notice.params["path"], "/run/scratch", "{agent:?}");
        assert!(
            notice.params["reason"].contains("scratch directory"),
            "{agent:?}"
        );
        assert!(launch.plan.private_server.is_none(), "{agent:?}");
        assert!(launch.plan.adoption.is_none(), "{agent:?}");
        assert!(
            launch.plan.env.is_empty(),
            "{agent:?}: {:?}",
            launch.plan.env
        );
        assert!(!launch.plan.resolves_approvals_itself, "{agent:?}");
    }
}

/// A provider whose plan fails starts bare, told why: the plan's error never refuses.
#[test]
fn a_failing_plan_starts_the_launch_bare() {
    let _failing = declaring_failing_plan();
    let mut fixture = fixture_for(AgentId::TEST);
    fixture.model = Some("model-a".to_string());
    let launch = planned(AgentId::TEST, &fixture, Ok(())).expect("not refused");
    assert_eq!(launch.tier, LaunchTier::Bare);
    assert_eq!(launch.plan.args, ["bare", "--model", "model-a"]);
    let notice = launch.notice.expect("told");
    assert_eq!(notice.params["reason_code"], FAILING_PLAN_CODE.as_str());
    assert_eq!(notice.params["path"], "/nowhere");
}

/// A plan that succeeds is the launch, at the enhanced tier, with nothing more to tell.
#[test]
fn a_plan_that_succeeds_is_the_enhanced_launch() {
    let launch = planned(AgentId::TEST, &fixture_for(AgentId::TEST), Ok(())).expect("planned");
    assert_eq!(launch.tier, LaunchTier::Enhanced);
    assert!(launch.notice.is_none());
    assert!(launch.plan.args.is_empty());
}

/// The provider's launch refusal refuses every tier, whatever else would have happened: a plan
/// that fails, or a launch held to the bare tier.
#[test]
fn a_launch_refusal_refuses_every_tier() {
    let _refusal = declaring_launch_refusal();
    let _failing = declaring_failing_plan();
    let fixture = fixture_for(AgentId::TEST);
    for highest in [LaunchTier::Enhanced, LaunchTier::Bare] {
        let mut spec = fixture.spec();
        let Err(err) = plan_launch(
            AgentId::TEST,
            adapter::provider(AgentId::TEST),
            &mut spec,
            highest,
            None,
            Ok(()),
            false,
        ) else {
            panic!("{highest:?}: not refused");
        };
        assert_eq!(
            CodedError::of(&err).code,
            error_code::PROVIDER_UNSUPPORTED,
            "{highest:?}"
        );
    }
}

/// The genuine refusals the providers keep: an account directory for an agent that has the
/// default account only refuses through the tier decision, not through a degraded plan.
#[test]
fn an_account_directory_for_a_default_only_agent_refuses() {
    for agent in AgentId::ALL
        .into_iter()
        .filter(|agent| !agent.descriptor().capabilities.owned_accounts)
        .filter(|agent| *agent != AgentId::TEST)
    {
        let mut fixture = fixture_for(agent);
        let account = fixture.scratch.join("account");
        std::fs::create_dir_all(&account).expect("account");
        fixture.config_dir = Some(account);
        let Err(err) = planned(agent, &fixture, unprepared()) else {
            panic!("{agent:?}: not refused");
        };
        let coded = CodedError::of(&err);
        assert_eq!(coded.code, error_code::PROVIDER_UNSUPPORTED, "{agent:?}");
        assert_eq!(
            coded.params.get("operation").map(String::as_str),
            Some(ProviderOperation::OwnedAccounts.wire_name()),
            "{agent:?}"
        );
    }
}

/// A pinned directory that has gone refuses a resume that needs its conversation at every tier,
/// as the CLI in a plain terminal would come up without it; a fresh launch goes on, bare where
/// the plan fails, with the directory exported through the layout.
#[test]
fn a_pinned_directory_gone_on_a_resume_refuses_and_a_bare_launch_exports_it() {
    let _failing = declaring_failing_plan();
    let mut fixture = fixture_for(AgentId::TEST);
    let gone = fixture.scratch.join("gone");
    fixture.config_dir = Some(gone.clone());
    let mut spec = fixture.spec();
    spec.resume_agent_session_id = Some("conversation-1");
    let err = plan_launch(
        AgentId::TEST,
        adapter::provider(AgentId::TEST),
        &mut spec,
        LaunchTier::Enhanced,
        None,
        Ok(()),
        false,
    )
    .err()
    .expect("refused");
    assert_eq!(
        CodedError::of(&err).code,
        error_code::CONFIG_DIR_UNREACHABLE
    );

    let launch = planned(AgentId::TEST, &fixture, Ok(())).expect("a fresh launch");
    assert_eq!(launch.tier, LaunchTier::Bare);
    let (var, value) = AgentId::TEST
        .descriptor()
        .layout
        .config_dir_export(&gone)
        .expect("exportable");
    assert_eq!(
        launch.plan.env,
        [(var.to_string(), value.to_string_lossy().into_owned())]
    );
}

/// A launch held to the bare tier is not planned enhanced and has nothing to tell of its own.
#[test]
fn a_launch_held_to_the_bare_tier_is_bare_without_a_notice() {
    let fixture = fixture_for(AgentId::TEST);
    let mut spec = fixture.spec();
    let launch = plan_launch(
        AgentId::TEST,
        adapter::provider(AgentId::TEST),
        &mut spec,
        LaunchTier::Bare,
        None,
        Ok(()),
        false,
    )
    .expect("planned");
    assert_eq!(launch.tier, LaunchTier::Bare);
    assert!(launch.notice.is_none());
}

/// Where the bare launch starts a script shim, free text goes in as the session's first message,
/// never on the shim's command line, and nothing is recorded as submitted at launch; the spec's
/// task is the caller's again afterwards.
#[test]
fn a_bare_launch_through_a_script_shim_gives_the_task_as_the_first_message() {
    let fixture = fixture_for(AgentId::TEST);
    for (script_launcher, args, first) in [
        (false, vec!["bare", "fix\nit"], None),
        (true, vec!["bare"], Some("fix\nit")),
    ] {
        let mut spec = fixture.spec();
        spec.task = Some("fix\nit");
        let launch = plan_launch(
            AgentId::TEST,
            adapter::provider(AgentId::TEST),
            &mut spec,
            LaunchTier::Bare,
            None,
            Ok(()),
            script_launcher,
        )
        .expect("planned");
        assert_eq!(launch.plan.args, args, "{script_launcher}");
        assert_eq!(
            launch.plan.first_message.as_deref(),
            first,
            "{script_launcher}"
        );
        assert_eq!(launch.plan.shaped_prompt, None);
        assert_eq!(spec.task, Some("fix\nit"));
    }
}

/// A launch records the conversation it resumes, and the id it gave a provider that preallocates
/// one; a bare launch of a new conversation gave none and records none.
#[test]
fn a_bare_launch_of_a_new_conversation_records_no_id() {
    let new = || "new-id".to_string();
    assert_eq!(
        recorded_conversation(Some("old".into()), true, LaunchTier::Bare, new()).as_deref(),
        Some("old")
    );
    assert_eq!(
        recorded_conversation(None, true, LaunchTier::Enhanced, new()).as_deref(),
        Some("new-id")
    );
    assert_eq!(
        recorded_conversation(None, true, LaunchTier::Bare, new()),
        None
    );
    assert_eq!(
        recorded_conversation(None, false, LaunchTier::Enhanced, new()),
        None
    );
}

/// The tiers are ordered from the floor up and read back from the terminal's meta as written.
#[test]
fn tiers_are_ordered_and_round_trip_through_the_meta() {
    assert!(LaunchTier::Bare < LaunchTier::Reduced);
    assert!(LaunchTier::Reduced < LaunchTier::Enhanced);
    for tier in [LaunchTier::Bare, LaunchTier::Reduced, LaunchTier::Enhanced] {
        let written = serde_json::to_value(tier).expect("written");
        assert_eq!(
            serde_json::from_value::<LaunchTier>(written).expect("read"),
            tier
        );
    }
    assert_eq!(serde_json::to_value(LaunchTier::Bare).unwrap(), "bare");
}

/// How a CLI that rejected an option exited.
/// What the provider's check of every launch finds to tell is told at every tier, beside the
/// tier's own notice: the enhanced, the reduced and the bare launch alike.
#[test]
fn what_the_check_finds_is_told_at_every_tier() {
    let _reduced = declaring_reduced_plan();
    let _checked = declaring_checked_notices(|| {
        vec![crate::records::resumability::record_not_found_notice(
            AgentId::TEST,
        )]
    });
    let exit = refused_option();
    for (highest, exit) in [
        (LaunchTier::Enhanced, None),
        (LaunchTier::Reduced, Some(&exit)),
        (LaunchTier::Bare, Some(&exit)),
    ] {
        let launch = planned_after(highest, exit);
        assert_eq!(launch.tier, highest);
        let codes: Vec<_> = launch.checked.iter().map(|notice| &notice.code).collect();
        assert_eq!(
            codes,
            [&notice_code::CONVERSATION_RECORD_NOT_FOUND],
            "{highest:?}"
        );
    }
}

fn refused_option() -> EarlyExit {
    EarlyExit {
        output: "error: unexpected argument '--hooks'".to_string(),
        status: Some(2),
    }
}

/// Plans the test-only provider's launch at `highest` after `exit`.
fn planned_after(highest: LaunchTier, exit: Option<&EarlyExit>) -> PlannedLaunch {
    let fixture = spec_fixture();
    let mut spec = fixture.spec();
    plan_launch(
        AgentId::TEST,
        adapter::provider(AgentId::TEST),
        &mut spec,
        highest,
        exit,
        Ok(()),
        false,
    )
    .expect("planned")
}

/// After an early exit of the enhanced launch, a provider that gives a reduced plan is launched
/// with it, handed the early exit, and the tier adds no notice of its own: the plan carries its
/// own.
#[test]
fn a_reduced_launch_takes_the_providers_reduced_plan_handed_the_early_exit() {
    let _reduced = declaring_reduced_plan();
    let exit = refused_option();
    let launch = planned_after(LaunchTier::Reduced, Some(&exit));
    assert_eq!(launch.tier, LaunchTier::Reduced);
    assert_eq!(launch.plan.args, ["reduced", exit.output.as_str()]);
    assert!(launch.notice.is_none());
}

/// For every provider that gives no reduced plan, the launch after an early exit is bare, and
/// the user is told with a standing `launch_retried_plain` naming the agent and carrying what
/// the CLI wrote.
#[test]
fn without_a_reduced_plan_the_launch_after_an_early_exit_is_bare_and_says_so() {
    let exit = refused_option();
    for agent in AgentId::ALL {
        let fixture = fixture_for(agent);
        let mut spec = fixture.spec();
        let provider = adapter::provider(agent);
        if provider.reduced_plan(&spec, &exit).is_some() {
            continue;
        }
        let launch = plan_launch(
            agent,
            provider,
            &mut spec,
            LaunchTier::Reduced,
            Some(&exit),
            Ok(()),
            false,
        )
        .unwrap_or_else(|err| panic!("{agent:?}: {err:#}"));
        assert_eq!(launch.tier, LaunchTier::Bare, "{agent:?}");
        let notice = launch.notice.expect("the user is told");
        assert_eq!(notice.code, notice_code::LAUNCH_RETRIED_PLAIN, "{agent:?}");
        assert!(notice.standing, "{agent:?}");
        assert_eq!(notice.params["agent"], agent.label(), "{agent:?}");
        assert_eq!(notice.params["detail"], exit.output, "{agent:?}");
    }
}

/// A launch held at the bare tier after the reduced one exited too is bare whatever the provider
/// would reduce, and says so; a bare launch that follows no early exit says nothing.
#[test]
fn a_bare_launch_tells_of_an_early_exit_only_when_it_follows_one() {
    let _reduced = declaring_reduced_plan();
    let exit = refused_option();
    let after = planned_after(LaunchTier::Bare, Some(&exit));
    assert_eq!(after.tier, LaunchTier::Bare);
    assert_eq!(
        after.notice.map(|notice| notice.code),
        Some(notice_code::LAUNCH_RETRIED_PLAIN)
    );
    let plain = planned_after(LaunchTier::Bare, None);
    assert!(plain.notice.is_none());
}

/// What the launch prepared failing still starts bare with the failure nested, at the reduced
/// tier too: the reduced plan needs what the enhanced one does.
#[test]
fn a_reduced_launch_whose_preparation_failed_starts_bare_with_the_failure_nested() {
    let _reduced = declaring_reduced_plan();
    let exit = refused_option();
    let fixture = spec_fixture();
    let mut spec = fixture.spec();
    let launch = plan_launch(
        AgentId::TEST,
        adapter::provider(AgentId::TEST),
        &mut spec,
        LaunchTier::Reduced,
        Some(&exit),
        unprepared(),
        false,
    )
    .expect("planned");
    assert_eq!(launch.tier, LaunchTier::Bare);
    assert_eq!(
        launch.notice.map(|notice| notice.code),
        Some(notice_code::LAUNCH_ENHANCEMENTS_UNAVAILABLE)
    );
}
