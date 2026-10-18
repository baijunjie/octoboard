# Handing a session off to another agent

## Problem

When a session's account reaches its usage limit, the work stops. Switching to another account of the same agent
("Switch account") resumes the same conversation and already covers that case. Moving the work to **another agent**
cannot resume the conversation: the agents' conversation records are in different formats and none resumes another's.

The user can open a new session and tell it to read the old one's record, and the receiving model understands the
work about as well either way. What a new session cannot take over on its own is the original session's place in
Octoboard: the branches it is responsible for, the sessions it owns, and the owner it reports to. Those are
Octoboard's records, and only Octoboard can move them.

## Plan outline

1. Build a handoff package from the original session by program alone: a filtered copy of its conversation record,
   plus the git state of the branches it is responsible for.
2. A handoff operation that starts the receiving session and moves the original session's branches, sessions and
   owner binding to it, then archives the original.
3. A **Hand off** entry on the session row that lets the user pick the receiving agent and account.
4. The same operation as an Octoboard tool, for an owner to hand off a session of its own.
5. An Octoboard tool for an owner to switch a session of its own to another account of the same agent.

## Key design decisions

- **Octoboard calls no model.** Extracting facts (who said what, the latest todo list, the files changed, the git
  state) is done by program. Understanding the work — the goal, what failed, the decisions taken, the next step — is
  left to the receiving session, which is a model already: an extra summarising call would cost another quota and
  can drop information.
- **The record is filtered and written to a file of its own**, parsed per agent (Claude Code, Codex, Grok Build).
  Tool output (file contents read, command results) is dropped; the whole conversation is otherwise kept — every
  user message and every model reply, not only the recent turns — together with the latest todo list and the list of
  files changed. The path of the original record goes with it, for the receiving session to look things up. Reason:
  a raw record is mostly tool output and can be larger than the receiving session's context, and the receiving
  session should not spend context learning another agent's record format.
- **The package file lives as long as the receiving session**: it is removed when the receiving session is deleted,
  and is still there when it is reopened from the archive, since its brief, replayed on every resume, points at it.
- **Only a session that is not working can be handed off.** A handoff is refused while the original session is in
  the middle of a turn; it is allowed once it is awaiting instructions, waiting for the user, interrupted, or stopped.
  A session still working would go on writing to its worktree while the receiving session starts on it.
- **The receiving session is always another agent.** Moving to another account of the same agent goes through
  Switch account, which keeps the whole conversation; a handoff to the original's own agent is refused. Owners have no
  way to switch a session's account today, so they get a tool for it (milestone 05).
- **The user triggers it, by hand, or an owner does for a session of its own.** No usage-limit detection and no
  automatic handoff in this round.
- **An owner hands off its own sessions without asking the user**, as with `transfer_session` and `archive_session`.
  An owner can already replace a session with `new_session` and `archive_session`; what it cannot do is move the
  branches the session is responsible for (today only a user-approved session request moves them), and building the
  package itself would spend the owner's context when its quota is tight.
- **Project sessions and console sessions can both be handed off.** A console session most often owns a batch of
  sessions, which is where a new session that only reads the record cannot take over. A console session has no
  branches, so for it only the sessions it owns, its record and its own binding move.
- **The original session is archived once the handoff is done.** Its branches, sessions and binding have all moved;
  keeping it would leave two sessions that both look responsible for the work. Its record stays, so it can be reopened
  to look at.
- **The receiving session takes the original's whole place**: the same kind (project or console), in the same project
  or console, with the same role (unbound, lead or bound) and the same owner; the branches it was responsible for name
  the receiving session from then on; every session it owned, archived ones included, is bound to the receiving
  session. The sessions concerned and the owner are told, as with a handover between owners.
- **Messages for the original during a handoff are not lost and do not start a turn on it.** From the moment the
  handoff claims the original, deliveries to it are queued instead of written; once the receiving session has taken its
  place, the original's queue (what was already in its outbox included) is forwarded to the receiving session before the
  original is archived. A handoff that fails before the receiving session has started writes the queue to the original
  as usual.
- **The receiving session carries over the original's pinned state.**
- **The original's brief is not carried separately.** Sessions do not store their brief, so the receiving session's
  opening prompt holds the package's path and the read-first instruction only; the brief reaches it as the first user
  message in the package's conversation.

## Milestones

1. 01 Handoff package (closed)
2. 02 Handoff operation (closed)
3. 03 Hand off entry in the UI (closed)
4. 04 Handoff tool for owners (closed)
5. [Account switch tool for owners](05-account-switch-tool.md)

## Open

- Which of Codex's and Grok Build's records carry a todo list (Codex's plan updates; Grok Build to be checked).
