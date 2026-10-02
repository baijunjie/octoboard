---
name: agent-bug-fix
description: Work a bug ticket under docs/bugs/ (one the user names, or one you pick yourself), one at a time — reproduce first, then locate the root cause; no touching the code until the root cause is located. A bug is code that disagrees with the product docs, so fix the code only, never the product docs; if the product design really has to change, report back first. Once fixed and verified (and reviewed, where the project requires review), delete the ticket. Use when the user says "fix this ticket", "work a bug ticket", "find a bug to fix".
---

# Fix a bug

"Report back" means stop, write out what has to be decided (and the options) or what information is missing, report it together with what you have already established, and let the side receiving the report (the user, or the dispatcher passing it on to the user) decide before you continue; do not decide on your own.

**A bug is by definition code behavior that disagrees with the product docs (or a position the user confirmed).** So a fix changes the code only,
**not the product docs** — the docs are the baseline that decides who is right; changing the baseline means declaring the bug to be correct behavior.

## Choosing the bug to fix

**Work one ticket at a time**. Only tickets already in the ticket directory; a defect the user just describes, with no ticket, does not go through this skill.

A ticket is a single file in that directory, `YYYYMMDD-{short-description}.md`, with a first line `> Severity: <one of Critical / Major / Moderate / Minor>`,
and a body with the sections Symptom, Reproduction steps, Expected vs. actual, Environment, Scope of impact, Leads, Acceptance criteria. What follows relies on these conventions:
the reproduction steps are what you reproduce with, the basis stated under expected vs. actual is what confirms the expected behavior, the acceptance criteria are what you verify the fix against; whether a lead is inferred or verified is marked on each one.

The ticket directory defaults to `docs/bugs/`; if the project already has its own convention, follow the project.

If the user named a ticket, work only that one. If not, pick one yourself from the ticket directory.

Selection criteria:

- Higher severity first
- Within the same level, take the one with clear reproduction steps and a well-defined scope of impact

Once chosen, and when you cannot choose:

- Say which one you are fixing before you start
- Directory missing or empty: just say there is no ticket waiting to be fixed
- No clear choice among the candidates (comparable severity, entangled with each other): report back, list the candidates and let the user sign off, do not force a pick

## Before touching anything

- If the project has its own documentation conventions, read them first; the doc directories and this project's extra rules come from there
- Otherwise start from the documentation index (`docs/README.md` by default), use the project map to locate the module you need to touch,
  then read the matching product docs to confirm the existing behavior
- **You must reproduce it first**. If you cannot, report back — state what you tried and what is missing; do not patch things up by guessing from the ticket description
- When the ticket points at a suspended reproduction test case (commented out wholesale, marked as a known failure, worked around with a shim, etc.), first apply the restore action the ticket
  describes to get it into a state where it runs, and reproduce with it: what it asserts is the expected behavior, so after restoring and before fixing it is supposed to fail.
  If it passes after restoring, or you cannot restore it, report back; do not write a separate reproduction
- After reproducing, locate the root cause; **no code changes until the root cause is located**. If you cannot find it, report back where you are stuck, do not change things speculatively to see what happens
- Confirm the expected behavior against the product docs, or against the basis stated in the ticket. **If there is neither, or the docs describe exactly the current behavior**,
  this is not an implementation deviation — report back, ask which behavior is wanted, and change nothing until that is settled

## When the product design has to change

If the root cause is in the product design itself and no code-only fix works, **report back**: state the current situation, why it cannot be fixed under the present design,
and what the options are. Only **after it has been decided to change the design** do you touch the product docs and the corresponding code; if the decision is not to change the design, fix the code to match the docs.

- When changing product docs, follow the project's documentation conventions if it has them; without them, read the few docs you are changing end to end before you write anything
- When the design change goes beyond this bug, that is a requirements change — schedule it separately through the project's requirements discussion and development workflow,
  and fix only the part directly related to the bug this time

## How to fix it

- Fix at the root cause, do not plug the symptom. When a temporary workaround really is the only option, mark it in the code with a `TODO` stating what is being worked around and under what conditions it can be fixed properly
- Change only what relates to this bug; note down other problems you come across and spell them out in the final reply, do not fix them in passing
- This reproduction path needs regression test coverage; if the project has its own rules on who writes tests and how, follow them; when handing the tests to someone else, spell out the reproduction path and acceptance criteria as the expectation.
  If you reproduced with an existing suspended test case, leave its restored state as this fix's regression test instead of writing a duplicate
- **Do not go back and edit the ticket**: no ticking progress, no fix log, no status changes. A ticket is only responsible for handing the problem over

## Verification and wrap-up

1. **Verify**: go through the ticket's acceptance criteria one by one; where the criteria are not enough to verify item by item, fall back to the reproduction steps and confirm the symptom is gone.
   **Not verified is not fixed**; for whatever you could not verify, state honestly which part and why
2. **Delete the ticket**: once this change is verified and the review the project requires (if any) has also passed, delete the ticket, and delete the ticket directory too if it is now empty;
   deleting is not allowed when verification failed or the fix is unfinished
