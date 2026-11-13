# Installing the server form

How the server form — the daemon `octoboardd` alone, on a Linux server, reached from a browser on another machine (see
`docs/product/server-form.md`) — is published, installed and upgraded. It is in beta, as the desktop application is.
The desktop application's own installation is in `docs/product/installing-the-desktop-app.md`.

## What it runs on

- **Linux on x86_64 or aarch64, any distribution.** Each build is statically linked (against musl), so it needs no C
  library or other package from the system, and carries the UI inside the binary (see "The UI the daemon serves" in
  `docs/product/server-form.md`).
- **Running it as a service needs systemd** (a user manager for the installing user); everything else runs without it.
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
```

| Option | Meaning |
|---|---|
| `--service` | Also install, enable and start the systemd user unit (see "Installing it as a service" below). |
| `--port N` | The port the service's daemon listens on, 1 to 65535; without it, the default server port, **3369**, the one `octoboardd start` listens on (see "Choosing the port" in `docs/product/server-form.md`). Accepted only with `--service`; given without it, it is an error. |
| `--beta` | Install the newest version of the Beta channel (`beta-linux.json`). |
| `--stable` | Install the newest version of the Stable channel (`stable-linux.json`), also when the installed beta is newer. |
| `--allow-downgrade` | Install the channel's newest version even when it is older than the installed one. |
| `--require-signature` | Stop, installing nothing, when the archive's signature cannot be checked, instead of going on with its SHA-256 alone (see step 3 below). |
| `--bin-dir DIR` | Install into `DIR` instead of `~/.local/bin`; a relative directory is taken from the current directory. |
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
5. **Says what next**, unless `--service` was given (below): with a daemon of this user running, that `octoboardd
   restart` moves it to the new version and that its sessions keep running; otherwise, to start it with `octoboardd
   start` (on the default server port, `--port N` picking another) and to open the address `octoboardd address` prints
   through an SSH forward, `ssh -L 3369:127.0.0.1:3369 <this server>`. In these messages, and in those of `--service`,
   `octoboardd` stands for the command as the script names it: `octoboardd` when that is the installed file on the
   `PATH`, its full path otherwise.
6. **Checks the `PATH`**: when the install directory is not on it, says to add it to run `octoboardd` by name.

### Installing it as a service

With `--service`, after installing the binary, the script:

- writes `octoboardd.service` into `$XDG_CONFIG_HOME/systemd/user/` (`~/.config/systemd/user/` without it), made from
  the archive's unit with its `ExecStart` line replaced by `<install directory>/octoboardd --port <port>
  --service-managed`. An existing unit that differs is replaced, and the script says so, with the version and the port,
  and that any changes made to it are gone;
- reloads the user's systemd manager and enables the unit, so that it starts at boot; when the user's manager cannot be
  reached it fails, saying to log in over SSH rather than with `su`, so that a user session exists;
- then:
  - **the service already running**: does not restart it, and says to run `octoboardd restart` to move it to the new
    version — and, when the unit changed, to the new unit, port included — with its sessions kept running, then to open
    the address `octoboardd address` prints. That planned restart has systemd start the next daemon from the new unit
    and the new binary (see "Planned restart and planned stop" in `docs/product/planned-restart-and-planned-stop.md`);
  - **a daemon of this user running outside the service**: enables the service without starting it, and says to stop
    that daemon (`octoboardd stop`) and start the service (`systemctl --user start octoboardd`);
  - **otherwise**: starts the service, says it started on its port and to open the address `octoboardd address` prints
    through an SSH forward; a service that does not start is an error pointing at `journalctl --user -u octoboardd`;
- finally, when lingering is not on for the user, turns it on with `loginctl enable-linger <user>`, never asking for a
  password (polkit lets users turn it on for themselves by default), and says it did, so that the service starts at boot
  and keeps running after logout. Only when that is refused does it say that user services stop at logout and do not
  start at boot until lingering is on, and to turn it on once with `loginctl enable-linger <user>`, which may need
  `sudo`.

### Checking an archive's signature

`octoboardd verify-archive <archive> <signature>` is how the install script has the installed daemon check a download.
It is hidden from the help and not meant to be run by hand, and every build has it, whether or not it updates itself.
`<archive>` is the archive saved under the name the release gave it; `<signature>` is a file holding the manifest's
`signature` (or the `.sig` file itself). It checks that the signature is the release key's (the key built into the
daemon) and that its trusted comment names the archive's file name, changing nothing, and exits with status **0** when
both hold, and **1**, saying why on the error output, when either does not. A command line it does not take, or a build
without the command, exits with status 2. The daemon's own options (`--port` and the like) are not accepted with it.

## Upgrading

A newer version is installed in any of three ways, each followed by a restart:

- **From the browser**: **Download and install** in the update notice or in Settings' Updates section, then **Restart
  now** (see `docs/product/server-form-updates.md`). This needs the official release build, which the install script
  installs, and an install directory the daemon's user may write to.
- **With `octoboardd update`** on the server, then `octoboardd restart` (see "The `octoboardd update` command" in
  `docs/product/server-form-updates.md`).
- **By running the install script again**, then `octoboardd restart` (see "Running in the background from the command
  line" in `docs/product/daemon-command-line.md`). This is the way for a daemon that does not update itself or cannot
  replace its own executable; for the latter the browser offers the script's command to copy.

All three keep the replaced executable as `octoboardd.previous`, which `octoboardd update --rollback` puts back.

The restart is a planned restart: every session and shell terminal keeps running and the next daemon, started from the
new binary, takes them over, on the same port for a daemon started with `octoboardd start`, and on the unit's port for
one the service runs. The owner key stays the same, so an address `octoboardd address` printed before still opens the
daemon (see "The UI the daemon serves" in `docs/product/server-form.md`).
