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
- There is no automatic update: a newer version is installed by running the install script again (see "Upgrading"
  below).

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

**The Linux manifests.** Each release is also written to `beta-linux.json`, and to `stable-linux.json` as a release
reaches a platform's stable manifest, on the `updater` release, beside the desktop application's manifests and by the
same rules (see "Where updates come from" in `docs/product/in-app-updates.md`):

| Channel | Address |
|---|---|
| Beta | `https://github.com/baijunjie/octoboard/releases/download/updater/beta-linux.json` |
| Stable | `https://github.com/baijunjie/octoboard/releases/download/updater/stable-linux.json` |

No application reads them: the install script reads them for the newest version, and the website for its download
links (see "Where download links point" in `docs/product/website-download-page.md`). A Linux manifest has the fields of
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
curl -fsSL https://octoboard.dev/install.sh | sh
curl -fsSL https://octoboard.dev/install.sh | sh -s -- --service [--port N]
```

| Option | Meaning |
|---|---|
| `--service` | Also install, enable and start the systemd user unit (see "Installing it as a service" below). |
| `--port N` | The port the service's daemon listens on, 1 to 65535; without it, the default server port, **3369**, the one `octoboardd start` listens on (see "Choosing the port" in `docs/product/server-form.md`). Accepted only with `--service`; given without it, it is an error. |
| `--beta` | Install the newest version of the Beta channel (`beta-linux.json`) instead of the Stable one (`stable-linux.json`). |
| `--bin-dir DIR` | Install into `DIR` instead of `~/.local/bin`; a relative directory is taken from the current directory. |
| `-h`, `--help` | Print the usage and exit. |

Both `--port N` and `--port=N`, and both forms of `--bin-dir`, are accepted. Any other argument is an error.
`OCTOBOARD_MANIFEST_BASE` in the environment replaces the address the manifests are read from (a mirror, or a test), and
`OCTOBOARD_UPDATE_PUBKEY` the public key a signature is checked with.

What it does, in order; any failure prints a message starting `octoboardd install:` on the error output and exits with
status 1:

1. **Checks the machine**: the system is Linux (otherwise it says to install the desktop application on macOS and
   Windows); the architecture `uname -m` reports is `x86_64` (or `amd64`) or `aarch64` (or `arm64`), there being no
   build for any other; `curl` or `wget` is installed to download with, and `sha256sum` or `shasum` to check with. With
   `--service`, `systemctl` must be installed and the install directory's path must hold no whitespace, percent sign,
   backslash, ampersand, vertical bar, dollar sign, semicolon, quote or backtick, since it goes into the unit as it is.
2. **Reads the manifest** of the chosen channel and takes its version and the entry of this machine's architecture. It
   downloads only from `https` (or `file`) addresses, and refuses any other address in a manifest without downloading
   from it. A manifest that cannot be read, names no version or has no build for this architecture is an error.
3. **Downloads the archive and checks it.** An archive whose SHA-256 does not match the manifest's is refused and nothing
   is installed. When `minisign` is installed, the signature is checked too, against the update key built into the
   script, and an archive whose signature does not verify (or a manifest with no signature) is refused the same way;
   it then says the signature is verified. Without `minisign` only the SHA-256 is checked, and nothing is said about the
   signature.
4. **Installs the binary** as `octoboardd` in the install directory, creating it, by copying it beside the old one and
   renaming it into place in one step: nothing ever sees half a file, and a daemon running from the old file keeps
   running on it until it restarts. It says which version it installed and where.
5. **Says what next**, unless `--service` was given (below): with a daemon of this user running, that `octoboardd
   restart` moves it to the new version and that its sessions keep running; otherwise, to start it with `octoboardd
   start` (on the default server port, `--port N` picking another) and to open the address `octoboardd address` prints
   through an SSH forward, `ssh -L 3369:127.0.0.1:3369 <this server>`.
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
    and the new binary (see "Planned restart and planned stop" in `docs/product/server-form.md`);
  - **a daemon of this user running outside the service**: enables the service without starting it, and says to stop
    that daemon (`octoboardd stop`) and start the service (`systemctl --user start octoboardd`);
  - **otherwise**: starts the service, says it started on its port and to open the address `octoboardd address` prints
    through an SSH forward; a service that does not start is an error pointing at `journalctl --user -u octoboardd`;
- finally, when lingering is not on for the user, says that user services stop at logout and do not start at boot
  until it is, and to turn it on once with `loginctl enable-linger <user>`, which may need `sudo`.

## Upgrading

Upgrading is running the install script again, then `octoboardd restart` (see "Running in the
background from the command line" in `docs/product/server-form.md`). The restart is a planned restart: every session
and shell terminal keeps running and the next daemon, started from the new binary, takes them over, on the same port
for a daemon started with `octoboardd start`, and on the unit's port for one the service runs. The owner key stays the
same, so an address `octoboardd address` printed before still opens the daemon (see "The UI the daemon serves" in
`docs/product/server-form.md`).
