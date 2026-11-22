# Installing the server form

How the server form — the daemon `octoboardd` alone, on a Linux server, reached from a browser on another machine (see
`docs/product/server-form.md`) — is published, installed and upgraded. It is in beta, as the desktop application is.
The desktop application's own installation is in `docs/product/installing-the-desktop-app.md`.

## What it runs on

- **Linux on x86_64 or aarch64, any distribution.** Each build is statically linked (against musl), so it needs no C
  library or other package from the system, and carries the UI inside the binary (see "The UI the daemon serves" in
  `docs/product/server-form.md`).
- **Running it as a service needs systemd** (a user manager for the installing user), version 236 or later for the
  sessions to outlive a restart of the service (see "Running as a systemd user service" in
  `docs/product/server-form.md`); everything else runs without it.
- **No root.** It is installed for one user, under that user's home directory, and runs as that user.
- It is installed only by the install script below, or by hand from a release's archive. There is no Homebrew formula,
  `.deb` or `.rpm` package, npm package or `cargo install`.
- **It updates itself**, as the desktop application does: it looks for newer releases on its own and offers them in
  the browser, and downloads one, or restarts into it, only when asked (see "Upgrading" below).

## What a release holds

Every release `desktop-v<version>` of the public GitHub repository carries the server form unless the release leaves
the Linux platform out (see "Where updates come from" in `docs/product/in-app-updates.md` for how a release selects its
platforms). For each architecture, `<arch>` being `x86_64` or `aarch64`, it holds:

- **`octoboardd_<version>_linux_<arch>.tar.gz`**, which unpacks into one folder, `octoboardd_<version>_linux_<arch>/`,
  holding the executable `octoboardd`, the example systemd user unit `octoboardd.service` (see "Running as a systemd
  user service" in `docs/product/server-form.md`) and the `LICENSE`;
- **`<archive>.sha256`**, one line in the format `sha256sum -c` checks: the archive's SHA-256 in lowercase hex, two
  spaces and the archive's name;
- **`<archive>.sig`**, the archive's signature, made with the same update key, and in the same format, as the desktop
  application's updates (see "Downloading and installing" in `docs/product/in-app-updates.md`).

**Build provenance attestation.** Each archive also has a GitHub build provenance attestation, made by the release
workflow when it publishes the release, signed on Sigstore's public-good instance (so each one is logged publicly) and
stored with the repository; it is not a file of the release. The release's macOS disk image and update archive and its
Windows installer have one each too; the `.sha256` and `.sig` files and the manifests have none. With the GitHub CLI
installed, a downloaded file is checked to have been built by the release workflow of this repository with:

```sh
gh attestation verify octoboardd_<version>_linux_<arch>.tar.gz --repo baijunjie/octoboard
```

The attestation identifies the file by its SHA-256, not by its name. The provenance names the tag `desktop-v<version>`
for a release made by pushing its tag, but the branch the run was started from for a release, or a platform added to
one, by a run by hand; so `--source-ref refs/tags/desktop-v<version>` holds only for the former. The attestation is an
optional check in addition to the `.sha256` and the `.sig`, replacing neither: the install script and the daemon's
self-update neither check nor require it. Releases published before attestations were introduced have none.

**The Linux manifests.** Each release is also written to `beta-linux.json`, and to `stable-linux.json` as a release
reaches a platform's stable manifest, on the `updater` release, beside the desktop application's manifests and by the
same rules (see "Where updates come from" in `docs/product/in-app-updates.md`):

| Channel | Address |
|---|---|
| Beta | `https://github.com/baijunjie/octoboard/releases/download/updater/beta-linux.json` |
| Stable | `https://github.com/baijunjie/octoboard/releases/download/updater/stable-linux.json` |

The server form's daemon reads them to update itself (see `docs/product/server-form-updates.md`), the install script for
the newest version, and the website for its download links (see "Where download links point" in
`docs/product/website-download-page.md`). A Linux manifest has the fields of
the others — `version`, `notes`, `pub_date`, `holder_floor` and `platforms` — with one entry under `linux-x86_64` and
one under `linux-aarch64`, each holding the archive's `url` on the versioned release, its `signature` (the content of
its `.sig`) and its `sha256` (lowercase hex):

