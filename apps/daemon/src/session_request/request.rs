//! What a request is: the [`Action`] an approval takes, the [`Ask`] a request makes, and the
//! [`Request`] the dialog is told of.

use uuid::Uuid;

use crate::branch_origin::transfer::BranchTransfer;
use crate::coordinator::open::OpenRequest;
use crate::protocol::{Agent, Event, Session, SessionKind, SessionRequestAction};
use crate::time::now_millis;

use super::outcome::approval_wording;
use super::Change;

/// What a call that changed nothing tells the model to do next: stopping there is the expected
/// outcome, and asking again would put the same question in front of the user straight away.
const DO_NOT_ASK_AGAIN: &str = "Do not ask again unless the user tells you to. Tell the user in \
                                this terminal what you wanted the session for, and follow their \
                                direction on whether to continue your own work or leave notes for \
                                the next session and end";

/// [`DO_NOT_ASK_AGAIN`] for a switch or a handoff, which has no new session to want.
const DO_NOT_ASK_AGAIN_TO_CHANGE: &str = "Do not ask again unless the user tells you to. Tell the \
                                          user in this terminal why you wanted it, and follow \
                                          their direction on what to do instead";

/// What an approval does, as the request was made.
pub(crate) enum Action {
    /// Starts a session, unbound, from `open`, and moves `transfer`'s branches to it. The transfer
    /// is boxed so that this variant stays near the size of the others.
    Start {
        open: OpenRequest,
        transfer: Option<Box<BranchTransfer>>,
    },
    /// Brings back the archived or interrupted session `target`, which holds the conversation;
    /// `archived` is which it was when the request was made, which is what the dialog words the
    /// question by. Its owner stays as it is, and an archived owner comes back with it
    /// (`reopened_owners`, as they stood when the request was made, which is what the dialog
    /// lists). `rename` is the name to give an archived session once it is back.
    Resume {
        target: String,
        archived: bool,
        owner: Option<String>,
        reopened_owners: Vec<String>,
        rename: Option<String>,
    },
    /// Opens a session, unbound, from `open`, which continues the agent's own record of the
    /// conversation `conversation` that no session holds.
    ResumeRecord {
        open: OpenRequest,
        conversation: String,
    },
}

/// What a request asks the user to approve: a session to start or bring back, or a change to a
/// session the requester owns (`change`).
pub(crate) enum Ask {
    Session(Action),
    Change(Change),
}

/// One request: what the dialog is told of it, and what an approval does.
pub(super) struct Request {
    pub(super) id: String,
    /// The requesting session.
    pub(super) session: String,
    requested_at: i64,
    /// The agent the session would run, settled when the request was made; for a resume, the
    /// agent of the target or of the record; for a switch, the target's, and for a handoff, the
    /// receiving agent.
    pub(super) agent: Agent,
    /// Where the session is, or would be.
    pub(super) console: String,
    pub(super) project: Option<String>,
    pub(super) ask: Ask,
    /// Whether the approval is Octoboard's own, under the user's setting, rather than the user's
    /// answer to the dialog. Only the wording of the outcome differs.
    pub(super) automatic: bool,
}

