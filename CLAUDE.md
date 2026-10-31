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

## Repository checks

- Do not add `--no-verify`, set skip environment variables (such as `REVERT_GATE_SKIP`), or change the hook
  configuration to get around the repository's checks or Octoboard's revert gate.

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
2. **UI preview**: as soon as the previous step is done, preview for the user via the `agent-gallery` skill, without
   waiting for the later steps; whether to preview and whether to wait for confirmation is up to its judgement. UI the
   user said they wanted to see first is shown at this step too, not reported before the self-check. Where it has to
   wait for confirmation, or for the user's answer on whether temporary scenarios stay, move on only once the answer is
   in: the user may still change the UI or keep scenarios, and running the later steps first would be wasted.
3. **Change check**: review this round's changes via the `agent-change-check` skill; only move on once the review
   passes, or the user has settled the remaining findings and the changes they decided on have been made.
4. **Documentation update**: when this round's changes call for updating the docs that later readers rely on,
   update them via the `agent-docs` skill.
5. **Formatting and checks**: first run the project's agreed formatting commands to rewrite the code, then run the
   checks by the scope of the change and confirm they all pass; if they do not, fix, then re-determine the scope from
   the changes as they now stand and run again (a fix that changes behavior, logic or UI goes through "You changed
   behavior, logic or UI again" below).
   Where this delivery goes through the project's merge flow and that flow aligns the branch with the latest target
   branch, only format and commit this round's changes here, and run the checks once, after it aligns and before the
   merge or push: by then the target branch's new code is in, so there is no need to check once before aligning and
   again after. Whatever the merge flow's own verification after aligning covers is not run separately; checks it does
   not cover are run right after, before the user is asked to confirm the merge or push.
   Where behavior, logic or UI is changed within the merge flow to pass the checks or resolve conflicts, stop before
   the merge or push, go through "You changed behavior, logic or UI again" below, then return to the merge flow.
   Without a merge flow, or with one that does not align, run them here; checks deferred to the merge flow that did
   not get run there (the flow was aborted, or delivery no longer goes through it) are run here before delivery —
   formatting and committing alone do not count as having run the checks. Scope:

   - **The full set** is the check commands the project states in its instruction file or docs (whichever of format
     checks, type checking, lint, build, and tests it states). If it states none, take the local check commands of those
     kinds that the project configuration already has as the full set (excluding anything with external side effects,
     such as publishing or deploying), and tell the user to write the full set into the instruction file; if there are
     none at all, tell the user there are no checks to run this time.
   - **Changes only to files that the build, type checking, and tests never read** (usually docs, instruction files, and
     skill and subagent definitions; files of this kind that a build or test validates, and the configuration files of
     format checks and lint, do not count): run only the format checks and lint that cover those files; if there are
     none, run nothing and tell the user.
   - **Apart from files of that kind, changes only within one package or module**, for which the project states its own
     check commands in its instruction file or docs, and which no other package depends on: run that package's checks,
     and run the checks for the files outside the package as in the previous bullet.
   - **Everything else runs the full set**: changes to a shared layer that many places depend on (shared code and types,
     a package that other packages depend on, the configuration of formatting, type checking, build, tests, and lint,
     dependency versions), changes spanning several packages, or a case that cannot be judged.

   The changes that set the scope are only the branch's own: all of its changes relative to the point where it diverged
   from the target branch (relative to before the change, for changes made directly on the target branch). Code a
   rebase or merge brings in from the target branch does not count as a change, and code on the target branch is
   treated as already checked; count from the new divergence point afterwards, with the places changed while resolving
   conflicts included. The grouping in the bullets above (whether a file is read by the build or tests, whether a
   package is depended on by other packages, which check configurations apply), however, is judged on the code after
   the rebase or merge. Where new code brought in would call for a re-run, none is needed when the branch's changes are
   all of the "files that the build, type checking, and tests never read" kind, those files are the same as when the
   format checks and lint covering them passed in this session, and the code brought in since has not touched the
   configuration of those format checks and lint, the commands written down for them, or the tool versions: the check
   results cannot change then.

Formatting and checks run only as the "Formatting and checks" step sets out, before delivery (push, PR or merge);
the earlier steps do not run them separately (the related tests a step runs to confirm its own change do not count).

- **You changed behavior, logic or UI again** (in response to the change check, to preview feedback, to pass the
  checks, or while resolving conflicts): walk this new change through again from the first relevant step; purely
  mechanical fixes such as formatting or type annotations do not go through again.
- **The same content is not re-run**: if the content has not changed since the self-check (such as only
  squashing), a later push, PR or merge does not redo it; if a rebase or merge brought in new code from the target
  branch after the checks, re-run the checks (the scope above sets what to run and the cases that need no re-run).
- **Clean-up**: before delivery (push, PR or merge), clean up via the `agent-gallery` skill; do the same before this
  round's changes are abandoned or the working copy is deleted.

### This project

- Within "Change check": when this round's changes touch the UI (components, styles, UI copy and catalogs, how an
  error or a state is shown), also dispatch the `ui-checker` subagent, in parallel with `change-checker`, handing it
  the changed files, the requirement and the trade-offs the user settled (including any rule of the UI standard or
  the copy conventions this round changes), the gallery scenarios that show the changed states, and the address of
  any gallery this checkout is serving; work through its findings as `agent-change-check` does with
  `change-checker`'s, and re-dispatch it until it passes.
