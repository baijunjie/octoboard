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
  shareable items (milestone 01), and the switch links all of them.
- **The default directory is the one the agent's default account resolves to** — the shell's `CLAUDE_CONFIG_DIR` /
  `CODEX_HOME` / `GROK_HOME` when set, else `~/.claude` / `~/.codex` / `~/.grok` — the same resolution Octoboard already
  uses for the default account.
- **The default account has no switch** — it is the directory being shared.
- **Never shared**: login credentials (Claude Code's credentials, `auth.json`), Claude Code's `.claude.json` (it holds
  the account's identity, per-project trust and user-scope MCP servers in one file, so it cannot be shared in part; a
  user-scope MCP server is therefore not shared), and Grok Build's `config.toml` and `trusted_folders.toml`, which Grok
  rewrites by replacing the file.
- **Conversation history is shared.** Claude Code's `projects/`, `file-history/` and the other per-session data,
  Codex's `sessions/`, Grok Build's `sessions/`. Session files are named by session id, so accounts writing at once do
  not collide.
- **Codex's state database is shared only if it is safe.** Codex resumes the rollout its `state_<n>.sqlite` names. If
  milestone 01 finds the database cannot be safely opened through a link by several processes, Codex shares
  `sessions/` without it — provided a conversation still resumes under another sharing account; if it does not, Codex
  shares no conversation history and its account switch keeps copying.
- **Turning the switch on:**
  - configuration items the account already holds are renamed to a backup (`<name>.bak-<date>`) and replaced by the
    link — nothing is lost and nothing is merged into the default directory behind the user's back;
  - conversation history the account already holds is moved into the default directory, then linked, so earlier
    conversations stay resumable;
  - a directory item the default directory lacks is created there, empty, then linked, so whatever any account
    installs later lands where every account sees it; a missing file item is skipped.
- **Turning the switch off** removes the links and restores the configuration backups made when it was turned on (an
  item with no backup is left absent). Conversation history stays in the default directory — both what was moved there
  and what was written while sharing — and the account starts with empty conversation directories.
- **Applied while no session of the account runs.** The switch can be changed at any time; the change is recorded and
  carried out before the account's next session starts or resumes, at a moment none of its sessions is running.
- **File links are repaired before every launch and resume.** An agent saving a linked file may replace the link with a
  plain file. Before a session of a sharing account starts or resumes, every file item that is no longer a link is
  backed up and linked again.
- **A switch between two accounts that resolve to the same conversation files copies nothing.** Today a switch copies
  the conversation into the target account; with both accounts sharing, source and target are the same file, and
  copying a file onto itself could empty it.

## Milestones

- [01 Shareable item catalog](01-shareable-item-catalog.md)
- 02 Account switch over shared conversations (closed)
- [03 Linking, restoring and launch-time repair](03-linking-and-repair.md)
- [04 Account settings switch](04-account-settings-switch.md)
