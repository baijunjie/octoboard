---
name: agent-plan-exec
description: Carry out development from a development plan doc — when no doc is named, pick the milestone that is currently the best one to start on, implement in number order and tick the checkboxes as you go; once a milestone is done, settle the debt first (finish what can be finished now, hand off what cannot to a later milestone or the user), then consolidate what should be kept into the product docs and delete the milestone doc, leaving nothing behind. Use when the user says "follow the plan", "continue with this plan", "start the next milestone".
---

# Carry out development from a development plan doc

"Report back" means stop, write out what has to be decided (and the options), what information is missing, or what needs the user's help to finish, report it together with what you have already established, and let the side receiving the report (the user, or the dispatcher passing it on to the user) decide before you continue; do not decide on your own. "Notify" means tell the user once it is done, without waiting for a reply.

**By default, one session handles one milestone**: once it is done and wrapped up, stop — do not go on to the next one; when the user explicitly asks for several in a row, follow that, but do not start the next one until the previous one is wrapped up. Terms used below:

- **Started**: the milestone has at least one checkbox already ticked, or carries an "Interim handoff" section; otherwise it is "not started"
- **Wrapped up**: all three steps under "Wrapping up a milestone" (settle the debt, consolidate, delete) are done; "final confirmation milestone" names the one doc that collects items needing the user's sign-off (see "Adjusting milestones")
- **"Goal / Completion criteria" block**: the quoted passage at the top of a milestone doc stating its goal and completion criteria
- **Hand off**: write what cannot be done into somewhere else, for that place to pick up

## Choosing a milestone

First check whether the project has its own documentation conventions (see "How to learn the project context") and read them first if it does; its convention for the plan directory then takes precedence.

The development plan doc directory defaults to `docs/plans/`; if the project already has its own convention, follow the project.

If the user named which doc to work on, work on that one; if they only named a topic, take the first milestone in that topic not yet wrapped up. If not named, pick the milestone in the development plan doc directory that is currently the best one to start on yourself. Directory missing or empty: just say there is no development plan to work on.

Selection criteria:

- Prefer carrying on with a topic already started — one with a milestone partly ticked (including one carrying an "Interim handoff" section), or whose preceding milestones are wrapped up
- Within a topic, take the first milestone not yet wrapped up in number order (`02` < `02a` < `03`); a milestone whose predecessors are not wrapped up cannot be skipped over
- When several topics could be started, take the one with the fewest dependencies; dependencies between topics are stated in each topic's `README.md` overview — when that is not stated, or there is no clear choice among the candidates (comparable priority, dependent on each other), report back, list the candidates and let the user sign off, do not force a pick

Once chosen, say which milestone you are working on first, then check it against the list below before you start. Whether you picked it yourself or the user named it, do not start under any of these conditions:

- The preceding milestone is not wrapped up (when the user named a later one)
- It is the final confirmation milestone (`NN-final-confirmation.md`) and the user is not present
- Its "Handoff" section has an item tagged "needs user help, do not start until this is done" and the user has not yet helped with it
- It carries an "Interim handoff" section and the condition it was stuck on last time has not been lifted

Whether the user is present, whether the help has been given, whether the condition has been lifted — ask the user or check it yourself; when you cannot check, treat it as not present, not done, not lifted.
If you picked the milestone yourself, switch to another topic first and only report back once none works; if the user named it, report back directly. When a "needs user help" tag's condition is already met, remove the tag and start — the item itself is still this milestone's task and gets settled as usual.

## How to read a development plan doc

- **Topic directory**: `YYYYMMDD-{short-description}/` under the development plan doc directory, one directory per development topic
- **Overview doc**: the `README.md` in the topic directory — problem description, plan outline, key design decisions;
  when split into several milestones, the milestone list is here too. Each entry is one line: a milestone not yet wrapped up is written `NN short-description`, and once wrapped up it changes to `NN wrapped up`, with the corresponding doc already deleted
- **Milestone docs**: the files in number order (`02a` sorts after `02` and before `03`), one per milestone that can be tested and merged on its own,
  opening with a "Goal / Completion criteria" block. The number is the dependency order, work them in order
- **A directory with only `README.md`**: this topic has a single milestone, with the overview and the milestone content both in that one doc;
  just develop from it, do not create a numbered doc
- **"Handoff" section**: sits right after "Goal / Completion criteria", and is the debt earlier work hands this milestone — transitional layers (with a matching `TODO` in the code),
  interfaces to wire up, conventions still to be unified, items that need the user's help to continue (tagged "needs user help, do not start until this is done").
  These are all tasks of this milestone, **read it before starting, settle every item once done, and run whatever cannot be settled back through the debt-settling triage**;
  transitional layers are all marked with a `TODO`, do not take them for oversights and delete them in passing, each one states which milestone deletes it
