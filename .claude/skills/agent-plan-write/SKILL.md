---
name: agent-plan-write
description: Turn a plan or a requirement into a development plan doc split into milestones, design only, no implementation — call to mind once a plan is settled in discussion, or when development turns up an add-on feature that can be built independently and does not belong to this task. Use when the user says "turn this plan into a development plan", "write a plan doc", "organize this requirement into a development plan", "record this add-on feature as a plan, not for this round".
---

# Write a development plan doc

"Report back" means: stop, write out what needs deciding (what the options are) or what information is missing, together with what has already been found out, and let the party receiving the report (the user, or the dispatcher passing it on to the user) decide before continuing — do not decide on your own.

What follows only sets the principles and the boundaries that must not be crossed; judge the rest of the details yourself — **when unsure, report back**.

## Scope

- Inputs are limited to three kinds: a plan settled in this discussion, a complete requirements description given by the user, and an add-on feature found during development and already explained to the user (one never explained: report back first, do not write it). An add-on feature is one found during the current task, that can be built independently, and does not belong to this task.
- **Organize the inputs concisely**, adding no information beyond the inputs and no plan you guessed at yourself (except for the notes for the developer); however long the input, never drop design decisions and their reasoning — only trim the narrative.
- Do not fill in design points the inputs left unsettled: ones that do not affect the milestone split or the completion criteria go into an "Open" section at the end of `README.md`'s overview; ones that would leave some milestone's goal or completion criteria unclear get reported back — write what is already settled, and leave the affected milestones unwritten for now.
- Only write new topics; do not append to or adjust milestones in an existing topic.
- Write the plan only, do not implement it: once written, do not go on to implement it, and the add-on feature does not enter this round of changes; just save the files, do not commit, and tell the user which directory it was written to.

## Directory and splitting

The development plan doc directory defaults to `docs/plans/`; if the project already has its own convention, follow the project's. The directory holds only what is not yet done.

- One directory per topic, `YYYYMMDD-{short-description}/`, the short description in the language of the repo's existing docs, the date being the directory's creation date.
- A milestone must be independently testable and independently mergeable: once it is done, it can be verified and merged on its own, without waiting on later milestones. Verifying is not limited to business behavior — a migration that runs, a type check, or a passing unit test all count.
- How finely to split is your own judgment call — do not split just to pad the count, and do not stuff two independent pieces into one; order by dependency, with the depended-on one first.
- When there is only one milestone, the directory holds just `README.md`: the overview first, then the milestone content. With two or more, `README.md` holds only the overview, and each milestone gets its own `[number]-[short-description].md` (e.g. `01-order-status-storage-and-migration.md`, the short description in the same language as the directory name), the number being the development order.

## What to write

**`README.md` overview**: problem description, plan outline, key design decisions and the reasoning behind them; when split into multiple milestones, add a milestone list, one milestone per line (number, short description), linking to the corresponding doc.

**Milestone** (written right after the overview in `README.md` when there is only one):

- Open with a goal and completion-criteria block:

  ```
  > Goal: <what this doc is to achieve>
  > Completion criteria: <what counts as done, how to verify it>
  ```

- **Technical design**: interface signatures, data structures, core method definitions. **Implementation plan**: refactoring strategy, method decomposition, the call-flow framework. Both are limited to the inputs; omit a section the inputs have nothing for.
- Write each item in technical design and the implementation plan as a `- [ ]` checkbox, at the granularity of the smallest independently verifiable piece; the goal / completion criteria block and the notes for the developer are never written as checkboxes. Ticking entries and cleaning up the doc are not this skill's job.
- End with **notes for the developer** — the only part that looks at the project's current state, and only down to the granularity of "what the project already has, which part to reuse"; omit items with nothing to say:
  - **Reusable capabilities**: existing mechanisms or modules that should be reused directly (auth, caching, error handling, shared components, utility functions, etc.) — name the capability and the module, no file paths; leave it out if you are not sure the project has it
  - **Development notes**: development conventions to follow, how to integrate with the existing system, existing global features (to prevent reimplementation)
  - **Reference docs**: paths to the relevant technical and convention docs

## What not to write

- **Implementation code**: never write the logic inside a function body. Examples go only as far as call relationships between methods and the execution flow ("call A once validation passes, then call B"), never implementation branches, algorithms, or third-party library calls. A business rule or constraint from the inputs (e.g. "a paid order cannot be cancelled") is a design decision — write it in plain language as given.
- **Where the code goes and the coding steps**: do not list which files to change, do not assign a code path, do not write out coding steps — leave that to be investigated at implementation time.
- **Repetition**: no repeating between docs — reference an earlier one briefly instead.

## Relationship to everything else

A development plan doc is a temporary doc:

- It does not go into the documentation index; adding or deleting a topic directory requires no change to that index.
- Referencing is one-way: nowhere outside its own topic directory (including code comments, commit messages, PR descriptions) may reference it; docs inside the same topic directory referencing each other is fine. If somewhere else needs what is in it, write that straight into that doc.
