> Severity: Moderate

## Symptom

`browse::nested_tests::reading_a_nesting_pair_writes_nothing_to_the_repository` fails intermittently in a full
`cargo test` run, on its own control assertion rather than on the behaviour it is testing.

## Reproduction steps

1. Check out `main` (observed at `94e8575`) in any worktree.
2. Run the whole daemon suite: `cargo test -p octoboardd`.
3. Repeat. The test failed once in seven full runs; the other six passed.

Running the test on its own does not reproduce it: `cargo test -p octoboardd
browse::nested_tests::reading_a_nesting_pair_writes_nothing_to_the_repository` passed eight times out of eight.

## Expected vs. actual

- **Expected**: the test either passes or fails on what it is testing — that a nesting read writes nothing under
  `.git`. Its control exists to prove the fixture can provoke an index refresh at all, so the control must hold
  whenever the test runs. `docs/memory/writing-automated-tests.md` is the standard this rests on: a test that fails
  without the code being wrong is worse than no test.
- **Actual**: the control fails — `apps/daemon/src/browse/nested_tests.rs:420`,
  `assert_ne!(git_dir_state(&repo), before, "the fixture does not provoke an index refresh")` — so a plain
  `git status` over the fixture left `.git` byte-identical, and the run reports a failure that says nothing about the
  behaviour under test.

## Environment

macOS (Darwin 25.2.0), arm64. `cargo test -p octoboardd`, default parallelism, release of the toolchain in use for
the repository. Observed on `main` at `94e8575` and on a branch cut from it; the test and its control predate both.
No configuration or data preconditions beyond the test's own fixture.

## Scope of impact

Developers and any CI running the daemon suite: a run fails for no defect in the product, which costs a re-run and
erodes trust in the suite. No user-facing impact. Workaround: re-run the suite, or run that test on its own, where it
has not been seen to fail.

## Leads

- **Verified**: the failing assertion is the control at line 420, not the `assert_eq!` at line 414 that checks the
  behaviour. The reads under test had already passed when the control failed.
- **Verified**: the failure appears only under the full suite's parallelism, not when the test runs alone (8/8 pass).
- **Inferred**: the control depends on `git status` deciding to rewrite the index, which git does by comparing a
  file's stat data against the index's. The test sets one file's mtime 60 seconds into the past
  (`file.set_modified(SystemTime::now() - Duration::from_secs(60))`) to provoke that decision. Whether git rewrites
  the index also depends on the index's own recorded timestamps and on git's racy-index rules, so a fixture built at
  a moment when those line up differently — more likely under load, where the gap between building the fixture and
  running the control widens — could leave git with nothing to refresh.
- **Inferred**: nothing in the failure points at the production code path; the behaviour assertions passed in the
  failing run.

## Acceptance criteria

- [ ] The control's premise holds whenever the test runs, or the test no longer depends on a control that can
  legitimately not fire — the test must not be able to fail while the behaviour it covers is correct.
- [ ] Twenty consecutive full `cargo test -p octoboardd` runs pass.
- [ ] The test still fails if a nesting read does write under `.git` (verify by making it write, then reverting).
