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
- Never let several subagents change the same place at once, and never dispatch several to do the same thing
  concurrently: the former overwrite each other, the latter fight over resources and none of them converges.
- If you need its result to go on, explicitly ask for the dispatch to block until it finishes; do not assume a
  dispatch will wait, and do not wait on a timer or poll its status: for a subtask dispatched in the background, the
  report never reaches you, and your turn is handed back the moment it stops.
- Do not hand back without a result: a report that says "dispatched, waiting on it" in place of a conclusion counts
  as not done.
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

## Dispatching subagents in this project

- The rule on blocking for a result reaches any dispatch a subagent makes in turn — the change check, the
  writers — not only your own.
- The UI state gallery is written only through its own skill, which dispatches the subagent that owns it.

<!-- setup-git:worktree:begin -->

## Development workflow: Git worktree

- Any task that produces code changes is done in a dedicated git worktree on a dedicated branch; read-only investigation, answering questions, and anything else that produces no changes are exempt.
- **Before creating the worktree, and before getting ready to merge the branch back into its target branch, read the `git-worktree` skill first**, and follow its steps.
- Do not add `--no-verify`, set skip environment variables, or change the hook configuration to get around the repository's checks.

<!-- setup-git:worktree:end -->

<!-- setup-agent:workflow:begin -->

## Workflow

This section only sets what gets invoked before starting work and before delivery, in what order, and when to
skip a step; how each step is done is up to the corresponding skill.
When unsure, report back: stop, spell out what needs deciding (with the options) or what information is missing,
together with what you have already found out, and let the recipient of the report (the user, or the dispatcher
passing it on to the user) decide before going on; do not decide for yourself.

### Before starting work

- Make sure the branch is up to date first: fetch only the branch the current branch merges back into (no need to
  fetch everything), then check whether the current branch is behind it, and align it before doing anything else;
  aligning never rewrites the history of a branch that has been pushed and is not exclusively yours.
- Pick up the project context via the `agent-docs` skill.

### Before delivery

The self-check targets one **complete delivery** covering all of this round's changes, not every single edit —
an intermediate commit does not count either; a user request to push, open a PR or merge counts as delivery too.
At delivery:

- If anything needs the user (a decision awaiting sign-off, a question to ask, a change the user said they
  wanted to see first, a significant call you made yourself), report back and wait for their feedback, then
  re-judge once you have acted on it.
- Otherwise, self-check item by item in the following order:

1. **Gallery**: bring the gallery up to date and run the quick check via the `agent-gallery` skill; whether to do it
   and what scope to run is up to its judgement.
2. **Change check**: review this round's changes via the `agent-change-check` skill; only move on once the review
   passes, or the user has settled the remaining findings and the changes they decided on have been made.
3. **Documentation update**: when this round's changes call for updating the docs that later readers rely on,
   update them via the `agent-docs` skill.
4. **UI preview**: preview for the user via the `agent-gallery` skill; whether to preview and whether to wait for
   confirmation is up to its judgement.
5. **Formatting and checks**: first run the project's agreed formatting commands to rewrite the code, then run all
   of the project's agreed checks — format checks, type checking, lint, build, tests — and confirm they all pass;
   if they do not, fix and re-run all of the checks. Skip any the project has not agreed on.

The full formatting and checks run only in the "Formatting and checks" step, before the final commit and delivery
(push, PR or merge); the earlier steps do not run them separately (the related tests a step runs to confirm its
own change do not count).

- **You changed behavior, logic or UI again** (in response to the change check, to preview feedback, to pass the
  checks, or while resolving conflicts): walk this new change through again from the first relevant step; purely
  mechanical fixes such as formatting or type annotations do not go through again.
- **The same content is not re-run**: if the content has not changed since the self-check (such as only
  squashing), a later push, PR or merge does not redo it; if a rebase or merge brought in someone else's new code,
  re-run formatting and checks on the whole current content, the brought-in code included, not only the original
  change.
- **Clean-up**: after formatting and checks pass, and before the branch is merged or a PR is opened, clean up via
  the `agent-gallery` skill; do the same before this round's changes are abandoned or the working copy is deleted.

### This project

- Within "Documentation update": `docs/ui-accessibility.md`, the UI's accessibility standard, has no writer subagent
  of its own — you maintain it yourself, from what the change turned up and from what a writer subagent hands back as
  belonging to it. Skip it when this round's changes touch nothing it states.

<!-- setup-agent:workflow:end -->
