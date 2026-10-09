> Severity: Moderate

## Symptom

On a Linux system whose `/bin/sh` is dash, the daemon's 17 `trust::tests` fail, because their stand-in agent
scripts use bash-only syntax.

## Reproduction steps

1. Run the daemon's tests in a Debian container where `/bin/sh` is dash, e.g. the `rust:1.86` image (bookworm):
   `docker run --rm -v <repo>:/src:ro -w /src rust:1.86 cargo test --locked -p octoboardd trust::`.
2. Observe the 17 `trust::tests::*` results.

## Expected vs. actual

- Expected: the daemon's test suite passes on Linux as on macOS — the daemon supports Linux
  (`apps/daemon/src/browse/live.rs` carries Linux-specific code) and its tests are the check for it.
- Actual: all 17 `trust::tests::*` fail; with `/bin/sh` pointed at bash in the same kind of image they pass.

## Environment

- `main` at 4a5b92a; Docker Desktop's Linux VM on arm64; `rust:1.86` (Debian bookworm, `/bin/sh` = dash). A trixie
  image with `/bin/sh` → bash passes all 372 tests.

## Scope of impact

Anyone running the daemon's tests on Debian or Ubuntu, including a future Linux CI. Workaround: point `/bin/sh` at
bash.

## Leads

- Verified: the stand-in's `take` helper builds `read -r -n {count} -d ''` and `${key//…}` substitutions, and its
  comment says these are bash's and that `/bin/sh` is bash on macOS (`apps/daemon/src/trust.rs`, around line 867).
- Unknown: the exact failure output of each of the 17 tests under dash.

## Acceptance criteria

- [ ] `cargo test -p octoboardd` passes in a Linux environment whose `/bin/sh` is dash.
- [ ] It still passes on macOS.
