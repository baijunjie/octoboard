# Switching a session's account

A session can be moved to another account of its own agent from its row's **Switch account** submenu (see "Session
rows" in `docs/product/sidebar-session-rows.md`); a console session is switched from its own row, among that session's
own agent's accounts. The submenu is offered only when the agent has more than one account, and the user is asked to
confirm. An owner moves a session of its own with the `switch_session_account` tool (see "Switching from an owner's
tool" below). Both run the same switch, described from "What a switch does" on. A switch is made only when the user or
an owner asks for one: Octoboard never switches a session on its own, not even off an account whose quota is used up.
A session cannot be switched onto a used-up account (see "A used-up account" in `docs/product/account-quotas.md`).

## Confirming a switch

The confirmation asks which sessions to switch, as two options of a radio group; the first is selected by default:

- **This session and its sub-sessions** — the session and every session under it: those bound to it, those bound to
  them, and so on to any depth, reached through archived sessions as well.
- **Every session in this console** — every session of the same agent in that console: console sessions,
  project sessions and bound sessions, across all projects in the console.

In both, only sessions of the session's own agent are switched, and archived sessions, sessions already on the target
account and sessions already being switched (see "While a session is switching" below) are left out. Each option shows
how many sessions it would switch, kept up to date while the confirmation is open, and Confirm is disabled while the
chosen option has none. Neither choice changes the console's or projects' account defaults for future sessions.

When the target account is used up (see "A used-up account" in `docs/product/account-quotas.md`), a danger callout at
the top of the confirmation names the account, the quota it has used up and when it resets, and Confirm is disabled
until the account is no longer used up. The submenu already disables such an account (see "Session rows" in
`docs/product/sidebar-session-rows.md`).

When the target account's last change of sharing is incomplete, Confirm first asks the user whether to start on it,
and nothing is switched when that is cancelled (see "Starting a session on an account whose last change is incomplete"
in `docs/product/sharing-the-default-configuration.md`).

Confirming closes the confirmation at once, and the switches go on in the background of the window; there is no progress
display. The set of sessions is fixed at that moment: sessions opened afterwards do not join it, and a session that a
switch started earlier in the same window has not finished with is not added to it. Up to four sessions are switched at
the same time and the rest wait for a turn; one that has been removed, archived or moved to the target account before
its turn is skipped. Every session of the set shows as switching its account (see "While a session is switching" below)
from confirming until its own turn is over, the waiting ones included. A failure does not stop the remaining sessions:
when the whole set is done, one error alert reports every failure of the set (see "The alert for failed switches"
below). When the whole set is done without a failure and at least one session was switched, the window announces to
assistive technology how many sessions were switched and to which account; a set with failures is told by its error
alert alone.

## The alert for failed switches

The failures of one confirmed set are reported together, in one error alert (see "Recovery and unexpected errors" in
`docs/product/error-alerts.md`):

- **The title** says how many sessions could not move, and to which account, the account's name as a name chip. One
  sentence after it says, once for all of them, that they stay on their current account.
- **One row per failed session**: its displayed name and only the short reason, a path in it as a reference chip. A
  conversation that could not be moved is worded for where it failed: the copy did not complete; the copy was made but
  the target's record of the Codex thread could not be updated; or the copy did not complete and an older item could
  not be put back, naming the folder it is kept in until the daemon next starts or the session is next switched into
  that account (see "Carrying the conversation" below). An unexpected failure with no sentence of its own reads only
  that it failed unexpectedly, and any other failure with none reads that something unexpected went wrong; neither
  shows the system's own text, which is in **Error details** (see "Failures shown on the screen" in
  `docs/product/error-alerts.md`).
- **Remedies after the list, each said once**: a failure whose code has a remedy sentence of its own gets it once for
  that code, a login shell environment that could not be read in time included; the other failures with a known
  remedy share one sentence per kind of remedy. A conversation that could not be moved has the remedy of its cause,
  or two of them (see "When a switch fails" below), and counts as unexpected when its cause is none the window knows.
