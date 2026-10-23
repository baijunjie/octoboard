# Verification speedup

## Problem

A full pre-merge verification of one branch takes about 15–20 minutes on the development machine (an Apple Silicon
Mac with 10 cores and 16 GB of memory, usually loaded by many parallel agent sessions, often swapping). Under that
load the UI tests also time out at random unless they are run with fewer workers and a raised timeout by hand.
A read-only analysis (nothing was timed; the machine was too loaded) found that the time goes mostly into four things,
not into running tests:

- **Cold Rust builds in every new worktree.** Each worktree has its own 5–9 GB `target/`, with no cache shared
  between them; one branch here built about 1,080 units cold, in two rounds (daemon, then desktop).
- **A few very long jsdom test files.** One file took about 130 s of a 266 s UI test run; the ten slowest took 83%.
  Work is spread across files, so the longest file sets the minimum wall-clock time.
- **Every check runs whatever changed.** About 33% of commits touch Rust, 8% the web app and 7% the desktop shell,
  yet each verification builds and tests all of them.
- **The same suite runs several times per branch**: the gallery writer's baseline, the implementer's checks, the
  delivery static checks, the change-check loop and the pre-merge verification, and again after a rebase that left
  the tree unchanged.

The test suites and gallery scenarios have also grown without a review for redundancy.

## Plan outline

1. (Dropped.) A full idle-machine baseline is not worth it, since absolute numbers do not transfer between machines;
   each milestone that changes performance measures its own before and after instead.
2. Apply the cheap configuration changes (caches, timeouts, build profile, the desktop test sidecar).
3. Run only the checks a change can affect, through one script, and verify each tree only once.
4. (Dropped.) Share Rust compilation across worktrees: a compilation cache measured no clear gain.
5. Split the slowest UI test files and find out why their tests are slow.
6. Assess the tests and gallery scenarios for redundancy and clean them up.

## Key design decisions

- **Do less work and cache it; do not add parallelism.** On this machine, under its usual load, running the steps of
  one verification side by side gains almost nothing and adds memory pressure. Parallelism is kept only for the light,
  single-threaded checks.
- **Tests stay local; no CI test runs** (user decision). Merges are local fast-forwards with no PR flow, so moving
  tests to CI would need branch pushes and macOS runner minutes.
- **One command applies the scope rule of `CLAUDE.md`**, not each agent's judgement, so no one misses a coupling by
  hand; it adds no rules of its own, and the full suite stays available on request.
- **No shared `CARGO_TARGET_DIR`** across worktrees: concurrent agents would queue on cargo's build lock. A
  shared compilation cache (sccache) was tried and dropped: it measured no clear gain.
- **The desktop crate's tests must not need a release build of the daemon**, while a real run of the app without a
  built sidecar must still fail loudly, as today. The route is a config override, used only for tests, that drops
  `externalBin`, so desktop tests need no daemon build.
- **Each milestone that changes performance (02, 04, 05) measures the steps it affects before and after its change,
  back to back, under the machine's normal load**, records the load, and records the gain as a ratio or percentage
  in its own milestone doc; a change that measures no gain is dropped rather than kept.
- **The agent instructions for the scoped, verify-once run are a pointer to the command in `CLAUDE.md` "This
  project"**, the area a skill reinstall does not overwrite; installed skills are not edited.

## Milestones

1. 01 Measure the baseline (dropped)
2. 02 Cheap configuration changes (closed)
3. 03 Scoped verification (closed)
4. 04 Shared Rust compilation (closed)
5. [05 Split the slowest UI test files](05-split-the-slowest-ui-test-files.md)
6. [06 Prune redundant tests and gallery scenarios](06-prune-redundant-tests-and-gallery-scenarios.md)