- Within "Documentation update": `docs/ui-accessibility.md`, the UI's accessibility standard, has no writer subagent
  of its own — you maintain it yourself, from what the change turned up and from what a writer subagent hands back as
  belonging to it. Skip it when this round's changes touch nothing it states.
- Timing override for "Formatting and checks", including the pre-merge flow and other skills' verification
  baselines: by the user's explicit workflow choice, routine development completion and local merges do not run
  format checks, lint, type checks, builds or test suites. Checks used specifically to establish a changed rule
  while implementing it remain appropriate; they do not replace push verification.
- Before committing, the `pre-commit` hook runs `scripts/format-staged.mjs` (the same as `pnpm format:staged`):
  format only staged supported source files, preserving unstaged edits. A partial commit (`git commit <paths>`) is
  refused; stage the files and commit without paths. A checkout without the script (a branch older than the hook)
  gets a warning and the stage is skipped; a missing `node_modules` fails it with a hint to run `pnpm install`. Do not
  run a whole-repository formatter as a delivery step. `pnpm install` in the main working copy, on the main
  branch, installs the hooks (`.githooks/auto-install.sh`); after hook changes have landed in the main branch, run
  it there again to refresh them (`pnpm hooks:install` does the same explicitly; run it on the main branch, since it installs the
  checkout's own `hook.sh`).
- Before pushing, the `pre-push` hook runs the checks through `scripts/pre-push.mjs` and
  `pnpm verify`. It measures from each actual destination's advertised remote commit to the exact pushed commit,
  including all unpushed local changes; a newly created branch is measured from its merge base with the live remote
  target branch (or remote HEAD), falling back to all checks when no baseline can be established. Tag pushes are not
  verified here, since release CI verifies the tag, and a checkout without `scripts/pre-push.mjs` skips this stage
  with a warning. It selects all affected consumers without adding unrelated packages merely because several
  packages changed: shared inputs select their consumers, and an unknown input selects the full set. Rust changes
  always run the Rust checks of both crates together. Checks are read-only and require the pushed commit in a clean
  checkout. `scripts/verify/` owns the scope and commands. `pnpm verify --dry-run` prints the plan; `--base <ref>`
  selects an explicit baseline; `--full` checks all packages; `--fresh` ignores earlier successful results for
  identical content.
- Within "Gallery" and "UI preview": continue gallery maintenance and previews at their existing steps, but defer
  the automated gallery quick check and sweep to before pushing, when the UI changes call for them; do not run
  them again at routine development completion or local merge. The agent runs them via the `agent-gallery` skill
  before pushing when the UI changed; the pre-push hook does not.
- Release verification is unconditional in CI: `quality.yml` runs `pnpm verify --release --fresh` before
  `release.yml` builds the app. It covers application lint, production UI type checks, application UI and desktop
  script tests, Rust formatting and tests of both Rust crates. Website and gallery-only checks are excluded;
  application tests may still consume their shared fixtures. A second job compiles both Rust crates on Linux
  (`cargo check`), which no local check covers.

<!-- setup-agent:workflow:end -->