- **When any failure is unexpected**, the alert's own paragraph asking for a screenshot and an issue report follows,
  with **Report issue** in the footer (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`).
- **Error details** holds, under a heading naming each session, what says more than its row: the system's own text
  of a conversation that could not be moved; for a login shell environment that could not be read in time, the
  command that was run and how long it was given; or, for a failure with no sentence of its own, its message,
  followed by its stack when it is an unexpected error of the window's own. A session whose row says it all has no
  entry there, and when no session has one the alert has no **Error details**, even when a failure is unexpected.
- **The footer** is the alert's usual one, with no retry: the acknowledgement button, **Report issue** when a failure
  is unexpected, and a remedy's button when every failure shares that remedy. The user switches a failed session again
  from its menu; a session that did switch is on the target account already.

## While a session is switching

From the moment a switch takes hold of a session until it lets go, the session is shown as *switching account* in
every window, whether the user's confirmation or an owner's `switch_session_account` started it (the session's
`switching_account` field in the daemon protocol). In the window that confirmed a set, a session still waiting for its
turn is shown the same way, though no switch holds it yet; other windows show it as it is until its turn comes.

While a session is shown as switching:

- its sidebar row, focus mode card, breadcrumb and its entry in a confirmation's list of sessions show the *switching
  account* status in place of its own (see "Session statuses" in `docs/product/sessions.md`);
- its terminal is covered by the loading cover with a spinner, saying that the session is switching its account,
  whatever state its connection passes through meanwhile, and takes no input: no key reaches the session's process.
  The terminal offers no Resume. When the session stops being shown as switching, the cover gives way to whatever the
  terminal would show otherwise, and keyboard focus that was in the terminal when the cover came goes back to it,
  unless it has been put somewhere else meanwhile;
- selecting it does not resume it, though it reads *interrupted* while its process is down;
- archiving it asks for confirmation first;
- its row's menu disables the other accounts of **Switch account**, **Hand off** and **Unbind** (see "Session rows" in
  `docs/product/sidebar-session-rows.md`).

## Switching from an owner's tool

`switch_session_account` is offered to every session that can own sessions — a console session, an unbound project
session and a lead session — and not to a session with the role of a bound one (see "The console session's tools" and
"The unbound project session's tools" in `docs/product/hub-orchestration.md`). It needs no approval: an owner switches
a session of its own without the user. It switches the one session it names; it has no counterpart of the
confirmation's choices of several sessions.

| Argument | Required | Contents |
|---|---|---|
| `session` | yes | The session to switch: one of the caller's (`yours: true`). |
| `account` | yes | The account to move it to: an account's id, an account's name (matched ignoring case and surrounding spaces), or `default` for the agent's default account. It is looked up among the accounts of the session's own agent. |

The session is sent no message about the switch, and neither is the caller, which has the result instead. The session
comes back *awaiting instructions* like any switched session; a turn it was in the middle of when its process was ended
is cut off, so the caller sends it its next step with `send_message`. A session with no conversation yet (see "Carrying
the conversation" below) starts a fresh one on the new account. The call is answered once the switch's outcome is
known, which can take a few seconds (see "What a switch does" below).

The call returns:

| Field | Contents |
|---|---|
| `session` | The switched session's id. |
| `agent` | Its agent. |
| `account` | The account it is now on: the account's id, or `default` for the default account. |
| `account_name` | That account's name; for the default account the name the user gave it, else `Default` (see "The default account" in `docs/product/agent-accounts.md`). |
| `note` | Prose naming the account and agent the session is now on, saying that it is back awaiting instructions with any turn in progress cut off and is to be sent its next step with `send_message`, and, for a session that had no conversation yet, that it starts fresh there. |

Refused, with the reason in prose (as a tool error) and nothing changed:

- a session that is not the caller's — an unbound one, one bound to another session, a console session, or the caller
  itself — with a reason naming who owns it, as for the other tools that act on a session of the caller's;
- a session that stopped being the caller's after the call was checked and before the switch took hold of it, handed
  to another owner meanwhile;
- an `account` that is not one of the session's agent's, an account of another agent included;
- everything a switch refuses (see "When a switch fails" below), such as an archived session or the account the
  session is on already.

What fails after that is answered as in "When a switch fails" below, as a tool error.

## What a switch does

A switch ends the session's process the way archiving does, copies the session's conversation into the target
account's config directory, records the new account and directory on the session, and relaunches it as a resume does,
with everything Octoboard injects reassembled. It archives nothing: while the process is down the session reads as
*interrupted*, and it comes back in *awaiting instructions* like any resumed session. Ending an owner's process this
way does not itself switch or archive the sessions bound to it; a confirmation whose set takes them in switches each of
them separately.

- **The session reads the target account's configuration and keeps its explicit model and reasoning effort choices.**
  An account's directory holds the agent's global configuration as well as its login. Settings kept only in the old
  account's directory do not follow the session; values explicitly supplied by Octoboard still apply according to
  `docs/product/model-and-reasoning-effort.md`. The same conversation can come up with different account defaults.
- **It can take a few seconds**, the time the agent is given to exit and the time the relaunched process is watched; the
  session shows as switching its account until the result is known (see "While a session is switching" above), and
  cannot be resumed or switched again meanwhile.

## Carrying the conversation

**The conversation continues** where the agent has one stored: it is copied — added to the target directory, or
replacing an earlier copy there — and the original stays in the account it came from, so a failed switch loses
nothing and switching back needs no second copy. A session nobody ever typed into, unless it was opened to resume a
conversation, has no conversation to copy; the switch records the account and relaunches into a fresh conversation,
as a resume of such a session does. The default account is a target like any other: the directory it resolves to at
that moment is where the conversation is copied, and the relaunch runs against that same resolution.

What is copied for each agent, each item to the same path relative to the account's directory as in the account it
came from:

| Agent | What is copied |
|---|---|
| Claude Code | the session's transcript (`projects/<project slug>/<session id>.jsonl`), and, where they exist, the directory of the same name beside it (`projects/<project slug>/<session id>/`: subagent transcripts, tool results, the conversation's custom title) and the session's rewind checkpoints (`file-history/<session id>/`) |
| Codex | the session's rollouts, as they are, compressed or not: the first rollout and, for a thread that had a turn reverted, every later history segment (see "Agent session data" in `docs/product/launching-agents.md`); Codex decompresses the compressed ones it reads when it resumes |
| Grok Build | the session's directory, whole |

- **A Codex thread is carried whole**: its first rollout and every history segment, the head among them (see "Agent
  session data" in `docs/product/launching-agents.md`). The head is located once the session's process has ended, so
  a segment started while it exited is the one carried as the head. An account given the first rollout alone would
  resume the conversation as it was before the revert, with no warning, and one given the head alone would refuse it
  (measured with codex-cli 0.162.1).
- **The target's own record of the thread is pointed at the head.** Codex keeps a database of its threads in each
  account (`state_<n>.sqlite`) and resumes the rollout that database names, so an account that held the thread before
  (the session was switched there, then reverted in another account) would follow its row to an older branch however
  whole the copied chain is, with no sign of it (measured with codex-cli 0.162.1). Once the copy is in place,
  Octoboard sets that thread's `rollout_path`, in every such database of the target that has a thread table, to the
  head's path in the target (without a `.zst` ending, the form Codex keeps for a compressed rollout), where the row
  names another file. Nothing else is changed: the row's other columns and the rows other tables keep of the thread
  are left as they are, so the older branch's title and preview stay on the row. An account with no row of the
  thread, a row that names no path, no database, or none with a thread table has nothing to point.
  - **Whether the row names another file** is decided by where the two paths lead, not by their text: the
    directories along each are resolved, links included, and the file names compared. A row that reaches the head by
    another route — through another account's directory that links to the same `sessions/`, for one — is left as it
    is. A row that leads to another file is pointed even when the accounts keep one database between them (the same
    `CODEX_SQLITE_HOME`, say) but not one `sessions/`; that database's row then names the target's copy.
  - **Where the database is**: the directory `sqlite_home` names in the target's `config.toml` (a relative one taken
    against the account's directory, a leading `~` as the home directory in the user's shell environment), else
    `CODEX_SQLITE_HOME` in that environment, else the account's directory. A directory that does not exist holds no
    database.
  - **Which entries are databases**: those of that directory named `state_<n>.sqlite` that are files, or symbolic
    links to files. One named so that is a link to nothing, or to something other than a file, is not a database.
    No other entry of the directory is looked at, so an unrelated link there that cannot be followed does not affect
    the switch.
  - **Rather than risk resuming the older branch unnoticed, the switch fails** (see "When a switch fails" below) when
    where the database is cannot be determined (a `config.toml` that cannot be read or parsed, a `sqlite_home` that
    starts with `~` while the user's shell environment has no `HOME`, a relative `CODEX_SQLITE_HOME`, a directory that
    cannot be listed), when an entry named `state_<n>.sqlite` cannot be
    inspected, or when a database cannot be opened or written to, or stays busy for 5 seconds. The conversation is
    already copied by then and stays in the target, and the failure says so.

  This is the one thing a switch writes to an agent's own state besides the copied conversation.
- **Codex subagents' rollouts stay behind.** A subagent the session spawned is a thread of its own, with a rollout
  named by its own id, which never contains the session's. Its conversation remains in the account the session came
  from.
- **Nothing else is copied**: no index, registry or per-project entry of the agent's, and none of its settings. What
  the agent keeps beside the record is gathered only once the session's process has ended, so what it wrote while
  exiting comes along.
- **The items are copied all together or not at all.** Each is first copied into a transient `.octoboard-switch`
  directory at the root of the target account's directory and checked to hold every file of its original at the
  same size, and only then moved into place, setting aside what it replaces. If any item cannot be copied — one that
  contains a symbolic link cannot — or cannot be put in place, the items already placed are removed and the earlier
  copies set aside are put back, so the target holds either all of the new items or what it held before, apart from
  empty directories the attempt may have created on the way; the switch then fails (see "When a switch fails" below).
  The staging directory is removed again either way. Should an earlier copy fail to be put back, it is left in the
  staging directory and the failure says where; it stays there only until Octoboard's daemon next starts or the
  session is next switched into that account, either of which clears it.
- **An item that is the same file in both accounts is skipped**, item by item: its path in the account it came from
  and its path in the target are compared by where each resolves, links included, so when both accounts' directories
  holding it link to the same place, it is left as it is and not copied. The other items are copied as above; when
  every item is skipped, nothing is copied and no staging directory is made.

## When a switch fails

**A switch that cannot be made leaves the session on the account it had, and says why.** It is refused, with nothing
recorded, when the session is being launched, resumed, handed off, switched already or having its owner changed at
that moment, when it is archived, when it is on that account already, when the target account is used up by the
reading the daemon holds at that moment (`account_quota_exhausted`, carrying what "Opening a session" in
`docs/product/sessions.md` describes for it), when the target is a Grok Build directory that is not an initialized Grok home, when the user's
shell environment could not be captured — which is read whenever either side of the switch is the default account,
since that is what resolves its directory, and for any Codex switch, since that environment says where the target
keeps its database (see "The launch environment" in `docs/product/launching-agents.md`) — when the conversation
record is not where it should be, and while a change of the sharing of the default configuration runs for the
session's account or for the target (`account_sharing_in_progress`; see "When a change is carried out" in
`docs/product/sharing-the-default-configuration.md`); the session's process is then left running. The one further
refusal comes after the process has been asked to end: a process that has not gone within 30 seconds stops the switch,
with nothing copied and nothing recorded, and that process may still be running. Once the process has been ended, a
change of the target's sharing that started meanwhile, a copy that
does not complete (a Codex account's database that cannot be updated included), a relaunch the launch rules refuse
and a relaunch whose process ends at once are each reported as a failed switch, not as a success: the session is left
*interrupted* on its old account, resumable as before, and already reads so everywhere by the time the failure is
shown. Octoboard tells that last case from a success by the process ending within a few seconds of the relaunch,
never by reading what the agent printed. An account that is not signed in is therefore not a failed switch: every
agent, Grok Build included, comes up on its own sign-in rather than exiting, so the switch succeeds and the session's
terminal shows that agent's sign-in, which may start its sign-in flow.

**A conversation that could not be moved says why**, as one of these causes, each of which the window answers with
a remedy for it (see "The alert for failed switches" above):

| Cause | When |
|---|---|
| Not allowed | A file or directory could not be read or written, or the volume is read-only. |
| In use | A file is in use, or a Codex thread database stayed busy for the 5 seconds waited. |
| No space | The volume, or a quota, is full. |
| Unreadable `config.toml` | The target's `config.toml` cannot be read for another reason than those above, or is not valid TOML. |
| Database location unknown | Where the target keeps its thread database cannot be told: a relative `CODEX_SQLITE_HOME`, or a `sqlite_home` starting with `~` with no `HOME`. |
| Symbolic link | An item to copy contains a symbolic link, which is never copied. |
| Not put back | The copy failed and an older item could not be put back (see "Carrying the conversation" above). |

Any other cause is shown as an unexpected failure. Where the system's own text of the cause names an operating-system
error the window recognises, that error's remedy is given instead, except when an older item could not be put back:
the remedy for that item, which is lost once the staging directory is cleared, is kept, and the operating-system
error's remedy is given as well.
