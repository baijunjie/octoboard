# In-app updates

The desktop application, on macOS and on Windows, updates itself: it looks for a newer version on the release channel the user follows, tells the
user when it finds one, downloads and installs it only when the user asks, and the new version takes effect the next
time Octoboard starts. Nothing is ever installed, and Octoboard never restarts, without the user asking. A browser has
none of this: no checks, no update notice, and no Updates or Update channel rows in Settings (see "About" in
`docs/product/settings.md`).

## Update channels

There are two channels, chosen in Settings' About section (see "The Updates and Update channel rows" below):

- **Stable**, the default, receives only stable releases;
- **Beta** receives every release, stable ones included.

A release is stable when its version has no prerelease suffix: `0.2.0` reaches both channels, `0.2.0-beta.1` only
Beta. Only a version newer than the one running is ever offered, so switching from Beta to Stable while running a beta
newer than the latest stable release offers nothing until a newer stable release is out; it never moves back to an
older version.

The choice is kept **per client**, in the window's own browser storage, as the choice to keep running in the background
is (see "Closing the window" in `docs/product/application-lifecycle.md`): the daemon neither stores nor carries
it, and it is not part of a configuration export (see "Which settings travel" in
`docs/product/settings-in-a-configuration.md`). A saved choice of either channel is kept; storage that holds no
choice, has been cleared, or holds anything but Beta reads as Stable.

**Switching the channel** withdraws at once an update found on the other channel, with its notice, and checks the new
channel straight away. An update already downloading is the exception: it is kept and installs as usual. A check still
running on the old channel when the channel changes says nothing about the new one: its result is dropped and the new
channel is checked in its place. The check a switch starts reports a failure the way an automatic check does, without an
alert (see "When something fails" below).

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

A manifest holds the version it announces, its release notes, its publication date and the address and signature of
the update for its platform: the macOS manifests, under both `darwin-aarch64` and `darwin-x86_64`, those of the same
update archive (a release is one universal build serving both Mac architectures); the Windows manifests, under
`windows-x86_64`, those of the NSIS installer, which is the Windows update itself. The update's address is its file on
the versioned release (below), fetched over HTTPS as the manifest is, and its signature is checked against the update
key built into the application, one key for both platforms (see "Downloading and installing" below). Every release is written to a
platform's beta manifest, and a release without a prerelease suffix to its stable manifest too, which is what puts
stable releases on the Beta channel (see "Update channels" above). A manifest never moves backwards: a release of an
older version published after a newer one leaves a manifest that already announces the newer version as it is, and
each platform's manifests are judged on their own. The `updater` release is itself marked a prerelease, so it is never
the repository's latest release.

The platforms share one version, and by default a release publishes both together. A version can be published for one
platform only (a platform left out of its tag, or run by hand later for the other one): only that platform's manifests
are written, so the other platform's users are offered nothing for that version until it is published for them. Each
version is one release `desktop-v<version>` — marked a prerelease when the version has a suffix — which holds, for
each platform published, the `.dmg` and the update archive `Octoboard_<version>_universal.app.tar.gz`, or the Windows
installer `Octoboard_<version>_x64-setup.exe`, each update file with its signature beside it as `.sig`. A platform
published later adds its missing files to the release; a published file is never replaced, and a changed build ships
as a new version.

## When the application checks

- **Automatically**: the first time **30 seconds** after Octoboard starts, so it never competes with the window
  opening, and then every **6 hours** for as long as it runs, whichever screen the window shows.
