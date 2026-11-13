# Server form updates

The server form (the daemon `octoboardd` alone on a Linux server; see `docs/product/server-form.md`) updates itself the
way the desktop application does (see `docs/product/in-app-updates.md`): the daemon looks for a newer release by itself
and the browser says when it finds one, but **nothing is downloaded and nothing is restarted on the daemon's own**. The
new executable is downloaded, checked and put in place only when the user asks — **Download and install** in a browser,
or `octoboardd update` on the server — and runs only after a restart the user asks for — **Restart now** in a browser,
or `octoboardd restart` — which is a planned restart, so every session keeps running. Upgrading with the install script
instead is in "Upgrading" in `docs/product/installing-the-server-form.md`.

## Which daemons update themselves

Only a daemon for which all of these hold:

- it is the **official Linux release build**, the one in a release's `octoboardd_<version>_linux_<arch>.tar.gz`, which
  the install script installs (see "What a release holds" in `docs/product/installing-the-server-form.md`). A daemon
  built any other way never checks and offers nothing;
- it was **not started by the desktop application**, which updates itself and its daemon together;
- it runs on **Linux on x86_64 or aarch64**.

Any other daemon has none of this: a browser on it shows no update notice, no Updates section in Settings and no
version in About, and `octoboardd update` says that this build does not update itself, prints the install command to
upgrade by hand with, and exits with status 1.

## Channels

The daemon follows one of two channels, **Stable** (the default, whatever version the daemon runs) or **Beta**, chosen in
Settings' Updates section (see "Update channel" in `docs/product/update-settings.md`). They are the Linux manifests of the
release, `stable-linux.json` and `beta-linux.json`, written by the same rules as the desktop application's (see "What a
release holds" in `docs/product/installing-the-server-form.md` and "Where updates come from" in
`docs/product/in-app-updates.md`): Stable receives the stable releases, Beta every release, stable ones included. A
release is offered only when its version is newer than the running daemon's, by semantic-version precedence, so
switching from Beta to Stable never moves back to an older version.

The channel is a setting the daemon keeps, `daemon_update_channel`, the same in every browser; agents read and change it
through the settings tools (see "Application settings: `get_settings` and `update_settings`" in
`docs/product/agent-settings-tools.md`). It is not part of a configuration export. Changing it withdraws at once what the
other channel offered, the update notice with it — except an update already downloading, which goes on, or already
installed — and, while the automatic check is on, checks the new channel straight away.

## When the daemon checks

- **Automatically**, while **Check for updates automatically** is on (the default; the daemon setting
  `daemon_update_check`, see "Check for updates automatically" in `docs/product/update-settings.md`): **30 seconds** after
  the daemon starts, then every **6 hours**, counted on the wall clock so that a check missed while the machine slept is
  made within an hour of it waking, and at once when the setting is turned on or the channel changes. With the setting
  off only a check the user asks for is made.
- **On request**: **Check for updates** in Settings' Updates section, from any browser. A request that comes within **30
  seconds** of the end of the last check or download is not made: the last check stands (see "The Version row" in
  `docs/product/update-settings.md` for how the row says so). `octoboardd update --check` checks too, in its own process,
  and leaves the daemon's status as it is (see "The `octoboardd update` command" below).

No check is made while another check or a download is under way — a check the channel or the setting asked for meanwhile
is made once that ends — nor once an update is installed, until the daemon restarts. An installed update is forgotten,
and checks resume, when the executable at the daemon's path is the running build again, as after `octoboardd update
--rollback`.

The daemon reads the channel's manifest over HTTPS, through the proxy the user's login shell environment names and with
the system's certificates, and waits at most **30 seconds** for it. A check ends in one of three ways:

- **Up to date**: the manifest names no newer version. Only an HTTP `404` for the manifest means there is no release.
- **A newer version is available**: offered in every browser open on the daemon (see "What the browser shows" below).
- **The check failed**: any other HTTP status, a network failure, a manifest that does not read, or one with no entry for
  this machine's architecture or whose archive is not at an `https` address. A failed check is never read as up to date,
  and leaves what an earlier check found offered; a later check that finds nothing newer withdraws it.

The status is the daemon's, so every browser shows the same: what was found, what is downloading, and when the last check
ended.

## What the browser shows

The update notice is the desktop application's strip along the bottom of the window (see "The update notice" in
`docs/product/in-app-updates.md`), with these differences:

- **A newer version is available**: as on the desktop, with **What is new**, **Download and install** and **Later**.
- **A newer version the daemon cannot install itself** — the user the daemon runs as may not write to the directory of
  its executable (see "Downloading and replacing the executable" below): the notice names the version, says that
  Octoboard cannot replace its own file and to update by hand with the install command, and offers **What is new**,
  **Copy install command**, which copies `curl -fsSL https://octoboard.dev/install.sh | sh`, and **Later**; there is no
  download.
