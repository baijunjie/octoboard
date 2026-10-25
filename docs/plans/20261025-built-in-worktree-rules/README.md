# Built-in Worktree Rules Development Plan

## Problem and approach

A project session that changes code works in the project's own working copy unless the project's instructions say
otherwise, so parallel sessions step on each other and the user's checkout moves under them. Octoboard is to give
every project session a short default worktree rule, which yields to a project's own worktree or branching rules, and
to let the user control it from a new **Worktrees** section in Settings, with a per-project override.

Alongside it, the revert gate this repository uses (it blocks a branch update that quietly undoes changes already on
the target branch) is built into Octoboard as a switch, so a project gets it without the user installing anything.

## Key design decisions

- **The rule is injected, never written into the project.** It goes into the project session's role description at
  launch, keeping the guarantee that Octoboard installs nothing into a project. Console sessions do not get it: they
  never change a project.
- **The project's own rules come first, and the agent judges that.** The rule opens by saying that where the project's
  instructions set out how to use worktrees or branches, those are followed instead. Octoboard does not scan
  instruction files for keywords: the agent reads them anyway, judges better than a keyword match, and the approach
  holds for every agent.
- **A rule change reaches only sessions begun after it.** Claude Code and Grok record the role text on the first turn
  and replay it on every resume; the Settings text says so.
- **Integration is not assumed to be a merge.** Work may end in a local merge or a pull request; how is set by the
  owner's or user's word, else by the delivery setting.
- **The default rule stays short:**
  1. Before changing files in a git repository, create a new branch from the current branch with its own worktree at
     `<project>/.worktrees/<short-name>`, adding the path to `.git/info/exclude` so no tracked file changes; work and
     commit there, running git as `git -C <worktree>` rather than after a `cd` (Claude Code asks approval for a `cd`
     before git). Read-only work (investigating, answering questions) needs none. The location sits inside the
     session's working directory, so agents do not ask permission for writes outside it.
  2. A branch handed over that already has a worktree is worked on there.
  3. When done, report the branch and its changes; merge, push or open a PR as the user or owner says, else as the
     delivery setting says.
  4. When merging: with rebase on, rebase onto the target branch and fast-forward; with it off, merge. A pushed branch
     that is not the session's alone is never rebased.
  5. The agent cleans up itself, whenever the worktree is no longer needed, rather than leaving it to the cleanup an
     archive runs:
     - After a local merge, remove the worktree and the local branch from the main working copy at once; this
       is part of the merge and is not asked again.
     - After a PR is opened, remove the worktree and the local branch at once (the agent cannot know when the PR is
       merged), leaving the remote branch. To change the PR later, recreate the worktree from the remote branch, push
       to the same branch, and clean up again.
     - A worktree opened only to verify something, never committed, is opened detached and removed as soon as the
       verification is done.
     - Uncommitted changes, or ignored files beyond regenerable dependencies and build output (`.env`, local
       configuration), are asked about first; nothing is discarded or force-removed on the user's behalf.
- **Settings**, application-wide with a per-project override where each item can follow the global value or set its
  own:

  | Item | Global default |
  |---|---|
  | Use worktrees for development | on |
  | Rebase before merging | on |
  | Delivery: ask / merge into the target branch / push and open a PR | ask |
  | Revert gate | on |

  The override covers every item, not only the main switch, because how work is delivered is what most often differs
  between repositories.
- **The built-in revert gate needs no install.** Octoboard already points `core.hooksPath` at its own hook
  dispatchers through each project session's environment; the gate runs from that `reference-transaction` dispatcher.
  What the gate needs (target branch, fork point, resident guarded branches) is recorded in Octoboard's own store, not
  in `.git/config`.
  - It is ported to Rust as a daemon subcommand rather than shipping the Python script: on a fresh macOS,
    `/usr/bin/python3` is a stub that prompts for the Command Line Tools.
  - It checks both a session's local branch updates (the `reference-transaction` dispatcher) and its pushes (the
    `pre-push` dispatcher): a branch pushed for a PR against its target branch's remote tip, and a guarded branch
    pushed over remote commits the clone does not have. Without the push check it would do nothing for a project that
    delivers through PRs, whose target branch never moves locally.
  - It guards only git run inside Octoboard sessions; the user's own terminal, IDE or git client is not covered, unlike
    the per-clone install.
  - It does not look for a gate a project installed itself: Octoboard minds its own feature, and the switch, per
    project, alone decides. A project that has its own gate and wants only that turns the built-in one off.
  - On by default: it blocks only a move that undoes merged content, so it costs normal work nothing.
  - It fails open: a failing hook fails every ref update, so any error of the gate itself lets the update through.
  - It reads the current setting when it runs, so the switch reaches running sessions at once.

## Milestones

- 01 Worktree settings (closed)
- 02 Injecting the worktree rule (closed)
- 03 Revert gate engine (closed)
- 04 Revert gate in the hook dispatcher (closed)
- [05 Settings UI](05-settings-ui.md)
