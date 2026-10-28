# Handing a session off to another agent

A **handoff** moves a session's work to a new session of **another agent**, which takes the original's whole place in
Octoboard; the original is archived. The agents' conversation records cannot resume one another, so the conversation
itself does not move: the new session — the **receiving session** — starts by reading a **handoff package** built from
the original's records. Moving a session to another account of the same agent, which keeps the conversation itself, is
switching its account instead (see `docs/product/switching-session-accounts.md`).

A handoff is made with the daemon client protocol's `hand_off_session` request, which names the session, the receiving
agent, and optionally the receiving session's account, model and reasoning effort. In the window it is made from a
session's **Hand off** menu item (see "Handing off from the window" below). An owner hands off a session of its own
with the `handoff_session` tool, once the user approves it unless they have chosen to have handoffs approved without
asking (see "Handing off from an owner's tool" below). All of them run the same handoff,
described in the rest of this document. A handoff is made only when the user or an owner asks for one: Octoboard never
hands a session off on its own, not even off an account whose quota is used up. What it can do on its own, where the
user has turned it on, is switch a session to another account of the same agent (see
`docs/product/automatic-account-switch.md`).

## What can be handed off

A console session or a project session, of either role and with any reporting relationship, can be handed off while it
is waiting for instructions, waiting for the user or interrupted. One working in background counts as waiting for
instructions: it can be handed off, and the work it left running in the background ends with its process (see "Working
in background" in `docs/product/session-statuses.md`). A handoff is refused, with nothing changed, when:

- the session does not exist (`unknown_session`), is archived (`session_archived`; reopen it first), or is in the middle
  of a turn or still starting (`session_working`: a session still at work goes on writing to its worktree while the
  receiving session starts on it);
- it is being started, resumed, switched to another account, handed off already or having its owner changed
  (`session_already_starting`);
- it is locked (`session_locked`; see "Locking a session" in `docs/product/sessions.md`);
- the receiving agent is the session's own (`handoff_to_same_agent`, which points to switching its account);
- the receiving agent is not available (`agent_not_available`), is not added (`agent_not_added`; see "When the agent or
  the account cannot open a session" in `docs/product/opening-sessions.md` for when that applies), the account named is
  not one of that agent's (`unknown_account`), the receiving session's account, once resolved, is turned off
  (`account_disabled`), used up (`account_quota_exhausted`) or not logged in (`account_not_signed_in`; see "When the
  agent or the account cannot open a session" in `docs/product/opening-sessions.md` for all three), or the model or
  reasoning effort is not one the agent offers (`invalid_launch_setting`);
- a change of the sharing of the default configuration runs for the session's account or for the receiving session's
  (`account_sharing_in_progress`; see "When a change is carried out" in
  `docs/product/sharing-the-default-configuration.md`);
- for a session bound to an owner, opening a session bound to that owner would be refused;
- the daemon has begun to shut down (`daemon_shutting_down`; see "Quitting" in
  `docs/product/application-lifecycle.md`).

The model and reasoning effort follow the rules for opening a session (see "Which model and reasoning effort a session
uses" in `docs/product/opening-sessions.md`): left out, they come from the project or console as for any new session of
that agent. The account follows **Automatically select an account** as for any new session (see "Which account a session
uses" in `docs/product/opening-sessions.md`): while that switch is on, the handoff dialog opens on, and an owner's
`handoff_session` that leaves the account out takes, the receiving agent's account whose weekly quota resets soonest.
When the switch is off, or no account ranks, the handoff dialog opens on the account the project or console resolves,
and `handoff_session` takes the owner's own account when the receiving agent is the owner's own and that account can
still open a session, otherwise the project's or console's (see "Handing off from an owner's tool" and "Handing off from
the window" below). A `hand_off_session` request that leaves the account out does not apply the pick: the account comes
from the project, else the console.

From the moment a handoff is accepted until it ends, the original cannot be resumed, switched to another account or
deleted.

## Handing off from the window

A session's action menu offers **Hand off** right after Switch account (where that is offered), before Archive, for a
project session or a console session that is not archived, wherever that menu is offered. It is disabled while the
session is locked, as Archive is, while it is *working* or *starting*, while no agent other than the session's own can
be picked (see "Which agents the agent pickers list" in `docs/product/consoles.md`), while a switch of its account holds
it and while its own account's sharing is being changed; which of these show a reason under the item is in "Session
rows" in `docs/product/sidebar-session-rows.md`.

It opens the **handoff dialog**, titled "Hand off session", which opens with keyboard focus on its **Hand off**
button, or on **Cancel** while Hand off cannot be pressed. Its first sentence names the session to be handed off; it
then says that a new session of another agent takes over this session's place and this one is archived, and that
the conversation does not carry over: the new session starts by reading a record of it. Then, under the heading
**Moves to the new session**, it lists what moves:

- the number of branches the session is responsible for, as the daemon's inspection of the session's branches finds
  them when the dialog opens — a project session only, a console session having no branches. While that inspection is
  under way the list says the branches are being checked;
- the number of sessions it owns, archived ones included;
- its place under its owner, named, when it is bound to one.

Only the lines that apply are shown. When none does, the dialog says that nothing moves — the session is responsible for
no branches, owns no sessions and reports to no session; it never says so when the branch inspection failed. **An
inspection that fails** leaves the dialog usable, as the handoff does not depend on it: a red error mark right after the
**Moves to the new session** heading, named that the branches to move are unknown, and the list says nothing of
branches. The mark's popover says that the branches the session is responsible for could not be checked, with the
reason, what to do, any details and, where trying again can help, **Try again**, which closes the popover and inspects
them again; the mark stays while that runs, and goes once an inspection succeeds, focus then moving to the dialog when
the mark or its popover held it. The dialog's status region says the failure after the announcement of the dialog's
opening focus move, and again each time a retry fails again, in the same words or not; it opens no error alert (see
"Failures, warnings and notes marked at their point" in `docs/product/error-alerts.md`). While the handoff request is
under way the list stays as it was when it was sent.

Then come the fields:

- **Hand off to** — an agent picker joined to an account picker, as in the session dialog (see the **Agent** field in
  "The session dialog" in `docs/product/opening-sessions.md`), except that the session's own agent is never offered. It
  opens on the agent a new session of the project (or, for a console session, of the console) would use by default (see
  "Which agent a session uses" in `docs/product/opening-sessions.md`) when that is another agent and can be picked (see
  "Which agents the agent pickers list" in `docs/product/consoles.md`), otherwise on the first other agent that can, in
  the order of the added agents. The session dialog's rule leaving out an agent whose accounts are all turned off,
  including its fallback when the selected agent's last enabled account is turned off, applies here too. The account
  it opens on for that agent — and for any other agent the user switches to without having picked one of its accounts
  — is the one the new-session dialog would open on: while **Automatically
  select an account** is on, the agent's account whose weekly quota resets soonest; otherwise, or when none ranks, the
  account the project or console resolves (see "Which account a session uses" in `docs/product/opening-sessions.md`).
  The account the field shows is sent as the receiving session's own account choice. Used-up accounts are treated as in
  the session dialog's field: listed with a usage bar but not pickable, passed over when the field opens, and, when the
  chosen account is used up all the same, the field says why under it, marked invalid, and **Hand off** cannot be
  pressed. An account whose sharing of the default configuration is being changed is treated the same way while the
  change runs, and so is an account that is not logged in; a turned-off account is left out unless the field ends on
  it, as in the session dialog.
- The collapsed **Advanced** section — the receiving session's model and reasoning effort, showing what a new session of
  the chosen agent and account would inherit, as in the session dialog (see "The Advanced section" in
  `docs/product/model-and-reasoning-effort.md`). Only values the user changed are sent with the request; the rest are
  left for the daemon to resolve.

If, while the dialog is open, no agent other than the session's own can be picked any more — the last one removed or
found unavailable meanwhile, or every other agent's accounts all turned off — nothing is sent, and the dialog turns
into a failure modal titled that the session cannot be handed off, saying that no other agent is available to take
the session over, with nothing but its
acknowledgement, which closes the dialog (see "The failure modal" in `docs/product/error-alerts.md`).

Pressing **Hand off** sends `hand_off_session`, once the user has confirmed starting on the chosen account when its
last change of sharing is incomplete (see "Starting a session on an account whose last change is incomplete" in
`docs/product/sharing-the-default-configuration.md`). When it succeeds the dialog closes and the receiving session is
selected. A refusal or failure is shown as an error alert (see `docs/product/error-alerts.md`) and the dialog stays
open for another try. A session in the middle of a turn or still starting (`session_working`) asks the user to wait for
its current turn or operation to finish; a receiving agent that is the session's own (`handoff_to_same_agent`), which
the dialog cannot send, is shown as an unexpected failure.

## Handing off from an owner's tool

`handoff_session` is offered to every session that can own sessions — a console session or project coordinator — and not
to a worker (see "The console session's tools" and "Project coordinators' tools" in
`docs/product/hub-orchestration.md`). The user is asked to approve the handoff first (see "The user's approval of an
owner's handoff" below), unless **Automatically approve handoffs** is on, in which case the handoff is made at once (see
"Approving requests automatically" in `docs/product/session-requests.md`).

| Argument | Required | Contents |
|---|---|---|
| `session` | yes | The session to hand off: one of the caller's (`yours: true`). |
| `agent` | yes | The receiving agent, `claude`, `codex` or `grok`; not the session's own. |
| `account` | no | The receiving session's account: an account's id or name, or `default` for the agent's default account. It must belong to `agent`. `default` chooses that account outright and does not fall through to a project pin. Left out or blank, while **Automatically select an account** is on, it is the account of `agent` whose weekly quota resets soonest, among accounts that can still open a session (see "Which account a session uses" in `docs/product/opening-sessions.md`). When the switch is off, or no account ranks, it is the caller's own account when `agent` is the caller's own agent and that account can still open a session; otherwise the project's account for that agent, else the console's. A left-out account is settled when the call is made; where the user is asked, that is the account their form starts on. |
| `title` | no | The receiving session's created name, stored trimmed. Left out or blank, it takes the original's displayed name (see "What the receiving session takes over" below). It is to be written in the user's language (see "Naming the sessions a session starts" in `docs/product/hub-orchestration.md`). |
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
| `account` | The receiving session's account: the account's id, or `default` for the default account. |
| `account_name` | That account's name; for the default account the name the user gave it, else `Default` (see "The default account" in `docs/product/agent-accounts.md`). |
| `model`, `effort` | The receiving session's own choice of model and of reasoning effort: a value, or `null` for the agent's own default; each absent where it inherits it. |
| `archived` | The original's id; it is archived. |
| `branches` | The names of the branches that now name the receiving session as their responsible session. `[]` when the branches could not be moved, the reason then being in `note`. |
| `owned_sessions` | The ids of the sessions now bound to the receiving session, archived ones included. |
| `note` | Prose saying the original was archived and the work moved to the receiving session, of which agent and on which account, which reports to the caller as the original did and is driven with the usual tools; where the user changed the agent or the account asked for, that they did, and each model or reasoning effort the user set — a value, or the agent's own default; when the branches could not be moved, also that, why, and that they still name the original. |

Refused before the user is asked, with the reason in prose (as a tool error) and nothing changed:

- a session that is not the caller's — an independent one, one bound to another session, a console session, or the
  caller itself — with a reason naming who owns it, as for the other tools that act on a session of the caller's;
- an `agent` other than the three, or an `account` that is not one of `agent`'s;
- a session that is archived, in the middle of a turn or still starting, or locked, and a receiving agent that is the
  session's own;
- what opening the receiving session would refuse (see "What can be handed off" above) — an agent that is not available
  or not added, a model or reasoning effort the agent does not offer, a change of sharing running for the receiving
  account, for a reporting session an owner that could not take it — except an account whose quota is used up or that is
  not logged in, which the user can replace in the form.

Refused when the handoff is made — on approval, or at once when the handoff is made without asking — with nothing
changed:

- a session that stopped being the caller's after the call was checked and before the handoff took hold of it, handed
  to another owner meanwhile;
- everything a handoff refuses (see "What can be handed off" above), an account whose quota is used up or that is
  not logged in included.

What fails after that is answered as in "When a handoff fails" below.

## The user's approval of an owner's handoff

While **Automatically approve handoffs** is off, a `handoff_session` call is a session request
(see `docs/product/session-requests.md`): it waits for the user's answer for up to 8 minutes, with the caller's hand
raised meanwhile, and ends as any request ends — approved, refused, timed out, or withdrawn, the agent cancelling the
call included — with an approval's outcome also written into the caller's session (see "Waiting for the answer", "How a
request ends" and "The outcome message" there). A later call for the same session takes the place of the one waiting; a
call for another of the caller's sessions waits beside it.

The confirmation is titled "Hand off a session?", with two buttons, **Refuse** and **Hand off session**. One sentence
names the requesting session, as a request for a session names it (see "The confirmation dialog" in
`docs/product/session-requests.md`), then the session to hand off, by its displayed name, and the agent and the account
the call asked for; it says that the session is archived and its work moves to the new session, and that the user can
change the agent, account, model and reasoning effort before confirming. **Nothing the agent wrote is shown**, neither
the title nor the note it passed. A form follows:

- **Hand off to** — the agent picker joined to an account picker of the handoff dialog (see "Handing off from the
  window" above), the session's own agent never offered, starting on the agent and the account the call asked for.
  Another agent the user switches to, without picking one of its accounts, opens on the account the handoff dialog
  would open on. An account whose quota is used up, that is not logged in, or whose sharing change runs, is treated as
  there: the field says
  why under it, marked invalid, and Confirm cannot be pressed. So is an account turned off while the request waits
  (a call naming one is refused before the user is asked): the field says that it is turned off, and Confirm cannot be
  pressed.
- The collapsed **Advanced** section — the receiving session's model and reasoning effort, starting on those the call
  asked for, else on what a new session of that agent and account would inherit (see "The Advanced section" in
  `docs/product/model-and-reasoning-effort.md`). The values the call asked for belong to the agent it asked for:
  another agent starts on what it would inherit.

**When the agent asked for can no longer be picked, or the account asked for is no longer among its accounts,** both
pickers show nothing chosen and the field says under them, marked invalid, that the agent or account asked for is no
longer available and to choose another; **Hand off session** cannot be pressed, and the Advanced section's fields cannot
be changed, until the user picks.

**Hand off session cannot be pressed either**, and an alert at the top of the dialog says why, while the session cannot
be handed off from its menu: it is locked, it is in the middle of a turn, a switch of its account holds it, its own
account's sharing is being changed, or it is still starting; when several hold, the alert names the first in that
order. Nor while no agent other than the session's own can be picked.
Where the chosen account's last change of sharing is incomplete and no change of it is running, an amber warning mark
inside the account picker of **Hand off to**, named that sharing may be incomplete, says so instead of
asking again on confirming: its popover says that the account's last sharing change did not finish, so its
configuration and conversations may be incomplete (see "Starting a session on an account whose last change is
incomplete" in `docs/product/sharing-the-default-configuration.md`). A mark that appears after the dialog opened is
announced once, as in a request to switch a session's account. While the pickers show nothing chosen, there is no
mark. A window that does not know the session's console shows no form, and so no mark, and its approval is for what the
call asked for.

**On approval the handoff is made with what the form ended on**: the agent and the account it shows, and the model and
reasoning effort the user changed, else, for the agent the call asked for, those it asked for. Everything a handoff
refuses is refused at that moment, and is answered as in "When an approval cannot be carried out" in
`docs/product/session-requests.md`. Approving selects nothing in any window.

## The handoff package

The package is a Markdown file built by program alone — no model is called — from the original's records, for the
receiving session to read. It holds:

- **the original session**: its title, id, agent and account, kind and project or console, its status when handed off,
  the title of the session it reports to, if any, and where the agent's own conversation record is;
- **an earlier handoff**, only when the original itself took another session over through a handoff and the package it
  read then is still there: that package's path, as the place the conversation before the original's own is kept,
  with the instruction to read it, and any earlier package it names in turn. It also says that if that file is gone,
  the original was deleted since and the chain cannot be followed further back. Whether there is one is judged by
  that file existing alone; nothing is read from it, and it is left as it is;
- **the branches it is responsible for** (see "The responsible session" in
  `docs/product/branch-responsible-session.md`), each as git has it at that moment: the worktree it is checked out in
  and whether that worktree has uncommitted changes, its head, and, against the branch it is compared with (as the
  project pane's Git mode compares it), the commits not in that branch (the first 50 listed) and the files changed
  since it forked (the first 200 listed);
- **the sessions it owns**, archived ones included, each with its title, status and id;
- **its latest todo list**, as the agent's last update left it;
- **the files its edits changed**, as its edit and write calls named them;
- **the conversation**: the original's own conversation — every user message and every reply, in order, each set apart
  so nothing in it reads as the file's own structure. Tool output, file contents read, command output, the model's
  reasoning, and what the agent injects into the conversation on its own are left out. Where the agent compacted its
  history and rewrote its record (Grok Build), the package keeps the summary it wrote and names the files holding the
  earlier conversation. For a Codex thread that had a turn reverted, the conversation is read as it stands across the
  thread's rollouts: each earlier file up to the point the next one continues from, then the head, which is also the
  record the package names (see "Agent session data" in `docs/product/launching-agents.md`). The reverted turns are left
  out, and so are the todo list updates and file changes made in them.

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
read the package whole before doing anything else and to carry on where the original stopped — and, when the package
names an earlier handoff package, to read that one too, and so on back along the chain, before carrying on; it also
tells it that
its place is about to move to it, that it will be told what it holds, and not to message the original. It comes up
*working* on that prompt.

Once it is running, it takes the original's whole place:

- **the same kind, project or console** as the original;
- **the original's displayed name as its created name**, unless the owner's `handoff_session` supplied another:
  **task title｜created name**, or the created name alone under "Displayed names" in
  `docs/product/session-titles.md`. This is saved as the receiving session's created name; its own task title is
  independent and, when present, prefixes that name under the same display rules. A later handoff takes the name
  displayed at that time;
- **the same role and parent**: a worker reports to the original's parent from its first launch; a coordinator
  reporting to a console session launches with the independent coordinator description, then has that same console
  parent set, retaining the coordinator role (see "Coordinators with a parent session" in
  `docs/product/hub-orchestration.md`); an independent coordinator stays independent;
- **every session the original owns**, archived ones included, bound to it instead;
- **the branches the original is responsible for**, which name it as their responsible session from then on;
- **the original's pin**, when it had one.

The binding and the owned sessions move together. Every connected window shows the moves at once. The original is
then archived, last, once nothing is bound to it: none of the
sessions that moved is archived with it, and **its branches and worktrees are not cleaned up**, since they moved to the
receiving session.

## Who is told

The sessions concerned are told by messages written through the delivery every message takes (see "Messages held
until a session can take them" in `docs/product/message-delivery.md`):

- **each moved session** is told that its owner has changed, which session took the original's place, that it takes
  work only from it or from the user typing in its terminal and reports to it, that this replaces what it was told of
  its owner when it was started, and that the original is archived; one with no process — interrupted or archived — is
  told of its owner when it next starts instead (see "A change of owner told at the next launch" in
  `docs/product/changing-session-owners.md`);
- **the owner, when there is one and it is not archived,** is told that its session was handed off to a session of
  another agent, which now reports to it and which it drives like any session of its own, and that the original is
  archived — unless the owner made the handoff itself with `handoff_session`, whose result tells it instead. An
  interrupted owner has the message kept for its resume (see "Reports and notices for an interrupted owner" in
  `docs/product/reporting-to-owners.md`);
- **the receiving session** is told what it now holds: its owner and how to report to it — for a coordinator with a
  parent, also that this replaces what its role description says about being independent, and how its reports archive it
  — or that it has no owner and reports to the user; the branches it is now responsible for, each with its target branch
  where one is recorded (see "Target branches and fork points" in `docs/product/revert-gate.md`), or that they could not
  be moved and still name the original; and each session it now owns, with its status.

An archived owner is told nothing.

## Messages for the original during a handoff

From the moment a handoff is accepted, messages for the original are **held**: they are accepted and queued as for a
session that cannot take one right now, and none is written into it, so that none starts a turn on work that is being
moved. The held queue is kept even if the original's process ends meanwhile.

- Once the receiving session has taken the place, everything queued for the original, including what was already
  waiting there before the handoff, is forwarded to the receiving session, in order, before the original is archived.
  What was kept for an interrupted original until its resume — information (see "Information for an interrupted
  session" in `docs/product/information-between-sessions.md`), and for an owner its reports and notices (see "Reports
  and notices for an interrupted owner" in `docs/product/reporting-to-owners.md`) — follows the queue, each piece once,
  and is then no longer kept for the original. A kept report or notice is forwarded as it was kept: it still says it
  was kept while its receiver was interrupted, and names the tools in it as the original's agent calls them.
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
- **Anything that fails once the receiving session is running** — the original's owner no longer able to take a
  coordinator with a parent when the binding moves, recording the move, or archiving the original — is answered with an
  error that names the receiving session, which is left running. Where the binding had not moved yet, nothing of the
  original's moved and it is not archived; where it had, the place stays with the receiving session and the original may
  be left unarchived.
- **Branches that cannot be moved** do not fail the handoff: they stay with the original, and the receiving session is
  told so.

A handoff that succeeds is answered with the receiving session; `handoff_session`'s answer is in "Handing off from an
owner's tool" above.