```json
{
  "version": "0.2.0",
  "notes": "…",
  "pub_date": "2026-11-13T00:00:00.000Z",
  "holder_floor": "…",
  "platforms": {
    "linux-x86_64": {
      "signature": "…",
      "url": "https://github.com/baijunjie/octoboard/releases/download/desktop-v0.2.0/octoboardd_0.2.0_linux_x86_64.tar.gz",
      "sha256": "…"
    },
    "linux-aarch64": { "signature": "…", "url": "…", "sha256": "…" }
  }
}
```

## The install script

The website publishes the install script at its root, so it is run as:

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh
curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh -s -- --service [--port N]
curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh -s -- --uninstall [--bin-dir DIR] [--yes] [--force] [--keep-changes]
```

| Option | Meaning |
|---|---|
| `--service` | Also install, enable and start the systemd user unit (see "Installing it as a service" below). |
| `--port N` | The port the service's daemon itself listens on, 1 to 65535; without it, one the system assigns at each start, as `octoboardd start` does (see "Choosing the port" in `docs/product/server-form.md`); the browser port, 3369, is another (see "Browser access" there). Accepted only with `--service`; given without it, it is an error. |
| `--beta` | Install the newest version of the Beta channel (`beta-linux.json`). |
| `--stable` | Install the newest version of the Stable channel (`stable-linux.json`), also when the installed beta is newer. |
| `--allow-downgrade` | Install the channel's newest version even when it is older than the installed one. |
| `--require-signature` | Stop, installing nothing, when the archive's signature cannot be checked, instead of going on with its SHA-256 alone (see step 3 below). |
| `--bin-dir DIR` | Install into `DIR` instead of `~/.local/bin`; a relative directory is taken from the current directory. |
| `--uninstall` | Install nothing, and hand over to the installed `octoboardd uninstall` instead (see "Uninstalling" below). Takes `--bin-dir` where the install had it, and `--yes`, `--force` and `--keep-changes`; no option of an install: given with one, it is an error naming that option. |
| `--yes`, `--force`, `--keep-changes` | With `--uninstall`: passed on to `octoboardd uninstall` (uninstall without asking; stop the daemon whatever work that interrupts; do not clear first what Octoboard's features have in effect outside `~/.octoboard`). Given without `--uninstall`, each is an error. |
| `-h`, `--help` | Print the usage and exit. |

Both `--port N` and `--port=N`, and both forms of `--bin-dir`, are accepted. Any other argument is an error. Without
`--beta` or `--stable` the channel is Stable, except when the `octoboardd` already in the install directory is a
prerelease (its version, read with `octoboardd --version`, has a prerelease part) newer than the version the Stable
manifest announces: then the script reads the Beta manifest instead, saying so in one line, so that a beta is not
offered an older stable release. A prerelease the Stable manifest itself announces (as it does until the first stable
release), the same as the installed one or newer, keeps the installation on Stable. When nothing is installed there or
its version cannot be read, the channel is Stable. Given both options, the last one counts.

Two variables in the environment change where it reads from and what it trusts:

| Variable | Meaning |
|---|---|
| `OCTOBOARD_MANIFEST_BASE` | Replaces the address the manifests are read from (a mirror, or a test). Only while it is set are `file://` addresses downloaded from. |
| `OCTOBOARD_UPDATE_PUBKEY` | Replaces the public key `minisign` checks the signature with, for tests. While it is set the installed `octoboardd`, which knows only the release key, is not asked to check the signature. |

What it does, in order; any failure prints a message starting `octoboardd install:` on the error output and exits with
status 1:

