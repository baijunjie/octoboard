# Sharing the default configuration

An agent account other than the default one can **share the agent's default configuration**: the entries of its
config directory that can be shared — configuration and conversation history alike — are replaced by symbolic links
into the directory the agent's default account resolves to, the **default directory** (see "Agent config directories"
in `docs/product/consoles.md`). Something installed once is then seen by every account that shares, and a
conversation is in one place whichever of those accounts runs it. The login stays the account's own, so accounts that
share still run side by side, each signed in as itself. That is why accounts do not simply share one directory with
only the credentials swapped: every running session would follow each swap, and one session rotating its refresh token
would invalidate the copy stored for another account.

The user changes the choice with **Share default configuration** on the account in Settings' Agent accounts section
(see "Sharing the default configuration" in `docs/product/agent-accounts.md`). A client reads it on the account and
sets it with the protocol request `set_account_sharing`.

## The sharing choice

Every account the user owns has the choice; the default account has none, being the directory the others share, and
asking for it is refused (`default_account_not_shareable`). So is turning sharing on for an account whose directory
resolves to the default directory under another spelling. The account carries two values:

| Value | Meaning |
|---|---|
| `share_default_config` | Whether the account shares the default configuration. |
| `share_default_config_incomplete` | The last change toward that choice did not finish: it failed part-way, or the daemon stopped while carrying it out. |

Both start false and survive a daemon restart. Like the account itself, the choice stays on this machine. A change is
recorded, with `share_default_config_incomplete` set, as soon as it starts, and the flag is cleared when the change
completes; every client is told both times. A failed change does not reset the recorded choice. Only the user clears
an incomplete change: by asking again for the value already recorded, which carries the change out again, or by
asking for the opposite one. The window's switch offers only the first: while the change is incomplete it shows the
position from before the change, so turning it asks for the recorded value again. The exception is turning sharing on
that failed because the default directory has moved (`default_moved`), which a retry cannot mend: the switch then
offers the opposite (see "Sharing the default configuration" in `docs/product/agent-accounts.md`). Asking for the
value recorded on an account whose last change is complete does nothing. An account that an earlier version of
Octoboard marked as having a change pending, to be carried out later, reads as incomplete.

Each link leads to the default entry's fully resolved target, not to the entry itself: when the default directory's
own `settings.json` is a link to a dotfiles copy, say, the account's link leads to the dotfiles copy, so a save made
through the account keeps that copy updated.

## What is shared

A fixed list of entries per agent, directly inside the config directory. There is no per-entry choice.

| Agent | Configuration | Conversation history |
|---|---|---|
| Claude Code | `settings.json`, `CLAUDE.md`, `keybindings.json`, `loop.md`; `agents/`, `commands/`, `skills/`, `output-styles/`, `rules/`, `plugins/`, `workflows/`, `routines/`, `themes/`, `hooks/`, `agent-memory/` | `projects/`, `file-history/`, `paste-cache/`, `image-cache/`, `session-env/`, `todos/`, `tasks/`, `history.jsonl` |
| Codex | `config.toml`, `AGENTS.md`, `AGENTS.override.md`, `hooks.json`; `rules/`, `skills/`, `prompts/`, `plugins/`, `themes/`, `agents/` | `sessions/`, `archived_sessions/`, `session_index.jsonl`, `history.jsonl`, `state_5.sqlite` |
| Grok Build | `config.toml`, `AGENTS.md`, `pager.toml`, `lsp.json`, `settings.json`, `sandbox.toml`, `statusline.sh`, `disabled-hooks`; `rules/`, `skills/`, `commands/`, `agents/`, `personas/`, `workflows/`, `plugins/`, `hooks/`, `installed-plugins/` | `sessions/` |

Nothing else is shared. In particular:

- **Login credentials** stay the account's own.
- **Claude Code's `.claude.json`** is not shared: it holds the account's identity, per-project trust and user-scope MCP
  servers in one file, so a user-scope MCP server is not shared either.
- **Claude Code's `plans/`** is not shared: through a link, Claude Code asks before writing a plan file in plan mode,
  or refuses outright in a headless run. An in-progress plan file therefore does not follow a conversation to another
  account.
- **Grok Build's `trusted_folders.toml` and `hooks-paths`** are not shared: Grok replaces the first with a plain file
  when it saves it, and refuses to write the second through a link.
