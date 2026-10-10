> Severity: Major

# The daemon keeps running after the application is gone

## Symptom

When the application process ends without a quit — killed with `SIGTERM` or `SIGKILL` — its `octoboardd` sidecar,
and every agent process it owns, keeps running instead of exiting by itself.

## Reproduction steps

In the app:

1. Build the app with `pnpm build:app --bundles app` (in `apps/desktop`) and copy
   `target/release/bundle/macos/Octoboard.app` to a directory on the internal disk.
2. Make two empty directories for a throwaway `HOME` and `TMPDIR`, and launch the copy with

   ```sh
   env -i HOME=<home> TMPDIR=<tmp>/ USER=$USER SHELL=/bin/zsh PATH=/usr/bin:/bin:/usr/sbin:/sbin /usr/bin/open -n <copy>/Octoboard.app
   ```

3. Once the window is up, note the two pids: the application (`…/Contents/MacOS/octoboard`) and its sidecar
   (`…/Contents/MacOS/octoboardd --parent-pid <application pid>`), e.g. with
   `ps -axo pid,ppid,command | grep <copy>/Octoboard.app`.
4. `kill -TERM <application pid>` (or `kill -KILL <application pid>`).
5. Poll `ps -p <sidecar pid>` once a second: the sidecar is still there, reparented to launchd, still listening on its
   port and holding `<home>/.octoboard/daemon.lock`.

Without the app, the same daemon binary shows it as soon as its stderr has no reader:

1. `sleep 1000 &` and note its pid as the stand-in parent.
2. Start the daemon with its stderr going into a pipe whose reader exits at once (`head -c 0` exits with an error on
   macOS, which is all that is needed):

   ```sh
   env -i HOME=<home> TMPDIR=<tmp>/ PATH=/usr/bin:/bin <copy>/Octoboard.app/Contents/MacOS/octoboardd --parent-pid <sleep pid> 3>&1 1>/dev/null 2>&3 | head -c 0 &
   ```

3. `kill <sleep pid>`, then poll the daemon's pid: it is still running.

Controls run the same way: with stderr redirected to a file, or with only stdout going into a pipe with no reader
(`2> <file> | head -1`), the daemon logs "the application is gone; shutting down" and exits within one to two seconds
of the stand-in parent being killed.

## Expected vs. actual

- Expected: the daemon exits by itself once the application is gone, and the agent processes go with it.
  `docs/product/application-lifecycle.md`, "Crashes and forced termination": "The daemon exits by itself once the
  application is gone, whether or not it was asked to." The same file's opening paragraph: "nothing keeps running once
  the application is gone".
- Actual: the daemon keeps running, with every agent session it launched, until it is signalled directly.

## Environment

- macOS (Darwin 25.2.0), Apple Silicon; an unsigned local build from `pnpm build:app --bundles app`.
- Built from branch `feat/lead-sessions` at `8034d53`. Between `main` and that commit, `apps/daemon/src/main.rs`
  differs only by a `mod console_request;` line, so the parent watch and the shutdown path are the same on `main`;
  `main` itself was not run.
- Throwaway `HOME` and `TMPDIR`; no `RUST_LOG` set. Reproduced with no session open and with nine agent processes
  running (Claude Code, Codex, Grok Build sessions).

## Scope of impact

- Every crash or forced termination of the application: the agents it ran keep running and working with no window to
  show them, and the daemon keeps the instance lock of that data directory.
- Workaround: find the orphaned `octoboardd` (parent pid 1, `--parent-pid` naming a pid that no longer exists) and
  send it `SIGTERM`; it then stops its agents and exits within about two seconds (verified).
- Whether a normal quit is affected: Unknown (not tried).

## Leads

- Verified: the daemon outlived the application for at least 183 s after `SIGTERM` and at least 61 s after `SIGKILL`
  (each run stopped there by killing the daemon by hand); in an earlier run with agent sessions open it was still
  running over a minute after `SIGTERM`.
- Verified: whether the daemon exits depends on its stderr, not on the parent: it exits on the parent's death with
  stderr writable, and stays with stderr a pipe that has no reader (stdout alone in that state does not matter). The
  application owns the read end of the sidecar's stdout and stderr, so its death leaves the daemon in exactly that
  state.
- Verified: logging goes to stderr with `INFO` always enabled (`apps/daemon/src/main.rs`, the
  `tracing_subscriber::fmt()` set-up), and the parent watch (`spawn_parent_watch` in the same file) logs
  `tracing::info!("the application is gone; shutting down")` immediately before `state.request_shutdown()`.
- Inferred: the watch's log write to the dead pipe never returns control to `request_shutdown()` — it fails in a way
  that ends the watch task (a panic, for instance) or blocks — so the shutdown is never requested. Not confirmed which.
- Inferred: a relaunch on the same data directory while the orphan holds `daemon.lock` would open on the startup error
  screen (`docs/memory/building-and-launching-the-app-for-verification.md` describes that outcome for a second daemon);
  not tried.

## Acceptance criteria

- [ ] With the packaged app launched as in the reproduction steps, `kill -TERM <application pid>` leaves no
      `octoboardd` and no agent process of that run within a few seconds.
- [ ] The same holds for `kill -KILL <application pid>`.
- [ ] The same holds with agent sessions open at the time, and the sessions read as *interrupted* on the next launch.
- [ ] The daemon run from a shell with `--parent-pid <pid>` and stderr into a pipe with no reader exits once `<pid>`
      is gone.
