---
name: agent-plan-exec
description: Carry milestones forward from a development plan doc — from starting work, through a mid-task handoff, to wrap-up closure; call to mind when the project has a development plan (default `docs/plans/`) and a milestone in it needs starting or continuing. Use when the user says "follow the plan", "continue with this plan", "start the next milestone", "this milestone is done, wrap it up".
---

# Carry out development from a development plan doc

"Report back" means: stop, write out what needs deciding (what the options are), what information is missing, or what needs the user's help, together with what has already been found out, and let the party receiving the report (the user, or the dispatcher passing it on to the user) decide before continuing.

What follows only sets the principles and the boundaries that must not be crossed; judge the rest of the details yourself — **when unsure, report back**.

## Structure of the plan doc

- The development plan doc directory defaults to `docs/plans/`; if the project already has its own convention, follow the project's. If the project has its own documentation conventions, read them first — any extra rules there take precedence
- One directory per topic, `YYYYMMDD-{short-description}/`. `README.md` is the overview: problem description, plan outline, key design decisions; when split into multiple milestones, it also has a milestone list
- A milestone doc is named `[number]-[short-description].md`, the number being the development order (`02` < `02a` < `03`). A topic with only `README.md` has just one milestone, its content folded into that one doc; when a new milestone is added, first split the milestone content out into `01-…`, leaving `README.md` with just the overview plus the list
- A milestone's **scope** is whatever is covered by its opening "Goal / Completion criteria" block, its checkboxes, and its "Handoff" section
- A ticked checkbox means implemented and verified; tick each item as it is done
- "Handoff" section: whatever an earlier milestone handed over to this one (a transitional layer, an interface to pick up, a convention still to be unified, etc.) is this milestone's task. An item marked "Blocked: reason" is a **blocker**: the whole milestone does not start until the condition is met
- "Mid-task handoff" section: the context left behind when the previous attempt did not finish. Whoever picks it up reads it first; fold whatever is still useful after resuming into the main body, and delete the rest
- Wrapping up means three steps — settle the debt, consolidate, delete — and once done the milestone is **closed**

## Principles

1. **One milestone at a time.** Do the one the user named; if none was named, pick the one currently best to do yourself — do not judge priority by the directory name's date — and report back if you cannot pick one. If the user asks to do several in a row, do not start the next until the previous one is closed; if it cannot be closed, stop and report back.

2. **Closing means leaving nothing behind.** When wrapping up, verify against the completion criteria first, then give every remaining item somewhere to go:

   | What's left | Where it goes |
   |---|---|
   | Within this milestone's scope, not yet done | Finish it now; if it cannot be finished, do not close — go to a mid-task handoff instead |
   | Belongs to a later milestone in this topic (including removing a transitional layer) | Write it into that milestone's "Handoff" section; create or insert one if there isn't a suitable one |
   | Implementation is complete, only verification is missing, and it cannot be verified now (this counts even within scope — handing it over this way is not scope-shrinking) | If a later milestone depends on its result, write it as a blocker into the first milestone that depends on it; if nothing depends on it, fold it into the final-confirmation milestone (create one, placed last, if there isn't one) |

3. **The plan directory holds only what is not yet done.** When closing, consolidate into the project docs (product docs, project map, etc. — if the project has documentation conventions, follow its division of labor, e.g. dispatching the corresponding writer subagent) whatever cannot be recovered by reading the code but a future reader still needs to know, then delete the milestone doc. Once every milestone in a topic is closed, consolidate whatever in the overview still has long-term value too, then delete the whole topic directory.

4. **When a milestone has been started but is not closed and control must be handed back, leave a mid-task handoff first.** This covers both ending the current session and an answer that will not arrive within it; when an answer can be waited for on the spot, or work has not yet started, just report back instead. The mid-task handoff states clearly how far it got, where it is stuck, what condition is needed to continue, what has already been found out and the design trade-offs made, and what is unverified — enough for the next person to pick it up and continue; whatever in the completed part cannot be recovered by reading the code still goes into the project docs, and items already ticked in the milestone doc stay ticked.

5. **Changing the goal, the completion criteria, or a key design decision gets reported back, whether it is the plan or the code that needs changing.** Adding a missed item to achieve the existing goal, correcting a non-key design point in the plan to match the code's actual state, or handing something over per the table above, can be done directly.

## Boundaries

- Do not skip ahead to a later milestone while an earlier one is not closed; the same applies even when the user names it — report back instead
- A final-confirmation milestone (`NN-final-confirmation.md`) collects only things that cannot be verified now and do not block what follows, placed last in the topic as the end point: its items must all be finished, nothing is left beyond it; if confirmation turns up a defect, fix it right there
- A closed milestone's line in the `README.md` list becomes plain text `NN short-description (closed)`; this line stays until the topic directory is deleted. A new milestone never reuses a number that has appeared before — one inserted in the middle gets a letter suffix
- A closed milestone leaves a trace only in that one list line — it is not mentioned in any "Handoff" section, project doc, or code comment
- Hand things over only within this topic; a finding that belongs to another topic, or to no topic at all, gets reported back
- Never shrink the scope on your own, never drop an item on your own
- A deliberately left transitional layer is marked `TODO` in the code, stating which topic's which milestone removes it, updated when handed over to a different milestone; do not casually delete one that does not belong to this milestone
- A transitional layer does not go into the project docs; if it causes a user-visible difference in behavior, describe the current state as it truly is and mark it as transitional — do not write it as a plan, and give it no number