- **Agent caches, process registries and per-machine state** (Claude Code's `sessions/`, for one) are not shared.

A configuration file is shared whole. Codex's and Grok Build's `config.toml` hold the model, MCP servers and plugin
switches, so a Codex account that shares has the default directory's per-project trust, and an absolute path written
in the default directory's `config.toml` applies to every account that shares it.

Codex names its thread database by its schema generation; `state_5.sqlite` is the one shared. A Codex that moves to
another generation makes a database of its own in each account, which is not shared (see "Repair before every launch"
below).

## Turning sharing on

Before anything is touched, the change records a stamp, the UTC time it started (`20261021T093000Z`), and the default
directory, fully resolved. Backups are named `<entry>.bak-<stamp>`, with `-2`, `-3`, … added when that name is taken.
An account directory that does not exist yet is created. Then each entry of the list:

- **Configuration the account holds** — a file, a directory or a link of its own — is renamed to its backup and
  replaced by the link. Nothing is merged into the default directory.
- **Conversation history the account holds** is moved into the default directory, then linked, so earlier
  conversations stay resumable under the account. A directory is merged entry by entry; where both directories hold a
  file of the same name, the copy written last stays in the default directory and the other is set aside under
  `<entry>.bak-<stamp>/` in the account's directory (two identical copies keep one).
- **Prompt histories** (`history.jsonl`) and **Codex's thread-name index** (`session_index.jsonl`) are appended to the
  default directory's copy rather than set aside, so the account's recall and its thread names survive.
- **Codex's state database**: when the default directory has none, the account's is moved there with its `-wal` and
  `-shm` and linked. When it has one, the account's is set aside under its backup name together with its `-wal` and
  `-shm`, and its thread rows are carried into the shared database: a thread the shared database does not know is
  added, and one it knows keeps the shared row, which is pointed at the account's rollout only when the account's row
  was updated later. Either way the rows are pointed at where the rollouts now are, in the default directory. The rows
  are carried once: a retry before turning sharing on has ever completed redoes it, a later one does not.
- **A directory the default directory lacks** is created there, empty, then linked, so whatever any account installs
  later lands where every sharing account sees it. A configuration file the default directory lacks is skipped: the
  account keeps it as it is, unlinked.
- **An entry already linked to the right place** is left as it is.

An entry is left unchanged and reported when the account's conversation history there is a link of the account's own
(what it leads to is not the account's to move), or when the default directory's entry leads back into the account's
directory.

A retry after the agent's default directory has moved is refused, with the cause `default_moved`: the links already
made lead into the one recorded when the change started. Turning sharing off and on again shares the new one.

## Turning sharing off

An account whose sharing was never carried out has nothing to undo, and the change completes at once. Otherwise:

1. **What the default directory records by the account's path is pointed at the default directory's own.** Codex's
   thread rows that name a rollout through the account's `sessions/` or `archived_sessions/` — Codex records the path
   it ran under — are rewritten to the default directory's path; left alone, they would name an empty directory for
   every account still sharing, and Codex refuses to resume such a thread. So are the plugin registries, so a plugin
   installed through the account stays usable for the others: Claude Code's `plugins/installed_plugins.json` and
   `plugins/known_marketplaces.json`, and Grok Build's `installed-plugins/registry.json`. An entry whose records could
   not be rewritten stays linked and is reported; for Codex's database that keeps `sessions/` and `archived_sessions/`
   linked too.
2. **The links are removed.** A link counts as sharing's when it leads into the default directory recorded when
   sharing was turned on, even when that directory has moved since or is gone, or into the current default directory.
   Any other entry — a file of the account's own, a link of the user's — is left alone.
3. **The backups made when sharing was turned on are restored.** A configuration entry with none is left absent. The
   account gets empty conversation-history directories of its own; the history files (`history.jsonl`, Codex's
   `session_index.jsonl` and state database) are left absent.
4. **The conversations of the account's own sessions are taken back.** Every Octoboard session on record in the
   account's directory, archived ones included, has its conversation moved from the default directory back into the
   account, so it stays resumable there: every file the agent keeps of it goes to the same path in the account's
   directory.

   | Agent | What is taken back |
   |---|---|
   | Claude Code | the transcript with the directory of the same name beside it, and the rewind checkpoints (`file-history/<id>/`) |
   | Codex | every rollout of the thread, those in `archived_sessions/` too |
   | Grok Build | the session's directory |

   - Where the account already holds a file of it, the copy written last stays and the other is set aside under
     `<directory>.bak-<stamp>/` in the account's directory.
   - A conversation that another Octoboard session outside the account also records is copied rather than moved, and
     stays in the default directory for that session.
   - For Codex, the shared database forgets a thread moved out of it (not one copied), the thread's names are copied
     into the account's `session_index.jsonl`, and a database the account has of its own is pointed at the thread's
     head.

Every other conversation — one run from a terminal, or by another account — stays in the default directory.

## When a change is carried out

**A change is carried out at once, or refused,** and once started it runs to its result; it cannot be cancelled. It must
not move files under anything running in the account's directory. Only **working** sessions refuse it — sessions of the
account, or holding its directory, that are in the middle of a turn, idle with background work still running, waiting
for the user's answer (a permission prompt, a question, a session request, or anything else only the user can answer
that shows the session as waiting for the user — its folder-trust confirmation, a Codex action title, or a screen of its
agent's own such as Claude Code's "Resume this conversation?"; see "What the statuses are derived from" in
`docs/product/sessions.md`), holding a message of Octoboard's that is queued, being written, or was left unsent in its
input box, or being started, resumed, switched, handed off or having their owner changed — and a sign-in of the account.
While any of these runs, `set_account_sharing` is refused with `account_in_use`, with nothing stopped or recorded; the
refusal carries how many working sessions there are (`sessions`) and whether a sign-in runs (`signing_in`). Octoboard's
short agent CLI reads of the account, of its plan quota and of its models, do not refuse a change: the change waits for
those running to end, and one that starts meanwhile waits for the change. A sign-in that starts while a change runs
waits for it too. The request is answered once the whole sequence below has ended.

**Idle sessions are ended and resumed around the change.** A session that is idle — at its prompt, waiting for the
user's next message, with nothing on screen only the user can answer and nothing running in the background — does not
refuse a change. The change goes through these phases, in order, each told in the account's sharing state:

1. **Waiting.** Changes of different accounts of the same agent do not overlap: the change first waits for the
   agent's other sharing work — another account's change, a launch's repair, a switch's copy — to finish. The idle
   sessions stay usable meanwhile.
2. **Stopping.** In one step, the account's sessions are judged again and its idle ones claimed; one that has turned
   working meanwhile refuses the change with `account_in_use`, with nothing stopped or recorded. Otherwise the idle
   sessions' processes are ended, and each is waited for. One that does not end in time stops the change, with
   nothing recorded or changed (`sharing_session_did_not_stop`, naming the account and the session); the sessions
   already ended are resumed.
3. **Changing.** The choice is recorded and the entries are brought in line.
4. **Resuming.** The ended sessions are resumed on their conversations, as resuming a session does, whatever the
   change's outcome, onto the directory as it then stands. One archived, deleted or ended by itself meanwhile is not
   resumed. One whose relaunch fails is left *interrupted*, to be resumed by hand; one whose process was never seen
   to end (only beside a `sharing_session_did_not_stop` answer) may still be running and is not relaunched. Either
   is told as a `sharing_resume_failed` notice, shown as a notice toast (see `docs/product/toasts.md`) naming the
   account and saying which of the two happened; the change itself stands.

   A session is resumed with no prompt, so a Claude Code session whose prompt cache has gone cold may come back at
   "Resume this conversation?" (`claude_resume_at_launch`, see "Messages held until a session can take them" in
   `docs/product/message-delivery.md`); it then waits for the user to answer that screen, and the messages held for it
   are written once it is answered.

Waiting and stopping are left out when there is nothing to wait for or to stop, and resuming when nothing was
stopped. While its process is down, an ended session reads as *starting* everywhere, not *interrupted*: in every
client, and in the agents' session tools, whose `list_sessions` lists it among the running sessions.

**Messages sent during the restart are held.** A message Octoboard is asked to send to a claimed session is held from
the moment it is claimed and written into the resumed process. If the session is not resumed, the held messages are
dropped: told by the `sharing_messages_dropped` notice about the session when it was not relaunched, and covered by
the `sharing_resume_failed` notice when its relaunch failed. `sharing_messages_dropped` is shown as an error alert
saying that messages held back for a session while its account's sharing changed were dropped because the session was
not restarted; its next step says that the messages were never written to a terminal and that the session may not be
running, and asks the user to resume the session if it is still needed and send the messages again. The messages were
never part-written into a terminal, so the notice says nothing of an input line.

**A draft typed but not sent is lost.** Text the user has typed into an idle session's terminal without sending it
cannot be seen by Octoboard, so it does not make the session count as working, and it is lost when the session's
process is ended for the change; a prompt submitted straight into the terminal in the moment before the process ends
has its turn ended.

**A change stopped before it records its choice keeps the earlier failure.** The account's last `failure` stays as it
was while a change waits or stops sessions, and after one refused or stopped before its choice is recorded; it is
cleared once the choice is recorded.

**The account is locked while its change runs**, from the check through the resume of its idle sessions. Each of
these is refused with `account_sharing_in_progress` until it ends:

- opening a session on the account, and launching or resuming one of its sessions (other than the change's own
  resumes);
- switching a session from the account or to it (see "When a switch fails" in
  `docs/product/switching-session-accounts.md`);
- handing a session off from the account, or to a receiving session on it;
- removing the account or pointing it at another directory;
- a second `set_account_sharing` for it.

**Nothing is carried out later.** No launch, resume or switch carries out or retries a change, and the daemon launches
a session of an account whose last change is incomplete all the same (see "Repair before every launch" below); the
window asks the user first (see "Starting a session on an account whose last change is incomplete" below).

**A change that fails is not rolled back.** The account stays `share_default_config_incomplete`, and the request is
answered with `account_sharing_failed`, which lists each entry that was not brought in line, what it was left as, why
in a word (`cause`) and why in the system's own text (`detail`):

| Left as | Meaning |
|---|---|
| `unchanged` | as it was; for a conversation that could not be taken back, still in the default directory |
| `backed_up` | renamed to its backup with nothing linked in its place |
| `partly_moved` | some of it in the default directory and the rest in the account's |
| `linked` | still a link into the default directory |
| `absent` | not there at all |

| Cause | Covers |
|---|---|
| `permission_denied` | not allowed to change a file or directory, or a read-only volume |
| `busy` | a file in use by another process, or a database locked by one |
| `not_found` | a file or directory gone |
| `no_space` | the volume or a quota full, or a file too large |
| `conflict` | something in the way or not what it has to be: an entry already there, a link leading elsewhere, a directory where a file is expected or the other way round |
| `default_moved` | the agent's default directory is no longer the one the links lead into or sharing was started into; turning sharing off, then on again, shares the new one |
| `other` | anything else |

A failure reported by Codex's thread database is classed by the database's own code: busy or locked is `busy`, a
permission or read-only failure `permission_denied`, a full disk `no_space`, and anything else, a database file not
found or too large included, `other`. An entry that met problems of different kinds is `other`, and so is a change
that found the account changed while it ran.

An entry about a conversation names the agent's id of it and the Octoboard sessions that record it. The list is empty
when the change could not start at all, and the error's `detail` then says why. The error's own `cause` says the same
of the change as a whole: the cause its entries share (`other` when they differ), or why it could not start when there
are none.

**How the window shows the entries.** In an error alert and in a repair notice, a sharing failure's entries are said
in plain language, one line per cause, each naming the entries it hit compactly: the first by name and the rest as a
count ("no permission to change `skills/` and 1 more"), and going on, on the same line, with what to do about that
cause. A conversation is named by a session that records it, under its current
name. The rest is behind a collapsed **Details** disclosure: each entry on one row with what it was left as, in the
interface language, a conversation naming the sessions that record it, and what the system said of it after that as
small inline code; what the system said in several lines is shown as a block under its row instead. With no entry
listed, the reason the change as a whole failed, from its `cause`, stands in place of the lines when it is one of the
named causes, followed in the same way by what to do about it; when it is not (`other`, none, or one this client does
not know) but there is an original diagnostic, a line saying to see Details stands there alone. Details holds the
original diagnostic. No raw diagnostic is shown outside Details. At the account's switch the same lines, entries and
diagnostic are all behind the alert's Details button, with no line pointing at it (see "Sharing the default
configuration" in `docs/product/agent-accounts.md`).

| Cause | What the window says to do |
|---|---|
| `permission_denied` | check that the files and folders involved can be changed and are not locked |
| `busy` | close other programs using the files involved, such as an agent in a terminal, an editor or a sync tool |
| `no_space` | free up disk space |
| `conflict` | move or rename the conflicting items |
| `other` | see Details |
| `not_found`, `default_moved` | nothing is added |

A failed change of the switch is shown at the account's switch, in every window and nowhere else (see "Sharing the
default configuration" in `docs/product/agent-accounts.md`). A failure that refuses removing or repointing the
account is shown in an error alert (see `docs/product/error-alerts.md`): its short explanation says that the change
did not finish and nothing was rolled back, followed by the entries as above; its next step is to resolve what is
reported, then turn the account's switch off again to try once more.

A refusal is shown in an error alert: `account_in_use` says how many of the account's sessions are working, or that
it is being signed in, and `account_still_sharing` that sharing cannot be turned off while one of its sessions is
working or it is being signed in; both ask to wait for the working sessions to finish and any sign-in to end, then to
try again; `account_sharing_in_progress` asks to wait for the change to finish, saying that its progress is shown at
the account's switch; `sharing_session_did_not_stop` names the session that did not stop in time, says sharing was
not changed, and asks to try again, ending that session in its terminal first if it keeps running.

## The account's sharing state

Every client is told each account's sharing state with the `account_sharing` event, sent whenever any of it changes,
and given every account's current state on connecting. Each part is absent when it has nothing to say; an account the
client has been told nothing of has nothing in use, no change running and no failure.

| Part | What it says |
|---|---|
| `in_use` | `sessions`, how many of the account's sessions are working under its directory, and `signing_in`, whether it is being signed in: what `account_in_use` refuses a change for. |
| `idle_sessions` | How many of the account's sessions are idle: those a change ends and resumes. |
| `progress` | Present while a change runs: its direction (`share`); its `phase`, `waiting`, `stopping`, `changing` or `resuming`; the entries processed so far (`done`) out of `total`, none before `changing` and all of them once `resuming`; and `item`, the entry being processed — an entry of the list in "What is shared", or, turning sharing off, the agent's id of a conversation being taken back. `item` is present only while `changing`, from the first entry on: it is absent while the change waits for short agent CLI reads. |
| `failure` | The last change of the account that failed since the daemon started: its direction (`share`), its `cause`, the entries it left (`items`) and `detail`, as in `account_sharing_failed`. Cleared when the next change records its choice; a change refused or stopped before that leaves it. |

A failure is not kept across a daemon restart: an account whose last change is incomplete then has
`share_default_config_incomplete` set and no `failure`.

## Starting a session on an account whose last change is incomplete

The daemon starts sessions on an account whose last change is incomplete as on any other, its directory left as it
stands. The window asks first: when the user starts a session on such an account while no change of it runs (a running
change refuses the launch, see "When a change is carried out" above), a confirmation asks whether to start on the
account, named in its title, and says that the account's last sharing change did not finish, so its configuration and
conversations may be incomplete. It has no Cancel button: its buttons are **View in Settings** and **Start anyway**, the
primary one, on which keyboard focus starts. Closing it — with the dialog's close button or Escape — cancels the launch
and sends nothing; when it stands over another dialog, Escape closes only the question, leaving that dialog open. **View
in Settings** sends nothing either: it closes the question, and the dialog it was asked from when there is one (the
session dialog, the handoff dialog, the switch confirmation), and opens Settings on its Agent accounts section, on the
account's agent's tab, with the account's sharing switch scrolled into view and focused; the alert under the switch
says what did not finish (see "Sharing the default configuration" in `docs/product/agent-accounts.md`). When the account
is no longer found there, Settings opens on the section all the same. **Start anyway** closes the question at once and
sends the request as it would have gone without it; its refusal or failure is shown where it would have been.

The account asked about is the one the session would run on. The question comes:

- opening a session from the session dialog, on the account chosen, a new session on a found conversation included;
- opening a session with defaults (see "Opening a session with defaults" in `docs/product/sessions.md`), on the
  account it would open on;
- resuming an interrupted session or reopening an archived one — by selecting it, with Resume or Reopen wherever they
  are offered, or from the session dialog's **Resume conversation** tab — on the session's own account; going to a
  session already running asks nothing;
- switching sessions onto the account, after the switch confirmation's Confirm, once for the whole set; switching a
  session away from the account asks nothing;
- handing a session off onto the account, after **Hand off**.

An account whose last change is complete is never asked about, nor is the default account, which has no sharing. A
launch Octoboard starts by itself, such as the resumes around a change (see "When a change is carried out" above),
does not ask, and neither does approving a session another session asked for with `request_session`, whose account
Octoboard picks. Approving another session's request to resume a conversation does not ask either: where the session
it would reopen, resume or open runs on such an account, that request's own confirmation carries a warning line saying
the same instead (see "The confirmation dialog" in `docs/product/requests-to-resume-a-conversation.md`), and an
automatic approval warns nothing.

## Repair before every launch

An agent saving a linked file may replace the link with a plain file. So before every launch and resume of a session
whose account's directory shares — an account switch's relaunch included — each entry is checked:

- **A file entry that is no longer a link to the default entry's resolved target** is set aside under a backup named
  with the stamp of turning sharing on, and linked again; a prompt history or thread-name index is appended to the
  default directory's copy, and a Codex database is handled as when turning sharing on.
- **A missing entry, or a directory entry that links elsewhere**, is linked again, under the same rules as turning
  sharing on (a file the default directory lacks is skipped, a directory it lacks is created there). A directory the
  account holds as a real directory of its own is left alone.

Nothing is repaired while the account's last change is incomplete, in either direction: its directory is left as it
is for the user to retry the change. Nothing is repaired either when the agent's default directory is no longer the one
the links lead into. A session that holds another directory than its account's (the account was repointed after the
session was opened) takes nothing from the account's sharing.

