# Opening a session in its agent's app

A session of an agent that has a desktop application of its own can have its conversation opened in that application,
to carry on there. Octoboard only asks the application to show the conversation: nothing is copied. A session that is
not archived yet is **archived first**, as the original of a handoff is, so that Octoboard and the application never
write to the same conversation: an interrupted session would otherwise be resumed by selecting it, or by **Resume
sessions on start** (see "Resuming the interrupted sessions at start" in
`docs/product/archiving-and-resuming-sessions.md`).

## Which agents have an app

| Agent | Application | How the conversation is opened |
|---|---|---|
| Claude Code | the Claude desktop application | `claude --desktop --resume <conversation id>`, run as the session's account (its config directory in the environment) in the session's directory; the application stays open once the command has exited |
| Codex | the Codex application (the ChatGPT application) | the address `codex://threads/<conversation id>`, opened with the system's opener |
| Grok Build | none | — |
| Antigravity CLI | No supported conversation opener | — |
| GitHub Copilot CLI | No supported conversation opener | — |
| OpenCode | No supported conversation opener | — |
| Qwen Code | No supported conversation opener | — |
| Pi | No supported conversation opener | — |

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

**An archived session** is opened as it is, at once.

**Any other session** — interrupted, or with a process that is **at rest** — is archived by the open and then opened.
At rest is judged as for a handoff (see "What can be handed off" in `docs/product/handing-off-sessions.md`): at its
prompt with nothing left running in the background, stopped on its account's usage limit (its usage-limit screen
included), or interrupted. A session still starting, in the middle of a turn, waiting for the user for anything else,
*dormant*, or at its prompt with work left running in the background is not opened until it is at rest.

In a session's action menu the item is disabled, in this order of precedence:

- for a session that is not archived and is locked, with "Session locked" under its label;
- for a session that is not at rest, with "Session busy" under its label, as Hand off shows it;
- while the session is shown as switching its account, and while it is *starting*, with no line under its label;
- for a session with no conversation yet (see "Done, awaiting instructions and child sessions working" in
  `docs/product/session-statuses.md`), which has nothing to open, with "No conversation yet" under its label.

Each has a full sentence as its description for assistive technology: that the session is locked, so it cannot be
archived and opened; that it is still working or waiting for the user and can be opened once it is waiting for
instructions or interrupted; that it is switching its account; that it is still starting; that it has no conversation
yet. A lock on an archived session does not keep it from being opened. In the archive view the button is unavailable
rather than disabled for a session with no conversation yet: its tooltip and its description for assistive technology
give the reason, and pressing it does nothing (see "How such a control behaves" in
`docs/product/controls-unavailable-for-a-reason.md`).

The daemon refuses the open, **with nothing archived, ended or changed**, when:

- the session is not at rest (`session_working`);
- the session, for one that is not archived, or a session under it is locked (`session_locked`; see "Locking a
  session" in `docs/product/sessions.md`);
- the session is being started, resumed or otherwise held by another operation (`session_already_starting`);
- the session's account is having its sharing of the default configuration changed (`account_sharing_in_progress`;
  see "When a change is carried out" in `docs/product/sharing-the-default-configuration.md`);
- the daemon has begun to shut down (`daemon_shutting_down`);
- the agent's provider declares no application (`provider_unsupported`, operation `native_app`; see "The error alert"
  in `docs/product/error-alerts.md`);
- the application is not installed, the session has no conversation yet or its conversation id is not one the agent
  would take (`native_app_unavailable`; see "When opening fails" below);
- the daemon's login-shell environment cannot be captured (`shell_environment_timeout`);
- a process tree chosen to be kept cannot be kept (`process_shares_agent_console` or `process_in_agent_command_group`;
  see "Processes that cannot be kept" in `docs/product/background-processes.md`).

The one exception to "nothing changed": the session is judged at rest again once the trees chosen to be kept have been
kept, just before it is archived, and a `session_working` found only then leaves those trees kept, and the session
running.

## The confirmation

Pressing the action on a session that is not archived always asks for confirmation first, from the menu as from its
shortcut; an archived session, in its menu or in the archive view, is opened without one. The confirmation is titled
with the action's label (**Open in Claude app**, **Open in Codex app**), which its confirm button carries too. It says
that the session, named, will be archived and then opened in the agent's app, and that its branches and worktrees are
kept, the session being reopened from the archive to continue it in Octoboard once the app is done with the
conversation. Under that it has up to two sections, each shown only while it has something to show, in this order:

