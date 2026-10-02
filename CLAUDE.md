# CLAUDE.md

## Language

This project is open source, so all prose committed to the repository must be in English — documentation, comments, commit
messages, user-facing strings, configuration. The only exception is internationalization resources, whose content stays in
its target language.

This applies to content brought in from elsewhere too: anything installed into the repository, such as a skill or a
template, gets translated to English before it is committed. Conversation with the user is not restricted.

<!-- setup-agent:subagents:begin -->

## Dispatching subagents

There are two reasons to dispatch a subagent — **to get a clean perspective** (reviewing code you just wrote,
or writing docs, where the context you are holding carries bias), or **to move mechanical work out of the
main context** (bulk search, locating code, bulk replacement).

**How to brief it:**

- One task per dispatch, with the completion criteria spelled out.
- Give the full background: it knows none of the premises you did not write down, so do not expect it to fill them in.
- Ask it for conclusions, not a replay of its process.
- Never let several subagents modify the same task scope at once.

**Who to pick: if a dedicated subagent owns the job, give it to them**; otherwise **pick by task difficulty,
do not reach for the strongest every time** — mechanical work such as bulk search and locating code goes to
the cheapest tier, and only architecture design and technology choices go to the strongest.

| agent | Use for |
|---|---|
| `mechanical` | Bulk search, locating code, bulk replacement, formatting, simple text and data processing |
| `implement` | Writing concrete logic, implementing a settled design, refactors and bug fixes with a clear target. The default value-for-money choice |
| `investigate` | Hard cross-module debugging, root-cause analysis, large-scale refactors |
| `architect` | Architecture design, cross-module rework plans, technology choices. Only when the strongest reasoning is genuinely needed |

### Claude Code model configuration

- A named subagent's model and effort come from its agent definition; do not override them when dispatching.
- With the built-in agents (`general-purpose` / `Explore` / `Plan`) you still pass `model` explicitly, but their
  effort always follows the main session, so do not use them for cheap bulk work.
- When adding or changing an agent definition, write only the tier name
  (model `fable` / `opus` / `sonnet` / `haiku`, effort `low` / `medium` / `high` / `xhigh` / `max`),
  never a specific model version, so the rule survives a change of model generation.

<!-- setup-agent:subagents:end -->

<!-- setup-git:worktree:begin -->

## Development workflow: Git worktree

- The target branch is whichever branch the current working copy has checked out when the worktree is created (the parent branch when nesting); "main working copy" below means the working copy that has the target branch checked out.
- Any task that produces code changes is done in a dedicated git worktree on a dedicated branch, with editing, building, testing and quality checks all happening inside it; the main working copy stays clean so it can be compared against the target branch at any time. Read-only investigation, answering questions and anything else that produces no changes are exempt. To inject code temporarily just to verify something, you may open a separate throwaway worktree with no branch (`git worktree add --detach`); changes in it are never committed and it is removed as soon as the check is done.
- The target branch is not necessarily the main branch; the development branch is cut from its latest commit and merged back into it in the end. When creating a branch you must record its target branch and branch point; miss that and a later session cannot find out where it should be merged back, nor check whether the merge reverted changes already on the target branch.
- Creating a worktree, aligning with the remote target branch, squashing commits, rebasing, merging back into the target branch, pushing the target branch, and reverting a commit or change already on the target branch all follow the steps in the `git-worktree` skill. Do not add `--no-verify`, set skip environment variables, or change the hook configuration to get around the repository's checks.
- When the project moves to merging through PRs, the PR process takes precedence: do not merge locally, and use the branch's recorded `worktreeTarget` as the PR's target branch.

<!-- setup-git:worktree:end -->

<!-- setup-agent:workflow:begin -->

## Workflow

This section only sets what gets invoked before starting work and before delivery, in what order, and when to
skip a step; how each step is done is up to the corresponding skill.

### Before starting work

Start by picking up the project context via the `agent-docs` skill.

### Before delivery

The self-check targets one **complete delivery**, not every single edit. Once all of this round's changes are
written:

- If anything needs the user (a decision awaiting sign-off, a question to ask, a change the user said they
  wanted to see first, a significant call you made yourself), report back and wait for their feedback, then
  re-judge once you have acted on it.
- If nothing does, go straight to the self-check and wrap up. Do the same when the user asks you to push,
  open a PR or merge.

An intermediate commit is not a delivery. Do as asked when the user explicitly says "review it now" or
"update the docs first" ahead of delivery.

At delivery, self-check in this order:

1. **Static checks**: run the project's agreed quality-check commands (formatting, type checking, lint and so
   on) and confirm they report nothing. Skip this if the project has no such commands.
2. **Change check**: review this round's changes via the `agent-change-check` skill; if you changed code in
   response to the findings, re-run the static checks and the affected tests first, then have it reviewed
   again. Only move on once the review passes, or the remaining findings have been handed to the user.
3. **Documentation update**: when this round's changes call for updating the docs that later readers rely on,
   update them via the `agent-docs` skill; if aligning with the docs changed code, re-run the static checks
   and the affected tests.

### This project

(None yet. Project-specific steps are written here one per entry, anchored on the step names above, e.g. "After 'Static checks': ..." or "Skip condition for 'Static checks': ...".)

<!-- setup-agent:workflow:end -->