1. **Checks the user and the machine**: it refuses to run as root unless `--bin-dir` is given (installing for root on
   purpose), and refuses a `$HOME` that does not belong to the user running it, as under `sudo` keeping `HOME`, since
   the files would then belong to someone other than the user the daemon runs as. Then: the system is Linux (otherwise it says to install the desktop application on macOS and
   Windows); the architecture `uname -m` reports is `x86_64` (or `amd64`) or `aarch64` (or `arm64`), there being no
   build for any other; `curl` or `wget` is installed to download with, and `sha256sum` or `shasum` to check with. With
   `--service`, `systemctl` must be installed and the install directory's path must hold no whitespace, percent sign,
   backslash, ampersand, vertical bar, dollar sign, semicolon, quote or backtick, since it goes into the unit as it is.
2. **Reads the manifest** of the chosen channel and takes its version and the entry of this machine's architecture. A
   manifest that cannot be read, names no version, has no build for this architecture or has no signature for it is an
   error. When the `octoboardd` in the install directory is newer than the manifest's version (as semantic versioning orders them), it
   refuses, installing nothing, since a daemon may not open what a newer one stored, unless `--allow-downgrade` is
   given; the same version is installed again.

   **Downloading** is over `https` only. It refuses any other address, the manifest's own and those in it, without
   downloading from it (`file://` is accepted too while `OCTOBOARD_MANIFEST_BASE` is set). With `curl`, redirects are
   followed to `https` addresses only and TLS 1.2 or later is required. With `wget` (used only where `curl` is not
   installed), GNU `wget` is given `--https-only`, which may not hold for redirects, and BusyBox `wget` has no such
   option; the checks of step 3 are what hold the download to the release then.
3. **Downloads the archive and checks it.** An archive whose SHA-256 does not match the manifest's is refused and nothing
   is installed. Then the signature in the manifest is checked against the release's update key:
   - **with `minisign`**, where it is installed, against the key built into the script;
   - otherwise, **with the installed `octoboardd`**, when an upgrade finds one in the install directory that has the
     `verify-archive` command, against the key built into it (see "Checking an archive's signature" below); an installed
     build without the command is not asked;
   - otherwise (a first install without `minisign`) it is **not checked**: the script prints a one-line warning on the
     error output, saying that only the SHA-256 from the manifest was checked and to install `minisign` to have it
     checked, and goes on; with `--require-signature` it stops instead, installing nothing.

   Either check also requires the signature's trusted comment to name the archive's own file,
   `octoboardd_<version>_linux_<arch>.tar.gz`, so that an older release's archive, which the same key signed, cannot
   stand in for it. A signature that does not verify, or names another file, is refused and nothing is installed; one
   that verifies makes it say so (and, when the installed `octoboardd` checked it, that it did).

   Then it unpacks the archive with `tar`. When `tar` cannot unpack it, the script says that it could not unpack the
   archive and that nothing was installed, and that if `tar` reports it cannot run `gzip`, installing `gzip` and running
   the script again is the remedy; `tar`'s own message is printed before it. An archive that does not hold `octoboardd`
   is an error too, and nothing is installed.
4. **Installs the binary** as `octoboardd` in the install directory, creating it, by copying it beside the old one and
   renaming it into place in one step: nothing ever sees half a file, and a daemon running from the old file keeps
   running on it until it restarts. The file it replaces is kept beside it as `octoboardd.previous` (a second link to
   it, or a copy where the file system has no hard links), replacing any earlier one, as the server form's own update
   does, so that `octoboardd update --rollback` puts back the build the script replaced; when the new build is the same
   file as the installed one, the existing `octoboardd.previous` is left as it is. When the old file cannot be kept,
   nothing is installed. It says which version it installed and where.

   This step is taken under the update lock, `~/.octoboard/update.lock`, which the daemon and `octoboardd update` hold
   while they replace the executable (see "Downloading and replacing the executable" in
   `docs/product/server-form-updates.md`), when `flock` is installed and `~/.octoboard` exists; while either of them
   holds it, the script refuses, installing nothing, and says to run it again once that update has ended. **Without
   `flock` the lock is not taken**, and nothing keeps the script and an update of the daemon's own (from the browser or
   `octoboardd update`) from replacing the executable at the same time: do not run the script while one is under way.
