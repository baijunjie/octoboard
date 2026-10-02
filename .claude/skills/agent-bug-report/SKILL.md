---
name: agent-bug-report
description: Turn a defect into a proper bug ticket under docs/bugs/ — symptom, reproduction steps, expected vs. actual, scope of impact, acceptance criteria; record only verified facts, never a fix plan. A ticket is a one-off handoff doc — deleted once fixed, never maintained. File the ticket only, do not fix it. Use when the user says "log a bug", "file a ticket", "this looks like a bug, open a ticket", "write this suspected issue up as a ticket".
---

# Create a bug ticket

"Report back" means stop, write out what has to be decided (and the options) or what information is missing, report it together with what you have already established, and let the side receiving the report (the user, or the dispatcher passing it on to the user) decide before you continue; do not decide on your own.

Turn the defect described this time into a ticket. **Record only facts** — what the user said and what you verified; do not write down anything unverified, and report back what is missing.

A ticket has one single purpose: **to hand the problem off to whoever fixes it** — what is wrong, how to reproduce it, what counts as fixed.
It is a one-off doc — **delete it once the fix is done**, and nothing anywhere may reference it.
Tickets do not go into the documentation index; neither filing nor deleting one requires touching the index.

**Once the ticket is filed you are done, do not fix it**. If you filed it mid-development, go back to the current task afterwards.

## Directory and naming

The ticket directory defaults to `docs/bugs/`; if the project already has its own convention, follow the project.

- One file per bug, `YYYYMMDD-{short-description}.md`, the short description in the language of the repo's existing docs; the date is the date the ticket was created
- A new manifestation of the same defect is appended to the existing ticket (this is the filer adding symptoms, not fix progress);
  defects that look alike but clearly have different root causes get separate tickets

## Ticket format

Mark the severity on the first line:

```
> Severity: <one of Critical / Major / Moderate / Minor>
```

Criteria: Critical = data corruption or a core flow unusable; Major = a main feature broken with no way around it; Moderate = a feature impaired but with a workaround;
Minor = an experience or wording problem. When in doubt pick the higher one.

Write the body in the sections below, putting "Unknown" where you could not find out (except for reproduction steps, expected behavior and acceptance criteria, see each), **do not omit a section and do not fill one in with guesses**:

- **Symptom**: what went wrong, in one sentence
- **Reproduction steps**: numbered steps, detailed enough to reproduce by following them; for intermittent ones state the trigger conditions and how often it shows up.
  When the code already has a suspended reproduction test case (commented out wholesale, marked as a known failure, worked around with a shim, etc.), the steps are how to restore it to a
  state where it runs and then run it: spell out the file path, the test name and the restore action, and do not write a separate manual reproduction path. **"Unknown" is not allowed**
- **Expected vs. actual**: one line each. Expected behavior **may not be "Unknown"**, and state what it rests on —
  which line of the product docs, or the position the user confirmed on the spot
- **Environment**: version, platform, configuration, data prerequisites and anything else that affects reproduction
- **Scope of impact**: who is affected, whether there is a workaround
- **Leads**: the errors, logs and suspicious spots you already have, **each marked as verified or inferred**
- **Acceptance criteria**: what counts as fixed, written as conditions that can be verified one by one. **"Unknown" is not allowed**

If the reproduction steps, expected behavior or acceptance criteria are missing information you cannot obtain, do not file the ticket; report back what is still missing.

## What not to write

- **A fix plan or implementation code** — a ticket only delimits the problem, how to fix it belongs to the fix stage
- **Todo lists, progress, status transitions, the course of the fix** — a ticket is not a task board, whoever fixes it does not come back to update it
- **Unverified guesses written as conclusions** — inferences always go under "Leads" and are marked as such
- Restating code
- Process and attribution: who reported it, when it was discussed, who wrote this code
