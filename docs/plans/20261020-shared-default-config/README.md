# Shared Default Configuration Development Plan

## Problem and approach

Every agent account is a configuration directory of its own, so skills, plugins, output styles and the like have to be
installed again in each account's directory, and a session switched to another account has its conversation copied
across. This plan adds a **Share default configuration** switch to an account in Settings' Agent accounts section:
once it is on, everything in the account's directory that can be shared is replaced by a symbolic link into the
agent's default directory — configuration and conversation history alike — so something installed once is seen by
every account that shares it, and a conversation is in one place whichever account runs it.

The login stays the account's own, so several accounts keep running side by side, each under its own login — which is
why sharing one directory and swapping only the credentials was rejected: running sessions of every account would
follow each swap, and a refresh-token rotation by a running session would invalidate the stored copy of another
account's login.

## Key design decisions

- **One switch per account, sharing everything shareable.** There is no per-item choice: each agent has a fixed list of
  shareable items, catalogued in the daemon, and the switch links all of them.
- **The default directory is the one the agent's default account resolves to** — the shell's `CLAUDE_CONFIG_DIR` /
  `CODEX_HOME` / `GROK_HOME` when set, else `~/.claude` / `~/.codex` / `~/.grok` — the same resolution Octoboard already
  uses for the default account.
- **The default account has no switch** — it is the directory being shared.
- **Never shared**: login credentials (Claude Code's credentials, `auth.json`), Claude Code's `.claude.json` (it holds
  the account's identity, per-project trust and user-scope MCP servers in one file, so it cannot be shared in part; a
  user-scope MCP server is therefore not shared), Grok Build's `trusted_folders.toml`, which Grok replaces with a plain
  file when it saves it, and Grok Build's `hooks-paths`, which Grok refuses to write through a link.
- **Configuration files are shared whole, account-specific settings included.** Codex's and Grok Build's `config.toml`
  are shared: they hold the model, MCP servers and plugin switches (Grok turns an installed plugin on there, so an
  unshared `config.toml` would leave shared plugins off), and both agents save them by replacing the link's target,
  keeping the link. The cost is accepted: Codex's per-project trust becomes common to every sharing account, and any
  absolute path written in the default directory's `config.toml` applies to all of them.
- **Conversation history is shared.** Claude Code's `projects/`, `file-history/` and the other per-session data,
  Codex's `sessions/`, `archived_sessions/`, `session_index.jsonl` and state database, Grok Build's `sessions/`.
  Session files are named by session id, so accounts writing different sessions at once do not collide.
- **Claude Code's `plans/` is not shared.** Claude Code writes a plan file in plan mode without asking only when its
  directory is the account's own `plans/`; through a link it asks, or refuses outright in a headless run, and no setting
  restores that outside the project tree. An in-progress plan file therefore does not follow a conversation to another
  account.
- **Claude Code's auto memory is pointed at the shared directory.** Its writes into `projects/<key>/memory/` are
  allowed without asking only when the path is spelled inside the account's own directory, so through a linked
  `projects/` they ask or are refused. Octoboard sets `CLAUDE_CODE_REMOTE_MEMORY_DIR` to the resolved default
  directory in the launch environment of a sharing Claude Code account's sessions, which keeps memory where the default
  account keeps it and restores the exemption. It is never written into the user's `settings.json`. Its limits are
  accepted: it is an undocumented internal variable, and it is not in effect when the account is run from a terminal.
- **Codex's state database is shared.** Codex resumes the rollout its `state_<n>.sqlite` names; opened through a link,
  SQLite keeps its write-ahead log beside the target, so several processes across accounts share one log safely, and a
  shared database keeps a per-account row from naming a stale rollout or an archived thread.
- **Turning the switch on:**
  - configuration items the account already holds are renamed to a backup (`<name>.bak-<date>`) and replaced by the
    link — nothing is lost and nothing is merged into the default directory behind the user's back;
  - conversation history the account already holds is moved into the default directory, then linked, so earlier
    conversations stay resumable;
  - a directory item the default directory lacks is created there, empty, then linked, so whatever any account
    installs later lands where every account sees it; a missing file item is skipped.
- **Turning the switch off** removes the links and restores the configuration backups made when it was turned on (an
  item with no backup is left absent). The conversations of the Octoboard sessions recorded under the account are moved
  out of the default directory back into the account's own, so those sessions stay resumable under it; every other
  conversation (one run from a terminal, say) stays in the default directory.
- **Applied while no session of the account runs.** The switch can be changed at any time; the change is recorded and
  carried out at once when none of the account's sessions is running, otherwise before the account's next session
  starts or resumes, at a moment none of its sessions is running.
- **File links are repaired before every launch and resume.** An agent saving a linked file may replace the link with a
  plain file. Before a session of a sharing account starts or resumes, every file item that is no longer a link is
  backed up and linked again.
- **A switch between two accounts that resolve to the same conversation files copies nothing.** Today a switch copies
  the conversation into the target account; with both accounts sharing, source and target are the same file, and
  copying a file onto itself could empty it.

## Milestones

- 01 Shareable item catalog (closed)
- 02 Account switch over shared conversations (closed)
- 03 Linking, restoring and launch-time repair (closed)
- [04 Account settings switch](04-account-settings-switch.md)
- [05 Grok session homes over shared entries](05-grok-farm-links.md)