**A repair never holds the launch up.** What it cannot fix, a default directory that has moved included, is told to
clients as an `account_sharing_repair_failed` notice about the session, carrying a `cause` and listing the entries as
`account_sharing_failed` does (none when the directory could not be checked at all, or when the default directory has
moved, with the cause `default_moved`), and the session launches all the same. A Codex `state_<n>.sqlite` in the
account's directory other than the one shared is told on every launch as an `unshared_state_database` notice: that
account's threads are kept apart from the other accounts' from then on.

The window presents a repair failure as a notice toast (see `docs/product/toasts.md`) explaining that the session
started with the directory as it stands. The affected entries, or with none listed the reason from the notice's
`cause`, are shown as for a failed change, by cause with what to do and **Details** (see "When a change is carried
out" above), the notice's original message standing for the original diagnostic behind Details. The notice does not
require acknowledgement or stop the session.

## Claude Code's auto memory

Claude Code writes its auto memory into `projects/<key>/memory/` without asking only when the path is spelled inside
the account's own directory; through a linked `projects/` it asks, or refuses outright in a headless run. A session of
a Claude Code account whose directory shares is therefore launched with `CLAUDE_CODE_REMOTE_MEMORY_DIR` set to the
default directory its links lead into, fully resolved, which keeps memory where the default account keeps it and
restores the exemption. For an account whose last change is incomplete, in either direction, the variable is set only
while the account's `projects/` currently links into that directory. No other session gets it, and it is set for
that launch only, never written into the account's `settings.json`. It is an undocumented internal variable of Claude
Code's, and it is not in effect when the account is run from a terminal.

## Removing or repointing a sharing account

Removing an account that shares, or whose last change of sharing is incomplete, turns its sharing off first; so does
pointing it at another directory, the sharing being undone in the directory it was carried out on, after which the
account's choice is off. Sharing is turned off at once, under the account's lock and with its progress told, as
`set_account_sharing` turns it off, its idle sessions ended and resumed around it the same way. Either is refused
with `account_still_sharing` while sessions of the account are working under its directory (one of its idle sessions
turning working before it was ended included) or it is being signed in, with `sharing_session_did_not_stop` when an
idle session's process does not end in time, with `account_sharing_in_progress` while a change of its sharing runs
(for any account, sharing or not), and with `account_sharing_failed` when turning sharing off does not
complete — the choice then stays off, incomplete — and the account is left in place, on its directory. A rename
changes nothing, and so does respelling the directory in another letter case on macOS, where directories compare
without regard to case.

## The sessions of a sharing account

- Their conversations are kept in the default directory, reached through the account's links.
- What a Grok Build session writes back into a shared entry is in the Grok Build paragraph of "Per-agent specifics a
  user will notice" in `docs/product/launching-agents.md`.
- A switch between two accounts whose links lead to the same conversation files copies nothing (see "Carrying the
  conversation" in `docs/product/switching-session-accounts.md`).
- Deleting one moves its agent records to the Trash from the default directory only while the account shares with its
  last change complete (see "Moving the agent's own records to the Trash" in `docs/product/deleting-sessions.md`).
