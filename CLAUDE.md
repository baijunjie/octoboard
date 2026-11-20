# CLAUDE.md

## Language

This project is open source, so all prose committed to the repository must be in English — documentation, comments, commit
messages, user-facing strings, configuration. The only exception is internationalization resources, whose content stays in
its target language.

This applies to content brought in from elsewhere too: anything installed into the repository, such as a skill or a
template, gets translated to English before it is committed. Conversation with the user is not restricted.

<!-- setup-agent:subagents:begin -->

## Dispatching subagents

**The test: by default, do the work yourself. Only in a long task (carrying out a development plan, a large batch of
changes or an investigation spanning several modules) hand a whole block of work that would eat a lot of context to a
subagent, and take only the conclusion: briefing a subagent on a small task costs more than finishing it yourself.**
The self-checks before delivery (adding tests, the change check, updating the docs and the like) and the fixes they
call for are dispatched as the workflow, their skills and the rules below say, and this test does not limit them.

- When you are a subagent, do only the task dispatched to you, and do it yourself and hand it back: dispatch no further
  subagents, do not push, merge or open a PR, and skip the self-checks before delivery in the instruction file's
  workflow; to confirm the change works, run only the tests that directly cover this change of yours, not the full test
  suites, lint, type checking or builds. Where the task dispatched to you is itself one of the self-checks (such as
  adding tests or a review), do it as the dispatch and your own definition say. Before starting, only read the project
  context, with no fetch and no branch alignment. The self-checks are run by whoever dispatched you, once all the tasks
  have been handed back. Subagents that each dispatch again and each self-check do so uncoordinated, and together
  saturate the machine.
  A subagent is an agent dispatched by another agent to do a specific task and hand it back to that agent when done, however
  it was dispatched (a subagent mechanism, a separately opened session and so on) and whether or not it works on a branch
  or working copy of its own: when the task description, your definition or your system prompt says you hand back to
  another agent, you are a subagent; delivery is done only by the agent answering directly to the user; when the user
  directly asks a subagent to push, merge or open a PR, it becomes the deliverer and runs the workflow's self-checks
  before delivering. When dispatching a subagent, write "you are a subagent; hand back when done" in the task description:
  some subagents can only recognize their role from there.
- One task per dispatch, with the completion criteria spelled out; give the full background (it knows none of the
  premises you did not write down); ask it for conclusions, not a replay of its process.
- Never let several subagents change the same place at once, and never dispatch several to do the same thing
  concurrently: the former overwrite each other, the latter fight over resources and none of them converges.
- If you need its result to go on, explicitly ask for the dispatch to block until it finishes; do not assume a
  dispatch will wait, and do not wait on a timer or poll its status: for a subtask dispatched in the background, the
  report never reaches you, and your turn is handed back the moment it stops.
- Do not hand back without a result: a report that says "dispatched, waiting on it" in place of a conclusion counts
  as not done.
- Where the code that a self-check's findings or a failed check call for changing was written by a subagent, hand it
  back to that subagent to fix: resume the original one where you can, otherwise dispatch a new one with the findings
  and the original task description; in that case, where a self-check skill says "you fix" or "you fix the source", it
  means you arrange the fix. You only judge whether each item gets fixed (giving your reasons where you disagree) and
  which go to the user to decide; where you need to look at the code to judge, look only at the few relevant places, and
  do not make the fix yourself: the context of writing the code would come back to you, and dispatching the subagent
  would have been for nothing.
- If a dedicated subagent owns the job, give it to them; otherwise pick by task difficulty from the table below,
  and do not reach for the strongest every time.

| agent | Use for |
|---|---|
| `mechanical` | Bulk search, bulk replacement, formatting, simple text and data processing within a long task |
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

## Coding conventions

- The coding conventions in `docs/conventions/` (listed in `docs/README.md`, "Coding conventions") are binding: code
  follows them, and a change that breaks one is not done unless the user accepts it as an exception. Read the ones a
  change touches before designing it, not only at review; where one seems wrong for the case, report back instead of
  deviating.
- The rule most often broken is the provider-neutral core (`docs/conventions/provider-neutral-core.md`): anything that
  differs between agents lives in that agent's provider, and generic code never branches on which agent it is. Check it
  before designing a feature that behaves differently per agent.