- **"Interim handoff" section**: same position, and sorts before "Handoff" when both are present (for a topic with only `README.md`, it goes after the milestone part's "Goal / Completion criteria"); it is the context left behind when work could not continue last time (see "Interim handoff when you cannot continue" below). Whoever takes over reads it first; once development resumes, fold whatever context is still useful into the body (design trade-offs go into the matching design section, work not yet done becomes a checkbox), and delete the rest
- **Checkbox entries**: a tick means implemented and verified; tick one the moment it is done, never save them up for a batch at the end

## How to learn the project context

- If the project has its own documentation conventions, read them first (before choosing a milestone); the doc directories and this project's extra rules come from there
- Otherwise (after choosing a milestone, before starting work) start from the documentation index (`docs/README.md` by default), use the project map to locate the module you need to touch,
  then read the matching product docs to confirm the existing behavior

## When the plan does not match reality, or you cannot proceed

- Fill in an entry that was missed, or correct a design that no longer matches the code, directly in the plan doc; changing the completion criteria, a key design decision, or shrinking scope needs a report back first, then act on the decision
- When one item in a milestone is blocked (missing an external condition, needs the user's help), carry on with the items that do not depend on it as usual; only stop when the whole milestone cannot proceed: if this session has to end or the code has to be merged, write it up under "Interim handoff when you cannot continue" first, then report back

## Adjusting milestones

Creating a new milestone, inserting one, and tidying numbers during execution are all decided here. The number is the development order; it does not fill gaps, and existing docs are never renumbered (except under "Inserting at the very front" below).

- **A newly created milestone**: same format as other milestone docs — opening with its goal and completion criteria, content items written as checkboxes, named `[number]-[short-description].md`;
  write only the work to do and what is being handed to it, add no design without a basis, and write neither implementation code nor where the code goes; update the `README.md` milestone list to match (when a single doc turns into several, create the list and write both `01` and the new milestone into it)
- **Position**: it must sort after the milestones it depends on; if a later milestone depends on it, it goes before the earliest such dependent; with no constraint on either side, put it near the end, but always before the final confirmation milestone.
  When the earliest dependent has already started, inserting before it would change a started milestone's premise, and inserting after it would invert the dependency — report back and let the user decide; also report back when the constraints on both sides leave no room for it
- **Numbering**: when it goes at the very end and there is no final confirmation milestone, it continues from the largest integer number that has appeared in the list and the directory;
  otherwise (inserted in the middle, or a final confirmation milestone already exists) take the integer part of the number right before the insertion point and add a letter suffix that sorts between the two neighbors (inserting between `02` and `03` gives `02a`; inserting after an existing `02a` gives `02b`, with the order `02` < `02a` < `02b` < `03`); report back if there is no room for it
- **Inserting at the very front**: this only comes up when no milestone has started at all (none wrapped up, none with a tick or an "Interim handoff" section, and no `TODO` in the code pointing at a number) —
  it amounts to changing the plan before work starts, so renumber every milestone in the plan and update the `README.md` list to match; once any milestone has started, a new milestone always sorts after them with a suffix, with its dependency on them written into the new milestone's doc
- **A topic with only `README.md` gaining a milestone**: move the milestone content (the "Goal / Completion criteria" block, technical design, implementation plan, checkboxes, handoff, notes for the developer) into the `01-…` numbered doc,
  number the new milestone from 02, and let `README.md` keep only the problem description, plan outline, key design decisions, and the milestone list
- **Final confirmation milestone**: only create one when an item turns up that cannot be done or confirmed now but does not block later development; name it `NN-final-confirmation.md`, place it at the end of the whole topic, and append to it if one already exists;
  each entry states how to confirm it and what result is expected. It is the end of the line, with nowhere further to hand things off to: a confirmation item only counts as done once the user has confirmed it in person, and it may not be deferred any further;
  if confirming it turns up a defect, fix it within this same milestone and reconfirm, do not create a new milestone

## Wrapping up a milestone

**A milestone is only wrapped up once it is genuinely done, leaving nothing behind**: everything not yet settled has to be handed off into a later, not-yet-developed milestone doc, or dealt with by the user.

### 1. Settle the debt

Before wrapping up, verify against the completion criteria in the "Goal / Completion criteria" block one by one, then triage every unticked entry, unverified item, and deliberately left transitional layer. **Do what can be done**:
anything that can be implemented and verified now without going beyond this milestone's scope must be finished now — "later" is not a valid destination. For what genuinely cannot be done now, there are only the destinations below.
An unimplemented item that cannot be done means the milestone cannot be wrapped up — follow "Interim handoff when you cannot continue" instead; only verification- or confirmation-type items can be handed off and the milestone still wrapped up, per the table below:


| Situation | What to do |
|---|---|
| Squarely within the scope of a later, not-yet-developed milestone (doing it now would go beyond this one), and needs no user help or external condition | Write it into that milestone doc's "Handoff" section |
| No milestone fits, and it needs no user help or external condition | Create or insert a milestone per "Adjusting milestones", then write it into that milestone's "Handoff" section |
| Needs the user's help or an external condition to do or verify, and **blocks later development** (a later milestone depends on its result) | Write it into the "Handoff" section of the first milestone that depends on it, tagged "needs user help, do not start until this is done"; write it first, then wrap up this milestone, then notify the user — the session ends here (even when the user explicitly asked for several in a row, stop here and wait for their help) |
| Needs the user's help or an external condition to do or verify, but **does not block later development** | Fold it into the "final confirmation milestone" (see "Adjusting milestones") |
| To be dropped, not done | A scope change — report back and let the user decide, do not drop it on your own; once the user agrees, delete the entry and update the scope description in `README.md` to match |

- Whether something blocks later development is your call first; report back when you are not sure
- When the final confirmation milestone itself is ending, there is nowhere left to hand off to, so none of the first four destinations above apply: only finishing it, a scope change (report back), or an interim handoff
- A "Handoff" section writes only the context whoever takes over needs: what it is (transitional layer / interface / convention / blocking item) and what to do, not where it came from;
  a transitional layer keeps its `TODO` in the code stating which later milestone deletes it, with a matching entry in the receiving milestone's "Handoff" section; if a transitional layer's current state is described in the product docs, note that in the entry too, and update it when the transitional layer is deleted
- Whatever is handed off is written down on the receiving end only — the milestone doc that hands it off keeps no record of the destination; a wrapped-up milestone leaves only one line in the `README.md` list, and is not mentioned anywhere else (handoff sections, product docs, code comments)

### 2. Consolidate

Once the debt is fully settled, take the parts of what this milestone built that have long-term value and work them into the product docs and the project map.

- The criterion is **what cannot be got from reading the code itself but has to be known when taking over**; purely internal implementation details, and changes that swap the implementation without changing external behavior, are not written
- If the project has its own documentation conventions, follow what they say
- Without them, organize it yourself; before writing anything, read the few docs you are changing end to end; when the project has no product docs or project map to write into, report back and let the user decide where it goes
- When wrapping up the last milestone in the topic (the final confirmation milestone, if there is one; if settling the debt leaves other milestones afterward, this is not the last one), also work the `README.md` overview's long-term-value content (problem description, plan outline, key design decisions) in by this same criterion, since the overview is about to be deleted too
- **Deliberately left transitional layers do not go into the product docs**, unless they cause a behavior difference the user can see — then state the current situation honestly and mark it as transitional,
  but write no plan and no milestone number

### 3. Delete

Deletion comes only once every completion criterion that can be verified now has been verified, everything that could not be verified has been triaged under "Settle the debt", consolidation is done, and the review the project requires (if any, as the project's documentation conventions specify) has also passed; if the review does not pass, fix it and go back through "Settle the debt" again before rechecking.

Delete this milestone doc, and change its entry in the `README.md` overview's milestone list to a single line `NN wrapped up` (`NN` being its number; if it is the last one in the topic, handle it as below directly instead of editing the list).
That line stays so that milestones added later do not reuse the number.
Once every milestone doc in the topic directory is deleted, the `README.md` overview goes with them, then the topic directory; once the whole topic is deleted, nothing remains in the plan directory for it.

A topic with only `README.md`: when there is nothing to hand off, delete `README.md` and the topic directory outright once consolidated; when settling the debt leaves something to hand off, first turn it into several docs per "Adjusting milestones", then wrap up `01` as above.

Delete the development plan doc directory too if it is now empty.

### Interim handoff when you cannot continue

**The one exception**: when this milestone is not yet done, and development genuinely cannot continue right now (for example an external condition is missing, or a decision needs the user and they will be away for a long time),
and the task has to end or the code has to be merged anyway, it is allowed to stop without wrapping up (this is the situation once a report back ends with the session having to end or the code having to be merged). In that case, do not delete the doc — add an "Interim handoff" section right after "Goal / Completion criteria" so whoever takes over next can pick it up from reading it:

- How far it got, which items are ticked, which are not done
- Where it is stuck, and what condition is needed to continue
- What has been established, and the design trade-offs already made
- What has not been verified
- Any other context whoever takes over needs

Whatever is already done and merged in with the code still gets its externally visible behavior worked into the product docs by the "Consolidate" criterion; a transitional layer already merged in still keeps its `TODO`, and is written into the interim handoff too.
When you can keep going, an interim handoff may not be used in place of wrapping up.
