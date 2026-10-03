---
name: agent-plan-write
description: Write a plan up as a development plan doc (a plan settled in discussion, a complete requirements description from the user, or an add-on feature found during development that can be built independently — the last only gets recorded as a plan, not implemented in the current task), split into numbered milestones that can each be tested and merged on their own, placed in docs/plans/YYYYMMDD-{short-description}/ (just a single README.md when there is only one milestone), design only, no implementation code. Use when the user says "write this plan up as a development plan", "write a plan doc", "turn this requirement into a development plan", "record this add-on feature as a plan, not for this round".
---

# Write a development plan doc

"Report back" means stop, write out what has to be decided (and the options) or what information is missing, report it together with what you have already established, and let the side receiving the report (the user, or the dispatcher passing it on to the user) decide before you continue; do not decide on your own.

**Organize it concisely** into a development plan doc, covering only what is within the following inputs, adding no extra information and no plans you came up with yourself; when the input is long, do not drop design decisions and the reasoning behind them either — only trim the narrative:

- The plan settled in this discussion
- A complete requirements description from the user
- An add-on feature found during development and already explained to the user (one never explained to the user: report back first, do not write it)

An add-on feature is a feature found while the current task is underway, that can be built independently and does not belong to this task. Writing a plan is not implementing it:
once written, do not go on to implement it; the add-on feature does not enter this round of changes.

Do not fill in design points the inputs left unsettled: ones that do not affect the milestone split or the completion criteria go into an "Open" section at the end of `README.md`'s overview; ones that would leave some milestone's goal or completion criteria unclear get reported back — write what is already settled, and leave the affected milestones unwritten for now.

The development plan doc directory defaults to `docs/plans/`; if the project already has its own convention, follow the project.

The development plan doc directory holds only what is **not yet done**: plan designs, development plans, todo lists.

This skill only writes new topics; appending to or adjusting milestones in an existing topic is out of its scope. Once written, just save the files — do not commit on your own, and tell the user which directory it was written to.

## Directory and splitting

- One directory per development topic, `YYYYMMDD-{short-description}`, the short description in the language of the repo's existing docs.
  Do not put docs directly in the root of the development plan doc directory.
  The date is the date the directory was created, not the development period — a topic that spans a long time and is done in several rounds still has only one directory
- **An add-on feature** must get its own topic directory, never merged into the topic currently in progress
- One milestone doc corresponds to one milestone that **can be tested on its own and merged on its own**: when that one doc is done, the code should be verifiable and mergeable by itself,
  without waiting for later docs to work. Verifying does not have to mean running the business behavior — a migration that can be run, a type check that passes, or a unit test that passes all count; when there is genuinely nothing to verify, fold it into a neighboring milestone (the next one first, the previous one if there is no next)
- How finely to split is your judgement: a milestone may span data, logic, interfaces and UI, or touch a single file.
  Do not split for the sake of splitting, and do not stuff two independent pieces into one doc
- Order by direction of dependency, with the depended-on milestones first (usually data structures → core logic → external interfaces → callers)
- **When there is only one milestone, the directory holds just `README.md`**, with the overview and that milestone's content written together in that one doc: the overview first, then the milestone's goal and completion criteria block, technical design, implementation plan, and notes for the developer
- When split into two or more, `README.md` falls back to a pure overview and the milestone content goes into its own numbered doc,
  named `[number]-[short-description].md` (e.g. `01-order-status-storage-and-migration.md`), the number being the development order.

## What to write

**`README.md` overview**: problem description, plan outline, key design decisions and the reasoning behind them. When split into several milestones, maintain the milestone list here.

**Milestone doc** (`README.md` itself when there is only one):

- Open by stating this milestone's goal and completion criteria, in this format:

  ```
  > Goal: <what this doc is to achieve>
  > Completion criteria: <what counts as done, how to verify it>
  ```

- **Technical design**: interface signatures, data structures and core method definitions within the inputs
- **Implementation plan**: refactoring strategy, method decomposition and call-flow framework within the inputs
- Write each item in technical design and the implementation plan as a checkbox entry, at the granularity of the smallest independently verifiable piece; always write it `- [ ]` — ticking entries and cleaning up the plan doc are the implementation stage's job, not this one's. Omit a section the inputs have nothing for; the goal / completion criteria block and the notes for the developer are never written as checkboxes
- End with notes for the developer — the one part that does look at the project's current state; everything else stays within the inputs — only down to the granularity of "what the project already has, which part to reuse" (omit items with no content):
  - **Reusable capabilities**: mechanisms or modules the project already has that this milestone should reuse directly (auth, caching, error handling, shared components,
    utility functions, etc.). Pointing at the capability and the module it belongs to is enough — a module name is fine, no file paths; if you are not sure the project has it, leave it out, do not guess
  - **Development notes**: development conventions to follow, how to integrate with the existing system, highlighting global features already implemented to prevent reimplementation
  - **Reference docs**: paths to the relevant technical and convention docs (a doc path is fine here)

## What not to write

- **Complete implementation code** — never write the logic inside a function body. Examples may go only as far as the call relationships between methods and the execution flow,
  and every method called must be one settled within the inputs; examples containing concrete business logic, algorithm implementations or third-party library calls are never written.
  "Call A once validation passes, then call B" is fine; implementation-level branching pseudocode such as "branch one when state is X, otherwise branch two" is not.
  A business rule or constraint from the inputs (e.g. "a paid order cannot be cancelled") is a design decision — write it into the design in plain language as given; this restriction does not apply to it
- **Where the code goes** — do not list which files to change, do not assign code to a path; the developer investigates and settles that at the implementation stage
- **Coding instructions** — do not write out the concrete coding steps
- **Repeated description** — no content repeated between docs; where a later doc depends on an earlier one, reference it briefly, and each doc focuses only on its own milestone
- When the inputs mention an abandoned implementation plan, it just gets a brief note in the milestone or topic doc it belongs to, not a list of its own

## Relationship to everything else

- **Development plan docs do not go into the project's documentation index**; adding or deleting a topic directory requires no change to the index
- **It is a temporary doc, not a long-term asset**
- **Nothing outside its own topic directory may reference it** (docs inside the same topic directory referencing each other is fine) — referencing is one-way: a development plan doc may reference anything else, never the reverse, and it is not referenced from code comments, commit messages, or PR descriptions either.
  If somewhere else needs what is in it, write that straight into that doc