- **On request**: **Check for updates** in Settings' About section or the menu bar icon's menu (see "The menu bar
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

The menu bar icon's menu always includes an update entry immediately above Open Octoboard
(see "The icon's menu" in `docs/product/menu-bar-icon.md`). It normally offers to check for updates; while a newer
version is known it names that version as available, and while it is downloading it identifies the version being
downloaded. Hiding the update notice does not hide this entry or the available version it names. After installation the entry
uses a neutral update label; Settings still shows the pending restart, and no further check runs until Octoboard
restarts.

Choosing the entry brings the main window to the front, restoring it from the background or from being minimized,
and opens Settings on its About section, where the update controls are (see "The Updates and Update channel rows"
below). Settings already open on another section moves to About. Before the UI is ready, or while another modal or
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
and starting it again later applies it just the same. On Windows every quit of Octoboard while it holds a downloaded
update installs it on the way out, without starting Octoboard again: a quit it confirms itself (its Quit, or closing the
window when it does not run in the background), and equally a quit that skips the confirmation — one made while the
daemon is not running, the second quit gesture within 2 seconds that forces a quit through (see "Quitting when the
window has stopped responding" in `docs/product/application-lifecycle.md`), or one made before the window has loaded far
enough to ask. Each stops the daemon first, then runs the installer in its passive mode, which shows its progress; a quit
made while the installer is being started waits for it. If the installer cannot be started, Octoboard just ends and the
update stays unapplied, offered again by a later check. The system ending the user's session — signing out or shutting
down Windows — is not a quit and installs nothing: Octoboard just ends, and the update is offered again by the
automatic check after the next start (see "When the application checks" above).

An update is never installed twice: Download and install does nothing while a check or a download is under way, or once
an update is installed.

## Restarting to apply an update

**Restart now**, in the notice or in Settings, exists only once an update is installed. It ends Octoboard the way a
confirmed quit does, asking the same confirmation when sessions or busy terminals would be ended, and starts it again
(see "Restarting" in `docs/product/application-lifecycle.md`). On Windows the daemon is stopped first and the passive
installer then starts the new version itself.

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

An automatic check that fails, including the one a channel switch starts, opens no alert: Settings' Updates row shows a
warning mark saying that the last check did not finish (see "The Updates and Update channel rows" below), and the next
check tries again. A check or install refused because the application is already busy with one is not a failure and says
nothing.

## The Updates and Update channel rows

Settings' About section holds two rows for updates, in the desktop application only (see "About" in
`docs/product/settings.md`).

**Updates** says in a sentence where the application's updates stand, the first that applies of:

1. an update installed, which takes effect after Octoboard restarts — on Windows, an update downloaded, which is
   installed when Octoboard quits or restarts (see "Installed, waiting for a restart" above) — naming its version where
   it is known;
2. a version being downloaded;
3. a check under way;
4. a newer version available, with the version running;
5. the version running being the latest on the channel;
6. otherwise — before any check, or after a check that did not finish — only the version running.

Beside it one button moves the update on: **Check for updates** until a newer version is found, showing a pending
spinner while a check runs; then **Download and install**, showing the spinner while a check or the download is under
way; then **Restart now** once the update is installed.

Before that button, a **What is new** link, drawn as a button, opens in the browser the GitHub release page of the
version the row's sentence is about, `https://github.com/baijunjie/octoboard/releases/tag/desktop-v<version>` (see
"Where updates come from" above): the newer version while one is available or downloading, the installed version once
an update is installed, and otherwise the version running. There is no link while the version running is not yet
known, nor for an installed update whose version is not known.
When the link goes away while it holds keyboard focus — the row turning to an installed update of unknown version —
focus moves to the row's button.

The row's controls are, in order: What is new, the warning mark described below when it is shown, and the row's
button.

**After a check that did not finish**, pressed here or automatic, while no newer version is known and nothing is under
way, an amber warning mark stands in the row before **Check for updates**, named for the last update check; the row's
sentence stays the version running. Its popover says that the last check for updates did not finish, that it is not
known whether a newer version exists, and that pressing **Check for updates** tries again; it has no button of its own,
the row's button being the retry, and it opens as a row's mark does (see "Failures, warnings and notes marked at their
point" in `docs/product/error-alerts.md`). The mark goes once a check starts; keyboard focus on the mark or in its
popover then goes to **Check for updates**.

While Settings is open, once the state of the update changes — through a press here, a check that was already running
when Settings opened ending, a download started from the notice finishing, or an automatic check — every change of the
sentence is announced, except the sentence of a check under way, which the button's pending state already says, and
the sentence after a check that did not finish. That the last check for updates did not finish is announced instead,
except when the check that did not finish was the one **Check for updates** started here, whose failure its error
alert says. A failed automatic check is announced even after an earlier check was pressed. What the row said
when Settings opened is not announced.

**Update channel** chooses between **Stable** and **Beta**, listed in that order (see "Update channels" above), with a
line saying that Stable gets only stable releases and Beta every release, stable ones included.
