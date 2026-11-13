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
upgrade by hand with, `curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh`, and exits with
status 1.

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
the system's certificates, and waits at most **30 seconds** for it. A check under way never holds up a stop or a planned
restart: the daemon does not wait for it to end. A check ends in one of three ways:

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
  its executable (see "Downloading and replacing the executable" below, which gives the install command): the notice
  names the version, says that Octoboard cannot replace its own file, so the owner of that file's folder has to run the
  install command on the machine Octoboard runs on, and offers **What is new**, **Copy install command**, which copies
  the install command the daemon gives for that directory, and **Later**; there is no download.
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
  update is offered for installing by hand instead (see "What the browser shows" above), by someone who may write to
  that directory — its owner — on the daemon's machine. A check that finds an update says so already. The daemon gives
  the command to run, the install script told that directory and the channel the daemon follows:

  ```sh
  curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh -s -- --bin-dir <directory> [--beta]
  ```

  `<directory>` is the directory of the executable, its links followed, written as it is when it holds only letters,
  digits and `/ . _ - + : @ % , =`, and otherwise in single quotes, a single quote in it written as `'\''`; `--beta` is
  there only on the Beta channel. `octoboardd update` gives the same command (see "The `octoboardd update` command"
  below).
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
- **Trying the new build**: the new executable is written beside the old one, with its permissions, and run there as
  `octoboardd --version` (see "Printing the version" in `docs/product/daemon-command-line.md`). It has to answer
  within **15 seconds** — past that its whole process group is killed — with exactly the release's version. Otherwise
  the path is not touched, the written file is removed, nothing is recorded, and the update fails as one whose new
  executable could not be put in place, its detail saying that the new build did not start, so it was not installed,
  and why (it could not be run, did not answer in time, failed, or named another version). A build that runs
  `--version` may still fail as a daemon; see "When the update does not start" below.
- **The replacement**: the old executable is kept beside the path as `octoboardd.previous`, replacing any earlier one; then
  the new file is renamed over the path in one step. The path never lacks a file and is never half written: a failure
  before the last step leaves the executable as it was, and a failure of that step too, with `octoboardd.previous`
  holding the same file.
