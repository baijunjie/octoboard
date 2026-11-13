# In-app updates

The desktop application, on macOS and on Windows, updates itself: it looks for a newer version on the release channel
the user follows, tells the user when it finds one, downloads and installs it only when the user asks, and the new
version takes effect the next time Octoboard starts. Nothing is ever installed, and Octoboard never restarts, without
the user asking. Settings' Updates section, where its update is followed and its checks and channel chosen, is in
`docs/product/update-settings.md`. A browser updates nothing itself: a browser open on a daemon of the server form that
updates itself shows that daemon's update instead, in the same notice and Settings section (see
`docs/product/server-form-updates.md`), and one open on any other daemon shows none.

## Update channels

There are two channels, chosen in Settings' Updates section (see "Update channel" in
`docs/product/update-settings.md`):

- **Stable** receives only stable releases (with one exception until the first stable release, below);
- **Beta** receives every release, stable ones included.

Until the user chooses one, the running build decides: a prerelease build (its version has a prerelease suffix, such as
`0.1.0-beta.3`) follows **Beta**, so a beta left alone keeps receiving betas, and any other build follows **Stable**.
The default follows whichever build runs: a beta build that updates to a stable release with nothing chosen follows
Stable from then on. Until the window has learned the running version (a moment after it opens, before any automatic
check), it follows Stable; a check that started then and settles once the version is known, on a build whose default
is Beta, is dropped and Beta is checked in its place, as with a switch of channel below. Learning the version is no
switch of channel by itself, so it starts no check of its own.

A release is stable when its version has no prerelease suffix: `0.2.0` reaches both channels, `0.2.0-beta.1` only
Beta. Only a version newer than the one running is ever offered, so switching from Beta to Stable while running a beta
newer than the latest stable release offers nothing until a newer stable release is out; it never moves back to an
older version.

One exception holds until the first stable release: the first betas, `0.1.0-beta.1` to `0.1.0-beta.3`, were built
before a prerelease build followed Beta by default, so left alone they follow Stable, which had no release. To reach
them, every prerelease also goes to a platform's Stable channel until that platform has a stable release newer than
the betas Stable announced; that release then replaces them there, and from then on Stable receives only stable
releases (see "Where updates come from" below). Meanwhile Stable offers the newer betas as Beta does, including to a
user who chose Stable on a beta build.

The choice is kept **per client**, in the window's own browser storage, as the appearance is (see "Where the choice
is stored" in `docs/product/appearance.md`): the daemon neither stores nor carries it, and it is not part of a
configuration export (see "Which settings travel" in `docs/product/settings-in-a-configuration.md`). Switching to either channel saves it, and a saved choice is kept
whichever build runs: Stable chosen on a beta build stays Stable. Storage that holds no choice, has been cleared, or
holds anything but Stable or Beta holds no choice, and the build's default applies.

**Switching the channel** withdraws at once an update found on the other channel, with its notice, and, while the
automatic checks are on (see "When the application checks" below), checks the new channel straight away; with them off
it starts no check. An update already downloading is the exception: it is kept and installs as usual. A check
still running on the old channel when the channel changes says nothing about the new one: its result is dropped and the
new channel is checked in its place. The check a switch starts reports a failure the way an automatic check does,
without an alert (see "When something fails" below).

## Where updates come from

Each channel has one static manifest per platform, in the format of Tauri's updater, kept on a release of the public
repository under the fixed tag `updater`, whatever version it announces. The application reads the one of its own
platform:

| Channel | macOS | Windows |
|---|---|---|
| Beta | `https://github.com/baijunjie/octoboard/releases/download/updater/beta.json` | `https://github.com/baijunjie/octoboard/releases/download/updater/beta-windows.json` |
| Stable | `https://github.com/baijunjie/octoboard/releases/download/updater/stable.json` | `https://github.com/baijunjie/octoboard/releases/download/updater/stable-windows.json` |

The macOS addresses are the ones every released macOS build has built in, so `beta.json` and `stable.json` stay the Mac
manifests for good: they hold only the `darwin` entries, and existing users never lose their updates — a macOS version
released before the Windows application existed is offered later releases from them as any other is. Windows, which
has its own pair, is never mixed into them.

