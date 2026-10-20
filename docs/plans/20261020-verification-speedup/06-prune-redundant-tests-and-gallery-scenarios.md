# 06 Prune redundant tests and gallery scenarios

> Goal: every test and gallery scenario covers something no other one does.
> Completion criteria: an assessment lists tests that cover the same behaviour twice and scenarios that show the same
> UI state twice or a state that no longer exists, each with the proposed action (merge, delete, keep with reason);
> the user has agreed to the list; the agreed changes are made, the suites pass, and the gallery's full check passes.

## Implementation plan

- [ ] Assess the UI tests, daemon tests and desktop tests for duplicated coverage.
- [ ] Assess the gallery scenarios for duplicated states, states that no longer exist, and scenarios of no use.
- [ ] Agree the list with the user.
- [ ] Make the agreed changes; gallery scenarios are changed only by the gallery writer.

## Notes for the developer

- **Development notes**: a test that pins a past defect is not redundant just because another test covers the same
  code path; keep it unless the user agrees otherwise. The gallery's own rules decide what a scenario must show.
- **Reference docs**: `packages/ui/README.md` "The UI state gallery", `docs/memory/writing-automated-tests.md`.
