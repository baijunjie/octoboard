# Shared session directories across accounts

## Problem

Moving a session to another account ("Switch account") copies that conversation's record file from the old account's
config directory into the new one, then resumes it there. The copy is cheap, but it has defects:

- **Codex sessions idle for about a week cannot be switched.** Codex compresses rollouts older than 7 days to
  `rollout-…-<id>.jsonl.zst` (measured: every rollout older than 7 days on the measured machine was compressed, none
  newer), while Octoboard's record lookup matches only `.jsonl`. Switching such a session fails with
  `conversation_not_found`; usage, titles and trash-on-delete miss it too. Inferred from code and the measured
  compression, not yet hit by a real session.
- **The copy leaves parts of the conversation behind.** Claude Code: the `projects/<slug>/<id>/` directory beside the
  transcript (subagent transcripts, tool results, custom title), `file-history/<id>/` (rewind checkpoints). Codex:
  subagent rollouts named `rollout-…-<parent>_<child>.jsonl`.
- **The old account keeps a stale copy**, which its own terminal resume picker still lists.

The user also wants an account to be able to share its sessions with the default account, so switching between them
needs no migration at all.

## Plan outline

1. Make the existing migration complete and correct.
2. Measure the open questions that decide whether sharing is safe.
3. Let an account opt to share its session directories with the default account by linking whole directories.

## Key design decisions

- **Sharing links whole directories, never single files** (set by the user). The agents write some files by
  temp-file-plus-rename, which replaces a file-level link with a real file (measured for Grok Build's
  `trusted_folders.toml`); inside a linked directory the same rename lands in the real directory. The directories:
  - Claude Code: `projects/` (carries the per-conversation sidecar directories and per-project memory with it);
    `file-history/` for rewind.
  - Codex: `sessions/` and `archived_sessions/` (Codex's own archive renames rollouts between the two).
  - Grok Build: `sessions/` (its search index lives inside it and is shared with it).
- **What stays per account, never linked:** login and account records (`.claude.json`, `auth.json`, Keychain items),
  `settings.json` / `config.toml`, prompt history, process registries (`sessions/<pid>.json` for Claude Code),
  shell snapshots, and Codex's SQLite databases at the home root (`state_*`, `thread_history_*`, `logs_*` and others),
  which carry account-specific rows such as `creator_account_id` and record canonical rollout paths per home.
- **All three agents follow a directory link for their session store** (measured for Claude Code 2.1.302 and Codex
  0.162.1 resuming through a linked `projects/` / `sessions/`; Grok Build already runs every Octoboard session through
  a linked `sessions/` in its per-session home). Codex records the link-resolved path in its own database and keeps its
  per-thread writer locks per home, not in the shared directory.
- **Octoboard's own record handling must resolve links to a known records root rather than refuse them.** Today the
  record index deliberately refuses a linked records directory, so a sharing account would lose usage collection,
  summary titles (Grok Build relies on the index alone) and presence; trash-on-delete skips paths under linked
  directories. These are taught where an account's records really live instead of relaxing the symlink rule.
- **Sharing is an explicit per-account opt-in in Octoboard's account settings.** Turning it on merges the account's
  existing records into the default account's (resolving conversations that exist in both, e.g. left by earlier
  switches); turning it off, or removing the account, removes the links Octoboard created. The setting travels with
  the configuration export; the links are machine-local and are re-created on import.
- **Creating links in an account's directory is an exception to Octoboard's rule of never editing the user's
  files**, made only on the user's opt-in and documented where that rule is stated.

## Milestones

1. [Complete account migration](01-complete-account-migration.md)
2. [Measure cross-account sharing](02-measure-cross-account-sharing.md)
3. [Shared session directories](03-shared-session-directories.md)

Milestone 3 depends on 1 and 2.

## Open

- Whether Grok Build sharing links the account's own `sessions/` or only redirects the `sessions/` link in Octoboard's
  per-session home to the default home (which leaves the user's directories untouched but hides the shared sessions
  from a terminal `grok` run under that account).
- Whether the stale copy left in the old account after a switch (without sharing) is removed, kept, or left to the
  user.