A manifest holds the version it announces, its release notes, its publication date, its holder floor and the address
and signature of the update for its platform: the macOS manifests, under both `darwin-aarch64` and `darwin-x86_64`,
those of the same update archive (a release is one universal build serving both Mac architectures); the Windows
manifests, under `windows-x86_64`, those of the NSIS installer, which is the Windows update itself. The **holder
floor**, `holder_floor`, is the oldest version whose process holders the announced version's daemon takes over (see
"Holders of other builds" in `docs/product/process-holders.md`), as the release names it; the running application
reads it with the update and, with the announced version, tells which sessions a restart into the update would
interrupt (see "Restarting" in `docs/product/application-lifecycle.md`). A manifest that names none, or names it as anything but text, gives no holder
floor. The update's address is its file on
the versioned release (below), fetched over HTTPS as the manifest is, and its signature is checked against the update
key built into the application, one key for both platforms, which signs the server form's archives too (see "Downloading and installing" below). Every release is
written to a platform's beta manifest, and a release without a prerelease suffix to its stable manifest too, which is
what puts stable releases on the Beta channel (see "Update channels" above). Until a platform's first stable release, a
prerelease goes to its stable manifest as well: while that manifest is missing or announces a prerelease, it is written
like the beta one, and once it announces a stable version it takes stable versions only again — the one-time exception
for the first betas described in "Update channels". A manifest never moves backwards: a release of an older version
published after a newer one leaves a manifest that already announces the newer version as it is, and each platform's
manifests are judged on their own. The `updater` release is itself marked a prerelease, so it is never
the repository's latest release.

A release has three platforms: macOS, Windows and Linux, the last being the server form, whose manifests,
`beta-linux.json` and `stable-linux.json`, follow the same rules and are read by the server form's daemon rather than by
an application (see "What a release holds" in `docs/product/installing-the-server-form.md` and
`docs/product/server-form-updates.md`). The platforms share one version, and by default a release
publishes all three together. A version can be published for some platforms only (a platform left out of its tag, or
run by hand later for another one): only those platforms' manifests are written, so the other platforms' users are
offered nothing for that version until it is published for them. Each version is one release `desktop-v<version>` —
marked a prerelease when the version has a suffix — which holds, for each platform published, the `.dmg`
(`Octoboard_<version>_universal.dmg`) and the update archive `Octoboard_<version>_universal.app.tar.gz`; the Windows
installer `Octoboard_<version>_x64-setup.exe`; or the server form's archive of each Linux architecture with its
`.sha256`; each update file, and each Linux archive, with its signature beside it as `.sig`. A platform published later
adds its missing files to the release; a published file is never replaced, and a changed build ships as a new version.
Once a release is published, the website is published again, so that its download links point at the new version (see
"Where download links point" in `docs/product/website-download-page.md`).

## When the application checks

