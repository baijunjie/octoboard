# Session requests

A **session request** is a session's `request_session` call asking the user to start a session that has **no owner**:
it reports to the user, and neither the caller nor the caller's owner drives it (see "Which sessions an owner drives"
in `docs/product/hub-orchestration.md`). It can be a project session in any project of any console, or a console
session. A session the caller wants to drive itself is one its `new_session` starts, which needs no approval; a
request is for a session that should be nobody's, or for one in a place `new_session` does not reach.

**The user's confirmation is enforced by Octoboard, not by a prompt.** Nothing is started on the call alone: the call
waits while the user is asked, and only the user's approval starts anything — or the user's standing approval, where
they have chosen to have session requests approved automatically (see "Approving requests automatically" below).

## Who is offered the tool

Every session is offered `request_session`: a console session, an unbound project session, a lead session, and a
project session bound to an owner. The role description of every project session and the console session's
instruction file describe it.

## Continuing work in another session

The console session's instructions and every project session's role description give the agent these rules:

- A request to hand its current work to the next session, open another session to continue it, or request a peer
  defaults to `request_session`. The peer reports directly to the user. The brief carries progress, remaining work
  and necessary context; `branches` names the branches the caller is responsible for that the peer should take over.
- A request to leave handover notes and finish, or permission to end without asking for a successor, means leaving
  the notes and ending work, without creating or transferring sessions or continuing work in the background.
- `new_session`, where offered, is for a recipient the caller should keep directing and receiving reports from.
  `handoff_session`, where offered, replaces an existing session the caller owns; it does not hand the caller's own
  work to a peer.
- Instructions to persist until completion, permission to act without approval and earlier permission to delegate
  do not override the user's handover intent or request to end. If the agent cannot distinguish a peer request,
  notes followed by ending, and a child session it should direct, it must ask the user before acting.

These are instructions for the agent, not intent detection enforced by Octoboard. The confirmation described above
still controls whether a `request_session` call starts anything.

## The call

| Argument | Required | Contents |
|---|---|---|
| `kind` | yes | `project` for a session in a project, `console` for a console session. No other value is accepted. |
| `project` | no | For `project`: the project, by id in any console, or by a name that is unambiguous among the caller's own console's projects, else among every console's. Defaults to a project session's own project; a console session has to give it. Refused with `kind: console`. |
| `console` | no | For `console`: the console, by id or by an unambiguous name. Defaults to the caller's own console. Refused with `kind: project`, whose session is in its project's console. |
| `brief` | yes | The task, as `new_session`'s brief (see "Handing out a task: the brief" in `docs/product/hub-orchestration.md`). |
| `agent`, `account`, `model`, `effort` | no | Overrides for the new session, exactly as `new_session` takes them (see "The console session's tools" in `docs/product/hub-orchestration.md`); `account` is read among the accounts of the agent the new session will run. |
| `branches` | no | For `project`: local branches, by name, to hand over to the new session (see "Handing branches over" below). Left out, or an empty list, the request hands nothing over and behaves as one without it. |

**A request that could never be carried out is refused before the user is asked**, with the reason in prose and no
dialog shown: whatever `new_session` would refuse for the same brief and overrides — a missing brief or a blank
goal, an unknown agent or account, an account of another agent, an agent determined unavailable on this machine (never
while that determination is still pending), an agent that is not added (see "Opening a session" in
`docs/product/sessions.md`), a model or effort the agent does not offer — and a `kind` that is neither
value, a `project` or `console` given for the other kind, a project or console that does not exist, a name that names
more than one, a console session asking for a project session without naming the project, and `branches` the request
cannot hand over (see "Handing branches over" below).

## Approving requests automatically

The application setting **Automatically approve session requests** (see "Automatically approve session requests" in
`docs/product/settings.md`) is **off by default**. While it is on, a call that passes every check made before the user
is asked is **approved at once**, without asking:

- **No one is asked.** No confirmation dialog is shown in any window, the session does not read *waiting for the user*
  and raises no hand, and no notification is fired. The call does not wait for an answer, so nothing in "Waiting for
  the answer" below applies to it: it returns once the approval has been carried out.
