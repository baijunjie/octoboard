//! The tools a session tells another session through: its report and a message.

use serde_json::json;

use super::schemas::object_schema;
use super::ToolDef;

pub(super) const REPORT: ToolDef = ToolDef {
    name: "report",
    description: "Report the round of work to your parent session, whether you are a worker or \
                  a coordinator. The parent is a console session or project coordinator. \
                  `summary` is natural language; `status` and `open_items` are what the parent \
                  acts on. Items that are the parent's to carry out, not work you left \
                  unfinished, go in `owner_items`: they are listed for the parent and do not \
                  count as open items. An interrupted parent with no process has the report kept \
                  until it resumes; an archived parent takes none, and the report is refused. With \
                  `done` and no open items the session is archived once the report is delivered \
                  or kept, and the git branches it created, with the linked worktrees that have \
                  them checked out, are removed once it has ended wherever git can do it \
                  without force (the answer names any that will stay); anything else leaves it \
                  running and awaiting instructions. Refused while this session is independent: \
                  it answers directly to the user.",
    schema: || {
        object_schema(
            json!({
                "summary": {
                    "type": "string",
                    "description": "What was done and what came of it, in natural language.",
                },
                "status": {
                    "type": "string",
                    "enum": ["done", "failed", "needs_decision"],
                    "description": "`done` when the task is finished, `failed` when it could not \
                                    be, `needs_decision` when your owner has to choose before you \
                                    can go on.",
                },
                "open_items": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Anything you left unfinished. Empty when nothing is.",
                },
                "owner_items": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Items handed to your parent to carry out (local testing \
                                    it runs, a decision only it can act on). Not open items: \
                                    they do not keep a `done` report from archiving you.",
                },
            }),
            &["summary", "status"],
        )
    },
};

pub(super) const INFORM_SESSION: ToolDef = ToolDef {
    name: "inform_session",
    description: "Tell a session something it may find useful: what you found, decided or \
                  changed that touches its work. Any session can be told, in any project or \
                  console, console sessions included; no session needs to own the other. It is \
                  delivered marked as information from you, not as an instruction, and the \
                  session it is for takes work only from its owner. When that session reports to \
                  another session, that owner is sent a copy. Held until the user is done when \
                  the session is waiting for them. For an interrupted session, with no process \
                  running, it is kept and delivered when the session is resumed, and dropped if \
                  the session is archived or deleted first; the answer says `Queued`. Refused \
                  for this session itself, and for an archived session, which takes nothing \
                  until it is reopened.",
    schema: || {
        object_schema(
            json!({
                "session": { "type": "string", "description": "The session's id." },
                "text": {
                    "type": "string",
                    "description": "The information, in natural language.",
                },
            }),
            &["session", "text"],
        )
    },
};
