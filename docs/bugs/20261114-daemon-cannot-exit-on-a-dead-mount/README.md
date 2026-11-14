> Severity: Moderate

# The daemon cannot exit while one of its threads is stuck on an unresponsive mount

## Symptom

Once a daemon thread has looked at a registered project's path on a network mount that stopped answering, the daemon
can no longer exit: `octoboardd stop` gives up saying it is still stopping, and the process stays until the mount
answers again.

## Reproduction steps

Observed on macOS with a daemon built from 4d0d565a3; no root is needed.

1. Start the server in this directory: `node hanging-webdav-server.mjs 47312`. It serves a read-only WebDAV share whose
   `/ok/` answers and whose `/hang/` accepts every request and never replies.
2. Mount it as your own user: `mkdir <mnt> && mount_webdav -S http://127.0.0.1:47312/ <mnt>`.
3. Run a daemon under a throwaway `HOME`, on a port of its own, and register a project at a normal git repository
   inside that home, e.g. `<home>/code/delta`, plus one other project the session will be in.
4. Replace `<home>/code/delta` with a symbolic link to `<mnt>/hang/delta`.
5. Turn **Keep sessions to their own project** on and open a project session in the other project. The launch waits
   about 5 seconds for the look at `delta`, then goes on (this part is as designed).
6. Run `octoboardd stop` against that daemon.
7. Stop the server from step 1 (or otherwise make the mount answer).

## Expected vs. actual

- Expected: at step 6 the daemon stops, every session interrupted, and exits, as `octoboardd stop` is described in
  "Running in the background from the command line" in `docs/product/daemon-command-line.md`.
- Actual: `stop` reports the daemon as still stopping after its wait; the process remains in uninterruptible wait
  (`ps` state `U`) and only exits at step 7.

## Environment

- macOS (Darwin 25), daemon at 4d0d565a3, run on its own with a throwaway `HOME`.
- Precondition: a registered project path that resolves into a mount whose server no longer answers. A user-mode
  WebDAV mount stands in for a dead network share here; whether a real NFS or SMB share that went away behaves the
  same, or times out first, is unknown.
- Linux and Windows: unknown.

## Scope of impact

Users with a project on a network share that becomes unreachable while the daemon runs. Every later stop, restart or
update of the daemon hangs until the share answers again, and the next daemon cannot take the data directory's lock
meanwhile. Workaround: bring the share back (or its server). Whether a forced unmount (`umount -f`) frees the daemon is
unknown.

## Leads

- Verified: a `sample` of the stuck daemon showed the blocked thread in
  `hostfs::place::looked_at` → `git::repo_files::repository_of` → `hostfs::canonical` → `realpath` →
  `__getattrlist`, running as a tokio blocking task that the launch had abandoned after its 5-second deadline
  (`LOOK_DEADLINE` in `apps/daemon/src/cross_project_guard/list.rs`).
- Verified: the launch itself was not held up beyond the deadline, and a second launch while the look was still stuck
  took 0.1 s.
- Inferred: the cause is not specific to the cross-project guard. Any daemon code that touches a path on such a mount
  from a thread of the daemon's own process leaves that thread in the kernel's uninterruptible wait, and a process
  cannot finish exiting while it has one; other looks with a deadline, such as `TRASH_LOOK_DEADLINE` in
  `apps/daemon/src/removal_safety/mod.rs`, abandon their thread the same way.

## Acceptance criteria

- [ ] With the reproduction above, `octoboardd stop` at step 6 ends the daemon within its usual wait, without step 7.
- [ ] The same holds after the removal-safety check (moving a project on the dead mount to the Trash) has abandoned a
      look at it.
- [ ] A launch with a project on the dead mount still goes on after the existing 5-second look.