- **The approval is carried out exactly as an approval from the dialog** (see "On approval" below): the session is
  started as `new_session` would start it, with no owner, the branches the request named are handed over, the call
  returns the same result and the same outcome message is written into the caller's session. The only difference in
  what is said is who approved: the paragraph opening the new session's prompt, and the result's note, say the request
  was approved automatically under the user's setting to approve session requests without asking, rather than by the
  user. An automatic approval that cannot be carried out fails as described in "When an approval cannot be carried
  out" below.
- **The setting is read once, when the call is made.** A request already waiting in its dialog when the setting is
  turned on stays there, waiting for the user's answer; turning the setting off does not stop an automatic approval
  already being carried out.
- **The rules on one request per session still hold** (see "How a request ends" below): an automatic request takes
  the place of a request the same session has waiting, which is withdrawn — its dialog closes and the hand comes down —
  and a call made while an approval of the same session's earlier request is being carried out is refused.
- **Carrying the approval out does not depend on the call.** Once the call is approved, cancelling it — the user
  pressing Escape in the agent mid-call, the agent cancelling the call over MCP, or the call's connection to the
  daemon dropping — does not stop it: the session is still started, the branches still handed over, and the outcome
  message still reaches the caller's session.

The tool's description tells the agent that the call waits for the user's answer unless the user has chosen to have
such requests approved automatically, in which case the result says so and the call does not wait.

