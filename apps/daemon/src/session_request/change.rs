//! The request an owner makes with `switch_session_account` or `handoff_session` while the user has
//! not chosen to have them run without asking: the same wait, hand and answer as a request for a
//! session (see `super`), with a change to a session of the owner's own to approve instead of a
//! session to start. The dialog is a form, so an answer carries the values the user changed
//! ([`Edits`]), and what an approval runs is the tool's own operation, handed over as a
//! [`Perform`] and run once with them.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::Value;
use tokio::sync::oneshot;

use crate::protocol::{Agent, Session, SessionRequestAction, SessionRequestEnding};
use crate::state::AppState;

use super::{outcome_message, tell_the_caller, wait_for_answer, Ask, Request};

/// What the user changed in the dialog of a switch or a handoff, each absent where the request's
/// own value stands. Empty for every other kind of request, and for one approved automatically.
#[derive(Debug, Default, Clone)]
pub struct Edits {
    pub agent: Option<Agent>,
    pub account: Option<Option<String>>,
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
}

/// What an approved switch or handoff did: the session it left running in the asking owner's
/// care, and the call's result, whose `note` is the outcome written into the caller's session.
pub(crate) struct Performed {
    pub session: Session,
    pub result: Value,
}

type PerformFuture = Pin<Box<dyn Future<Output = Result<Performed>> + Send>>;
type PerformOnce = Box<dyn FnOnce(Edits) -> PerformFuture + Send>;

/// The operation an approval of a switch or a handoff runs, with the values the user ended the
/// dialog on. It runs at most once.
pub(crate) struct Perform(Mutex<Option<PerformOnce>>);

impl Perform {
    pub(crate) fn new<F, Fut>(run: F) -> Self
    where
        F: FnOnce(Edits) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Performed>> + Send + 'static,
    {
        Self(Mutex::new(Some(Box::new(move |edits| {
            Box::pin(run(edits))
        }))))
    }

    pub(super) fn start(&self, edits: Edits) -> Result<PerformFuture> {
        let run = self
            .0
            .lock()
            .expect("perform lock poisoned")
            .take()
            .ok_or_else(|| anyhow!("this request was carried out already"))?;
        Ok(run(edits))
    }
}

/// A change to a session of the requester's, as the call asked for it and the dialog starts on it.
pub(crate) enum Change {
    /// Moves the session `target` to `account` (`None` for the agent's default account).
    Switch {
        target: String,
        account: Option<String>,
        perform: Perform,
    },
    /// Hands the session `target` off to a new session of `agent` on `account` (`None` for the
    /// default account), with the `model` and `effort` the call asked for.
    Handoff {
        target: String,
        agent: Agent,
        account: Option<String>,
        model: Option<Option<String>>,
        effort: Option<Option<String>>,
        perform: Perform,
    },
}

/// What the dialog is told of a change.
pub(super) struct Described {
    pub action: SessionRequestAction,
    pub target: String,
    pub account: Option<String>,
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
}

impl Change {
    /// The tool the change was asked with.
    pub(super) fn tool(&self) -> &'static str {
        match self {
            Change::Switch { .. } => "switch_session_account",
            Change::Handoff { .. } => "handoff_session",
        }
    }

    /// What the change is called in what the call is told of how it ended.
    pub(super) fn what(&self) -> &'static str {
        match self {
            Change::Switch { .. } => "switch of the session's account",
            Change::Handoff { .. } => "handoff of the session",
        }
    }

    /// What names the change among the requester's: its kind and the session it acts on.
    pub(super) fn slot(&self) -> String {
        match self {
            Change::Switch { target, .. } => Self::slot_of("switch_session_account", target),
            Change::Handoff { target, .. } => Self::slot_of("handoff_session", target),
        }
    }

    /// [`Self::slot`] of the change `tool` makes to `target`.
    fn slot_of(tool: &str, target: &str) -> String {
        format!("{tool}\0{target}")
    }

    pub(super) fn same_change(&self, other: &Change) -> bool {
        self.slot() == other.slot()
    }

    pub(super) fn describe(&self) -> Described {
        match self {
            Change::Switch {
                target, account, ..
            } => Described {
                action: SessionRequestAction::Switch,
                target: target.clone(),
                account: account.clone(),
                model: None,
                effort: None,
            },
            Change::Handoff {
                target,
                account,
                model,
                effort,
                ..
            } => Described {
                action: SessionRequestAction::Handoff,
                target: target.clone(),
                account: account.clone(),
                model: model.clone(),
                effort: effort.clone(),
            },
        }
    }

    pub(super) fn perform(&self) -> &Perform {
        match self {
            Change::Switch { perform, .. } | Change::Handoff { perform, .. } => perform,
        }
    }
}