- **What keeps running**: the daemon, its sessions and its shell terminals run on unchanged until the restart: the
  daemon from the file it started as, the process holders and the helpers of the launches they run from their copy of
  the running build (see "The copy of the build holders run from" in `docs/product/process-holders.md`), and the helpers
  of a launch whose terminal runs in the daemon itself from a copy of the running build too (see "When holders cannot
  be set up" in `docs/product/process-holders.md`). A project session's git hooks are the exception: they run the
  installation's own executable, so from the replacement on they run the new build.
- **A stop meanwhile**: a stop of any kind that begins during the download — an ordinary or planned stop, a termination
  signal — ends it; one that begins before the new file is renamed over the path, while the new build is being tried
  included, keeps the executable from being replaced.
- **One at a time**: the daemon and `octoboardd update` never replace the executable together; whichever comes second is
  refused as busy. The install script takes the same lock where it can, and is refused the same way (see step 4 of "The
  install script" in `docs/product/installing-the-server-form.md`). A daemon learns of an update `octoboardd update` installed under it at its next check or download
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

## When the update does not start

A new build that answered `--version` (see "Downloading and replacing the executable" above) can still fail to come up
as a daemon.

**Without a service manager**, the daemon of the planned restart falls back to its own build. When the next daemon does
not come up (see "Who starts the next daemon" in `docs/product/planned-restart-and-planned-stop.md`), the file at the
launch path is another build than the one the daemon runs (put there by an update, or by a rollback made under it),
and the one it runs is the one kept beside it as `octoboardd.previous`, the daemon swaps the two back as `octoboardd
update --rollback` does — the build that did not start becomes `octoboardd.previous`, and `update-installed.json` is
removed — logs a warning saying so, and starts the next daemon again, now of the build that ran. The restart then ends with a daemon of the version that ran before, every
session taken over; nothing records the update as installed any more, so checks resume and offer it again. It does not
fall back, and exits as any restart whose next daemon does not come up, with the sessions kept for the next start:

- when the database is at a schema version above the one the running build reads (the build that did not start may
  have migrated it, and the running build would not start on it either; see "Rolling back across a database migration"
  below);
- while another process holds `update.lock` (an `octoboardd update` under way);
- when the file at the path is the running build, or the running build is not the one kept as `octoboardd.previous`;
- when the swap fails, or the build that ran does not come up either.

**Under a service manager** nothing falls back: the daemon exits with status 75 and the unit starts whatever file is
at its path. The example unit restarts the daemon after each failure (`Restart=on-failure`), so a new build that keeps
failing as it starts is started again until systemd's start limit stops it and leaves the service failed; the unit sets
no limit of its own.

**Going back by hand.** `octoboardd update --rollback` runs the build at the path, the one that does not start, so it
may not work either. Put the previous build back with a rename in the executable's directory instead, for an
installation in the default place

```sh
mv ~/.local/bin/octoboardd.previous ~/.local/bin/octoboardd
```

then start the daemon again (`octoboardd start`, or the service). This leaves no `octoboardd.previous`, and takes the
database as it is: where the update migrated it, the previous build does not start on it either, as below.

## Rolling back across a database migration

A build reads the database up to its own schema version and refuses to start on a newer one (see "The database's
schema version" in `docs/product/files-octoboard-owns.md`). When a newer build migrated the database, it kept a copy
from before the migration beside it, `octoboard.db.pre-v<N>`, `<N>` being the version migrated from. Going back to an
older build across that migration means putting that copy back by hand, which loses everything changed since the
update; nothing restores it automatically. `octoboardd update --rollback` refuses the rollback in that case and names
the copy and the steps (see "The `octoboardd update` command" below).

## When something fails

As on the desktop (see "When something fails" in `docs/product/in-app-updates.md`), only the page where the user started
something opens an error alert for it; each holds the daemon's own text behind **Error details**, and none asks for a
report:

- **Check for updates** pressed in that page whose check fails: titled "Could not check for updates", as on the desktop.
- **Download and install** whose download fails: titled "Could not install the update", saying that the update could not
  be downloaded and nothing was changed, and to check the internet connection and the free disk space, then try again.
- One whose archive is not what the release published: the same title, saying that the downloaded update is not what was
  published, so it was discarded, and nothing was changed.
- One whose new executable could not be put in place — a new build that did not answer `--version` with the release's
  version included: the same title, saying that the update could not be put in place and the installed daemon was not
  changed.
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
  newer, which it says. It exits with status 1, saying why on its error output, when the check fails, the update fails
  (a new build that did not answer `--version` with the release's version included), the file at the path is no longer
  the build it runs, or the directory is not writable, in which case it says to have the owner of that directory upgrade
  it with the install command for that directory and the channel (see "Downloading and replacing the executable"
  above).
- **`--check`** only checks, printing what it found: exit status **0** when up to date, **10** when a newer version is
  available, **1** when the check failed.
- **`--rollback`** puts `octoboardd.previous` (kept by the last update, rollback or run of the install script) back at
  the executable's path in one step and keeps what was there as `octoboardd.previous`, so a rollback can itself be
  rolled back, removes `update-installed.json`, says which version it rolled back to and that the build it replaced is
  kept as `octoboardd.previous`, then says to run `octoboardd restart`.
  The version is the one `update-installed.json` records as replaced, when it records the replacement of this file,
  else the one `octoboardd.previous --version` answers within **10 seconds**; when neither tells, it says it rolled back
  to the previous build, whose version is not known. A daemon that had an update installed forgets it once its path
  leads to the running build again. A directory that is not writable, or a failure, makes it say so and exit with status
  1.

  Before it changes anything it checks that the previous build reads the database, `~/.octoboard/octoboard.db`, by the
  schema version `update-installed.json` records for the build the update replaced. When the database is at a newer
  version (see "Rolling back across a database migration" above), it refuses, changing nothing, and exits with status
  1, saying the database's version and the one the previous build reads. Where a copy from before the migration,
  `octoboard.db.pre-v<the version the previous build reads>` (or one of its timestamped siblings), is beside the
  database, it names the newest and the steps to go back to it, which lose what changed since the update: `octoboardd stop`, move the database and
  any `-wal` and `-shm` file beside it aside, copy that copy into the database's place, run `octoboardd update
  --rollback` again, then `octoboardd start`. Where there is none, it says no such copy was found. When it cannot tell —
  `update-installed.json` records no replacement of this file, or one made by a build that did not record the schema
  version, or the database cannot be read — it rolls back anyway, with a note that if the build does not start after
  the restart, an update replaced it since and migrated the database, pointing at the `.pre-v<N>` copies. With no
  database yet there is nothing to check.
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