Only the user changes the setting, in Settings. An agent can neither read nor change it through the settings tools
(see "Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`), so a
session cannot approve its own requests, and a configuration document neither carries nor applies it (see "The
application settings a full export carries" in `docs/product/configuration-export-and-import.md`).

## Waiting for the answer

The call **waits for the user's answer for up to 8 minutes**. The limit is Octoboard's own, the same for every agent,
and lies below the 10 minutes within which Octoboard forwards any tool call from the session to the daemon, leaving
room for an approval given at the last moment to be carried out. Octoboard does not obey an agent's own time limit on a
tool call; where the agent has one, Octoboard sets it above Octoboard's own instead (see below).

**While the call waits, the session reads *waiting for the user*.** The hand goes up as the call starts waiting and
comes down as the request ends — when the user answers, and equally when the request times out or is withdrawn; a
request that takes an earlier one's place continues the same wait. It is the ordinary raised hand throughout: shown
wherever a hand is shown, counted in the rail's waiting count and firing the same system notification as any other
(see "The raised hand" in `docs/product/sessions.md`). What answers it is the dialog, not the session's terminal, so
unlike a hand the agent itself raised it holds nothing back: a message addressed to the session is still accepted
rather than queued (see "Messages held until a session can take them" in `docs/product/message-delivery.md`).

Once the request has ended, the session's status follows its own agent again, from wherever the agent was last seen to
be while the hand was up — *working* where it is still in the turn that made the call, *awaiting instructions* where
it has since reported itself at its prompt, or where a Claude Code prompt declined during the wait left it (see
"Declining a Claude Code prompt or question" in `docs/product/sessions.md`). That is also where the session is left
when no further event from the agent arrives at all — as when a request times out after its agent gave up on the call.

Claude Code (observed on 2.1.295) moves an MCP tool call that is still running after 120 seconds to the background:
its turn ends with the call still waiting, and the call's result reaches it later as the background task's
notification. None of that moves the session out of *waiting for the user* while the request is waiting.

**Codex and Grok Build each have a time limit of their own on an MCP tool call, and for a session Octoboard starts
that limit is set above Octoboard's own when the session launches**, so the call is never what gives up first (see
"Per-agent specifics a user will notice" in `docs/product/launching-agents.md`; for each agent's own default see
`docs/agent-cli-reference.md`). It is set because the two agents end an abandoned call differently, and only one of
the two endings leaves the request alive:

- **Codex sends nothing at all** when it gives up — no cancellation, no end of the connection, no frame of any kind.
  Octoboard is left unaware, and nothing about the request changes: the dialog stays up, the request goes on waiting,
  and an approval given after that point is still carried out in full. All the caller loses is the call's result,
  which the outcome written into its session as a message covers (see "The outcome message" below).
- **Grok Build cancels the call over MCP**, which withdraws the request like any other cancellation (see "How a request
  ends" below): the dialog closes with nothing started, and an approval given after that point starts nothing. The
  limit Octoboard sets is what keeps Octoboard's 8 minutes, and not Grok Build's own limit, deciding when the wait
  ends.

**A Codex session is not held silent while its call waits.** For as long as the request is waiting, that session goes
on taking short turns of its own, one after another at its own interval, each saying only that the tool is still
running. So a request left on the dialog until the 8 minutes run out costs that session several such turns — tokens on
its account, and activity in its terminal — and none of them answers the request or shortens the wait (the interval,
and what was measured, are in "Codex" in `docs/agent-cli-reference.md`).

**A Grok Build session is held silent.** Between the turn that makes the call and the turn that carries its outcome it
takes no turns of its own and spends no tokens, however long the request waits, so a request left on the dialog until
the 8 minutes run out costs that session nothing.

## How a request ends

A request is held by the daemon, in memory only, until one of the following happens. Each closes its dialog in every
connected window.

| Ending | What the call returns | What changes |
|---|---|---|
| The user approves | The result described under "On approval" below. | The session is started, as described there. |
| The user refuses — the **Refuse** button, Escape or the dialog's close button | A tool error in fixed prose saying the user refused the request and nothing was started; it ends by telling the agent not to ask again unless the user tells it to, to tell the user in its terminal what it wanted the session for, and to follow the user's direction on continuing its own work or leaving handover notes and ending. | Nothing. |
| Nobody answers within the time limit | A tool error of the same shape, saying the user did not respond within the limit. | Nothing. |
| The request is withdrawn | A tool error saying the request was withdrawn before the user answered and nothing was started, where the call is still there to receive it. | Nothing. |

The user is not asked for a reason when refusing.

A request is **withdrawn** on the signals Octoboard sees for every agent alike:

- the caller's process ends — it is interrupted, archived, deleted or exits;
- the call's connection to the daemon is dropped;
- the agent cancels the call over MCP, which gets it a tool error saying the call was cancelled. This holds for
  `request_session` only: a cancelled call of any other Octoboard tool is left to finish in the daemon;
- the same session makes another request.

**A session has at most one request waiting.** A new request takes the place of the one the session already has
waiting, which is withdrawn; the hand stays up through the change.

A request made while an approval of the same session's earlier request — from the dialog or automatic — is being
carried out is refused, saying the earlier request has just been approved and is being carried out, and that its
outcome will arrive in the session's terminal as a message.

Nothing about a request is persisted. Every agent process ends with the daemon, so a daemon restart leaves no call
waiting.

## On approval

Once the user approves, the dialog closes everywhere at once, and then — as for an automatic approval, which has no
dialog (see "Approving requests automatically" above):

1. **The session is started where the request named, exactly as `new_session` would start it** with the same brief
   and overrides: titled from its goal, the overrides applied, listed as *starting* until its launch settles, and its
   folder-trust confirmation handled the same way (see "Handing out a task: the brief" in
   `docs/product/hub-orchestration.md`). A console session is started in the named console, with that console's badge
   colour assigned as for any console session. The approval is answered only once the launch has settled, with the
   launch's own refusal when it fails (see "Opening a session" in `docs/product/sessions.md`).
2. **It has no owner.** A project session is an ordinary unbound project session, which the user owns and which can
   start and drive sessions of its own project (see "The unbound project session's tools" in
   `docs/product/hub-orchestration.md`); a console session is an ordinary console session of its console.
3. **Its opening prompt says who asked for it.** The brief is preceded by a paragraph naming the requesting session by
   id and title, saying the session was started at that session's request with the user's approval — or, for an
   automatic approval, automatically under the user's setting to approve session requests without asking — that
   nobody owns it but the user, and that the requesting session is not its owner and the two can only share information with each
   other. Where the request named branches, the paragraph also says the requesting session handed it those branches,
   naming each, with the same approval, that it is now the session responsible for them and is to carry on the work
   on them. The paragraph is written when the request is made, so it names every branch the request named, whatever
   the move then finds.
4. **The branches the request named are handed to it** once it has started (see "Handing branches over" below).
5. **The approval does not archive the caller.** It keeps running, and the approval does not count as a report.
   The result tells the caller to follow the user's direction on continuing its own work or ending after the
   handover. It does not own the new session, so `send_message`, `archive_session`, `reopen_session` and the other
   tools that act on a session of the caller's own refuse it; `share_info` reaches it (see
   `docs/product/information-between-sessions.md`), and `get_session` reads it where the caller has that tool.

The call returns:

| Field | Contents |
|---|---|
| `session` | The new session's id. |
| `role` | `project` or `console`. |
| `agent` | The agent it runs. |
| `title` | Its title. |
| `console` | The console it is in. |
| `project` | The project it is in, or null for a console session. |
| `note` | Prose telling the caller that the user approved — or, for an automatic approval, that Octoboard approved the request automatically because the user has chosen to have session requests approved without asking — and which session was started — its kind, id, title, agent and project or console; that it has no owner and reports to the user, not to the caller; that it is not the caller's to drive and the caller should follow the user's direction on continuing its own work or ending after the handover; where the request named branches, what became of them (see "Handing branches over" below); that `share_info` tells it something that touches its work, and, where the caller has `get_session`, that it reads it. Where the caller is unbound, has `bind_to_session`, and could be bound to the new session under the rules a binding keeps, the note adds that `bind_to_session` with the new session as `owner` makes the caller one of its sessions — the way a project session comes to be driven by a console session it asked for (see "Lead sessions" in `docs/product/hub-orchestration.md`). |

**No window follows the new session.** Approving selects nothing and changes no window's view or focus mode.

## Handing branches over

A request for a project session can hand the new session branches the requester is the **responsible session** of
(see "The responsible session" in `docs/product/branch-responsible-session.md`), so that the new session carries on
the work on them. Only the branches named in `branches` are handed over; nothing is inferred, so a request without
them hands over none, whichever branch the requester has checked out. The tool's description tells the agent to pass
`branches` when it is handing over the work on a branch it is responsible for, and to leave it out when it is only
working on whatever branch is checked out. A project session's `new_session` takes the same `branches`, for a session
the caller owns, without the user's approval (see "Handing branches to a session the caller starts" in
`docs/product/branch-responsible-session.md`).