/// The ask of `switch_session_account` and `handoff_session`: asks the user to approve `change`, a
/// switch or a handoff of the session `target`, and waits up to `limit` for the answer, as
/// `ask_to_approve` does. What it could never be carried out for is refused by the caller before
/// this. What the call returns is the result `change`'s [`Perform`] gives, run with the values the
/// user ended the dialog on.
pub(crate) async fn ask_to_change(
    state: &Arc<AppState>,
    caller: &Session,
    target: &Session,
    change: Change,
    limit: Duration,
) -> Result<Value> {
    let agent = match &change {
        Change::Handoff { agent, .. } => *agent,
        Change::Switch { .. } => target.agent,
    };
    let request = Request::new(
        caller,
        agent,
        target.console_id.clone(),
        target.project_id.clone(),
        Ask::Change(change),
        false,
    );
    wait_for_answer(state, caller, request, limit).await
}

/// Runs the operation of `tool` without asking, as the user's standing approval has it run: the
/// request of its kind for `target` that is still waiting is closed, as an asked one would
/// replace it, and the operation runs through [`run_to_the_end`].
pub(crate) async fn run_unasked(
    state: &Arc<AppState>,
    caller: &Session,
    tool: &'static str,
    target: &str,
    run: impl Future<Output = Result<Performed>> + Send + 'static,
) -> Result<Value> {
    // A request of the same kind for the same session that is still waiting is this call's to
    // replace, as an asked one would.
    let slot = Change::slot_of(tool, target);
    super::close(
        state,
        |request| {
            request.session == caller.id
                && matches!(&request.ask, Ask::Change(change) if change.slot() == slot)
        },
        SessionRequestEnding::Withdrawn,
    );
    let run = async move { run.await.map(|done| done.result) };
    run_to_the_end(state, caller, tool, run).await
}

/// Runs the operation of `tool`, which must finish once started, on a task of its own and answers
/// what it returns: the call is cancelled by dropping it (see `mcp::stdio`), and with it a call's
/// future at whichever `.await` it had reached, which must not leave the operation half done. When
/// the call is gone by the time the operation ends, the outcome (the result's `note`, or the
/// failure) is written into the caller's session instead, so the caller still learns what was
/// done.
pub(crate) async fn run_to_the_end(
    state: &Arc<AppState>,
    caller: &Session,
    tool: &'static str,
    run: impl Future<Output = Result<Value>> + Send + 'static,
) -> Result<Value> {
    let (reply, awaiting) = oneshot::channel::<Result<Value>>();
    let (state, caller_id, agent) = (state.clone(), caller.id.clone(), caller.agent);
    tokio::spawn(async move {
        let (note, answer) = match run.await {
            Ok(result) => (
                result["note"].as_str().unwrap_or_default().to_string(),
                Ok(result),
            ),
            Err(err) => (format!("It could not be carried out: {err:#}."), Err(err)),
        };
        if reply.send(answer).is_err() {
            tell_the_caller(&state, &caller_id, outcome_message(agent, tool, &note)).await;
        }
    });
    match awaiting.await {
        Ok(answer) => answer,
        Err(_) => Err(anyhow!("the `{tool}` call did not finish")),
    }
}
