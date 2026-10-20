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

1. Measure where the time goes on an idle machine, so the later milestones act on numbers, not estimates.
2. Apply the cheap configuration changes (caches, timeouts, worker caps, build profile, the desktop test sidecar).
3. Run only the checks a change can affect, through one script, and verify each tree only once.
4. Share Rust compilation across worktrees.
5. Split the slowest UI test files and find out why their tests are slow.
6. Assess the tests and gallery scenarios for redundancy and clean them up.

## Key design decisions

- **Do less work and cache it; do not add parallelism.** On this machine, under its usual load, running the steps of
  one verification side by side gains almost nothing and adds memory pressure. Parallelism is kept only for the light,
  single-threaded checks.
- **Tests stay local; no CI test runs** (user decision). Merges are local fast-forwards with no PR flow, so moving
  tests to CI would need branch pushes and macOS runner minutes.
- **The scoping rules live in one script**, not in each agent's judgement, so no one misses a coupling by hand; the
  full suite stays available on request.
- **No shared `CARGO_TARGET_DIR`** across worktrees: concurrent agents would queue on cargo's build lock. Sharing
  goes through a compilation cache instead.
- **The desktop crate's tests must not need a release build of the daemon**, while a real run of the app without a
  built sidecar must still fail loudly, as today.
- **Every estimate in the analysis is confirmed by milestone 01's measurements** before a later milestone relies on
  it; a change that measures no gain is dropped rather than kept.

## Milestones

1. [01 Measure the baseline](01-measure-the-baseline.md)
2. [02 Cheap configuration changes](02-cheap-configuration-changes.md)
3. [03 Scoped verification](03-scoped-verification.md)
4. [04 Shared Rust compilation](04-shared-rust-compilation.md)
5. [05 Split the slowest UI test files](05-split-the-slowest-ui-test-files.md)
6. [06 Prune redundant tests and gallery scenarios](06-prune-redundant-tests-and-gallery-scenarios.md)

## Open

- How the desktop tests get their sidecar: a documented placeholder script that exits with an error, a config
  override that drops the external binary for tests, or a build script that writes the placeholder when none is
  there (the last changes today's "fails loudly at compile time" in a never-built worktree).
- Whether the UI test transform cache is shared across worktrees or kept per worktree; sharing is only safe if its
  key covers file content and plugin configuration.
- Where the scoping rule and the verify-once rule are written for agents: the verification steps live in installed
  skills (`git-worktree`) and in `CLAUDE.md`, and an installed skill is overwritten when it is reinstalled.