**What is refused before the user is asked**, with the reason in prose and no dialog shown, as any other refusal:

- `branches` that is not a list of strings, or that holds a blank name;
- any branch named for `kind: console`;
- branches named for a project that is not in a git repository;
- a branch that is not the requester's to hand over in the repository of the project the new session would be in —
  it does not exist there, the requester's own git did not report creating it and it was not handed to it, or another
  session is responsible for it now. These are one refusal, which names the first such branch and says that only a
  branch the requester is responsible for can be handed over. A branch deleted and created again under the same name
  is another branch, which the requester was never responsible for.

Each name is taken with surrounding blanks trimmed, and a name given more than once is one branch. The records belong
to the repository, so a branch can be handed to a session of any project in the repository it was recorded in. In a
repository whose references are kept in reftable format, a record cannot be held against its branch's birth (see
"Where a branch has no responsible session" in `docs/product/branch-responsible-session.md`), so there a branch
deleted since the Git mode last listed the branches still passes the check, and so does one deleted and created again
under the same name while the first one's record stands. Elsewhere, a branch renamed since the Git mode last listed
the branches still has its record under its old name, so handing it over is refused, by either name, until the Git
mode lists the branches again and the record moves to the new name.

**Nothing moves until the request is approved.** A request refused, timed out or withdrawn leaves every branch with the
requester, and so does an approval that cannot be carried out.

**On approval, the session is started first, and only then do the branches move** to it: each moves only where the
requester is still its responsible session at that moment. The new session is then responsible for the branches that
moved, from the moment of the move, and the requester no longer is. A branch whose responsible session changed in the
meantime, or whose record has gone, is not handed over and stays as it was. A branch that moved carries the new
session's responsible-session badge in the Git mode from then on (see "The responsible-session badge" in
`docs/product/branch-responsible-session.md`).

**A move that cannot be recorded does not fail the approval**: the session is running by then, the call returns the
approval's result as usual, and the requester stays responsible for every branch it named.

The call's note, and the outcome message that repeats it (see "The outcome message" below), say what became of the
branches: those handed over, each named, now the new session's responsibility and no longer the caller's; those not
handed over, each named, because by then the caller was no longer responsible for them or they were gone, staying as
they were; or, where the move could not be recorded, that the handover of the branches named could not be recorded and
the caller is still responsible for them.

