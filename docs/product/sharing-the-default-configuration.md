# Sharing the default configuration

An agent account other than the default one can **share the agent's default configuration**: the entries of its
config directory that can be shared — configuration and conversation history alike — are replaced by symbolic links
into the directory the agent's default account resolves to, the **default directory** (see "Agent config directories"
in `docs/product/consoles.md`). Something installed once is then seen by every account that shares, and a
conversation is in one place whichever of those accounts runs it. The login stays the account's own, so accounts that
share still run side by side, each signed in as itself.

The choice is the daemon's: a client reads it on the account and sets it with the protocol request
`set_account_sharing`. The window offers no control for it.

## The sharing choice

Every account the user owns has the choice; the default account has none, being the directory the others share, and
asking for it is refused (`default_account_not_shareable`). So is turning sharing on for an account whose directory
resolves to the default directory under another spelling. The account carries two values:

| Value | Meaning |
|---|---|
| `share_default_config` | Whether the account shares the default configuration. |
| `share_default_config_pending` | The account's directory has not been brought in line with that choice yet: the change is waiting for nothing to run under the directory, or carrying it out failed. |

Both start false and survive a daemon restart. Like the account itself, the choice stays on this machine. A change is
recorded as soon as it is asked for, and every client is told when the choice changes and again when the change has
been carried out. Asking again for the value already recorded retries a change still pending.

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

A retry after the agent's default directory has moved is refused: the links already made lead into the one recorded
when the change started. Turning sharing off and on again shares the new one.

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

Only while **nothing runs under the account's directory**: none of its sessions running or being started (a session
of the account, or one holding its directory), and no agent CLI that Octoboard runs there to read the account's plan
quota or its models or to sign it in. Such a CLI that starts while a change is being carried out waits for it.

- **At once**, when `set_account_sharing` finds nothing running; the request is answered once the change is done.
  Otherwise the change stays pending and the request is answered at once.
- **Before the account's next session starts or resumes**, or a session is switched to the account, at a moment
  nothing else runs there; the session being launched does not count. When something else still runs, the session
  launches with the directory as it stands and the change keeps waiting.
- **A switch carries out its target's pending change before it ends the session's process**, so a refusal leaves the
  session running, and makes sure again once the process has ended, before the conversation is copied in (see "When a
  switch fails" in `docs/product/switching-session-accounts.md`).

**A change that fails is not rolled back.** It stays pending, is tried again on the next request or launch, and is
answered with `account_sharing_failed`, which refuses the launch, resume or switch that tried it. The error lists each
entry that was not brought in line, what it was left as and why:

| Left as | Meaning |
|---|---|
| `unchanged` | as it was; for a conversation that could not be taken back, still in the default directory |
| `backed_up` | renamed to its backup with nothing linked in its place |
| `partly_moved` | some of it in the default directory and the rest in the account's |
| `linked` | still a link into the default directory |
| `absent` | not there at all |

An entry about a conversation names the agent's id of it and the Octoboard sessions that record it. The list is empty
when the change could not start at all.

The outcome is recorded only for the directory the change was carried out on: an account pointed at another directory
meanwhile stays pending, for its new one.

## Repair before every launch

An agent saving a linked file may replace the link with a plain file. So before every launch and resume of a session
whose account's directory shares — an account switch's relaunch included — each entry is checked:

- **A file entry that is no longer a link to the default entry's resolved target** is set aside under a backup named
  with the stamp of turning sharing on, and linked again; a prompt history or thread-name index is appended to the
  default directory's copy, and a Codex database is handled as when turning sharing on.
- **A missing entry, or a directory entry that links elsewhere**, is linked again, under the same rules as turning
  sharing on (a file the default directory lacks is skipped, a directory it lacks is created there). A directory the
  account holds as a real directory of its own is left alone.

Nothing is repaired while turning sharing off is pending, or when the agent's default directory is no longer the one
the links lead into. A session that holds another directory than its account's (the account was repointed after the
session was opened) takes nothing from the account's sharing.

**A repair never holds the launch up.** What it cannot fix, a default directory that has moved included, is told to
clients as an `account_sharing_repair_failed` notice about the session, listing the entries as `account_sharing_failed`
does, and the session launches all the same. A Codex `state_<n>.sqlite` in the account's directory other than the one
shared is told on every launch as an `unshared_state_database` notice: that account's threads are kept apart from the
other accounts' from then on.

## Claude Code's auto memory

Claude Code writes its auto memory into `projects/<key>/memory/` without asking only when the path is spelled inside
the account's own directory; through a linked `projects/` it asks, or refuses outright in a headless run. A session of
a Claude Code account whose directory shares is therefore launched with `CLAUDE_CODE_REMOTE_MEMORY_DIR` set to the
default directory, fully resolved, which keeps memory where the default account keeps it and restores the exemption.
No other session gets it, and it is set for that launch only, never written into the account's `settings.json`. It is
an undocumented internal variable of Claude Code's, and it is not in effect when the account is run from a terminal.

## Removing or repointing a sharing account

Removing an account that shares, or has a change of sharing pending, turns its sharing off first; so does pointing it
at another directory, the sharing being undone in the directory it was carried out on, after which the account does
not share. Either is refused with `account_still_sharing` while anything runs under the account's directory, and with
`account_sharing_failed` when turning sharing off does not complete — sharing then stays turned off, pending — and
the account is left as it was. A rename changes nothing, and so does respelling the directory in another letter case
on macOS, where directories compare without regard to case.

## The sessions of a sharing account

- Their conversations are kept in the default directory, reached through the account's links.
- A switch between two accounts whose links lead to the same conversation files copies nothing (see "Carrying the
  conversation" in `docs/product/switching-session-accounts.md`).
- Deleting one moves its agent records to the Trash from the default directory only while the account shares with no
  change pending (see "Moving the agent's own records to the Trash" in `docs/product/deleting-sessions.md`).
