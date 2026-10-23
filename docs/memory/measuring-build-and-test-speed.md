# Measuring build and test speed

## Judge a speed change by CPU time and output size, measured back to back, never by wall-clock time

Applies to deciding whether a change makes a build, type check, lint or test run cheaper, on a development machine
shared with other agent sessions building and testing in their own worktrees, which is the normal state here. Wall-clock
time on such a machine follows the other sessions' load rather than the change: within a single before/after pair the
load average has moved between about 15 and 170.

- Run the before and the after back to back, at least two pairs, and record the load average with every run.
- Compare CPU time (user plus system) and, for a build, the size of what it produces; treat a difference no larger than
  the spread between repeat runs of the same side as no gain.
- CPU time rises with load too, if less than wall-clock time: the same cold build cost about 24 % more CPU at a load
  of about 37 than at about 4.5. Compare only runs made at a similar load, and discard a pair whose loads differ
  widely instead of reading it as a result.
- Count the CPU of every process that does the work, not only the command under `time`: a compiler wrapper that hands
  compilation to a server of its own (sccache does) leaves most of the cost outside it, and timing cargo alone showed
  about 58 s for a build that really cost 143 s. Time that server separately and add the two.
- A change that shows no gain by those numbers is dropped, not kept on the hope that it helps on a quieter machine.

## Do not add a shared Rust compilation cache across worktrees

Applies to speeding up a new worktree's first Rust build. sccache (0.18.0, as `RUSTC_WRAPPER`) was measured on the cold
daemon test build (`cargo test -p octoboardd --no-run`) in fresh worktrees at different paths, one run per variant at a
similar load, and dropped. It does hit across worktree paths (about 87 % of cacheable requests). But with those hits the
build saved only about 25 % CPU and 9 % wall-clock time, about 30 CPU-seconds, while the run that fills the cache cost
about 18 % more CPU than no cache. Dependencies are built without debug information (`[profile.dev.package."*"]` in
the root `Cargo.toml`), so a cold daemon test build is already only about 35 s wall and 120 s CPU on a quiet machine,
and the workspace crates, which are compiled incrementally and never cached, are the part a cache cannot reach. Copying
another worktree's `target/` is a separate approach this measurement does not cover.

- Try it again only if that premise changes, for example if dependencies are built with debug information, and measure
  per the rule above.
- Run the sccache server in the foreground under its own `time` on macOS
  (`SCCACHE_START_SERVER=1 SCCACHE_NO_DAEMON=1 /usr/bin/time -l sccache &`, then `sccache --stop-server` after the
  build). Never set `SCCACHE_NO_DAEMON=1` on the build command itself: cargo then waits forever on a server it forked.
