# Handing a session off to another agent

A **handoff** moves a session's work to a new session of **another agent**, which takes the original's whole place in
Octoboard; the original is archived. The agents' conversation records cannot resume one another, so the conversation
itself does not move: the new session — the **receiving session** — starts by reading a **handoff package** built from
the original's records. Moving a session to another account of the same agent, which keeps the conversation itself, is
switching its account instead (see `docs/product/switching-session-accounts.md`).

A handoff is made with the daemon client protocol's `hand_off_session` request, which names the session, the receiving
agent, and optionally the receiving session's account, model and reasoning effort. In the window it is made from a
session's **Hand off** menu item (see "Handing off from the window" below). An owner hands off a session of its own
with the `handoff_session` tool (see "Handing off from an owner's tool" below). All of them run the same handoff,
described in the rest of this document. A handoff is made only when the user or an owner asks for one: Octoboard never
hands a session off on its own, not even off an account whose quota is used up.

## What can be handed off

A project session or a console session, of any role — unbound, lead or bound — can be handed off while it is waiting
for instructions, waiting for the user or interrupted. One running in background counts as waiting for instructions:
it can be handed off, and the work it left running in the background ends with its process (see "Running in
background" in `docs/product/sessions.md`). A handoff is refused, with nothing changed, when:

- the session does not exist (`unknown_session`), is archived (`session_archived`; reopen it first), or is in the middle
  of a turn or still starting (`session_working`: a session still at work goes on writing to its worktree while the
  receiving session starts on it);
- it is being started, resumed, switched to another account, handed off already or having its owner changed
  (`session_already_starting`);
- it is locked (`session_locked`; see "Locking a session" in `docs/product/sessions.md`);
- the receiving agent is the session's own (`handoff_to_same_agent`, which points to switching its account);
- the receiving agent is not available (`agent_not_available`), is not added (`agent_not_added`; see "Opening a
  session" in `docs/product/sessions.md` for when that applies), the account named is not one of that agent's
  (`unknown_account`), the receiving session's account, once resolved, is used up (`account_quota_exhausted`; see
  "Opening a session" in `docs/product/sessions.md`), or the model or reasoning effort is not one the agent offers
  (`invalid_launch_setting`);
- for a session bound to an owner, opening a session bound to that owner would be refused.

The account, model and reasoning effort follow the rules for opening a session (see "Which account a session uses" and
"Which model and reasoning effort a session uses" in `docs/product/sessions.md`): left out, they come from the project
or console as for any new session of that agent.

From the moment a handoff is accepted until it ends, the original cannot be resumed, switched to another account or
deleted.

## Handing off from the window

A session's action menu offers **Hand off** right after Switch account (where that is offered), before Archive, for a
project session or a console session that is not archived, wherever that menu is offered. It is disabled while the
session is *working* or *starting*, and while the session is locked, as Archive is, and while no agent other than the
session's own can be picked (see "Which agents the agent pickers list" in `docs/product/consoles.md`).

It opens the **handoff dialog**, titled with the session's name, which opens with keyboard focus on its **Hand off**
button, or on **Cancel** while Hand off cannot be pressed. It says that a new session of another agent takes over this session's place and this one is archived, and that
the conversation does not carry over: the new session starts by reading a record of it. Then, under the heading
**Moves to the new session**, it lists what moves:

- the number of branches the session is responsible for, as the daemon's inspection of the session's branches finds
  them when the dialog opens — a project session only, a console session having no branches. While that inspection is
  under way the list says the branches are being checked; when it fails, the list says they could not be checked;
- the number of sessions it owns, archived ones included;
- its place under its owner, named, when it is bound to one.

Only the lines that apply are shown. When none does, the dialog says that nothing moves — the session is responsible
for no branches, owns no sessions and reports to no session; it never says so when the branch inspection failed. While
the handoff request is under way the list stays as it was when it was sent.

Then come the fields:

- **Hand off to** — an agent picker joined to an account picker, as in the session dialog (see the **Agent** field in
  "Opening a session" in `docs/product/sessions.md`), except that the session's own agent is never offered. It opens on
  the agent a new session of the project (or, for a console session, of the console) would use by default (see "Which
  agent a session uses" in `docs/product/sessions.md`) when that is another agent and can be picked (see "Which agents
  the agent pickers list" in `docs/product/consoles.md`), otherwise on the first other agent that can, in the order of
  the added agents, with the account resolved for that agent as for a new session. Used-up accounts are treated as in
  the session dialog's field: listed with a usage bar but not pickable, passed over when the field opens, and, when the
  chosen account is used up all the same, a danger callout at the top of the dialog says why and **Hand off** cannot
  be pressed.
- The collapsed **Advanced** section — the receiving session's model and reasoning effort, showing what a new session of
  the chosen agent and account would inherit, as in the session dialog (see "The Advanced section" in
  `docs/product/model-and-reasoning-effort.md`). Only values the user changed are sent with the request; the rest are
  left for the daemon to resolve.

If, while the dialog is open, no agent other than the session's own can be picked any more — the last one removed or
found unavailable meanwhile — the dialog closes at once, with nothing sent, and an error alert says that no other agent
is available to take the session over; that alert offers nothing but its acknowledgement (see "Recovery and unexpected
errors" in `docs/product/error-alerts.md`).

Pressing **Hand off** sends `hand_off_session`. When it succeeds the dialog closes and the receiving session is
selected. A refusal or failure is shown as an error alert (see `docs/product/error-alerts.md`) and the dialog stays
open for another try. A session in the middle of a turn or still starting (`session_working`) asks the user to wait for
its current turn or operation to finish; a receiving agent that is the session's own (`handoff_to_same_agent`), which
the dialog cannot send, is shown as an unexpected failure.

## Handing off from an owner's tool

`handoff_session` is offered to every session that can own sessions — a console session, an unbound project session and
a lead session — and not to a session with the role of a bound one (see "The console session's tools" and "The unbound
project session's tools" in `docs/product/hub-orchestration.md`). It needs no approval: an owner hands off a session of
its own without the user.

| Argument | Required | Contents |
|---|---|---|
| `session` | yes | The session to hand off: one of the caller's (`yours: true`). |
| `agent` | yes | The receiving agent, `claude`, `codex` or `grok`; not the session's own. |
| `account` | no | The receiving session's account: an account's id or name, or `default` for the agent's default account. It must belong to `agent`. Left out or blank, it comes from the project or console, as for any handoff (see "What can be handed off" above). |
| `model`, `effort` | no | The receiving session's model and reasoning effort. Left out, each comes from the project or console, as for any handoff; blank, it is the agent's own default; any other must be one the agent offers. |
| `note` | no | What the caller tells the receiving session beyond the package, such as where the work stands and what to do first. It is added at the end of the receiving session's opening prompt, introduced as a note from the owner that handed the work off, every line quoted. Blank is the same as left out. |

The receiving session takes the original's place, so it is the caller's, as the original was, and reports to it. The
caller is not sent the message that tells an owner of a handoff (see "Who is told" below), since it made the call; the
receiving session and the sessions that moved are told as for any handoff.

The call returns:

| Field | Contents |
|---|---|
| `session` | The receiving session's id. |
| `agent` | The receiving session's agent. |
| `archived` | The original's id; it is archived. |
| `branches` | The names of the branches that now name the receiving session as their responsible session. `[]` when the branches could not be moved, the reason then being in `note`. |
| `owned_sessions` | The ids of the sessions now bound to the receiving session, archived ones included. |
| `note` | Prose saying the original was archived and the work moved to the receiving session, of which agent, which reports to the caller as the original did and is driven with the usual tools; when the branches could not be moved, also that, why, and that they still name the original. |

Refused, with the reason in prose (as a tool error) and nothing changed:

- a session that is not the caller's — an unbound one, one bound to another session, a console session, or the caller
  itself — with a reason naming who owns it, as for the other tools that act on a session of the caller's;
- a session that stopped being the caller's after the call was checked and before the handoff took hold of it, handed
  to another owner meanwhile;
- an `agent` other than the three, or an `account` that is not one of `agent`'s;
- everything a handoff refuses (see "What can be handed off" above).

What fails after that is answered as in "When a handoff fails" below.

## The handoff package

The package is a Markdown file built by program alone — no model is called — from the original's records, for the
receiving session to read. It holds:

- **the original session**: its title, id, agent and account, kind and project or console, its status when handed off,
  the title of the session it reports to, if any, and where the agent's own conversation record is;
- **the branches it is responsible for** (see "The responsible session" in
  `docs/product/branch-responsible-session.md`), each as git has it at that moment: the worktree it is checked out in
  and whether that worktree has uncommitted changes, its head, and, against the branch it is compared with (as the
  project pane's Git mode compares it), the commits not in that branch (the first 50 listed) and the files changed
  since it forked (the first 200 listed);
- **the sessions it owns**, archived ones included, each with its title, status and id;
- **its latest todo list**, as the agent's last update left it;
- **the files its edits changed**, as its edit and write calls named them;
- **the conversation**: every user message and every reply, in order, each set apart so nothing in it reads as the
  file's own structure. Tool output, file contents read, command output, the model's reasoning, and what the agent
  injects into the conversation on its own are left out. Where the agent compacted its history and rewrote its record
  (Grok Build), the package keeps the summary it wrote and names the files holding the earlier conversation. For a
  Codex thread that had a turn reverted, the conversation is read as it stands across the thread's rollouts: each
  earlier file up to the point the next one continues from, then the head, which is also the record the package
  names (see "Agent session data" in `docs/product/launching-agents.md`). The reverted turns are left out, and so are
  the todo list updates and file changes made in them.

The original's brief is not kept by Octoboard, so it is not carried separately: where the agent recorded it, it is the
first user message of the conversation. A conversation record that is missing or cannot be read, or a reverted Codex
thread missing an earlier rollout the head continues from, gives a package without the conversation, which says why;
it does not stop the handoff. Building the package reads only: nothing is
written to the agent's directories or to the project's repository.

The package is written to `~/.octoboard/handoff/<receiving session id>.md`, readable by its owner alone, since a
conversation can hold secrets. It stays for as long as the receiving session's record does; when it is removed is in
"Files Octoboard owns" in `docs/product/application-lifecycle.md`.

## What the receiving session takes over

The receiving session starts with an opening prompt that names the original, gives the package's path and tells it to
read the package whole before doing anything else and to carry on where the original stopped; it also tells it that
its place is about to move to it, that it will be told what it holds, and not to message the original. It comes up
*working* on that prompt.

Once it is running, it takes the original's whole place:

- **the same kind, project or console, and title** as the original;
- **the same role and owner**: a bound session is bound to the original's owner from its first launch; a lead session
  launches as an unbound project session does and is then bound to the original's console session as its lead session
  (see "Lead sessions" in `docs/product/hub-orchestration.md`); an unbound session stays unbound;
- **every session the original owns**, archived ones included, bound to it instead;
- **the branches the original is responsible for**, which name it as their responsible session from then on;
- **the original's pin**, when it had one.

The binding and the owned sessions move together. Every connected window shows the moves at once. The original is
then archived, last, once nothing is bound to it: it keeps its saved screen as any archived session does, none of the
sessions that moved is archived with it, and **its branches and worktrees are not cleaned up**, since they moved to the
receiving session.

## Who is told

The sessions concerned are told by messages written through the delivery every message takes (see "Messages held
until a session can take them" in `docs/product/message-delivery.md`):

- **each moved session that has a process** is told that its owner has changed, which session took the original's
  place, that it takes work only from it or from the user typing in its terminal and reports to it, that this replaces
  what it was told of its owner when it was started, and that the original is archived;
- **the owner, when there is one and it has a process,** is told that its session was handed off to a session of
  another agent, which now reports to it and which it drives like any session of its own, and that the original is
  archived — unless the owner made the handoff itself with `handoff_session`, whose result tells it instead;
- **the receiving session** is told what it now holds: its owner and how to report to it — for a lead session, also
  that this replaces what its role description says about being unbound, and how its reports archive it — or that it
  has no owner and reports to the user; the branches it is now responsible for, or that they could not be moved and
  still name the original; and each session it now owns, with its status.

A session with no process — interrupted or archived — is told nothing: a relaunch tells it who its owner is.

## Messages for the original during a handoff

From the moment a handoff is accepted, messages for the original are **held**: they are accepted and queued as for a
session that cannot take one right now, and none is written into it, so that none starts a turn on work that is being
moved. The held queue is kept even if the original's process ends meanwhile.

- Once the receiving session has taken the place, everything queued for the original, including what was already
  waiting there before the handoff, is forwarded to the receiving session, in order, before the original is archived.
  So is anything queued for the original after that, until the handoff ends.
- A handoff that fails before the receiving session has started, or before its place has moved, releases the hold: the
  queue is written into the original as usual.
- Forwarded to a receiving session that is being resumed at that moment, the queue is delivered once it is up, and
  goes with it if that launch fails.
- Where the queue ends up with a session that has no process to take it, it is dropped. So is a forwarded message that
  cannot be delivered to the receiving session. Either way every window is sent the `handoff_messages_dropped` notice
  about the session the messages ended up with — the original, or the receiving session — shown as an error alert
  saying that messages held back for a session during a handoff could not be delivered and were dropped, and asking
  the user to open the session they were meant for, or the one that took over, check its terminal, and send again
  anything that did not arrive. These messages were never part-written into a terminal, so the notice says nothing of
  an input line. The exception is a forwarded message lost because the receiving session stopped taking input part way
  through a write: that is reported as any such loss is (see `docs/product/message-delivery.md`), not again as
  `handoff_messages_dropped`.

## When a handoff fails

Every refusal comes before anything is written or started. After that:

- **Writing the package or starting the receiving session fails** — opening it is refused, or its agent does not come
  up: the package is removed again, and the original, its binding, its branches and its sessions are left as they were.
- **Anything that fails once the receiving session is running** — the original's owner no longer able to take a lead
  session when the binding moves, recording the move, or archiving the original — is answered with an error that names
  the receiving session, which is left running. Where the binding had not moved yet, nothing of the original's moved
  and it is not archived; where it had, the place stays with the receiving session and the original may be left
  unarchived.
- **Branches that cannot be moved** do not fail the handoff: they stay with the original, and the receiving session is
  told so.

A handoff that succeeds is answered with the receiving session; `handoff_session`'s answer is in "Handing off from an
owner's tool" above.