5. **Restarts a running daemon, or says what next**, unless `--service` was given (below): with a daemon of this user
   running, it lets the update lock go and runs `octoboardd restart --if-safe` with the new binary, which restarts the
   daemon into the new version where that interrupts no session, and otherwise lists what the restart would interrupt
   and leaves it to `octoboardd restart`, which asks first (see "Running in the background from the command line" in
   `docs/product/daemon-command-line.md`). Where it does not restart (it would interrupt work, the daemon refused, the
   restart failed, or the new binary does not take `--if-safe`), the script says that `octoboardd restart` moves the
   daemon to the new version and that its sessions keep running: the restart itself names no command to run next, so
   the advice is given once. With no daemon running, to start it with `octoboardd
   start` and then to run `octoboardd address`, which prints
   the address to open and the SSH forward to open it through (see "The browser socket" in
   `docs/product/server-form.md`). In these messages, and in those of `--service`,
   `octoboardd` stands for the command as the script names it: `octoboardd` when that is the installed file on the
   `PATH`, its full path otherwise.
6. **Checks the `PATH`**: when the install directory is not on it, says to add it to run `octoboardd` by name.

### Installing it as a service

With `--service`, after installing the binary, the script:

- writes `octoboardd.service` into `$XDG_CONFIG_HOME/systemd/user/` (`~/.config/systemd/user/` without it), made from
  the archive's unit with its `ExecStart` line replaced by `<install directory>/octoboardd --service-managed`, with
  `--port <port>` before `--service-managed` where `--port` was given. An existing unit that differs is replaced, and
  the script says so, with the version, and that any changes made to it are gone;
- reloads the user's systemd manager and enables the unit, so that it starts at boot; when the user's manager cannot be
  reached it fails, saying to log in over SSH rather than with `su`, so that a user session exists;
- then:
  - **the service already running**: restarts it as step 5 does, with `octoboardd restart --if-safe`, where that
    interrupts no session; where it does not restart it, it says to run `octoboardd restart` to move it to the new
    version — and, when the unit changed, to the new unit — with its sessions kept running. That planned restart has systemd start the next daemon from the new unit
    and the new binary (see "Planned restart and planned stop" in `docs/product/planned-restart-and-planned-stop.md`);
  - **a daemon of this user running outside the service**: enables the service without starting it, and says to stop
    that daemon (`octoboardd stop`, with `--force` where it refuses because the daemon runs work) and start the service (`systemctl --user start octoboardd`);
  - **otherwise**: starts the service, says it started and to run `octoboardd address`, which prints the
    address to open and the SSH forward to open it through; a service that does not start is an error pointing at
    `journalctl --user -u octoboardd`;
- finally, when lingering is not on for the user, turns it on with `loginctl enable-linger <user>`, never asking for a
  password (polkit lets users turn it on for themselves by default), and says it did, so that the service starts at boot
  and keeps running after logout. Having turned it on, it notes that it did in `~/.octoboard/install.lingering`, holding
  the user's name, creating `~/.octoboard` for the user alone where it does not exist yet, so that the uninstall
  (`octoboardd uninstall`, see "Uninstalling" below) turns lingering off again; where the note cannot be written it
  says, in a warning, that the uninstall will leave lingering on. Lingering that was already on gets no note. Only when turning it on is refused does it say that user services
  stop at logout and do not start at boot until lingering is on, and to turn it on once with
  `loginctl enable-linger <user>`, which may need `sudo`.

### Checking an archive's signature

`octoboardd verify-archive <archive> <signature>` is how the install script has the installed daemon check a download.
It is hidden from the help and not meant to be run by hand, and every build has it, whether or not it updates itself.
`<archive>` is the archive saved under the name the release gave it; `<signature>` is a file holding the manifest's
`signature` (or the `.sig` file itself). It checks that the signature is the release key's (the key built into the
daemon) and that its trusted comment names the archive's file name, changing nothing, and exits with status **0** when
both hold, and **1**, saying why on the error output, when either does not. A command line it does not take, or a build
without the command, exits with status 2. The daemon's own options (`--port` and the like) are not accepted with it.

