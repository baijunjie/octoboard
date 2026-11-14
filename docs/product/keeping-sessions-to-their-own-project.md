# Keeping sessions to their own project

The **cross-project guard** stops a project session from editing or committing in another registered project, and has
it hand that work to a session of the project the work belongs to. It is off until the user turns on **Keep sessions to
their own project** (see that section in `docs/product/agent-behavior-settings.md`), and has two checks behind that one
switch:

- **the edit check**, which refuses a session's file-edit tool call that targets a path inside another project (see
  "The edit check" below);
- **the git check**, which refuses a session's git an update of a branch in a repository that belongs to another project
  (see "The git check" below).

Both apply to project sessions only; a console session is never checked. "Another project" is any project registered
with Octoboard other than the session's own, whatever console it is in. Both checks fail open: whenever a check cannot
decide, the edit or the update goes through, saying nothing.

## The edit check

### Which sessions have it

The edit check is set up at the launch of a project session made while the switch is on, every launch included — a new
session, a resume and the relaunch of an account switch alike — and the session keeps it for as long as that launch's
process runs. So turning the switch on reaches only sessions launched afterwards, and turning it off leaves the edit
check in sessions launched while it was on until they are next launched. The check judges by the projects registered
when the session launched: a project registered afterwards is not covered until the session's next launch.

### Which calls it checks

A synchronous `PreToolUse` hook, with a timeout of **10 seconds**, is run before each call of the agent's file-edit
tools (how each agent is given it is in "How each agent is given the injection" in
`docs/product/per-agent-launch-specifics.md`):

- **Claude Code**: `Edit`, `Write`, `MultiEdit` and `NotebookEdit`;
- **Codex**: `apply_patch`, judged by every file its patch names, a move's destination included;
- **Grok Build**: its edit tools, under whichever of their names it calls them by: Claude Code's four above,
  `search_replace`, `write`, `hashline_edit`, `StrReplace` and `EditNotebook`.

Shell commands are not checked, so a file a session writes through its shell is caught only by the git check, when it
is committed. A call whose target cannot be read, or a relative target with no working directory to resolve it against,
is let through.

### Which targets it refuses

A target is refused when it lies inside the directory of another project, unless:

- the deepest root that holds it is **the session's own**: its project's directory, or the directory the session was
  launched in, which can be a worktree of the project outside the project's directory;
- the deepest root that holds it is **a shared directory** (see "Shared directories" below);
- it lies in **the session's own repository**, wherever that repository's working tree is: a linked worktree, made
  before or after the launch, or the directory of another project in the same repository (a monorepo's sibling project)
  is the session's own.

Of the roots that hold a target, the deepest one decides. So a project registered at an ancestor of others, the home
directory for one, still has everything below it guarded, except what a deeper root of the session's own or a shared
directory holds; and a shared directory or a root of the session's own passes a target only where it lies within the
deepest project that holds that target. Another project registered in the session's own repository is never counted as
another project's; a project whose repository cannot be told within a few seconds at launch is counted as another
project's.

Places are told apart by what each path leads to, not by its text: a target is resolved to its nearest existing
directory, since a file about to be created has none of its own, and compared by identity, so letter case, symbolic
links and firmlinks do not change the outcome.

### Shared directories

These pass whatever project holds them:

- every agent's configuration and data directories: every account's configuration directory, and each agent's default
  configuration directory as the session's shell resolves it, together with the usual one under the home directory
  (`~/.claude`, `~/.codex`, `~/.grok`);
- Octoboard's data directory, the sessions' scratch directories included (see `docs/product/files-octoboard-owns.md`);
- the system's temporary directories, and those the session's shell names in `TMPDIR`, `TEMP` or `TMP`, except one that
  is the root of a disk or holds the home directory.

### What the agent is told

A refused call does not run. The agent is given a refusal that names the project the target is in, says not to write
it another way, to keep any changes already made there as they are, and to call `request_session` (under the agent's
own name for it) for a session of that project, listing the changed files and where they are, so that session reviews
them and takes the work over. The target's full path comes last. Grok Build shows the model only the first 256
characters of the refusal, so the hand-over comes first and, for a project name of up to about 15 characters, fits
within them, leaving only the path to be lost; a longer name is not cut.

### When it cannot decide

The check decides from the list written at launch alone and never contacts the daemon, so it works while the daemon is
unreachable. It lets the call through, printing nothing, when the list is missing or unreadable, the hook's input is not
one it can read, or a target cannot be resolved; a hook killed at its timeout lets the call through too. When the list
cannot be built or written at launch, the session launches without the edit check.

## The git check

### What it refuses

While the switch is on, a git run inside a project session is refused an update that moves an existing branch — a commit
on a branch, a merge, a rebase, a reset, a forced `git branch -f` or `git update-ref` — in a repository that another
project is in and the session's own project is not. The refusal comes when the transaction is prepared, after the user's
own `reference-transaction` hook has passed it (see "The session's git hooks" in
`docs/product/branch-responsible-session.md`), and fails the git command (the hook exits with status 3, a commit with
128); the revert gate is not run for it.

These pass:

- the session's own repository, its linked worktrees included, and a repository the session's project shares with
  other projects;
- a repository no registered project is in, a test suite's temporary repository for one;
- creating or deleting a branch, a commit on a detached `HEAD`, the first commit of a branch that does not exist yet,
  and a push.

### When it applies

The switch is read at every update, so turning it on or off reaches running sessions at once, without relaunching them,
whether or not they were launched with the edit check. `REVERT_GATE_SKIP=1` does not skip it: that variable still skips
only the revert gate (see "A deliberate revert" in `docs/product/revert-gate.md`), and the hook asks the daemon all the
same.

### What the agent is told

The refusal, on git's error output, names the project the repository belongs to, says to keep the changes as they are
(no revert, stash or reset), and to call `request_session` (under the agent's own name for it) for a session of that
project, listing the changed files, saying they are uncommitted in the working tree the git ran in, and asking it to
review them there and take over the rest of the work.

### When it cannot decide

The check needs the daemon's word. Wherever the revert gate's question would get no answer — no route to a daemon, a
daemon that does not answer in time or answers with something unreadable (see "When it cannot decide" in
`docs/product/revert-gate.md`) — the update goes through; and so it does when the daemon cannot tell which project the
repository belongs to.

## What neither check covers

- A console session, and the user's own terminal, IDE or git client.
- A git that does not reach the session's hook dispatchers (see "Where a branch has no responsible session" in
  `docs/product/branch-responsible-session.md` for a git run without the session's environment or told to look
  elsewhere for hooks).
- A file written through a shell command, until it is committed in another project's repository.
- Changes left uncommitted in another project: neither check reverts anything.
