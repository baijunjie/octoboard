# 01 Measure the baseline

> Goal: know how long each verification step takes, cold and warm, and what inside each step costs the time.
> Completion criteria: a table of wall-clock times per step, taken on an idle machine, cold (fresh worktree) and
> warm (second run, after a one-line change), recorded in this topic; each estimate in the overview confirmed or
> corrected from it; later milestones adjusted with the user where the numbers disagree with the plan.

## Implementation plan

- [ ] Time each step separately: lint and format check, UI type check, web type check, UI tests, daemon tests,
      desktop tests, a gallery sweep of five scenarios (dev server start and first page separate from per-scenario
      time).
- [ ] Rust: a build timings report for the daemon's test build, cold and after a one-line daemon edit, splitting
      front end, code generation and linking; one workspace-wide test build against the two separate ones.
- [ ] UI tests: per-test durations of the three slowest files, import time against test time, and the run time at
      worker counts 2, 3, 4 and the default.
- [ ] Type check and lint: cold against a repeat run.
- [ ] Record the table and the corrections in this topic.

## Notes for the developer

- **Development notes**: measure only when the load average is low, and say what the load was; per-file durations
  in the UI test results cache come from different runs and loads, so take fresh ones.
- **Reference docs**: `docs/memory/writing-automated-tests.md`, `docs/memory/building-and-launching-the-app-for-verification.md`.