## The outcome message

**An approval's outcome is also written into the caller's session as a message**, after the call's result, through
the delivery every message takes (see "Messages held until a session can take them" in
`docs/product/message-delivery.md`). It carries the same note as the result, says it is from Octoboard about the
`request_session` call, and that there is nothing more to do if the call already returned it. This is how a caller
whose agent gave up on the call by a time limit of its own, without any signal reaching Octoboard, still learns what
came of its request (see "Waiting for the answer" above).

No report is synthesised for the turn this message starts (see "When a session does not report" in
`docs/product/reporting-to-owners.md`). On Claude Code that turn is recognised by the prompt it opens with; with Codex
and Grok Build, whose prompts Octoboard does not read, the next turn to start is the one spared a report instead.

The turn in which Claude Code delivers the call's result after moving the call to the background, as described above,
is not spared: that turn is reported on as any other, whether the caller continues work or ends after the handover.

## When an approval cannot be carried out

When an approval cannot be carried out, nothing is started and no branch is handed over, the call returns a tool
error saying the user approved but it could not be carried out, with the reason, and the window that approved shows
the reason in an error alert about the requesting session. For an automatic approval the error says instead that the
request was approved automatically but could not be carried out, and no window shows an alert. It is for one of these
reasons:

- the caller's process ended before the session could be started, which gives the same error as an approval after a
  withdrawal (see "The confirmation dialog" below);
- the session could not be launched — including any refusal opening it meets at that moment, such as an agent that has
  become unavailable, or been removed from the added agents, meanwhile.

## The confirmation dialog

A plain confirmation titled "Start a session?", with two buttons, **Refuse** and **Start session**.

- **One sentence says who asks and for what.** It names the requesting session by its displayed name (see
  "Displayed names" in `docs/product/session-titles.md`; its id where the window does not know the session) and where
  that session lives — its project for a project session, plus its console only where that is not the console the new
  session would be in, since a request can come from any project or console; a requester in the very project the new
  session would be in, or one the window does not know, is named without either; then
  the agent the new session would run and where it would be — a project and that project's console, or, for a console
  session, the console alone; and it ends saying the new session would report to the user. The console is so named
  once where both sessions share it. A project or console the window does not know is named by its id.
- **Where the request hands branches over, a second sentence names them**, saying they would be handed over to the new
  session. Each branch is shown as code, and they are joined as the window's language joins a list. With none, the
  dialog says only the first sentence.
- **Nothing the agent wrote is shown**, its brief included: the dialog is the one place the user's approval is
  enforced, so it says only what Octoboard knows. The names of sessions, projects and consoles in the sentence are the
  ones the user gave, each shown as a name in the ordinary font rather than in quotation marks; the branches it names
  were each checked to be one the requester is responsible for before the user was asked. What the agent wants the
  session for is for the requesting session to say, in its own terminal.
- The text wraps anywhere, so a long unbroken title or branch name does not widen the dialog.

It behaves as follows:

- **Keyboard focus starts on the dialog itself** rather than on a button, for each request it shows, so Enter answers
  nothing until the user moves focus onto a control (see "Where keyboard focus starts in a confirmation" in
  `docs/product/window-layout.md`).
- **One dialog at a time.** Requests wait in one line, shown oldest first by when they were made, after any waiting
  folder-trust prompts (see "The trust dialog" in `docs/product/folder-trust.md`). A request already on screen stays
  there while it is waiting, whatever arrives — a trust prompt, or a request that sorts before it — and what arrived
  follows it.
- **Shown in every connected window**, and again whenever the application reloads its state (a connect, a reconnect, or
  catching up after falling behind) for every request still waiting.
- **The first answer from any window wins** and closes the dialog everywhere. The dialog closes as soon as the daemon
  takes the answer, before the session has started. An answer that arrives after another window's is ignored without
  a message.
- **An approval that comes too late** — the request was withdrawn or timed out — starts nothing, and an error alert about the
  requesting session says it is no longer waiting for an answer, so the session it asked for was not started. A
  refusal that comes too late shows nothing.
