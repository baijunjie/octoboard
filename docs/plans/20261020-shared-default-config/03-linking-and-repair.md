# 03 Linking, restoring and launch-time repair

> Goal: the daemon keeps each account's sharing choice and makes the account's directory match it.
> Completion criteria: daemon tests cover turning the switch on (configuration backed up then linked, conversation
> history moved into the default directory then linked, a missing directory created in the default directory, a missing
> file skipped), turning it off (links removed, backups restored, conversation history left in the default directory),
> a change recorded while a session of the account runs and carried out only once none runs, a replaced file link
> repaired before a launch or resume, the default account refused, and the choice surviving a daemon restart; the
> protocol exposes reading and changing it.

## Technical design

- [ ] A per-account setting: whether sharing is on, and whether its directory has been brought in line with it yet.
- [ ] Protocol requests to read and change the setting, rejected for the default account.
- [ ] Turning on and turning off, as set out in the overview, over the agent's catalog
      (`apps/daemon/src/shared_items.rs`).
- [ ] Before a session of the account starts or resumes: a pending change is carried out if none of the account's
      sessions is running, then every file item that is no longer a link is backed up and linked again.

## Implementation plan

- [ ] Store the setting with the account, then add the turn-on and turn-off operations.
- [ ] Hook the pending change and the repair into the launch and resume path of every agent, a switch to the account
      included.

## Handoff

- A shared Codex state database names a rollout by whichever account's path wrote it — Codex records an absolute path
  through the running account's `sessions/` link, and an account switch leaves a row alone when it already names the
  carried rollout by another account's route. Turning an account's sharing off replaces its `sessions/` link with an
  empty directory, so every row naming a rollout through that account's link would then name a missing file for the
  accounts still sharing. Turning off has to account for this (for example by rewriting such rows to the default
  directory's path), and a test should cover it.
- Launching a session of a sharing Claude Code account (a resume and a switch to the account included) sets
  `CLAUDE_CODE_REMOTE_MEMORY_DIR` to the default directory, fully resolved, in the session's environment, and a
  session of an account that does not share never gets it; see the overview for why and for its limits.
- Point each link at the default entry's fully resolved target, not at the entry itself: when the default directory's
  own `settings.json` is a link (to a dotfiles copy, say), a save from a sharing Claude Code account follows one link
  and replaces the default directory's link with a plain file, and the dotfiles copy stops updating (Claude Code
  2.1.304).
- Codex's state database is versioned by name (`state_5.sqlite` in 0.162.1, the name the catalog lists). A Codex that
  moves to a new number would create a plain database of its own in each account and quietly stop using the link, so
  the repair has to notice a `state_*.sqlite` the catalog does not name and say so rather than let it pass.
- Plugin registries record the installing account's own path: Claude Code's `installed_plugins.json` and
  `known_marketplaces.json` (no effect measured on 2.1.304), and Grok Build's `installed-plugins/registry.json`, whose
  entry points into the account's own, now empty, directory once the account stops sharing. Turning the switch off
  has to leave a plugin installed through a sharing account usable, or say what it leaves broken.
- Conversation-history files the default directory already holds cannot simply be moved over it: the prompt
  histories (`history.jsonl` for Claude Code and Codex) and Codex's thread-name index (`session_index.jsonl`, whose
  loss makes a resume by name silently start a new session) have to be merged into the default's copy, and Codex's
  state database set aside, together with its own `-wal` and `-shm`, which can hold committed data. Turning sharing on
  then has to confirm that the rollouts it moves are listed and resumable through the shared database (Codex 0.162.1
  backfills a database from `sessions/` once, though a start was seen adding a row for a newer rollout).

## Notes for the developer

- **Reusable capabilities**: the account store and its protocol requests; the launch path every adapter shares; the
  record of which sessions are running under which account.
- **Development notes**: an operation that fails half-way reports which items it left in which state rather than
  rolling back silently; the protocol contract is updated with the new requests and errors.
- **Reference docs**: `apps/daemon/PROTOCOL.md`, `docs/product/agent-accounts.md`.