- **Downloading**: as on the desktop.
- **Installed**: says the version is installed and takes effect after Octoboard restarts, with **Restart now** and
  **Later**; an update the daemon learned of without its version (see below) is named as an update.
- **The daemon now runs another version than the page was loaded for** — after a restart into an update, or any other
  restart into another build: the notice says that Octoboard now runs that version and to reload the page to use it,
  with **Reload**. It outranks every other notice and has no Later. Nothing reloads by itself, except the page that
  asked for the restart (see "Restarting into the update" below).

**Later** hides the notice in that page alone, until it is reloaded; a different version found later brings it back.

**After the daemon has started in place of an older version** — by an update from a browser, `octoboardd update` or the
install script — each browser shows once, per version, a toast saying that Octoboard was updated to that version, with a
**What is new** link to its release page (see `docs/product/toasts.md`). It comes in the first tab of the browser to see
it, at any time while that daemon runs; a browser that keeps no site data shows it once per page load instead. A daemon
knows the version it replaced from `daemon.version` (see `docs/product/files-octoboard-owns.md`), so the first start of
a daemon that updates itself shows nothing.

Settings' Updates section holds the version, the update's controls, the automatic check and the channel (see
`docs/product/update-settings.md`), and About names the daemon's version (see `docs/product/about-octoboard.md`).

## Downloading and replacing the executable

**Download and install**, in the notice or in Settings, starts the download of the update the last check found, in the
daemon; the download goes on whether or not the page stays open, and every browser shows its progress, a bar with the
percentage when the archive's size is known. `octoboardd update` does the same in its own process.