- **Automatically**, while **Check for updates automatically** is on, as it is to begin with (see "Check for updates
  automatically" in `docs/product/update-settings.md`): the first time **30 seconds** after Octoboard starts, so it
  never competes with the window opening, and then every **6 hours** for as long as it runs, whichever screen the window
  shows; and at once when the switch is turned on. With the switch off no check is made but on request.
- **On request**: **Check for updates** in Settings' Updates section or the menu bar icon's menu (see "The menu bar
  update entry" below).

A check does nothing while another check or a download is under way, and nothing once an update is installed and waits
for its restart: until Octoboard restarts nothing more is looked for or offered. The update state survives the screen
for a UI that failed and its Try again (see "The window chrome and the content panel" in
`docs/product/window-layout.md`): an update installed or downloading is still known afterwards.

A check ends in one of three ways:

- **Up to date**: the channel offers no version newer than the one running. A channel with no release published yet,
  and a release with no build for this platform, count as up to date too.
- **A newer version is available**: it is offered in the update notice (see "The update notice" below) and in
  Settings, with a link to its release page, and named in the menu bar icon's update entry (see "The menu bar update
  entry" below).
- **The check failed**: the update server could not be reached, did not answer within **30 seconds**, or answered with
  any error other than the channel having no release (an HTTP `404` for the channel's manifest). A failed check is never
  read as "up to date". A version offered by an earlier check stays offered after a later check fails, and is withdrawn
  by a later check that finds nothing newer.

## The menu bar update entry

The menu bar icon's menu always includes an update entry immediately above the entry for opening the board
(see "The icon's menu" in `docs/product/menu-bar-icon.md`). It normally offers to check for updates; while a newer
version is known it names that version as available, and while it is downloading it identifies the version being
downloaded. Hiding the update notice does not hide this entry or the available version it names. After installation the entry
uses a neutral update label; Settings still shows the pending restart, and no further check runs until Octoboard
restarts.

Choosing the entry brings the main window to the front, restoring it from the background or from being minimized,
and opens Settings on its Updates section (see `docs/product/update-settings.md`). Settings already open on another
section moves to Updates. Before the UI is ready, or while another modal or
an action menu is open, only the window is brought back; opening Settings and checking are both skipped rather than
deferred.

When no newer version is known, choosing the entry also starts a manual check, with the same busy and installed-state
limits as any other check (see "When the application checks" above). When a version is already offered, it only opens
Settings. The entry itself starts no download, installation or restart.

## The update notice

The update notice is a full-width strip along the window's bottom edge, in the accent colour, directly above the
connection banner when that is shown too (see "Losing the daemon connection" in
`docs/product/application-lifecycle.md`). The content panel and everything that overlays it end above the strip, as they
do above the connection banner (see "What an open dialog, menu or popover covers" in
`docs/product/window-layout.md`). It is shown in the main window, not on the screens shown before there is anything to
show. Its row wraps onto more lines in a narrow window. It says one of three things.

### A newer version is available

The notice names the version, with:

- **What is new**, a link drawn as a button with an external-link icon, which opens the offered version's GitHub
  release page in the browser, whether or not the update includes release notes (see "Where updates come from" above);
- **Download and install**, which starts the download (see "Downloading and installing" below);
- **Later**, an icon button, which hides the notice. The update is still offered in Settings. The notice comes back
  when a later check finds a different version, or when an update is found after the channel was switched; the same
  version found again leaves it hidden.

### Downloading

One row: the sentence naming the version being downloaded and a progress bar, with the percentage at the row's end
when the download's size is known; when it is not, the bar is indeterminate and no percentage is shown. A download has
no buttons and cannot be hidden.

### Installed, waiting for a restart

On macOS the notice says the version is installed and takes effect after Octoboard restarts; on Windows, where the
installer has not run yet, it says the version is downloaded and is installed when Octoboard quits or restarts, never
that it is installed. Either way it has **Restart now** (see "Restarting to apply an update" below) and **Later**, which
hides the notice for the rest of this run; Settings still shows the pending restart. When the application reports an
update as already installed that this window never saw being installed, the notice and Settings say the same without
naming its version.

### Keyboard focus and announcements

The notice is a stop of the `F6` cycle while it is shown (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`). Pressing its buttons with the mouse leaves keyboard focus where it was.
When a button holding keyboard focus goes away because the notice changes or disappears, focus goes to an open modal
dialog, otherwise to the terminal, as when the connection banner's Retry goes away.

While a dialog is open the notice stays on screen under the dialog's dimmed overlay, like the rest of the window, and
its buttons cannot be pressed or reached with `F6`: unlike the connection banner it is not drawn above the overlay (see
"What an open dialog, menu or popover covers" in `docs/product/window-layout.md`).

What the notice says is announced to assistive technology. Because a check or a finished install can land while the
user is in another application, an announcement waits until the window is in use again, and a sentence replaced before
it was announced is never announced.

## Downloading and installing

**Download and install**, in the notice or in Settings, downloads the update the last check found and installs it. The
download is checked against the release's signature before anything is installed; one that does not verify is not
installed and the install fails. A download may take up to **30 minutes** before it fails. On macOS the new version
replaces the application in place while Octoboard keeps running. On Windows the downloaded installer is kept and runs
when Octoboard quits: the installer cannot replace the running application, and starting it ends Octoboard at once.
Either way the running application, its daemon and every session carry on unchanged until Octoboard next starts. From
then on the update counts as installed on both platforms, though on Windows its installer has not run yet: what this
document says of an installed update (the notice, Settings, **Restart now**, no further checks) applies to it, but
the notice and Settings say it is downloaded, not installed (see "Installed, waiting for a restart" above).

On macOS, until the restart a daemon that ends unexpectedly is not started again, since the application's files are
already the new version's; the window says so and asks for the restart (see "Starting up" in
`docs/product/application-lifecycle.md`). On Windows the files stay the running version's until the installer runs, so
a daemon that ends unexpectedly is started again under the same rules as when no update is waiting.

The restart is suggested, never forced: **Restart now** applies it at once, and quitting Octoboard in any ordinary way
and starting it again later applies it just the same, except that only **Restart now** keeps the sessions running (see
"Restarting to apply an update" below): a quit interrupts them as any quit does. On Windows every quit of Octoboard while it holds a downloaded
update installs it on the way out, without starting Octoboard again: a quit it confirms itself (its Quit, or closing the
window when it does not run in the background), and equally a quit that skips the confirmation — one made while the
daemon is not running, the second quit gesture within 2 seconds that forces a quit through (see "Quitting when the
window has stopped responding" in `docs/product/application-lifecycle.md`), or one made before the window has loaded far
enough to ask. Each stops the daemon first, then runs the installer in its passive mode, which shows its progress; a quit
made while the installer is being started waits for it. If the installer cannot be started, Octoboard just ends and the
update stays unapplied, offered again by a later check. The system ending the user's session — signing out or shutting
down Windows — is not a quit and installs nothing: Octoboard stops its daemon as the session ends (see "When the system
ends the session" in `docs/product/application-lifecycle.md`) and ends, and the update is offered again by the
automatic check after the next start (see "When the application checks" above).

On Windows the installer replaces every file it ships without asking, even while a process still runs from the
installation folder. Octoboard itself is ended first, so it never runs on past its update. Any other shipped file a
process still holds — the daemon's executable, `conpty.dll`, an `OpenConsole.exe` or a bundled license file, held by a
process an agent started from the installation, say, or by a process holder running from it (see "The copy of the build
holders run from" in `docs/product/process-holders.md`) — is renamed beside itself to `<file>.old-<n>`, `n` the first
number not taken, and the new file takes the original name. No such process is ended: it runs on from the renamed file.
A file that cannot be renamed either is left to the installer's own prompt. An install that fails or is aborted puts
back each file it renamed where no new file has taken its place, so no shipped file is left missing. A renamed file is
deleted as soon as nothing holds it: right after the install; otherwise at the daemon's next start (the daemon's
executable, `conpty.dll` and the `OpenConsole.exe` files only), at the next install, or at uninstall. An update never
asks for a reboot.

An update is never installed twice: Download and install does nothing while a check or a download is under way, or once
an update is installed.

## Restarting to apply an update

**Restart now**, in the notice or in Settings, exists only once an update is installed. It restarts Octoboard into the
new version and hands every session and shell terminal over to it, running, instead of ending them: it asks for
confirmation whenever the new version would not take over a running session, idle ones included, a busy shell
terminal or a shell terminal that cannot be reached — one whose process holder is older than the update's holder
floor, say — or when that cannot be checked and something running may be interrupted, and names the sessions it would
or may interrupt (see "Restarting" in `docs/product/application-lifecycle.md`). From the press until the confirmation is open
or the restart is under way, while the window asks the daemon what the restart would not carry over, the button shows
a pending spinner and a second press does nothing. On Windows the daemon is stopped first, with its planned stop, and
the passive installer then starts the new version itself.

## When something fails

Only a failure of something the user started opens an error alert (see "The error alert" in
`docs/product/error-alerts.md`); each says what failed and what to do, with the updater's own text, where there is any,
behind **Error details**:

- **Check for updates** pressed in Settings or the menu bar icon's menu that fails: titled "Could not check for
  updates"; Octoboard keeps running the current version and whether a newer one is available is not known; check
  the internet connection and try again.
- **Download and install** that fails: titled "Could not install the update"; the update could not be downloaded and
  installed; check the internet connection and the free disk space, then try again. The update stays offered, so it
  can be tried again.
- **Download and install** when the application no longer holds the update the window offers: titled "Could not
  install the update"; the update is no longer available to install; check for updates to look for it again. The offer
  and its notice are withdrawn, and the alert has no Error details.

None of these asks for a report or offers **Report issue**.

An automatic check that fails, including the one a channel switch starts, opens no alert: the Version row of Settings'
Updates section shows a warning mark saying that the last check did not finish (see "The Version row" in
`docs/product/update-settings.md`), and the next check tries again. A check or install refused because the application
is already busy with one is not a failure and says nothing.
