# Rust dependency caches

The composite action in `action.yml` shares dependency caches between hosted quality checks, release builds and manual
cache warming. It wraps the pinned `Swatinem/rust-cache` action; workflows install the repository's pinned Rust
toolchain before invoking it so the compiler identity contributes to the cache key.

## Interface and profiles

- Input `profile` selects a shared key, `octoboard-<profile>-<platform-sdk>-v1`, across workflows.
- Output `cache-hit` reports an exact cache-key match. A false value can still accompany a partial restore.

| Profile | Consumer | Main-branch warming |
|---|---|---|
| `checks` | The macOS native quality job | `pnpm clippy:rust`, then `pnpm test:rust --no-run`. |
| `release` | The macOS universal release build | `pnpm --filter @octoboard/desktop release`, unsigned and without signing credentials. |
| `linux-check` | The Linux compilation job and the clippy job of `linux-daemon.yml` | `node scripts/cargo.mjs check --locked`, after installing the Linux prerequisites. |
| `linux-musl` | The musl jobs of `linux-daemon.yml` (the daemon's tests on `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`, on a runner of each architecture; run by the quality and release workflows) | `cargo test --locked -p octoboardd --target <musl target> --no-run` on a runner of each architecture, after installing `musl-tools`. |
| `linux-release` | The Linux release builds (`release.yml`'s `linux` jobs, one per architecture) | `pnpm --filter @octoboard/desktop release --arch <arch> --no-sign-update` on a runner of each architecture, after installing `musl-tools`; unsigned and without the update key. |
| `windows-check` | The Windows compilation job | None: the job saves its own cache when it runs on main. |
| `windows-release` | The Windows release builds (`release.yml`'s Windows jobs) and the Windows installer build of the quality workflow (`windows-bundle`) | `WINDOWS_SIGNING=none pnpm --filter @octoboard/desktop release --no-sign-update`, unsigned and without the update key. |

The platform component hashes `xcodebuild -version` on macOS, uses the distribution ID and version from
`/etc/os-release` on Linux, and is the runner OS's name on Windows. The cache keys also distinguish runner OS,
architecture, compiler, dependency manifests and compilation settings, including `SDKROOT`, `DEVELOPER_DIR` and
`MACOSX_DEPLOYMENT_TARGET`. Profiles keep the check, universal-build, Linux compilation, Linux musl, Linux release, Windows compilation and Windows
release artifacts separate. Cargo's registry and dependency build artifacts are reused;
installed Cargo binaries, workspace crates and application bundles are excluded.
The [pinned source](https://github.com/Swatinem/rust-cache/tree/d1527c8f67ef9af357290475d4af4b0fab8f12e0)
defines the action's key and cleanup behavior.

Only runs on `refs/heads/main` save caches. `cache-on-failure` allows a failed main run to save compiled dependencies;
this does not save a passing test result. Version-tag runs restore compatible caches from main and do not write their
own. GitHub allows a run to restore caches from its current ref or the default branch; caches belonging to a different
tag are inaccessible. See "Restrictions for accessing a cache" in the
[GitHub cache reference](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching).

## Warm caches on main

Once the workflow has landed on main, use **Actions → Warm Rust caches → Run workflow**, select `main`, or run:

```sh
gh workflow run warm-rust-cache.yml --ref main
```

The workflow rejects other refs. It runs the `checks`, `release`, `linux-check`, `linux-release` and `linux-musl` (each
on an x86_64 and an aarch64 Ubuntu runner) and `windows-release` profiles on GitHub-hosted macOS, Ubuntu and Windows runners, without signing
secrets or publishing. The macOS release profile builds
both architectures; the checks profile compiles its tests without executing them. Warming does not replace the full checks in the quality workflow.

Warm before the first release and after dependency or toolchain changes; repeat when eviction leaves useful caches
unavailable. It is an explicit operation rather than a build on every main push. A warm run does not guarantee the
next release will find or benefit from a compatible cache.

GitHub's default repository cache limit is **10 GB**, and entries unused for more than **7 days** are removed. Storage
pressure can evict older entries sooner; administrators can opt into a larger, billed allowance. Check "Usage limits
and eviction policy" in the
[GitHub cache reference](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)
when deciding whether to change the cache policy.

## Measure a run

The action adds its profile, runner OS and architecture, exact-hit result and restored `target` directory size to the
job summary. That directory size is uncompressed disk usage, not the saved cache archive size; inspect the action's
post-run logs for the uploaded compressed size.

Quality checks, Linux compilation, cache warming and the signed release build use `/usr/bin/time -v` on Linux or
`/usr/bin/time -l` on macOS. Their logs report elapsed time, CPU use and maximum resident memory. Compare matching
profiles and toolchains, accounting for cache restoration and saving overhead; a cache hit alone does not establish
a speed improvement. The quality jobs still use `--fresh` and execute their checks regardless of cache restoration.
