# Cross-project guard

## Problem

A project session that is given a task touching another project registered in Octoboard often edits that project's
files itself instead of asking for a session in that project. Instructions alone do not stop it. The guard stops it
mechanically, and turns the stop into a hand-over rather than lost work.

## Plan outline

One switch in Settings' **Agent behavior** section, **off to begin with**, turns on two layers for project sessions:

1. **Edit layer.** A synchronous `PreToolUse` guard hook injected into Claude Code, Codex and Grok Build. When a
   file-edit tool call targets a path inside another registered project, the hook denies it with a reason telling the
   session to keep what it changed in place and hand the work over through `request_session`.
2. **Git layer.** The session's git hook dispatcher refuses a ref update in the repository of another registered
   project, at `reference-transaction` `prepared`, beside the revert gate. By then the code is usually written, so the
   refusal asks the session to hand the existing changes to a session of that project for review and to let that session
   take over the rest, rather than discard them.

The edit layer catches the common case early; the git layer is the backstop for everything the edit layer cannot see
(shell writes, a hook that failed to run).

## Key design decisions

- **A switch in Agent behavior, off by default** (user decision). Like the other switches there, it is kept by the
  daemon, agents can neither read nor change it, and it reaches sessions started after the change.
- **The edit layer decides locally from a list written at launch** (user decision). At launch Octoboard writes the
  roots of the other registered projects to a file in the session's scratch directory; the guard hook is a local
  command that reads only that file. No daemon round-trip per edit, no new relay route, no dependency on the holder's
  build, and it still works while the daemon is unreachable. Accepted cost: a project registered after the session
  started is not in its list.
- **The edit layer guards file-edit tools only** (user decision): Claude Code `Edit`/`Write`/`MultiEdit`/
  `NotebookEdit`, Codex `apply_patch`, Grok `search_replace`/`write`. Shell commands are not guarded there: where a
  command writes cannot be told reliably from its text, and reading another project is legitimate. Shell writes are
  left to the git layer.
- **The guard fails open on all three agents** (user decision): a guard that cannot run lets the call through, and the
  git layer catches the result at commit time. Grok's `failClosed` is not used, so a broken guard never stops a
  session from editing its own project.
- **Refuse only other registered projects, never "anything outside my project".** Test suites commit in temporary
  repositories and inherit the session's git environment; scratch, temp and agent-data directories must stay writable.
- **The git layer rides the revert gate's existing trigger** (user decision): it is consulted when an existing branch
  moves, which covers every commit on a branch (a branch just created moves with its first commit), and merges, rebases
  and resets as well. Commits on a detached HEAD and the first commit of an unborn branch are not covered; the trigger is
  not widened for them.
- **The git layer reads the switch on every ref update** (user decision), as the revert gate does, so a change applies
  to running sessions at once; the edit layer keeps the value it was launched with.
- **The deny reason is short, with the hand-over first.** Grok shows the model only about the first 270 characters of a
  hook's reason; the tool name is the agent's own qualified name for `request_session`, which worker and coordinator
  project sessions both have.

Measured before planning (Claude Code 2.1.322, Codex 0.167.1, Grok Build 1.0.77):

- All three honour `hookSpecificOutput.permissionDecision: "deny"` with `permissionDecisionReason` from a `PreToolUse`
  hook, in every permission mode including bypass / full access / always-approve; the model reads the reason, does not
  retry another way and tries to call `request_session`.
- Claude Code permission `deny` rules also block, but their generic message ("denied by your permission settings")
  replaces the hook's reason and leads the model to ask the user to loosen permissions, so they are not used.
- Codex runs a hook that blocks only when it is synchronous (Octoboard's status hooks are `async=true`); its
  `apply_patch` payload carries the patch text in `tool_input.command`, with paths relative to the payload's `cwd`, and
  can name several files.
- Grok's tool payloads name the file in `path` for some tools rather than `file_path`.
- Every agent lets a call through when its hook cannot run (Grok unless `failClosed`).

## Milestones

1. 01 the Agent behavior switch and the git-layer refusal (closed)
2. 02 the launch-time project list, the guard command, and its injection into Claude Code (closed)
3. 03 injecting the guard into Codex and Grok Build (closed)
4. [04-final-confirmation.md](04-final-confirmation.md) — confirming in real sessions what unit tests cannot reach

## Registered projects that contain shared directories

A project registered at an ancestor, such as the home folder or the agent-data folder, stays in the list, and editing
its own files is refused; the deepest containing project wins, so the session's own project always passes. Shared
directories always pass whatever project contains them: the agents' configuration and data directories, Octoboard's
data and session scratch directories, and the system temporary directories (user decision).

## Refusal texts

Agreed with the user; `<tool>` is the agent's own qualified name for `request_session`.

Edit layer (within about 270 characters):

> Blocked: `<path>` is in project "`<name>`", not this session's. Don't write it another way. Keep any changes there as
> they are and call `<tool>` for a "`<name>`" session, listing the changed files and where they are, so it reviews them
> and takes over.

Git layer:

> Refused: this repository belongs to project "`<name>`", not this session's. Keep the changes as they are (no revert,
> stash or reset) and call `<tool>` for a "`<name>`" session: list the changed files, say they are uncommitted in
> `<worktree path>`, and ask it to review them there and take over the rest of the work.

The worktree path is named because the receiving session, working in a worktree of its own, would not otherwise see
changes left uncommitted in the other project's main checkout or one of its worktrees.
