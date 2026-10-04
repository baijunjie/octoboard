---
name: agent-bug-report
description: Turn a defect into a bug ticket and hand it off to whoever fixes it — file only, never fix. Also reach for this when, mid-development, you spot a defect unrelated to the current task. Use when the user says "log a bug", "file a ticket", "this looks like a bug, open a ticket", "write this suspected issue up as a ticket".
---

# Create a bug ticket

"Report back" means: stop, write out what needs to be decided (which options exist) or what information is missing, hand it over together with what you have already found, and let the side that receives the report (the user, or the dispatcher passing it on to the user) decide before you continue — do not decide on your own.

Turn the defect described this time into a ticket and hand it off to whoever fixes it: what is wrong, how to reproduce it, what counts as fixed. Report back when you're not sure.

- **Facts only**: the fact sections hold only what the user said and what you verified yourself; anything inferred goes under "Leads" and is marked as such; report back what is missing.
- **Once the ticket is filed, stop — do not fix it**; if you filed it mid-development, go back to the task you were on.
- A ticket is a one-off document, **delete it once it's fixed**: nothing may ever reference it, it never enters the documentation index, and neither filing nor deleting one touches that index.

## Directory and naming

The ticket directory defaults to `docs/bugs/`; follow the project's own convention where it already has one.

- One file per bug, `YYYYMMDD-{short-description}.md`, dated by the ticket's creation date, with the short description in the language the repo's existing docs use.
- File by defect: a new symptom of the same defect is appended to its existing ticket; symptoms that look alike but have different root causes get separate tickets.

## Ticket format

The first line marks severity:

```
> Severity: <one of Critical / Major / Moderate / Minor>
```

Critical = data corruption or a core flow unusable; Major = a main feature broken with no workaround; Moderate = a feature impaired but with a workaround; Minor = a cosmetic or copy issue. When unsure, pick the higher grade — that alone is not worth reporting back over.

Write the body in the sections below, **do not skip a section, and do not fill one in with a guess** — write "Unknown" where you couldn't find out. "Unknown" is not allowed for the reproduction steps, the expected side of "expected vs. actual", or the acceptance criteria: if information is missing there and you can't get it, don't file the ticket — report back what's still missing.

- **Symptom**: what went wrong, in one sentence
- **Reproduction steps**: numbered steps, detailed enough to reproduce by following them; for an intermittent one, state the trigger condition and how often it occurs. If the code already has a suspended reproduction case (commented out, marked as a known failure, shimmed around, etc.), give its file path, test name, and how to restore it, instead of writing a separate manual reproduction path.
- **Expected vs. actual**: one line each; for expected, state what it rests on — which line of the product docs, or a position the user confirmed on the spot.
- **Environment**: version, platform, configuration, data preconditions, and anything else that affects reproduction.
- **Scope of impact**: who's affected, and whether there's a workaround.
- **Leads**: errors, logs, suspicious spots — **mark each one as verified or inferred**.
- **Acceptance criteria**: what counts as fixed, written as conditions that can be checked off one by one.

## What not to write

- A fix plan or implementation code — a ticket only delimits the problem.
- Todos, progress, status transitions, or the course of the fix — whoever fixes it never comes back to update this.
- Restating the code; process and attribution (who reported it, who wrote the code).