- **Archived with it (n)**, the unarchived sessions under it that are archived along with it, as an archive lists them
  (see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`), followed live;
- **Background processes**, the process trees the archive stops, with the choice of keeping those that can be kept
  running, as an archive confirmation offers it (see "Processes in an archive confirmation" in
  `docs/product/background-processes.md`).

It has no section for branches and worktrees, since none is removed. It cannot be confirmed until the inspection of
those sessions and processes has answered. A refusal or failure is shown as an error alert titled "Could not open the
agent's app" (see `docs/product/error-alerts.md`), and the confirmation stays open.

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

For a session that is not archived, the open first archives it as the user's own archive does: every unarchived session
under it is archived with it, those starting or at work included (see "Archiving, interruption and resuming" in
`docs/product/archiving-and-resuming-sessions.md`), and their processes are ended. Unlike that archive it **removes no
branch or worktree**, of the session or of those archived with it: the application works in the session's directory.
The process trees chosen to be kept are kept before the archive; the rest of what the session's process started is
stopped as it ends, not carried over (see "Automatic stopping and its boundaries" in
`docs/product/background-processes.md`). The application is asked only once the session's own process is gone.

Any wait Octoboard still holds to bring the session back on its own — the automatic account switch's wait for quota
(see `docs/product/automatic-account-switch.md`) or the retry of a turn stopped at its model's capacity (see "When the
selected model is at capacity" in `docs/product/turns-hooks-do-not-report.md`) — is ended, the user having taken the
conversation over: by the archive, as the session's process leaves, or, for a session with no process or one archived
already, just before the application is asked. An open refused with nothing changed leaves those waits as they were.

The request succeeds once the application has accepted the open. The session is then archived in Octoboard and can be
reopened there later as usual (see "Archiving, interruption and resuming" in
`docs/product/archiving-and-resuming-sessions.md`); while it runs again, the application should not be writing to the
conversation at the same time.

## When opening fails

Once the archive is made, a failure leaves the session archived.

**The session's process does not end in time.** The open waits up to **30 seconds** for it, then answers
`session_did_not_stop` with `cause: "open_in_app"`: the application is not asked, the session stays archived, and its
process may still be running. The alert says that the session did not stop in time, so its conversation was not opened
in the agent's app, and that the session is archived but may still be running; as its remedy, to wait a moment and try
again, and, if it keeps failing, to exit the agent from the session's terminal with its quit command. A retry waits for
the process again.

**The application cannot be asked.** The daemon answers `native_app_unavailable` when the application is not installed, the
session has no conversation yet, its conversation id is not one the agent would take, or the launch fails (for Claude
Code, the `claude --desktop` command not running or exiting with an error). The window shows an error alert titled
"Could not open the agent's app" (see `docs/product/error-alerts.md`), naming the session, saying that the app may not
be installed, may not have started, or has no conversation to open, and, as its remedy, to see the details for the
cause and try again; the daemon's own reason is under Error details. Only a failed launch comes after the archive;
the other causes are found before anything is archived.

## The protocol requests

| Request | Fields | Answer |
|---|---|---|
| `open_session_in_app` | `session`, `keep_process_ids?` | `ack` once the application has accepted the open; refused as in "When it can be used" and "When opening fails" above, and with `unknown_session` for an unknown session. `keep_process_ids` (absent means none) are the ids of the session's process trees, as its process list names them, to keep running when the archive ends the session's process; it is ignored for an archived session. |
| `probe_native_apps` | `sessions?` | `native_apps`, with `agents` — the agents whose application is installed — and, for the sessions named, `sessions_seen` (those whose latest record the default directory holds) and `sessions_unseen` (those it does not; absent when none). |

`probe_native_apps` judges at most 200 sessions per request. A session it was asked about that is in neither list was
not judged: an unknown session, one of an agent with no application, one past the limit, or any session when the
daemon could not capture its login shell's environment. An agent's application is declared by its provider
(`native_app_scheme` in its capabilities, the URL scheme it is looked up by), which the window reads to know which
sessions to ask about.
