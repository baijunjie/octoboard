---
name: agent-plan-exec
description: Carry out development from a development plan doc — when no doc is named, pick the milestone that is currently the best one to start on, implement in number order and tick the checkboxes as you go; once a milestone is done, consolidate what should be kept into the product docs, then delete or keep the plan doc depending on whether it leaves debt behind. Use when the user says "follow the plan", "continue with this plan", "start the next milestone".
---

# Carry out development from a development plan doc

"Report back" means stop, write out what has to be decided (and the options) or what information is missing, report it together with what you have already established, and let the side receiving the report (the user, or the dispatcher passing it on to the user) decide before you continue; do not decide on your own.

## Choosing the development task

The development plan doc directory defaults to `docs/plans/`; if the project already has its own convention, follow the project.

If the user named which development plan doc to work on, work that one. If not, pick the milestone in the development plan doc directory that is currently the best one to start on.

Selection criteria:

- Prefer carrying on with a topic already under way — one with a milestone partly ticked, or whose preceding milestones are wrapped up
- Within a topic, take the first milestone by number that is not yet wrapped up; a milestone whose predecessors are not wrapped up cannot be skipped over
- When several topics could be started, take the one with the fewest dependencies

Once chosen, and when you cannot choose:

- Say which milestone you are working on before you start
- Directory missing or empty: just say there is no development plan to work on
- No clear choice among the candidates (comparable priority, dependent on each other): report back, list the candidates and let the user sign off, do not force a pick

## How to read a development plan doc

- **Topic directory**: `YYYYMMDD-{short-description}/` under the development plan doc directory, one directory per development topic
- **Overview doc**: the `README.md` in the topic directory — problem description, plan outline, key design decisions;
  when split into several milestones, the milestone list is here too. An entry written as `NN wrapped up` means that doc has been deleted; a debt-leaving milestone still present counts as wrapped up all the same (see "Debt-leaving milestone doc" below)
- **Milestone docs**: the files ordered by number, one per milestone that can be tested and merged on its own,
  opening with its goal and completion criteria. The number is the dependency order, work them in order
- **A directory with only `README.md`**: this topic has a single milestone, with the overview and the milestone content both in that one doc;
  just develop from it, do not create a numbered doc
- **Debt-leaving milestone doc**: a milestone doc with a "Landing status" section right after "Goal / Completion criteria" is wrapped up and kept only because of the debt it left; treat it as wrapped up when choosing,
  do not judge it by unticked entries. Before starting work, read the "Landing status" sections of the milestones this one depends on — that is where it says what was actually built and
  which transitional layers were left behind. **The transitional layers are all marked with a TODO, do not take them for oversights and delete them in passing**; each one states which milestone deletes it
- **Checkbox entries**: tick one the moment it is done, never save them up for a batch at the end

## How to learn the project context

- If the project has its own documentation conventions, read them first; the doc directories and this project's extra rules come from there
- Otherwise start from the documentation index (`docs/README.md` by default), use the project map to locate the module you need to touch,
  then read the matching product docs to confirm the existing behavior

## Wrapping up a milestone

Before wrapping up, verify against the "Completion criteria" at the top one by one; not verified is not done, and for whatever you could not verify, state it honestly.

| Unfinished entries | Can it be wrapped up |
|---|---|
| All ticked | Yes |
| Some unfinished, but each already has a definite destination (attached to a later milestone, or waiting on an external condition) | Yes, write the destination down on the spot |
| Some unfinished, with no definite destination | No, report the destination first (schedule separately / finish now / drop it), and act on the decision |

Wrapping up has three steps, and **the next one is not allowed until the previous one is finished**.

### 1. Consolidate

Take the parts of what this milestone built that have long-term value and work them into the product docs and the project map.

- The criterion is **what cannot be got from reading the code itself but has to be known when taking over**; purely internal implementation details, and changes that swap the implementation without changing external behavior, are not written
- If the project has its own documentation conventions, follow what they say
- Without them, organize it yourself; before writing anything, read the few docs you are changing end to end
- **Deliberately left transitional layers do not go into the product docs**, unless they cause a behavior difference the user can see — then state the current situation honestly and mark it as transitional,
  but write no plan and no milestone number

### 2. Write the landing status

**The criterion is whether this milestone has anything to hand to later milestones**:

| Type | Criterion | This step |
|---|---|---|
| **Self-contained milestone** | Nothing to hand to later milestones | Skip |
| **Debt-leaving milestone** | Left a transitional layer for later to take apart, an interface for later to wire up, or a position for later to settle | Write "Landing status" |

In this milestone doc, right after the opening "Goal / Completion criteria" and before the body, add a "Landing status" section, writing:

- How the final form differs from the original plan
- **Deliberately left transitional layers**, each stating which milestone deletes it; mark a matching `TODO` in the code
- The debt handed to later milestones: interfaces with no caller yet, positions not yet unified, tests still to be added
- How it was verified, and **what was not verified**

Plans in the body that are now void need not be cleaned up.

### 3. Delete

Deletion comes after the verification against the completion criteria above has passed and the review the project requires (if any) has also passed.

| Type | What to do once this milestone is done |
|---|---|
| **Self-contained milestone** | Confirm it has been consolidated, delete this doc, and change its entry in the `README.md` overview's milestone list to a single line `NN wrapped up` (`NN` being its number) |
| **Debt-leaving milestone** | **Do not delete it**, it can only be deleted once all the debt it handed out is settled — delete it while nobody has taken the debt on and whoever takes over can only work it back from the code |

When this milestone settles the last piece of some debt-leaving milestone's debt, delete that doc too, and change its entry in the `README.md` overview to `NN wrapped up` the same way.
That line stays so that milestones added later do not reuse the number —
a debt-leaving milestone's "Landing status" and the `TODO`s in the code name by number which milestone deletes them.
Once every milestone doc in the topic directory is deleted, the `README.md` overview goes with them, then the topic directory; only when the whole topic is deleted does **nothing remain**.

A topic with only a `README.md` is handled as a self-contained milestone: once consolidated, delete `README.md` and the topic directory outright.

Delete the development plan doc directory too if it is now empty.