- A brief to a subagent or a session that writes or reviews code names the convention docs that apply and says where
  each piece of code goes; for work touching agent differences, which provider holds each agent-specific piece. A brief
  that prescribes behavior without saying where it lives invites a broken rule.

## Repository checks

- Do not add `--no-verify`, set skip environment variables (such as `REVERT_GATE_SKIP`), or change the hook
  configuration to get around the repository's checks or Octoboard's revert gate.
- Run Rust and UI tests, clippy and the Windows cross-check only through the package scripts, never `cargo test`,
  `cargo clippy` or `vitest` directly: `pnpm test:rust [cargo arguments]`, `pnpm clippy:rust`, `pnpm check:windows`,
  `pnpm --filter @octoboard/ui test [<file>]`, `pnpm --filter @octoboard/ui test:related <files>` and
  `pnpm --filter @octoboard/ui test:app`. They wait for the machine-wide heavy-step slot `pnpm verify` uses
  (`scripts/verify/heavy.mjs`), so sessions testing at once queue instead of running the machine out of memory; a wait
  message names what holds the slot. The wait can take minutes, so run them with a long timeout or in the background.
  A UI run that names at most eight test files (`pnpm --filter @octoboard/ui test <files>`) runs at once instead, so
  name the test files rather than a directory or a pattern when checking a few, and put no `--` or flag without `=`
  before them.
