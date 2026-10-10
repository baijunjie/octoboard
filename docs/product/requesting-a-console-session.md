# Requesting a console session

An unbound project session can start and drive sessions only in its own project (see "The unbound project session's
tools" in `docs/product/hub-orchestration.md`). When its work needs sessions in other projects of its console, it asks
for a console session with `request_console_session`. The user is asked first; on approval Octoboard starts a console
session in the caller's console and binds the caller to it, which makes the caller a **lead session** (see "Lead
sessions" in `docs/product/hub-orchestration.md`). This is the only way a session opened unbound becomes bound.

**The user's confirmation is enforced by Octoboard, not by a prompt.** Nothing is started or bound on the call alone:
the call waits while the user is asked, and only the user's approval acts on it.

## Who is offered the tool

| Session | `request_console_session` |
|---|---|
| Unbound project session | Offered. |
| Lead session | Offered, since it keeps the unbound project session's tool list, and every call is refused, with the reason in prose: it is already bound to its console session, a session becomes bound only once, and it is to report to that console session and ask it there for what it needs done elsewhere. |
| Project session bound to an owner (a console session, or a project session that started it) | Not offered. |
| Console session | Not offered. |

## The call

| Argument | Required | Contents |
|---|---|---|
| `request` | yes | What the caller needs done that takes sessions in other projects, in natural language. |
| `summary` | yes | What it has done and found out so far that the console session needs to know. |
| `open_items` | no | Strings naming the separate pieces of work it needs done elsewhere, when the work divides into several. |

The call **waits for the user's answer for up to 8 minutes**. The limit is Octoboard's own, the same for every agent,
and lies below the 10 minutes within which Octoboard forwards any tool call from the session to the daemon, leaving
room for an approval given at the last moment to be carried out. No agent's own time limit on a tool call is followed.

**While the call waits, the session reads *waiting for the user*.** The hand goes up as the call starts waiting and
comes down as the request ends — when the user answers, and equally when the request times out or is withdrawn; a
request that takes an earlier one's place continues the same wait. It is the ordinary raised hand throughout: shown
wherever a hand is shown, counted in the rail's waiting count and firing the same system notification as any other
(see "The raised hand" in `docs/product/sessions.md`). What answers it is the dialog, not the session's terminal, so
unlike a hand the agent itself raised it holds nothing back: a message addressed to the session is still accepted
rather than queued (see "Messages held until a session can take them" in `docs/product/hub-orchestration.md`).

Once the request has ended, the session's status follows its own agent again, from wherever the agent was last seen to
be while the hand was up — *working* where it is still in the turn that made the call, *awaiting instructions* where
it has since reported itself at its prompt, or where a Claude Code prompt declined during the wait left it (see
"Declining a Claude Code prompt or question" in `docs/product/sessions.md`). That is also where the session is left
when no further event from the agent arrives at all — as when a request times out after its agent gave up on the call.

Claude Code (observed on 2.1.295) moves an MCP tool call that is still running after 120 seconds to the background:
its turn ends with the call still waiting, and the call's result reaches it later as the background task's
notification. None of that moves the session out of *waiting for the user* while the request is waiting.

## How a request ends

A request is held by the daemon, in memory only, until one of the following happens. Each closes its dialog in every
connected window.

| Ending | What the call returns | What changes |
|---|---|---|
| The user approves | The tool result described in "On approval" below. | A console session is started and the caller bound to it. |
| The user refuses — the **Refuse** button, Escape, the dialog's close button or a click outside it | A tool error in fixed prose saying the user refused the request, nothing was started and the session is still unbound; it ends by telling the agent not to ask again unless the user tells it to, but to carry on with what its project allows and tell the user in the terminal what is left that needs other projects. | Nothing. |
| Nobody answers within the time limit | A tool error of the same shape, saying the user did not respond within the limit. | Nothing. |
| The request is withdrawn | A tool error saying the request was withdrawn before the user answered and nothing was started, where the call is still there to receive it. | Nothing. |

The user is not asked for a reason when refusing.

A request is **withdrawn** on the signals Octoboard sees for every agent alike:

- the caller's process ends — it is interrupted, archived or exits;
- the call's connection to the daemon is dropped;
- the agent cancels the call over MCP. This holds for `request_console_session` only: a cancelled call of any other
  Octoboard tool is left to finish in the daemon, and the agent is told the call was cancelled;
- the same session calls `request_console_session` again: the new request takes the earlier one's place, which is
  withdrawn.

A call made while an approval of the same session's earlier request is being carried out is refused, saying its
outcome will arrive in the session's terminal as a message.

Nothing about a request is persisted. Every agent process ends with the daemon, so a daemon restart leaves no call
waiting.

## On approval

Once the user approves, the dialog closes everywhere at once, and then:

1. **A console session is started in the caller's console exactly as one the user opens by hand**: the console's
   console session agent and account, its default title, and no opening prompt of its own.
2. **The caller is bound to it as its lead session.** It keeps the sessions it had started and goes on driving them
   (see "Lead sessions" in `docs/product/hub-orchestration.md`).
3. **The request reaches the console session as the lead session's first report**, through the ordinary reporting path
   (see "Reporting" in `docs/product/hub-orchestration.md`), queued while the console session cannot take a message —
   which includes the time while its trust confirmation may still be up. Its status is `needs_decision` and its open
   items are the call's `open_items`. Its summary tells the console session that this project session asked for it and
   the user approved, that the session is now bound to it as a lead session which keeps and drives its own sessions
   (readable but not drivable by the console session), and that it is to drive the session like any of its own and
   dispatch the work in the other projects; then the call's `request` and `summary`.
4. **The request counts as the caller's report for its current turn**, so that turn ending without a `report` produces
   no synthesised report (see "When a session does not report" in `docs/product/hub-orchestration.md`).

The call returns:

| Field | Contents |
|---|---|
| `console_session` | The new console session's id. |
| `title` | Its title. |
| `note` | Prose telling the caller that the user approved, which console session was started, that it is now bound to it as its lead session and whether its request was delivered as its first report; that from now on it reports to that console session with `report`, which replaces the instruction in its role description not to call `report`; that `done` with no open items archives it and is refused while a session it started is not archived, while `needs_decision` or open items leave it running; that it keeps driving its own sessions; and not to call `request_console_session` again. |

**The same outcome is also written into the caller's session as a message**, after the result, through the delivery
every message takes (see "Messages held until a session can take them" in `docs/product/hub-orchestration.md`). It
carries the same note, says it is from Octoboard about the `request_console_session` call, and that there is nothing
more to do if the call already returned it. This is how a caller whose agent gave up on the call by a time limit of its
own, without any signal reaching Octoboard, still learns that it is bound. No report is synthesised for the turn this
message starts. On Claude Code that turn is recognised by the prompt it opens with; with Codex and Grok Build, whose
prompts Octoboard does not read, the next turn to start is the one spared a report instead. Nor is a report
synthesised for the turn in which Claude Code delivers the call's result after moving the call to the background, as
described above: the turn that ended when it did names the call's task, and the turn whose prompt is that task's
notification is spared too. The two turns can start in either order.

**When an approval cannot be carried out**, the call returns a tool error saying the user approved but it could not be
carried out, with the reason, and the window that approved shows the reason as a toast about the requesting session:

- the caller's process ended before it could be bound — the same error as an approval after a withdrawal (see "The
  confirmation dialog" below);
- the console session could not be launched — nothing is bound;
- the caller could not be bound after the console session was launched — the new console session is archived, since it
  was started for that binding alone.

Once the caller is bound, the approval stands. If the first report could not be delivered, the note says so, with the
reason, and tells the caller to send it to the console session itself with `report`.

## The confirmation dialog

A plain confirmation titled "Start a console session?", with one sentence naming the requesting session, its project
and the console the console session would be started in, and two buttons: **Refuse** and **Start console session**.
What the agent wrote in its request is not shown in the dialog; it reaches the console session as the first report.

- **One dialog at a time.** Requests are shown oldest first, after any waiting folder-trust prompts (see "The trust
  dialog" in `docs/product/folder-trust.md`). A request already on screen stays there when a trust prompt arrives; the
  trust prompt follows it.
- **Shown in every connected window**, and again whenever the application reloads its state (a connect, a reconnect, or
  catching up after falling behind) for every request still waiting.
- **The first answer from any window wins** and closes the dialog everywhere. The dialog closes as soon as the daemon
  takes the answer, before the console session has started. An answer that arrives after another window's is ignored
  without a message.
- **An approval that comes too late** — the request was withdrawn or timed out — starts nothing, and a toast about the
  session says it is no longer waiting for an answer, so no console session was started. A refusal that comes too late
  shows nothing.
- What the approving window does once the console session has started is in "Approving a request for a console
  session" in `docs/product/focus-mode.md`.
