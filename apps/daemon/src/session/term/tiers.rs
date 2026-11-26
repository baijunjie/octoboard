//! The tier decision of a launch (`docs/plans/terminal-first-degradation.md`, D1 and D6): which
//! of the enhanced, reduced and bare launches a request is planned as, and what the user is told
//! of it, apart from the spawn ([`plan_launch`]); and the conversation id a launch records for its
//! session ([`recorded_conversation`]).

use anyhow::Result;

use super::is_blank_once_stripped;
use crate::adapter::{self, AgentProvider, EarlyExit, LaunchPlan, LaunchSpec, LaunchTier};
use crate::protocol::{nested_params, notice_code, AgentId, Notice};

/// What a launch is planned as ([`plan_launch`]).
pub(crate) struct PlannedLaunch {
    pub tier: LaunchTier,
    pub plan: LaunchPlan,
    /// What the user is told of a launch that fell to the bare tier: that the enhanced launch
    /// could not be prepared, and why.
    pub notice: Option<Notice>,
    /// What the provider's check of every launch found to tell, whatever the tier
    /// ([`AgentProvider::check_launch`]).
    pub checked: Vec<Notice>,
}

/// Plans a launch of `agent` through `provider` for `spec` at the highest tier it may use
/// (`highest`), apart from the spawn: what stops every launch is asked first, each a refusal (the
/// pinned directory gone on a resume, the provider's [`AgentProvider::check_launch`], a pinned
/// directory the agent's variable cannot name), the provider's check also giving what it found to
/// tell, which every tier tells; then the provider's plan, where the enhanced tier may be used and
/// what the launch prepared for its helpers (`prepared`, the scratch directory,
/// the key file, the hook script and the program that carries the enhanced arguments) is in
/// place; and on any failure there, the provider's bare plan with the account's directory
/// exported through the layout, and a standing `launch_enhancements_unavailable` nesting the
/// failure. A plan's error never refuses.
///
/// A launch held at the reduced tier after an early exit (`early_exit`) asks the provider's
/// [`AgentProvider::reduced_plan`] instead, and is bare where it gives none; a bare launch that
/// follows an early exit tells the user so with a standing `launch_retried_plain`.
///
/// `script_launcher` says the program the bare launch starts is a script shim
/// (`platform::shell::is_script_launcher`), where free text cannot go on the command line: the
/// task is then left out of the bare plan and given as the session's first message.
pub(crate) fn plan_launch(
    agent: AgentId,
    provider: &dyn AgentProvider,
    spec: &mut LaunchSpec<'_>,
    highest: LaunchTier,
    early_exit: Option<&EarlyExit>,
    prepared: Result<()>,
    script_launcher: bool,
) -> Result<PlannedLaunch> {
    adapter::pinned_config_dir(spec, agent)?;
    let checked = provider.check_launch(spec)?;
    let pinned = adapter::pinned_config_dir_export(spec, agent)?;
    let failure = match highest {
        LaunchTier::Enhanced => match prepared.and_then(|()| provider.plan(spec)) {
            Ok(plan) => {
                return Ok(PlannedLaunch {
                    tier: LaunchTier::Enhanced,
                    plan,
                    notice: None,
                    checked,
                })
            }
            Err(err) => Some(err),
        },
        // What the reduced plan leaves out is the provider's to say, and the plan carries its own
        // notice of it.
        LaunchTier::Reduced => match prepared {
            Ok(()) => {
                if let Some(plan) = early_exit.and_then(|exit| provider.reduced_plan(spec, exit)) {
                    return Ok(PlannedLaunch {
                        tier: LaunchTier::Reduced,
                        plan,
                        notice: None,
                        checked,
                    });
                }
                None
            }
            Err(err) => Some(err),
        },
        LaunchTier::Bare => None,
    };
    if let Some(err) = &failure {
        tracing::warn!(
            session = %spec.session_id,
            error = %format!("{err:#}"),
            "the enhanced launch could not be prepared; starting the agent bare"
        );
    }
    let task = spec.task.filter(|_| script_launcher);
    if task.is_some() {
        spec.task = None;
    }
    let mut bare = provider.bare_plan(spec);
    if let Some(task) = task {
        spec.task = Some(task);
        bare.shaped_prompt = None;
        if bare.first_message.is_none() && !is_blank_once_stripped(task) {
            bare.first_message = Some(task.to_string());
        }
    }
    let mut plan = bare.into_launch_plan();
    plan.env.extend(pinned);
    let notice = match (failure, early_exit) {
        (Some(err), _) => Some(enhancements_unavailable(agent, &err)),
        (None, Some(exit)) => Some(retried_plain(agent, exit)),
        (None, None) => None,
    };
    Ok(PlannedLaunch {
        tier: LaunchTier::Bare,
        plan,
        notice,
        checked,
    })
}

/// The standing notice that `agent`'s CLI exited as it started (`exit`), so it was started again
/// bare.
pub(crate) fn retried_plain(agent: AgentId, exit: &EarlyExit) -> Notice {
    Notice::new(
        notice_code::LAUNCH_RETRIED_PLAIN,
        format!(
            "{} exited right as it started, so it was started again without Octoboard's \
             additions, as it runs in a plain terminal. It wrote:\n{}",
            agent.label(),
            exit.output
        ),
        &[("agent", agent.label()), ("detail", &exit.output)],
    )
    .standing()
}

/// The conversation id a launch records for its session: the one it resumes, or the one it gave a
/// provider that preallocates the id of a new conversation (`preallocates`, `new`). A bare launch
/// of a new conversation gave the CLI no id, so none is recorded: a later resume must not ask for
/// a conversation that never existed.
pub(super) fn recorded_conversation(
    resume: Option<String>,
    preallocates: bool,
    tier: LaunchTier,
    new: String,
) -> Option<String> {
    match resume {
        Some(id) => Some(id),
        None if preallocates && tier.integrated() => Some(new),
        None => None,
    }
}

/// The standing notice that `agent`'s enhanced launch could not be prepared, so it started bare,
/// nesting the failure `err` as `launch_failed` does.
fn enhancements_unavailable(agent: AgentId, err: &anyhow::Error) -> Notice {
    let (params, reason) = nested_params(err, &[("agent", agent.label())]);
    Notice {
        code: notice_code::LAUNCH_ENHANCEMENTS_UNAVAILABLE,
        message: format!(
            "Octoboard's additions to {} could not be prepared, so it started as it does in a \
             plain terminal: {reason}",
            agent.label()
        ),
        params,
        standing: true,
    }
}

#[cfg(test)]
mod tests;
