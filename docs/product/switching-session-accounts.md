# Switching a session's account

A session can be moved to another account of its own agent from its row's **Switch account** submenu (see "Session
rows" in `docs/product/sidebar.md`); a console session is switched from its own row, among that session's own agent's
accounts. The submenu is offered only when the agent has more than one account, and the user is asked
to confirm. An owner moves a session of its own with the `switch_session_account` tool (see "Switching from an owner's
tool" below). Both run the same switch, described from "What a switch does" on. A switch is made only when the user or
an owner asks for one: Octoboard never switches a session on its own, not even off an account whose quota is used up.
A session cannot be switched onto a used-up account (see "A used-up account" in `docs/product/account-quotas.md`).

## Confirming a switch

The confirmation has an **unchecked by default** checkbox to switch every session of the same agent in that console
to the selected account. It includes console sessions, project sessions and bound sessions, across all projects in
the console; archived sessions and sessions already on the target account are excluded. The confirmation shows how
many sessions are eligible. Leaving the checkbox unchecked switches only the selected session. Neither choice
changes the console's or projects' account defaults for future sessions.

When the target account is used up (see "A used-up account" in `docs/product/account-quotas.md`), a danger callout at
the top of the confirmation names the account, the quota it has used up and when it resets, and Confirm is disabled
until the account is no longer used up. The submenu already disables such an account (see "Session rows" in
`docs/product/sidebar.md`).

Confirming fixes the set of sessions to process and locks the checkbox. Sessions opened afterwards do not join that
set, including on a retry. Sessions are switched one at a time; one that has been removed, archived or moved to the
target account before its turn is skipped. A progress bar shows the number processed out of the fixed total, counting
failed and skipped sessions as processed, and the title of the session currently being switched is shown and
announced to assistive technology. A failure does not stop the remaining sessions: an error alert lists each failed
session and its reason when the pass finishes. After acknowledgement, the confirmation offers Retry for those failures alone. Successful switches are kept and never repeated
by Retry. The confirmation closes when the pass finishes without failures.

## Switching from an owner's tool

`switch_session_account` is offered to every session that can own sessions — a console session, an unbound project
session and a lead session — and not to a session with the role of a bound one (see "The console session's tools" and
"The unbound project session's tools" in `docs/product/hub-orchestration.md`). It needs no approval: an owner switches
a session of its own without the user. It switches the one session it names; it has no counterpart of the
confirmation's console-wide choice.

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
way does not itself switch or archive the sessions bound to it; the console-wide choice switches each eligible bound
session separately.

- **The session reads the target account's configuration and keeps its explicit model and reasoning effort choices.**
  An account's directory holds the agent's global configuration as well as its login. Settings kept only in the old
  account's directory do not follow the session; values explicitly supplied by Octoboard still apply according to
  `docs/product/model-and-reasoning-effort.md`. The same conversation can come up with different account defaults.
- **It can take a few seconds**, the time the agent is given to exit and the time the relaunched process is watched; the
  confirmation stays open, its button reading "Switching account…" and the session's terminal "Resuming session…",
  until the result is known, and shows a failure in place. The confirmation cannot be dismissed meanwhile, and the
  session cannot be resumed or switched again.

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
  thread, no database, or none with a thread table has nothing to point.
  - **Where the database is**: the directory `sqlite_home` names in the target's `config.toml` (a relative one taken
    against the account's directory, a leading `~` as the home directory in the user's shell environment), else
    `CODEX_SQLITE_HOME` in that environment, else the account's directory. A directory that does not exist holds no
    database.
  - **Rather than risk resuming the older branch unnoticed, the switch fails** (see "When a switch fails" below) when
    where the database is cannot be determined (a `config.toml` that cannot be read or parsed, a relative
    `CODEX_SQLITE_HOME`, a directory that cannot be listed), or when a database cannot be opened or written to, or
    stays busy for 5 seconds. The conversation is already copied by then and stays in the target, and the failure
    says so.

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
- An item that is the same file in both accounts, as when two accounts share a directory, has nothing to copy.

## When a switch fails

**A switch that cannot be made leaves the session on the account it had, and says why.** It is refused, with nothing
recorded, when the session is being launched, resumed, handed off, switched already or having its owner changed at
that moment, when it is archived, when it is on that account already, when the target account is used up by the
reading the daemon holds at that moment (`account_quota_exhausted`, carrying what "Opening a session" in
`docs/product/sessions.md` describes for it), when the target is a Grok Build directory that is not an initialized Grok home, when the user's
shell environment could not be captured — which is read whenever either side of the switch is the default account,
since that is what resolves its directory, and for any Codex switch, since that environment says where the target
keeps its database (see "The launch environment" in `docs/product/launching-agents.md`) — and when the conversation
record is not where it should be; the session's process is then left running. The one further refusal comes after
the process has been asked to end: a process that has not gone within 30 seconds stops the switch, with nothing
copied and nothing recorded, and that process may still be running. Once the process has been ended, a copy that
does not complete (a Codex account's database that cannot be updated included), a relaunch the launch rules refuse
and a relaunch whose process ends at once are each reported as a failed switch, not as a success: the session is left
*interrupted* on its old account, resumable as before, and already reads so everywhere by the time the failure is
shown. Octoboard tells that last case from a success by the process ending within a few seconds of the relaunch,
never by reading what the agent printed. An account that is not signed in is therefore not a failed switch: every
agent, Grok Build included, comes up on its own sign-in rather than exiting, so the switch succeeds and the session's
terminal shows that agent's sign-in, which may start its sign-in flow.
