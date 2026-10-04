---
name: agent-bug-fix
description: Fix an existing bug ticket under docs/bugs/. Use when the user says "fix this ticket", "work a bug ticket", "find a bug to fix".
---

# Fix a bug

"Report back" means: stop, write out what needs to be decided (which options exist) or what information is missing, hand it over together with what you have already found, and let the side that receives the report (the user, or the dispatcher passing it on to the user) decide before you continue — do not decide on your own.

What follows only sets the principles and the boundaries you must not cross; use your own judgment on everything else. **Report back when you're not sure.**

**A bug is by definition code behavior that disagrees with the product docs (or a position the user confirmed).** So fix the code only; **do not change the product docs without a decision to do so** — the docs are the yardstick for right and wrong, and changing them would be declaring the bug correct behavior.

## Choosing a ticket

The ticket directory defaults to `docs/bugs/`; follow the project's own convention where it already has one.

A ticket is a single file `YYYYMMDD-{short-description}.md` in that directory, opening with `> Severity: <one of Critical / Major / Moderate / Minor>`, and a body with sections for symptom, reproduction steps, expected vs. actual, environment, scope of impact, leads, and acceptance criteria: use the reproduction steps to reproduce it, the basis stated under expected vs. actual to confirm the expected behavior, the acceptance criteria to verify the fix, and each lead's verified/inferred mark as given.

- **Work one ticket at a time**, and only ones already filed in the directory; a defect the user describes directly with no ticket does not go through this skill — say so and stop.
- If the user names one, work only that one. Left unnamed, pick for yourself: prefer higher severity, and skip anything with an unmerged fix already in flight (e.g. an open PR that already removes it). If the choice isn't clear, report back with the candidates instead of forcing a pick.
- Once chosen, say which ticket you're fixing before you start work.

## Before touching anything

- Read the project's own documentation conventions first if it has them — the doc directory and any extra rules follow those; then read the product docs for the module you're touching to confirm the current behavior.
- **You must reproduce it first** — if you can't, report back what you tried and what's missing, rather than guessing a fix from the ticket description. When the ticket points at a suspended reproduction case (commented out, marked as a known failure, shimmed around, etc.), restore it per the ticket's restore action and reproduce with that, rather than writing a separate one: it asserts the expected behavior, so report back if restoring it doesn't fail, or can't be restored.
- **Do not touch product code until the root cause is located** (restoring a reproduction case aside); if you can't locate it, report back where you're stuck instead of changing things speculatively to see what happens.
- Confirm the expected behavior against the product docs or the basis stated in the ticket. If neither exists, or the docs describe exactly the current behavior, this isn't an implementation deviation — report back to settle which behavior is wanted, and don't change anything until it's decided.

## When the product design itself needs to change

If the root cause is in the product design itself and a code-only fix can't address it, report back: the current state, why the present design can't be fixed, and the available options. Only touch the product docs and the corresponding code once the decision to change the design is made; if the decision is not to change it, fix the code to match the docs.

- Changing product docs follows the project's documentation conventions.
- A design change beyond this bug's scope is a requirements change — schedule it separately through the project's requirements process; fix only the part directly tied to this bug this round.

## How to fix it

- Fix the root cause, don't paper over the symptom; where only a temporary workaround is possible, mark it in the code with a `TODO` stating what it's working around and under what condition it can be properly fixed.
- Change only what's related to this bug; note anything else you notice along the way in your final reply instead of fixing it in passing.
- This reproduction path needs regression test coverage — who writes it and how follows the project's rules; when handing it to someone else, spell out the reproduction path and the acceptance criteria as the expectation. Where you reproduced with a restored suspended case, that restored case is the regression test.
- **Never go back and edit the ticket**: no checking off progress, no status transitions, no fix log.
- Code comments, commit messages, and PR descriptions never reference the ticket file — it's gone once deleted, so anything needed from it goes in directly.

## Verification and wrap-up

1. **Verify**: check off the ticket's acceptance criteria one by one — not fixed until verified. Where a criterion can't be verified, fall back to confirming the symptom is gone per the reproduction steps, and say plainly which one and why.
2. **Delete the ticket**: delete it only once every acceptance criterion has been verified one by one and the fix is at the root cause; submit the deletion together with the fix in the same change, and send them for review together. If a criterion couldn't be verified, only a workaround was applied, or verification didn't pass, don't delete it — report back and let the user decide.