- **What is replaced** is the executable at the path a planned restart starts the next daemon from (see "Who starts the
  next daemon" in `docs/product/planned-restart-and-planned-stop.md`), its links followed to the file itself.
- **Not writable**: when the user the daemon runs as may not write to that file's directory, nothing is downloaded; the
  update is offered for installing by hand instead (see "What the browser shows" above). A check that finds an update
  says so already.
- **Replaced by something else**: when the file at that path is no longer the one the daemon runs — the install script
  put a new one there meanwhile — nothing is downloaded or changed, and the update counts as installed, of a version not
  known.
- **The download**: the release archive for the machine's architecture, from an `https` address alone, at most **256
  MiB**, into `~/.octoboard/updates/`. It fails when no connection is made within **20 seconds**, no answer starts within
  **60 seconds**, no data arrives for **60 seconds**, or the whole takes more than **30 minutes**. While two earlier
  downloads that stopped receiving data are still being given up, a new download or check fails at once.
- **The checks**: the archive is installed only when its SHA-256 is the one the manifest names, its signature is made
  with the release key built into the daemon (the update key of the desktop application), and the file the signature
  names is this release's archive for this architecture, so an older release's archive, signed with the same key, cannot
  stand in for it. Only then is the one executable in it unpacked (at most **256 MiB**).
- **The replacement**: the new executable is written beside the old one, with its permissions; the old one is kept
  beside it as `octoboardd.previous`, replacing any earlier one; then the new file is renamed over the path in one step.
  The path never lacks a file and is never half written: a failure before the last step leaves the executable as it was,
  and a failure of that step too, with `octoboardd.previous` holding the same file.
- **What keeps running**: the daemon, its sessions and its shell terminals run on unchanged, the daemon from the file it
  started as and the process holders from their copy of the running build (see "The copy of the build holders run from"
  in `docs/product/process-holders.md`), until the restart.
- **A stop meanwhile**: a stop of any kind that begins during the download — an ordinary or planned stop, a termination
  signal — ends it, and keeps the executable from being replaced.
- **One at a time**: the daemon and `octoboardd update` never replace the executable together; whichever comes second is
  refused as busy. A daemon learns of an update `octoboardd update` installed under it at its next check or download
  request, and then shows it as installed.

**Download and install** is refused, saying why in an error alert (see "When something fails" below), while a download is
already under way, once an update is installed, when no update has been found, when the executable's directory is not
writable, and while the daemon is stopping or restarting.

## Restarting into the update

**Restart now**, in the notice or in Settings, exists once an update is installed. It goes through the same check and
confirmation as the desktop application's restart (see "Restarting" in `docs/product/application-lifecycle.md`): the page
asks the daemon which sessions and shell terminals the new version would not take over, given the update's holder floor
and version, and asks the user to confirm when the restart would interrupt something. For an update installed without a
known version or floor the daemon cannot tell, and the page asks as it does with no answer.

The daemon then makes a planned restart (see "Planned restart and planned stop" in
`docs/product/planned-restart-and-planned-stop.md`): every session and shell terminal keeps running and the next daemon,
started from the new executable, takes them over. Under a service manager the daemon exits with status 75 and the
manager starts the new file; the unit file is never changed.

The page that pressed **Restart now** reloads itself once the daemon is back on the installed version, or on any other
version when that one is not known, so that it runs the new version's UI. Every other page, and that one when the daemon
comes back on another version than the installed one, shows the reload notice instead (see "What the browser shows"
above).

The restart is refused, with an error alert titled that Octoboard could not restart and saying why, while a download is
under way (a restart would end it) and while the daemon is already stopping or restarting — unlike `octoboardd restart`,
which takes a second request as the first. A page whose connection to the daemon is not open sends nothing and says that
the connection is closed. `octoboardd restart` on the server restarts into the update just the same.

## When something fails

As on the desktop (see "When something fails" in `docs/product/in-app-updates.md`), only the page where the user started
something opens an error alert for it; each holds the daemon's own text behind **Error details**, and none asks for a
report:

- **Check for updates** pressed in that page whose check fails: titled "Could not check for updates", as on the desktop.
- **Download and install** whose download fails: titled "Could not install the update", saying that the update could not
  be downloaded and nothing was changed, and to check the internet connection and the free disk space, then try again.
- One whose archive is not what the release published: the same title, saying that the downloaded update is not what was
  published, so it was discarded, and nothing was changed.
- One whose new executable could not be put in place: the same title, saying that the update could not be put in place
  and the installed daemon was not changed.
- A request the daemon refuses (see the refusals above): the same title, or "Could not check for updates" for a check,
  with the refusal's own sentence. When the daemon no longer holds the update the page offered, the alert is the
  desktop's for that case and the offer is withdrawn.

An automatic check that fails, or one another page asked for, opens no alert: the Version row's warning mark says it in
every page (see "The Version row" in `docs/product/update-settings.md`). A download another page started that fails is
not reported in this one; the update stays offered, so it can be tried again.

## The `octoboardd update` command

```sh
octoboardd update [--channel stable|beta]
octoboardd update --check [--channel stable|beta]
octoboardd update --rollback
```

It works on the installation, not through the running daemon, in its own process, and never restarts anything: a daemon
running from the executable it replaces runs on as the build it started as until `octoboardd restart`.

- **`octoboardd update`** checks the channel and, when a newer version is available, downloads, checks and replaces the
  executable as "Downloading and replacing the executable" above describes, saying when it starts downloading. It then
  says which file it updated, from which version to which, that the previous build is kept as `octoboardd.previous`, and
  to run `octoboardd restart`, which keeps every session running; when the release's holder floor is above the version
  the running daemon runs (or, with none running, this build's), it adds that the sessions started by builds older than
  the floor cannot be handed over and the restart interrupts them. It exits with status 0, also when there is nothing
  newer, which it says. It exits with status 1, saying why on its error output, when the check fails, the update fails,
  the file at the path is no longer the build it runs, or the directory is not writable, in which case it prints the
  install command to upgrade by hand with.
- **`--check`** only checks, printing what it found: exit status **0** when up to date, **10** when a newer version is
  available, **1** when the check failed.
- **`--rollback`** puts `octoboardd.previous` back at the executable's path in one step and keeps what was there as
  `octoboardd.previous`, so a rollback can itself be rolled back, then says to run `octoboardd restart`. A daemon that
  had an update installed forgets it once its path leads to the running build again. A directory that is not writable,
  or a failure, makes it say so and exit with status 1.
- **`--channel stable|beta`** chooses the channel; without it the command takes the daemon's setting, reading the
  daemon's database read-only and never upgrading it, and Stable where there is none. `--check` and `--rollback` cannot
  be given together, `--rollback` takes no `--channel`, any other channel name is a usage error, and the daemon's own
  options (`--port`, `--parent-pid`, `--service-managed`) are not accepted with it.

It refuses, with status 1, while the running daemon follows the desktop application, which updates it itself; on a
daemon that does not update itself (see "Which daemons update themselves" above); and, except with `--check`, while the
daemon or another `octoboardd update` is downloading or replacing the executable, saying to try again once that has
ended.

## Files

The update keeps `update.lock`, `update-installed.json`, `daemon.version` and `updates/` in `~/.octoboard`, and
`octoboardd.previous` beside the executable; what each holds and when it is written and removed is in
`docs/product/files-octoboard-owns.md`.
