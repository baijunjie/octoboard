---
name: agent-change-check
description: Review this round's changes and work through the comments until it passes; this should come to mind once the changes are done, before delivery. Use when the user says "review this round of changes", "wrap-up review", "check the changes I just made".
---

# Change check

When you are a subagent, do not follow this skill: finish and hand back, and let whoever dispatched you arrange it.
A subagent is an agent dispatched to do one job, which hands its conclusions or changes back to the dispatcher, who is
responsible for bringing them into the branch; an agent whose deliverable is a branch (no matter who ends up pushing or
merging it) is not a subagent.

## Project conventions

"Project conventions" is filled in only by the installer and is not changed at run time: when an item you need is still
`<…>`, the command or path filled in has itself stopped working (unrelated to this change; a missing database, network
or other environment condition is reported as usual and does not count), or an item needs to follow a change in the
project, stop and ask the user to rerun the installer that installed this skill, saying which item needs changing and
why.

- Coding conventions:
  - `docs/conventions/provider-neutral-core.md`
  - `docs/conventions/daemon.md`
  - `docs/conventions/daemon-processes-and-io.md`
  - `docs/conventions/error-codes.md`
  - `docs/conventions/protocol-and-contracts.md`
  - `docs/conventions/ui-code.md`
  - `docs/conventions/ui-components.md`
  - `docs/conventions/ui-standard.md`
  - `docs/conventions/i18n-in-code.md`
  - `docs/conventions/desktop-shell.md`
  - `docs/conventions/website.md`
  - `docs/conventions/docs-site.md`
  - `docs/conventions/comments.md`
  - `docs/conventions/tests.md`
  - `docs/conventions/tooling.md`
  - `.claude/skills/knowledge-i18n-copy/SKILL.md`

  The UI conventions are `docs/conventions/ui-standard.md` (the whole document), `docs/conventions/ui-components.md`,
  `docs/conventions/ui-code.md`, `docs/conventions/i18n-in-code.md`, `docs/conventions/website.md`,
  `docs/conventions/docs-site.md`, the UI sections of `docs/conventions/provider-neutral-core.md`, and the UI copy
  conventions in `.claude/skills/knowledge-i18n-copy/SKILL.md`.
- UI code:
  - `packages/ui/`
  - `apps/web/`
  - `apps/docs/`

When "UI code" says "unspecified", judge from the content of this change. When this change adds UI code outside the
listed paths, do not interrupt the review: dispatch as usual, and where `ui-checker` is not installed, do not dispatch it
and let `change-checker` review the UI as well; when reporting, ask the user to rerun the installer that installed this
skill. When "UI code" is "none", having no `ui-checker` is expected and does not count as an incomplete installation.

## Review