- Before committing, the `pre-commit` hook runs `scripts/hooks/format-staged.mjs` (the same as `pnpm format:staged`):
  format only staged supported source files, preserving unstaged edits. A partial commit (`git commit <paths>`) is
  refused; stage the files and commit without paths. A checkout without the script (a branch older than the hook)
  gets a warning and the stage is skipped; a missing `node_modules` fails it with a hint to run `pnpm install`. Do not
  run a whole-repository formatter as a delivery step. `pnpm install` in the main working copy, on the main branch,
  installs the hooks (`.githooks/auto-install.sh`); after hook changes have landed in the main branch, run it there
  again to refresh them (`pnpm hooks:install` does the same explicitly; run it on the main branch, since it installs
  the checkout's own `hook.sh`).

<!-- setup-agent:workflow:begin -->

## Workflow

This section only sets what gets invoked before starting work and before delivery, in what order, and when to
skip a step; how each step is done is up to the corresponding skill.
When unsure, report back: stop, spell out what needs deciding (with the options) or what information is missing,
together with what you have already found out, and let the recipient of the report (the user, or the dispatcher
passing it on to the user) decide before going on; do not decide for yourself.

- When you are a subagent, do only the task dispatched to you, and do it yourself and hand it back: dispatch no further
  subagents, do not push, merge or open a PR, and skip the self-checks before delivery in the instruction file's
  workflow; to confirm the change works, run only the tests that directly cover this change of yours, not the full test
  suites, lint, type checking or builds. Where the task dispatched to you is itself one of the self-checks (such as
  adding tests or a review), do it as the dispatch and your own definition say. Before starting, only read the project
  context, with no fetch and no branch alignment. The self-checks are run by whoever dispatched you, once all the tasks
  have been handed back. Subagents that each dispatch again and each self-check do so uncoordinated, and together
  saturate the machine.
  A subagent is an agent dispatched by another agent to do a specific task and hand it back to that agent when done, however
  it was dispatched (a subagent mechanism, a separately opened session and so on) and whether or not it works on a branch
  or working copy of its own: when the task description, your definition or your system prompt says you hand back to
  another agent, you are a subagent; delivery is done only by the agent answering directly to the user; when the user
  directly asks a subagent to push, merge or open a PR, it becomes the deliverer and runs the workflow's self-checks
  before delivering. When dispatching a subagent, write "you are a subagent; hand back when done" in the task description:
  some subagents can only recognize their role from there.

### Before starting work

- Make sure the branch is up to date first: fetch only the branch the current branch merges back into (no need to
  fetch everything), then check whether the current branch is behind it, and align it before doing anything else;
  aligning never rewrites the history of a branch that has been pushed and is not exclusively yours.
- Pick up the project context via the `agent-docs` skill.

### Before delivery

The self-check targets one **complete delivery** covering all of this round's changes, not every single edit —
an intermediate commit does not count either; changes a subagent hands back (including those it made on a branch of its
own and you merge in) count as this round's changes too, merging them in is not a delivery, and the self-check runs once
all of them are in; the self-check runs even when you did not touch the code yourself. A user request to push, open a PR or merge counts as
delivery too.
At delivery:

- If anything needs the user (a decision awaiting sign-off, a question to ask, a change the user said they
  wanted to see first, a significant call you made yourself), report back and wait for their feedback, then
  re-judge once you have acted on it.
- Otherwise, self-check item by item in the following order:

1. **Unit tests**: add tests for this round's changes via the `agent-unit-test` skill.
2. **Gallery**: bring the gallery up to date via the `agent-gallery` skill; whether to do it is up to its judgement.
3. **UI preview**: as soon as the previous step is done, preview for the user via the `agent-gallery` skill, without
   waiting for the later steps; whether to preview and whether to wait for confirmation is up to its judgement. UI the
   user said they wanted to see first is shown at this step too, not reported before the self-check. Where it has to
   wait for confirmation, or for the user's answer on whether temporary scenarios stay, move on only once the answer is
   in: the user may still change the UI or keep scenarios, and running the later steps first would be wasted.
4. **Change check**: review this round's changes via the `agent-change-check` skill; only move on once the review
   passes, or the user has settled the remaining findings and the changes they decided on have been made.
5. **Documentation update**: when this round's changes call for updating the docs that later readers rely on,
   update them via the `agent-docs` skill.
6. **Formatting and checks**: first run the project's agreed formatting commands to rewrite the code, then run the
   checks by the scope of the change and confirm they all pass; if they do not, fix, then re-determine the scope from
   the changes as they now stand and run again (a fix that changes behavior, logic or UI goes through "Changed
   behavior, logic or UI again" below).
   Where this delivery goes through the project's merge flow and that flow aligns the branch with the latest target
   branch, only format and commit this round's changes here, and run the checks once, after it aligns and before the
   merge or push: by then the target branch's new code is in, so there is no need to check once before aligning and
   again after. Whatever the merge flow's own verification after aligning covers is not run separately; checks it does
   not cover are run right after, before the user is asked to confirm the merge or push.
   Where behavior, logic or UI is changed within the merge flow to pass the checks or resolve conflicts, stop before
   the merge or push, go through "Changed behavior, logic or UI again" below, then return to the merge flow.
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

   Where it is already settled that the checks will run again later on code that contains this change (run by you, later
   in this same task), and this change can stay local until that run passes (not pushed, no PR; a merge flow that
   pushes at once or merges on the remote does not count), do not run them now: that run's result stands in for this
   one. For example, when several branches are merged locally into the same target branch one after another, each later
   branch is checked only once aligned with the target branch that already holds the earlier changes, so the checks run
   once, on the final merge result. A check run to decide whether a branch may be rewritten (such as verifying its
   unpushed commits before rebasing it onto its target branch while aligning) runs on the spot and is not stood in for
   by a later one. When asking the user to confirm a merge or reporting check results, say that this change has not
   been checked yet and which run stands in for it. When that run sets its scope, the stood-in changes count as changes,
   not as already checked because they are now in the target branch; if that run fails, trace which change the failing
   part comes from, and fix one that belongs to the stood-in changes on its own, not mixed into another branch's
   commits; if that run does not complete, run the checks on the current code with the scope set by the stood-in
   changes. When unsure whether that run will certainly happen, or whether it will contain this change, run them as
   usual.

Formatting and checks run only as the "Formatting and checks" step sets out, before delivery (push, PR or merge);
the earlier steps do not run them separately (the related tests a step runs to confirm its own change, and the tests
the test-adding subagent runs as its definition says, do not count).

- **Changed behavior, logic or UI again** (in response to the change check, to preview feedback, to pass the checks,
  or while resolving conflicts, including changes handed to a subagent): walk this new change through again from the
  first relevant step; purely mechanical fixes such as formatting or type annotations do not go through again.
- **The same content is not re-run**: if the content has not changed since the self-check (such as only
  squashing), a later push, PR or merge does not redo it; if a rebase or merge brought in new code from the target
  branch after the checks, re-run the checks (the scope above sets what to run and the cases that need no re-run).
- **Clean-up**: before delivery (push, PR or merge), clean up via the `agent-gallery` skill; do the same before this
  round's changes are abandoned or the working copy is deleted.

### This project

- Within "Before starting work": when creating a branch to work on, record the branch it merges back into with
  `git config branch.<name>.targetBranch <target branch>`, so the checks measure the branch against its own target
  (`pnpm verify --quick` from the local target, the pre-push hook a new branch from the remote one) instead of
  guessing it from the remote HEAD.
- UI copy: where `docs/conventions/ui-standard.md` and the `knowledge-i18n-copy` skill disagree, the UI standard wins (a
  name referenced within copy is an `InlineReference`, not quoted). Whoever writes or translates UI copy reads both.
- Within "Documentation update": `docs/conventions/ui-standard.md`, the UI standard, has no writer subagent of its own —
  you maintain it yourself, from what the change turned up and from what a writer subagent hands back as belonging to
  it. Skip it when this round's changes touch nothing it states.
- Timing override for "Formatting and checks", including the pre-merge flow and other skills' verification baselines: by
  the user's explicit workflow choice, routine development completion and local merges do not run the full checks.
  Before a local merge only, once the branch is aligned with its target branch and right before merging, run the quick
  check, `pnpm verify --quick`, and do not merge until it passes; it runs at no other point, neither at commits nor at
  development completion. When it fails only on tests that look intermittent (unrelated to the branch's changes), rerun
  just the failed test files on their own; if they pass, the quick check counts as passed, without rerunning the whole
  quick check (by the user's instruction). Skip it when the branch's changes are only files that the build, type
  checking, and tests never read. The full checks, builds and full test suites included, run only before pushing. Checks
  used specifically to establish a changed rule while implementing it remain appropriate; neither they nor the quick
  check replace push verification.
- Before pushing, the `pre-push` hook runs the checks through `scripts/hooks/pre-push.mjs` and
  `pnpm verify`. It measures from each actual destination's advertised remote commit to the exact pushed commit,
  including all unpushed local changes; a newly created branch is measured from its merge base with the live remote
  target branch (or remote HEAD), falling back to all checks when no baseline can be established. Tag pushes are not
  verified here, since release CI verifies the tag, and a checkout without `scripts/hooks/pre-push.mjs` skips this stage
  with a warning. It selects all affected consumers without adding unrelated packages merely because several
  packages changed: shared inputs select their consumers, and an unknown input selects the full set. Rust changes
  always run the Rust checks of both crates together. Checks are read-only and require the pushed commit in a clean
  checkout. `scripts/verify/` owns the scope and commands. `pnpm verify --dry-run` prints the plan; `--base <ref>`
  selects an explicit baseline; `--full` checks all packages; `--fresh` ignores earlier successful results for
  identical content. `pnpm verify --quick` runs the same scope without builds or the tests that need them, and runs
  only the UI and Rust tests related to the changed files in place of the full suites, falling back to a full suite
  where a change's reach cannot be traced; a quick run measures a branch with no upstream of its own (including one
  that only tracks its target's remote branch) from its merge base with its local target branch
  (`branch.<name>.targetBranch`, else the local branch it tracks, else the local branch the remote HEAD names), so the
  target's own commits, pushed or not, are not counted; with no local target, or when the branch is the target itself,
  it falls back to the remote target branch (or remote HEAD), as a new branch is at push.
- Within "Gallery" and "UI preview": continue gallery maintenance and previews at their existing steps, but defer
  the automated gallery quick check and sweep to before pushing, when the UI changes call for them; do not run
  them again at routine development completion or local merge. The agent runs them via the `agent-gallery` skill
  before pushing when the UI changed; the pre-push hook does not.
- Before creating a release tag: `release:version` runs `pnpm verify --release --fresh --head <release-commit>`
  locally before push verification. This application check is unconditional, regardless of the changed files or
  earlier successful checks, and includes the Windows cross-check of both crates and a link of the daemon's
  executable (`pnpm check:windows`), which fails, naming what to install, on a machine without its toolchain.
  `release.yml` validates and publishes the tag without calling `quality.yml`; the latter remains available on
  request, including its Linux compilation job, and does not gate publication. For a release with Linux in it,
  `release.yml` runs the daemon's Linux checks itself (`linux-daemon.yml`: clippy, the tests on glibc with a writable
  cgroup v2 hierarchy and on musl for both architectures, which `pnpm verify` on macOS cannot run), and signs and
  publishes only once they pass.

<!-- setup-agent:workflow:end -->
