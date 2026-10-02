---
name: agent-plan-write
description: Write a plan up as a development plan doc (a plan settled in discussion, a complete requirements description from the user, or an add-on feature found during development that can be built independently — the last only gets recorded as a plan, not implemented in the current task), split into numbered milestones that can each be tested and merged on their own, placed in docs/plans/YYYYMMDD-{short-description}/ (just a single README.md when there is only one milestone), design only, no implementation code. Use when the user says "write this plan up as a development plan", "write a plan doc", "turn this requirement into a development plan", "record this add-on feature as a plan, not for this round".
---

# Write a development plan doc

"Report back" means stop, write out what has to be decided (and the options) or what information is missing, report it together with what you have already established, and let the side receiving the report (the user, or the dispatcher passing it on to the user) decide before you continue; do not decide on your own.

**Organize it concisely** into a development plan doc, covering only what is within the following inputs, adding no extra information and no plans you came up with yourself:

- The plan settled in this discussion
- A complete requirements description from the user
- An add-on feature found during development and already explained to the user

An add-on feature is a feature found while the current task is underway, that can be built independently and does not belong to this task. Writing a plan is not implementing it:
once written, go back to the current task; the add-on feature does not enter this round of changes.

Do not fill in design points the inputs left unsettled; list them as open, or report back.

The development plan doc directory defaults to `docs/plans/`; if the project already has its own convention, follow the project.

The development plan doc directory holds only what is **not yet landed or not yet settled**: plan designs, development plans, todo lists.

## Directory and splitting

- One directory per development topic, `YYYYMMDD-{short-description}`, the short description in the language of the repo's existing docs.
  Do not put docs directly in the root of the development plan doc directory.
  The date is the date the directory was created, not the development period — a topic that spans a long time and is done in several rounds still has only one directory, with later docs appended into it
- When this discussion belongs to an existing topic, append to or update the docs in that directory instead of creating another one.
  **Exception: an add-on feature** must get its own topic directory, never merged into the topic currently in progress
- One milestone doc corresponds to one milestone that **can be tested on its own and merged on its own**: when that one doc is done, the code should be verifiable and mergeable by itself,
  without waiting for later docs to work
- How finely to split is your judgement: a milestone may span data, logic, interfaces and UI, or touch a single file.
  Do not split for the sake of splitting, and do not stuff two independent pieces into one doc
- Order by direction of dependency, with the depended-on milestones first (usually data structures → core logic → external interfaces → callers)
- **When there is only one milestone, the directory holds just `README.md`**, with the overview and that milestone's content written together in that one doc
- When split into two or more, `README.md` falls back to a pure overview and the milestone content goes into its own numbered doc,
  named `[number]-[short-description].md` (e.g. `01-order-status-storage-and-migration.md`), the number being the development order.
  When appending a milestone to an existing topic, the number follows the highest number that has appeared in the overview list and the directory; do not fill gaps, and do not renumber existing docs
- If the directory already has a `README.md`, do not create another one, just update its content; when a topic that was a single doc gains a milestone this time,
  move the milestone content out of `README.md` into the `01-…` numbered doc, number the new milestone from 02, and let `README.md` fall back to a pure overview

## What to write

**`README.md` overview**: problem description, plan outline, key design decisions and the reasoning behind them. When split into several milestones, maintain the milestone list here, leaving one line `NN wrapped up` in the list for each milestone deleted at wrap-up.

**Milestone doc** (`README.md` itself when there is only one):

- Open by stating this milestone's goal and completion criteria, in this format:

  ```
  > Goal: <what this doc is to achieve>
  > Completion criteria: <what counts as done, how to verify it>
  ```

- **Technical design**: interface signatures, data structures and core method definitions within the inputs
- **Implementation plan**: refactoring strategy, method decomposition and call-flow framework within the inputs
- Content items are always written as checkbox entries (`- [ ]`)
- End with notes for the developer, only down to the granularity of "what the project already has, which part to reuse" (omit items with no content):
  - **Reusable capabilities**: mechanisms or modules the project already has that this milestone should reuse directly (auth, caching, error handling, shared components,
    utility functions, etc.). Pointing at the capability and the module it belongs to is enough, no file paths; if you are not sure the project has it, leave it out, do not guess
  - **Development notes**: development conventions to follow, how to integrate with the existing system, highlighting global features already implemented to prevent reimplementation
  - **Reference docs**: paths to the relevant technical and convention docs

## What not to write

- **Complete implementation code** — never write the logic inside a function body. Examples may go only as far as the call relationships between methods and the execution flow,
  and every method called must be one settled within the inputs; examples containing concrete business logic, algorithm implementations or third-party library calls are never written
- **Where the code goes** — do not list which files to change, do not assign code to a path; the developer investigates and settles that at the implementation stage
- **Coding instructions** — do not write out the concrete coding steps
- **Repeated description** — no content repeated between docs; where a later doc depends on an earlier one, reference it briefly, and each doc focuses only on its own milestone
- An abandoned implementation plan just gets a brief note in the milestone or topic doc it belongs to, not a list of its own

## Relationship to everything else

- **Development plan docs do not go into the documentation index**; adding or deleting a topic directory requires no change to the index
- **It is a temporary doc, not a long-term asset**: delete it once what should be consolidated has gone into the product docs
- **Nothing anywhere may reference it** — referencing is one-way: a development plan doc may reference anything else, never the reverse.
  If somewhere else needs what is in it, write that straight into that doc
