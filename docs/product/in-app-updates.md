# In-app updates

The macOS application updates itself: it looks for a newer version on the release channel the user follows, tells the
user when it finds one, downloads and installs it only when the user asks, and the new version takes effect the next
time Octoboard starts. Nothing is ever installed, and Octoboard never restarts, without the user asking. A browser has
none of this: no checks, no update notice, and no Updates or Update channel rows in Settings (see "About" in
`docs/product/settings.md`).

## Update channels

There are two channels, chosen in Settings' About section (see "The Updates and Update channel rows" below):

- **Beta**, the default, receives every release, stable ones included;
- **Stable** receives only stable releases.

A release is stable when its version has no prerelease suffix: `0.2.0` reaches both channels, `0.2.0-beta.1` only
Beta. Only a version newer than the one running is ever offered, so switching from Beta to Stable while running a beta
newer than the latest stable release offers nothing until a newer stable release is out; it never moves back to an
older version.

The choice is kept **per client**, in the window's own browser storage, as the choice to keep running in the background
is (see "Closing the window" in `docs/product/application-lifecycle.md`): the daemon neither stores nor carries
it, and it is not part of a configuration export (see "The application settings a full export carries" in
`docs/product/configuration-export-and-import.md`). Storage that has been cleared, or holds anything but Stable, reads
as Beta.

**Switching the channel** withdraws at once an update found on the other channel, with its notice, and checks the new
channel straight away. An update already downloading is the exception: it is kept and installs as usual. A check still
running on the old channel when the channel changes says nothing about the new one: its result is dropped and the new
channel is checked in its place. The check a switch starts reports a failure the way an automatic check does, without an
alert (see "When something fails" below).

## When the application checks

- **Automatically**: the first time **30 seconds** after Octoboard starts, so it never competes with the window
  opening, and then every **6 hours** for as long as it runs, whichever screen the window shows.
- **On request**: **Check for updates** in Settings' About section.

A check does nothing while another check or a download is under way, and nothing once an update is installed and waits
for its restart: until Octoboard restarts nothing more is looked for or offered. The update state survives the screen
for a UI that failed and its Try again (see "The window chrome and the content panel" in
`docs/product/window-layout.md`): an update installed or downloading is still known afterwards.

A check ends in one of three ways:

- **Up to date**: the channel offers no version newer than the one running. A channel with no release published yet,
  and a release with no build for this Mac, count as up to date too.
- **A newer version is available**: it is offered in the update notice (see "The update notice" below) and in
  Settings, with its release notes where the release has any.
- **The check failed**: the update server could not be reached, did not answer within **30 seconds**, or answered with
  any error other than the channel having no release (an HTTP `404` for the channel's manifest). A failed check is never
  read as "up to date". A version offered by an earlier check stays offered after a later check fails, and is withdrawn
  by a later check that finds nothing newer.

## The update notice

The update notice is a full-width strip along the window's bottom edge, in the accent colour, directly above the
connection banner when that is shown too (see "Losing the daemon connection" in
`docs/product/application-lifecycle.md`). The content panel and everything that overlays it end above the strip, as they
do above the connection banner (see "What an open dialog, menu or popover covers" in
`docs/product/window-layout.md`). It is shown in the main window, not on the screens shown before there is anything to
show. Its row wraps onto more lines in a narrow window. It says one of three things.

### A newer version is available

The notice names the version, with:

- **What is new**, only when the release has notes: a disclosure that opens the release notes, as plain text keeping
  their line breaks, in a box under the strip's row that scrolls when the notes are longer than it is tall;
- **Download and install**, which starts the download (see "Downloading and installing" below);
- **Later**, an icon button, which hides the notice. The update is still offered in Settings. The notice comes back
  when a later check finds a different version, or when an update is found after the channel was switched; the same
  version found again leaves it hidden.

### Downloading

One row: the sentence naming the version being downloaded and a progress bar, with the percentage at the row's end
when the download's size is known; when it is not, the bar is indeterminate and no percentage is shown. A download has
no buttons and cannot be hidden.

### Installed, waiting for a restart

The notice says the version is installed and takes effect after Octoboard restarts, with **Restart now** (see
"Restarting to apply an update" below) and **Later**, which hides the notice for the rest of this run; Settings still
shows the pending restart. When the application reports an update as already installed that this window never saw
being installed, the notice and Settings say an update is installed without naming its
version.

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
installed and the install fails. A download may take up to **30 minutes** before it fails. The new version replaces the
application in place while Octoboard keeps running; the running application, its daemon and every session carry on
unchanged until Octoboard next starts. The notice and Settings then say it takes effect after a restart.

The restart is suggested, never forced: **Restart now** applies it at once, and quitting Octoboard in any ordinary way
and starting it again later applies it just the same.

An update is never installed twice: Download and install does nothing while a check or a download is under way, or once
an update is installed.

## Restarting to apply an update

**Restart now**, in the notice or in Settings, exists only once an update is installed. It ends Octoboard the way a
confirmed quit does, asking the same confirmation when sessions or busy terminals would be ended, and starts it again
(see "Restarting" in `docs/product/application-lifecycle.md`).

## When something fails

Only a failure of something the user started opens an error alert (see "The error alert" in
`docs/product/error-alerts.md`); each says what failed and what to do, with the updater's own text, where there is any,
behind **Error details**:

- **Check for updates** pressed in Settings that fails: titled "Could not check for updates"; Octoboard keeps running
  the current version and whether a newer one is available is not known; check the internet connection and try again.
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

Settings' About section holds two rows for updates, in the macOS application only (see "About" in
`docs/product/settings.md`).

**Updates** says in a sentence where the application's updates stand, the first that applies of:

1. an update installed, which takes effect after Octoboard restarts (naming its version where it is known);
2. a version being downloaded;
3. a check under way;
4. a newer version available, with the version running;
5. the version running being the latest on the channel;
6. otherwise — before any check, or after a check that did not finish — only the version running.

Beside it one button moves the update on: **Check for updates** until a newer version is found, showing a pending
spinner while a check runs; then **Download and install**, showing the spinner while a check or the download is under
way; then **Restart now** once the update is installed.

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

**Update channel** chooses between **Beta** and **Stable** (see "Update channels" above), with a line saying that Beta
gets every release, stable ones included, and Stable only stable releases.
