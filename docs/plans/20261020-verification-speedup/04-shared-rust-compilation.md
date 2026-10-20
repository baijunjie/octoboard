# 04 Shared Rust compilation

> Goal: a new worktree's first Rust build reuses what other worktrees have already compiled.
> Completion criteria: a trial in a fresh worktree shows the cold daemon test build's time against milestone 01's
> cold baseline, with the cache hit rate; the approach is adopted with setup documented only if it measures a clear
> gain, and dropped otherwise with the numbers recorded here.

## Technical design

- [ ] A compilation cache (sccache) as the user-level Rust compiler wrapper, covering the C code some dependencies
      build too.
- [ ] Alternative to try if the cache misses across worktree paths: cloning main's debug build directory
      copy-on-write when a worktree is created.

## Notes for the developer

- **Development notes**: workspace crates are compiled incrementally and are not cached; the gain is in dependencies.
  Build-script outputs in a cloned build directory carry absolute paths into main's checkout, so check a clone stays
  fresh without a full rebuild. Do not share one target directory between worktrees (see the overview).
- **Reference docs**: `docs/memory/building-and-launching-the-app-for-verification.md`.