## Upgrading

A newer version is installed in any of three ways, each followed by a restart, which each makes by itself where it
interrupts no session:

- **From the browser**: **Download and install** in the update notice or in Settings' Updates section, which restarts
  into the update once installed, asking first where that would interrupt work (see
  `docs/product/server-form-updates.md`). This needs the official release build, which the install script installs,
  and an install directory the daemon's user may write to.
- **With `octoboardd update`** on the server, which restarts the same way, asking in a terminal and otherwise leaving
  the restart to `octoboardd restart` where it would interrupt work (see "The `octoboardd update` command" in
  `docs/product/server-form-updates.md`). Both of these first run the new build with `--version`, and replace nothing
  unless it answers with the release's version.
- **By running the install script again**, which restarts a running daemon where that interrupts nothing and otherwise
  leaves it to `octoboardd restart` (see "Running in the background from the command line" in
  `docs/product/daemon-command-line.md`). This is the way for a daemon that does not update itself or cannot
  replace its own executable; for the latter the browser offers the script's command to copy, told the executable's
  directory and the daemon's channel, for the owner of that directory to run on the server (see "Downloading and
  replacing the executable" in `docs/product/server-form-updates.md`).

All three keep the replaced executable as `octoboardd.previous`, which `octoboardd update --rollback` puts back, except
across a database migration (see "Rolling back across a database migration" in `docs/product/server-form-updates.md`).

The restart is a planned restart: every session and shell terminal keeps running and the next daemon, started from the
new binary, takes them over, on the same port for a daemon started with `octoboardd start`, and on a port the system assigns again for
one the service runs, unless the unit names one with `--port`; no address a user opens names the daemon's own port. The owner key stays the same, so an address `octoboardd address` printed before still opens the
daemon, through the forward it prints (see "The UI the daemon serves" in `docs/product/server-form.md`).

## Uninstalling

The server form is uninstalled with its own command, run on the server as the user it was installed for, with nothing
downloaded:

```sh
octoboardd uninstall
```

It first clears what Octoboard's features have in effect on the system outside `~/.octoboard` (each account that shares
its agent's default configuration), which only `octoboardd` can clear and nothing clears once it is removed: a daemon
that runs is stopped first, and the clearing is done by a daemon the command starts only for it, which runs no sessions.
Where something stays in effect, nothing is removed and the daemon it stopped is started again as it ran (through its
service, or in the background). Otherwise it removes what the install set up, leaving `~/.octoboard`. What it does, in order, its options (`--yes`,
`--force`, `--keep-changes`, `--dry-run`) and its exit statuses are in "Uninstalling the server form" in
`docs/product/daemon-command-line.md`.