1. **Dispatch review subagents to review this round's changes; do not review what you wrote yourself**: your own
   context treats the structure you just wrote as settled fact.
   `change-checker` reviews all the code and is dispatched every round; when this change touches UI code (stylesheets
   and copy resources included), also dispatch `ui-checker` in parallel to review the UI side.
   Hand over what changed this round (that scope's, when split by scope), what the requirement is, whether `ui-checker`
   was also dispatched this time, and the trade-offs the user settled over several rounds of revision — they cannot see
   your conversation, and anything you leave out comes back reported as a problem.
2. **Work through the comments one by one**: fix the ones you agree with; where you think it is the convention rather
   than the code that should change, first recheck it against the standard in "Convention suggestions": one that falls
   short goes back to this step to be judged again; one that qualifies goes to the user together with the convention
   suggestion, and the comment does not count as passed until it is settled — if the user does not agree to change the
   convention, change the code as the comment said; for the ones you do not agree with, explain your reasoning, and when
   you cannot reach agreement, or a comment would overturn a trade-off the user settled, leave it to the user to decide,
   and work through the other comments as usual without waiting for it.
3. **Review again once the fixes are in**, until it passes: re-dispatch per step 1 for the scope the fixes touched (when
   split by scope, only the scopes the fixes touched); in the re-review, say how each comment and convention suggestion
   of the previous round was handled (which were changed, which were not and why, which went to the user or have been
   settled by the user), so the same one is not reported again.
   When everything else passes and some comments are still with the user, report back; once changed per the user's
   decision, review again as well, and only a pass counts as passing. In your final reply, state how many rounds of
   review there were, what was changed, and what the user settled.

- When the volume of changes is large and spans several relatively isolated modules, split by scope and dispatch the
  subagents each scope needs per the rules above, reviewing in parallel: one subagent given too much full text to read
  can only skim, and details get missed. Where scopes share an interface contract, hand it to every side involved; both
  sides of an interface count as scopes responsible for it.
- The subagents this skill dispatches are installed together with it: if there is a subagent mechanism but the
  definition of a subagent to dispatch cannot be found, the installation is incomplete: stop and ask the user to rerun
  the installer that installed this skill. With no subagent mechanism, read the installed definitions and do the work
  yourself following the rules inside.

## Convention suggestions

The "Convention suggestions" listed separately in a subagent's report do not count as a failed review and do not block
this delivery; the conventions are the whole project's long-term agreement and change only by the user's decision. The
code, comments and convention documents changed under this section count as this round's change too and are reviewed
again per "Review".

A coding convention only takes a rule that meets all three: it constrains many future changes, in many places; it
cannot be derived without the conventions (neither general knowledge nor the code's consistent practice yields it, or the
code holds both practices side by side); a violation can be judged a defect at review. The conventions are read at every
review, so what falls short stays out.

What falls short goes elsewhere by its nature:

| Content | Where it goes |
|---|---|
| Why a piece of code is written the way it is, what to watch out for when using it | That code's comment |
| Development experience, system behavior and business rules | The matching project doc (which one by the project's documentation conventions; with none, give only the content) |
| What formatting and lint tools can enforce automatically | The tool configuration |

The shape of the conventions: an opening "General principles" section states a few overarching principles (the
project's consistent leanings, such as following the surrounding existing practice and reusing the existing design
system and shared components), and cases not written down are judged by them; topic sections follow, each entry written
as a rule that can be judged right or wrong, with half a sentence of why; a table of worked cases is added only for
typical cases the rule itself cannot settle.

Changes to the conventions subtract before they add: what existing entries or the general principles already imply
leaves the conventions as they are; what rewriting, merging or relaxing existing entries, or lifting several detailed
rules into one principle, can cover is rewritten; only when none of that works is something added. Whether a change
qualifies is judged on the conventions as they stand after it: added or rewritten entries must meet all three
conditions; a deletion or relaxation holds when the part removed falls short of them. A single convention document over
about 500 lines has detail or content from elsewhere mixed in and needs streamlining as a whole.

- Merge the convention suggestions and the destinations of content that falls short from every subagent and round,
  removing duplicates, then recheck each against the standard above instead of taking them all: only the ones that
  qualify go to the user as convention changes; one that is merely a failure to follow an existing entry or a general
  principle becomes a review comment to fix; one that belongs in a code comment, you add yourself; one that belongs
  elsewhere is listed in the report with the document it should go into, for the user to decide whether to write it.
- Report each one by the fields of the subagent's report (the convention changes you hand the user in step 2 likewise),
  adding whether you recommend adopting it and why; where unsure, recommend not adopting. Say in one sentence how many
  the recheck removed and why each.
- For the ones the user agrees to, change the convention document the suggestion points to: only the ones agreed to,
  straightening out any existing entries they contradict, without changing anything else along the way.
- Where a suggestion points to content installed by an installer (an installed skill or subagent, a passage an
  installer wrote into the instruction file; the convention documents belong to the project and are not among them), do
  not change it; list what should change for the user: a reinstall would overwrite it back.
- When "Coding conventions" says "none" or "unspecified", or a convention document exceeds the size stated above, tell
  the user: rerun the installer that installed this skill at project level in the project, and it will create, register
  or streamline the convention documents.

When unsure, report back: stop, spell out what needs deciding (with the options) or what information is missing,
together with what you have already found out, and let the recipient of the report (the user, or the dispatcher passing
it on to the user) decide before going on; do not decide for yourself.
