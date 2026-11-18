# Opening a session in its agent's app

A session of an agent that has a desktop application of its own can have its conversation opened in that application,
to carry on there. Octoboard only asks the application to show the conversation: nothing is copied, the session is not
changed, and nothing Octoboard runs is stopped for it.

## Which agents have an app

| Agent | Application | How the conversation is opened |
|---|---|---|
| Claude Code | the Claude desktop application | `claude --desktop --resume <conversation id>`, run as the session's account (its config directory in the environment) in the session's directory; the application stays open once the command has exited |
| Codex | the Codex application (the ChatGPT application) | the address `codex://threads/<conversation id>`, opened with the system's opener |
| Grok Build | none | — |

Everything happens on the **daemon's host**: for a remote window (see `docs/product/remote-daemons.md`) that is the
remote machine, not the one the window runs on.

An application counts as **installed** when the host knows a handler for the agent's URL scheme (`claude`, `codex`):
on macOS as Launch Services registers it, on Windows as the shell's association for the protocol names it, packaged
applications included. On any other host no application is ever installed, so the action is never offered there.

## Where it is offered

The action is labelled for the agent's application — **Open in Claude app**, **Open in Codex app** — and is offered
only for a session whose agent has an application that is installed. It appears:

- in a session's action menu, right after Hand off on a session that is not archived, and right after Reopen, in the
  same group, on an archived one, with the letter shortcut `W` (see "Session rows" and "Session
  menu keyboard shortcuts" in `docs/product/sidebar-session-rows.md`);
- on each row of the archive view, as the first of the row's icon buttons (see `docs/product/archive-view.md`).

Whether each application is installed is asked again whenever the window connects to the daemon (a reconnect
included), whenever a session's action menu opens, and whenever the archive view opens; an application installed or
removed meanwhile is found then. A failed check keeps the last answer. The menu also asks about its own session, and
the archive view about each row it lists that has a conversation, whether the application can see the conversation
(see "Whether the app can see the conversation" below).

## When it can be used

**Only a session with no process — *interrupted* or *archived* — can be opened.** The application and Octoboard would
both write to the conversation while the agent runs in Octoboard, and Octoboard does not stop a session for this: the
user stops it first. While the session has a process (whatever its status, at its prompt included), the menu item is
disabled with "Session running" under its label, and its description says to stop the session first, since both would
write to the conversation.

**A session with no conversation yet** (see "Done, awaiting instructions and child sessions working" in
`docs/product/session-statuses.md`) has nothing to open: the action is disabled with "No conversation yet". In the
archive view the button is unavailable rather than disabled: its tooltip and its description for assistive technology
give the reason, and pressing it does nothing (see "How such a control behaves" in
`docs/product/controls-unavailable-for-a-reason.md`).

The daemon refuses the open, with nothing changed, while the session:

- has a process (`session_already_running`);
- is being started, resumed or otherwise held by another operation (`session_already_starting`);
- is on an account whose sharing of the default configuration is being changed (`account_sharing_in_progress`; see
  "When a change is carried out" in `docs/product/sharing-the-default-configuration.md`);
- or when the daemon has begun to shut down (`daemon_shutting_down`).

## Whether the app can see the conversation

Both applications read only the agent's **default directory** (see `docs/product/sharing-the-default-configuration.md`),
never another account's. The daemon judges whether the default directory holds the conversation's **latest record**.
It does not when the record the agent names as current there is
missing, or when the session's own account directory holds a different copy that is longer or more recent — as for a
session on an account other than the default one that does not share the default configuration.

That only warns: the action stays available, since the application may still show the conversation, or an older state
of it.

- In the menu, the item carries a short muted line under its label saying that the agent's app may not see this
  conversation; its description for assistive technology says why — the latest record is kept in the account's own
  directory, not the default one.
- In the archive view, the button's tooltip carries that sentence under the action's name (see "Tooltips on
  icon-only controls" in `docs/product/labels-and-tooltips.md`), and it is the button's description for assistive
  technology.

A session not yet judged shows no warning. The default directory is taken as Octoboard resolves it, from its login
shell's environment (`CLAUDE_CONFIG_DIR` or `CODEX_HOME`, else `~/.claude` or `~/.codex`); an application started
with a different environment may read another directory, which this check does not see.

## What opening does

Once nothing refuses it and the application is about to be asked, any wait Octoboard still holds to bring the session
back on its own — the automatic account switch's wait for quota (see `docs/product/automatic-account-switch.md`) or
the retry of a turn stopped at its model's capacity (see "When the selected model is at capacity" in
`docs/product/turns-hooks-do-not-report.md`) — is ended, the user having taken the conversation over. An open that is
refused leaves those waits as they were.

The request succeeds once the application has accepted the open. The session stays as it was in Octoboard —
interrupted or archived — and can be resumed or reopened there later as usual; while it is, it is again a session
with a process, which the application should not be writing to at the same time.

## When opening fails

The daemon answers `native_app_unavailable` when the agent has no application, the application is not installed, the
session has no conversation yet, its conversation id is not one the agent would take, or the launch fails (for Claude
Code, the `claude --desktop` command not running or exiting with an error). The window shows an error alert titled
"Could not open the agent's app" (see `docs/product/error-alerts.md`), naming the session, saying that the app may not
be installed, may not have started, or has no conversation to open, and, as its remedy, to see the details for the
cause and try again; the daemon's own reason is under Error details.

## The protocol requests

| Request | Fields | Answer |
|---|---|---|
| `open_session_in_app` | `session` | `ack` once the application has accepted the open; refused as in "When it can be used" and "When opening fails" above, and with `unknown_session` for an unknown session. |
| `probe_native_apps` | `sessions?` | `native_apps`, with `agents` — the agents whose application is installed — and, for the sessions named, `sessions_seen` (those whose latest record the default directory holds) and `sessions_unseen` (those it does not; absent when none). |

`probe_native_apps` judges at most 200 sessions per request. A session it was asked about that is in neither list was
not judged: an unknown session, one of an agent with no application, one past the limit, or any session when the
daemon could not capture its login shell's environment. An agent's application is declared by its provider
(`native_app_scheme` in its capabilities, the URL scheme it is looked up by), which the window reads to know which
sessions to ask about.