The install script's one-liner still works, and hands over to that command:

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh -s -- --uninstall
```

with `--bin-dir DIR` where the install was given one, and `--yes`, `--force` and `--keep-changes`, which it passes on;
no option of an install goes with it. It makes the same checks of the user as an install (step 1 of "The install
script" above): it refuses to run as root without `--bin-dir`, and a `$HOME` that does not belong to the user running
it. Off Linux it refuses, saying there is nothing of it to remove there. Any failure of its own prints a message
starting `octoboardd install:` on the error output and exits with status 1. Then it looks at `octoboardd` in the install
directory: where that executable knows the command (asked with `octoboardd uninstall --help`), it runs `octoboardd
uninstall` with the options passed on, taking the terminal for its input where the script itself was piped in and a
terminal is there, so that the command can ask, and ends with the command's exit status.

**Only for a build from before the command** does the script remove the installation itself, as it always has. It warns
first that nothing is cleared first, that an older build may have left an account sharing its agent's default
configuration (entries of the account's directory then being links into the default account's directory, which can be
deleted by hand), and that `--yes`, `--force` and `--keep-changes` have no effect there. Then, in order:

1. **Nothing installed**: where the install directory holds none of the files below, there is no unit, and
   `~/.octoboard` holds no `install.lingering`, it says that `octoboardd` is not installed in that directory and has no
   service, that nothing was removed, and to pass the same `--bin-dir` where it was installed with one.
2. **A unit that runs another file**: it reads the program of the unit's first `ExecStart` line, its first word, taking
   a leading `%h/` as the user's home directory, so both the unit the script writes (which names the executable by its
   full path) and the archive's own `octoboardd.service` copied by hand
   (`ExecStart=%h/.local/bin/octoboardd --service-managed`) count for the default install directory. Where that program
   is not `octoboardd` in this install directory, it stops, removing nothing, naming the file the unit runs; it says to
   pass `--bin-dir` with the directory where `octoboardd` was installed into another one, and that a unit the user wrote
   is not the script's to remove.
3. **Files it could not remove**: where any of the files below does not belong to the user running the script (as when
   run under `sudo` with `--bin-dir`), it stops, removing nothing, and says to run it as the user who installed
   `octoboardd`, without `sudo`; where the install directory holds any of them and cannot be written by that user, it
   stops, removing nothing, saying so. Both are checked before anything is stopped.
4. **The daemon**: where the installed `octoboardd` answers `octoboardd status` (a daemon of this user runs), it stops
   it with `octoboardd stop`. Where that does not stop it, whatever the reason (work that stopping would interrupt, or a
   daemon still running), it stops, removing nothing, and says that `octoboardd stop` did not stop the daemon, pointing
   at what that command said; where that named work stopping would interrupt, `octoboardd stop --force` stops it all
   the same and ends its sessions as interrupted, after which the uninstall is run again (see "Running in the
   background from the command line" in `docs/product/daemon-command-line.md`).
5. **The unit**, where there is one: `systemctl --user disable --now octoboardd.service`, then the unit file
   `octoboardd.service` is deleted from `$XDG_CONFIG_HOME/systemd/user/` (`~/.config/systemd/user/` without it), the
   user's manager is reloaded and the unit's failed state reset. Where `systemctl` is missing or the user's manager
   cannot be reached, the unit file and the link that enabled it, `default.target.wants/octoboardd.service` beside it,
   are deleted by hand, and a warning says so, and that a service still running stops at logout or with
   `systemctl --user stop octoboardd`.
6. **Lingering**, only where `~/.octoboard/install.lingering` says the install turned it on (see "Installing it as a
   service" above). Where another service of the user's is enabled — a link in any `*.wants/`, `*.requires/` or
   `*.upholds/` folder of `${XDG_CONFIG_HOME:-~/.config}/systemd/user/`, or of the generators' directories of the user's
   runtime directory, other than `octoboardd.service` — it leaves lingering on and the note in place, names those units, and says that
   `loginctl disable-linger <user>`, which may need `sudo`, turns it off all the same. Otherwise it turns lingering off
   with `loginctl disable-linger <user>`, never asking for a password, deletes the note and says so. Where that is
   refused, the note stays and it says to turn lingering off with `loginctl disable-linger <user>`, which may need
   `sudo`. Lingering that was on before the install, and lingering an install by a script that kept no note turned on,
   are left as they are.
7. **The files**, in the install directory: `octoboardd`, `octoboardd.previous` and every `.octoboardd.*` file, what an
   update, a rollback or an interrupted install leaves beside the executable (see "Beside the server form's executable"
   in `docs/product/files-octoboard-owns.md`). A file that cannot be deleted is an error. The directory itself stays,
   and so does any line the user added to put it on the `PATH`.
8. **`~/.octoboard`**, the daemon's own data (its consoles, session records, settings and logs), is left as it is,
   where it exists; it says so, with `rm -rf ~/.octoboard` as the way to remove it too. This is said in case 1 as well.

A browser on the daemon offers no clean-up of its own: Settings' Cleanup section says that it is part of uninstalling on
the server, with `octoboardd uninstall` (see "Which installation it speaks for" in
`docs/product/clean-up-before-uninstalling.md`). What stays on the server afterwards, and how to remove it by hand, is in
`docs/product/what-remains-after-uninstalling.md`.
