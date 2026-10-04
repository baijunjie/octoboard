# CLAUDE.md

## Language

This project is open source, so all prose committed to the repository must be in English — documentation, comments, commit
messages, user-facing strings, configuration. The only exception is internationalization resources, whose content stays in
its target language.

This applies to content brought in from elsewhere too: anything installed into the repository, such as a skill or a
template, gets translated to English before it is committed. Conversation with the user is not restricted.

<!-- setup-agent:subagents:begin -->

## Dispatching subagents

**The test: a task that will eat a lot of context — exploring, debugging, researching, bulk search-and-replace,
repeated trial — should go to a subagent wherever you can; you take only the conclusion.**

- Make small edits yourself: briefing a subagent costs more than making the change.
- Dispatch one when you need a clean perspective too: reviewing code you just wrote, or writing docs, where the
  context you are holding carries bias.
- One task per dispatch, with the completion criteria spelled out; give the full background (it knows none of the
  premises you did not write down); ask it for conclusions, not a replay of its process.
- Never let several subagents change the same place at once.
- If a dedicated subagent owns the job, give it to them; otherwise pick by task difficulty from the table below,
  and do not reach for the strongest every time.

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

- Any task that produces code changes is done in a dedicated git worktree on a dedicated branch; read-only investigation, answering questions, and anything else that produces no changes are exempt.
- **Before creating the worktree, and before getting ready to merge the branch back into its target branch, read the `git-worktree` skill first**, and follow its steps.
- Do not add `--no-verify`, set skip environment variables, or change the hook configuration to get around the repository's checks.

<!-- setup-git:worktree:end -->

<!-- setup-agent:workflow:begin -->

## Workflow

This section only sets what gets invoked before starting work and before delivery, in what order, and when to
skip a step; how each step is done is up to the corresponding skill, and when unsure, report back.

### Before starting work

- Make sure the branch is up to date first: after `git fetch`, check whether the current branch is behind the
  branch it merges back into, and align it before doing anything else.
- Pick up the project context via the `agent-docs` skill.

### Before delivery

The self-check targets one **complete delivery** covering all of this round's changes, not every single edit —
an intermediate commit does not count either; a user request to push, open a PR or merge counts as delivery too.
At delivery:

- If anything needs the user (a decision awaiting sign-off, a question to ask, a change the user said they
  wanted to see first, a significant call you made yourself), report back and wait for their feedback, then
  re-judge once you have acted on it.
- Otherwise, self-check item by item in the following order:

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