impl Request {
    pub(super) fn new(
        caller: &Session,
        agent: Agent,
        console: String,
        project: Option<String>,
        ask: Ask,
        automatic: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session: caller.id.clone(),
            requested_at: now_millis(),
            agent,
            console,
            project,
            ask,
            automatic,
        }
    }

    /// Whether the session asked for is a console session, which is opened without a project.
    fn kind(&self) -> SessionKind {
        match self.project {
            Some(_) => SessionKind::Project,
            None => SessionKind::Console,
        }
    }

    /// The tool the request was made with, as the outcome message names it.
    pub(super) fn tool(&self) -> &'static str {
        match &self.ask {
            Ask::Session(Action::Start { .. }) => "request_session",
            Ask::Session(Action::Resume { .. } | Action::ResumeRecord { .. }) => {
                "request_resume_session"
            }
            Ask::Change(change) => change.tool(),
        }
    }

    /// What the request is called in what the call is told of how it ended.
    pub(super) fn what(&self) -> &'static str {
        match &self.ask {
            Ask::Session(_) => "request for a session",
            Ask::Change(change) => change.what(),
        }
    }

    /// What a call that changed nothing is told to do next.
    pub(super) fn do_not_ask_again(&self) -> &'static str {
        match &self.ask {
            Ask::Session(_) => DO_NOT_ASK_AGAIN,
            Ask::Change(_) => DO_NOT_ASK_AGAIN_TO_CHANGE,
        }
    }

    /// How the user's approval of `action` is worded in the outcome, by the request's kind.
    pub(super) fn approval(&self, action: &Action) -> &'static str {
        approval_wording(self.automatic, action)
    }

    /// What a request that came to nothing says was left undone.
    pub(super) fn untouched(&self) -> &'static str {
        match &self.ask {
            Ask::Session(Action::Start { .. }) => "nothing was started",
            Ask::Session(Action::Resume { .. } | Action::ResumeRecord { .. }) => {
                "nothing was resumed"
            }
            Ask::Change(_) => "nothing was changed",
        }
    }

    /// Whether this request takes the place of `other`, which the same session has waiting: a
    /// session asking again has stopped waiting for its earlier call. A switch or a handoff
    /// replaces only an earlier one of the same kind for the same session, so an owner can have
    /// several sessions' changes waiting at once.
    pub(super) fn replaces(&self, other: &Request) -> bool {
        self.session == other.session
            && match (&self.ask, &other.ask) {
                (Ask::Session(_), Ask::Session(_)) => true,
                (Ask::Change(this), Ask::Change(that)) => this.same_change(that),
                _ => false,
            }
    }

    /// What the session's approval being carried out is marked by: the session itself for a
    /// request for a session, and the change for a switch or a handoff, so that one change being
    /// carried out does not hold another back.
    pub(super) fn approval_slot(&self) -> String {
        match &self.ask {
            Ask::Session(_) => self.session.clone(),
            Ask::Change(change) => format!("{}\0{}", self.session, change.slot()),
        }
    }

    pub(super) fn event(&self) -> Event {
        let mut target = None;
        let mut owner = None;
        let mut reopened_owners = Vec::new();
        let mut account = None;
        let mut conversation = None;
        let mut branches = Vec::new();
        let mut model = None;
        let mut effort = None;
        let action = match &self.ask {
            Ask::Change(change) => {
                let described = change.describe();
                target = Some(described.target);
                account = described.account;
                model = described.model;
                effort = described.effort;
                described.action
            }
            Ask::Session(Action::Start { transfer, .. }) => {
                branches = transfer
                    .as_ref()
                    .map(|transfer| transfer.branches.clone())
                    .unwrap_or_default();
                SessionRequestAction::Start
            }
            Ask::Session(Action::Resume {
                target: session,
                archived,
                owner: reports_to,
                reopened_owners: owners,
                ..
            }) => {
                target = Some(session.clone());
                owner = reports_to.clone();
                reopened_owners = owners.clone();
                if *archived {
                    SessionRequestAction::Reopen
                } else {
                    SessionRequestAction::Resume
                }
            }
            Ask::Session(Action::ResumeRecord {
                open,
                conversation: id,
            }) => {
                account = open.account.clone().flatten();
                conversation = Some(id.clone());
                SessionRequestAction::ResumeRecord
            }
        };
        Event::SessionRequest {
            request_id: self.id.clone(),
            session: self.session.clone(),
            action,
            kind: self.kind(),
            console: self.console.clone(),
            project: self.project.clone(),
            agent: self.agent,
            target,
            owner,
            reopened_owners,
            account,
            conversation,
            branches,
            model,
            effort,
            requested_at: self.requested_at,
        }
    }
}
