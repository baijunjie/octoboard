# Verifying the daemon on Linux

## Run the tests of Linux-only code, and of how a PTY ends, in a Linux container

Applies when you are developing on macOS (the daemon is meant to run on a Linux host too) and change either kind of
daemon code below, since a clean run on macOS says nothing about it:

- Code under `#[cfg(target_os = "linux")]`. `cargo check`, `cargo clippy` and the test suite on macOS skip it
  entirely, and `--target x86_64-unknown-linux-gnu` from macOS fails in rusqlite's `bundled` build script without a
  Linux C cross-compiler.
- How a terminal's PTY comes to an end — the reader thread, closing the master, hanging the terminal up
  (`apps/daemon/src/terminal/pty.rs`). On macOS a session leader's exit revokes its controlling terminal, so the reader
  ends with the shell or agent even while a background job it left behind still holds the PTY's other side; Linux
  revokes nothing, and the reader, the master and the buffers live on until that job goes. A test of a terminal
  outliving its process therefore cannot fail on macOS.

So run `cargo test -p octoboardd`, and `cargo clippy --all-targets -p octoboardd -- -D warnings` for the Linux-only
code (a warning fails the Linux checks a release runs), inside a Linux container
(Docker). Start the container with `docker run --init`: without an init that reaps, orphans reparented to PID 1 (here
cargo) stay zombies, and every test that waits for a process to be gone (`test_support::assert_gone_by`, which waits for
`ESRCH`) fails, on `main` as on the change. Give cargo there a target directory of its own (a Docker volume), not the
worktree's `target/` that the host's builds use, and one per architecture when you also run an x86_64 container under
emulation on an arm64 host (`--platform linux/amd64`, which works at about half the native speed): their build-script
artifacts share paths and collide. Set `CARGO_PROFILE_DEV_DEBUG=0` (or `line-tables-only`, which
also fits and keeps line numbers in backtraces): linking the daemon's test binary with full debug info takes several GiB
of memory, enough for a Docker VM's memory limit to SIGKILL it, and the tests behave the same without it. For the same
reason run one Linux build at a time, with `CARGO_BUILD_JOBS=2`.

Run the suite as a normal user, not as root, the container's default user: some `accounts::shared_config` tests fail
as root. Switching to that user with Debian's `su` gives it umask 002, under which the `path_links` tests fail, since
they require temp paths closed to writes by group and others; set `umask 022` before running cargo. Install
`procps` (the suite runs `ps`; slim images lack it) and install the toolchain for that user rather than reusing root's:
when `ps` is missing and a `/root/...` directory is left on the user's `PATH`, the lookup hits that unsearchable
directory and about 150 tests fail with `ps ran: PermissionDenied`, which reads as a permission bug in the daemon.

To exercise a fallback for a refused system call (the browse reads' `openat2` refused as `ENOSYS` or `EPERM`), run
the container with a seccomp profile that returns that errno for the call.

A plain container's `/sys/fs/cgroup` is read-only, so it exercises only the fallback of the launch cgroup groups
(`apps/daemon/src/platform/proc/cgroup.rs`): the tests of the groups themselves return early and pass, their "skipped" line hidden
with the rest of a passing test's output. When a change
can affect those groups — the cgroup code, the Unix `Containment`, the process ledger, or how a launch's process is
started (`apps/daemon/src/platform/term/unix/start.rs`, `apps/daemon/src/platform/proc/server.rs`), since it enters its
group in the child before its program runs — run the suite a second time with a
writable cgroup v2 hierarchy, through `.github/daemon-cgroup-tests.sh`, which sets the hierarchy up for a normal user,
runs the suite with `OCTOBOARD_REQUIRE_CGROUP=1` and fails on a group left behind. Start it as the `cgroup-tests` job of
`.github/workflows/linux-daemon.yml` does: as root from the checkout, in a container run with
`--privileged --cgroupns=private --init` and `TEST_UID` set to the uid that owns the checkout there; locally, pass the
settings above in with `-e`, and give it a target directory that user can write. Without a container runtime, a manual
run of `quality.yml` on the pushed branch runs the same job. With a setup of your own instead, set
`OCTOBOARD_REQUIRE_CGROUP=1` yourself: without it a hierarchy you failed to make writable passes as skipped tests. Every
cgroup test that returns early for want of a group says so through `test_support::cgroups::skip`, so with it a
skipped test fails instead of passing unrun.

Without a container runtime, Linux-only code gets a compile check only: copy the Linux-only functions into a scratch
crate outside the repository that depends only on what they use (typically `libc`), and run
`cargo check --target x86_64-unknown-linux-gnu` there after `rustup target add x86_64-unknown-linux-gnu`; say when
reporting that it proves it compiles, not how it behaves. How a PTY ends has no such fallback: report it as
unverified on Linux.

## Verify the daemon under systemd and a real login in a container with systemd as PID 1

Applies when a Linux check depends on systemd or on a login session: the example user unit
(`apps/daemon/dist/systemd/octoboardd.service`) and its planned restart, anything run as a systemd user unit, the unit's
`Delegate=yes`, a browser reaching the daemon over an SSH forward. A test-suite
container (`docker run --init`) has neither a service manager nor a login session.

- Image: Debian with `systemd`, `systemd-sysv`, `systemd-container`, `dbus-user-session`, `libpam-systemd` (and
  `openssh-server` when the check needs SSH), `CMD ["/sbin/init"]`,
  `STOPSIGNAL SIGRTMIN+3`; run it with `docker run -d --privileged --cgroupns=private --tmpfs /run --tmpfs /run/lock`.
  It boots to `degraded` only because `systemd-modules-load` fails, which is harmless; cgroup v2 is writable and
  `Delegate=yes` works.
- systemd mounts a tmpfs over `/tmp`, hiding whatever `docker cp` put there, before boot or after: copy binaries and
  files elsewhere, such as `/opt`.
- Run each user's commands through a real login, not `docker exec … su`, which gets no logind session and no
  `XDG_RUNTIME_DIR`, so `systemctl --user` and `journalctl --user` fail. Without SSH in the check, use
  `docker exec <container> machinectl shell -q <user>@.host /bin/sh <script>`; when the check involves an SSH forward,
  log in as `ssh <user>@<server>` from a second container on the same Docker network. `loginctl enable-linger <user>`
  keeps the user manager, and its units, past the login.
- Open an SSH forward (`ssh -N -L <port>:127.0.0.1:<port>`) from anywhere but the server itself, where its listener
  collides with the daemon's own on the same loopback port: from that second container for a client inside Docker, or,
  for a real browser on the macOS host, from the host. For the latter publish only the server container's SSH port, and
  only to the host's loopback (`-p 127.0.0.1:2222:22`), log in with a key the image authorizes, and open
  `http://localhost:<port>/?key=…` there; leave the daemon's port unpublished.
- Debian trixie makes home directories 0700, and an SSH login there has umask 0002, so what the daemon creates without
  an explicit mode comes out group-writable (`~/.octoboard` as 0775).
- To exercise a host shutdown, `docker restart -t 120 <container>` (or `docker stop -t 120`): systemd shuts down
  fully on the `STOPSIGNAL`, but Docker's default 10-second timeout SIGKILLs it partway, well inside the daemon unit's
  stop allowance, and the result says nothing about the real order. To stop only one user's manager and its units,
  `systemctl stop user@<uid>.service` as root. Read the order units actually stopped in from the journal.

## Prove that no key reaches another user by scanning as that user, with a negative control

Applies when a Linux check has to show that another user of the host cannot get a key the daemon or a process holder
accepts. Run it in a container with two users: A runs the daemon, B scans. A plain container with the daemon started
by hand is enough; systemd is needed only when what you check is something a systemd unit or a login session holds.

- Give A umask 0002 and a 0755 home. Debian makes homes 0700, which keeps B out of everything and proves nothing about
  the modes inside it.
- As root, collect every secret first (`~/.octoboard/daemon.key`, `run/*/key`, the secret in `holders/*.json`), so the
  scan has exact strings to look for. Make the harness refuse an empty secret: one collected before what holds it
  exists comes out empty and "matches" every process. Where a secret only appears after a process that could leak it
  has already come and gone (a holder started through `systemd-run`, whose command line lives a moment), have B record
  every distinct `cmdline` and `environ` it sees, and check that record against the secrets collected afterwards.
- As B, look in all three places: every `/proc/*/cmdline`, read in a tight loop while the helpers run; every
  `/proc/*/environ`; every file B can read. A helper that only lives for a moment (`git-ref`, `gate-hook`) appears only
  to a tight loop running during the git commands. Keep the longer-lived helpers alive by holding their stdin open.
- Agents in a container are not signed in, so the daemon refuses to open sessions with `account_not_signed_in` unless
  each account has a dummy API key (Codex: `codex login --with-api-key`; Claude Code: `{"env":{"ANTHROPIC_API_KEY":"<dummy>"}}`
  in that user's `~/.claude/settings.json`). No turns run either, so start each helper from the exact command the launch
  generated, and check that the daemon still accepts each helper's request (a wrong key gets 401).
- Add a negative control: put a key in a dummy process's arguments and in a 0664 file, and the scan must find both.
- Keep the key out of your own harness's arguments (such as a WebSocket client) and out of a world-readable file
  capturing the daemon's stdout, whose first line carries it. Otherwise the scan finds your leak, not the daemon's.
- Root in a default Docker container cannot read another user's `/proc/<pid>/environ` (`CAP_SYS_PTRACE` is dropped),
  so you cannot use it to see what an environment holds.

## Test the self-update end to end from a copy of the tree pointed at a local origin

Applies when a check needs the server form's real self-update (`octoboardd update`, or the daemon's own) to fetch,
verify and install a release, beyond the self-update's unit tests. It needs no change to the daemon's code, and none of
it is done in the real tree, where the edits could be committed. In a copy of the tree: point `config/app.json`'s
`repositoryUrl`, which `apps/daemon/build.rs` builds in as the base of the manifest's address
(`apps/daemon/src/self_update/manifest.rs`), at an HTTPS origin served locally; replace `PUBLIC_KEY` in
`apps/daemon/src/self_update/verify.rs` with the public key of a throwaway minisign key made by
`tauri signer generate`, and sign the test archives with that key. Serve the origin with a certificate from a
self-signed CA installed into the container's system trust store: the fetcher verifies against the platform's store
(`RootCerts::PlatformVerifier`), so nothing else has to be told to trust it.
