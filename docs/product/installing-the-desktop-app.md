# Installing the desktop app

What the desktop application runs on, how it is distributed and how it is installed, on macOS and on Windows. Both are
in beta. Once installed, the application updates itself from the project's releases, on the user's request: on macOS in
place, on Windows by running the new version's installer (see `docs/product/in-app-updates.md`). How the application
starts and quits is in `docs/product/application-lifecycle.md`, the files it keeps are in
`docs/product/files-octoboard-owns.md`, and uninstalling is under "Uninstalling" below.

## Where releases are published

Every version is published as a release of the public GitHub repository, `desktop-v<version>`, holding the macOS disk
image and the Windows installer, beside the server form's Linux archives; which files a release holds, and how a version
can reach one platform before another, is in "Where updates come from" in `docs/product/in-app-updates.md`. Octoboard is
installed from there; it is not offered through an application store. The website's download page links each
platform's newest file (see `docs/product/website-download-page.md`).

## macOS

- **macOS 11 or later**, on Apple silicon or Intel — the release build is one universal bundle holding both
  architectures, the bundled daemon included.
- On macOS, Octoboard is distributed as a `.dmg` disk image. It cannot be distributed through the Mac App Store: its translucent
  window (see "The window chrome and the content panel" in `docs/product/window-layout.md`) relies on a private macOS
  interface, which the store does not admit. The release build, the bundled daemon included, is signed with a Developer
  ID and notarized, so Gatekeeper admits the `.app` and the `.dmg` as built, a copy carrying the quarantine attribute
  included; that has been confirmed only on the machine that built it, not yet by installing on a clean one.

## Windows

- **The installer**: `Octoboard_<version>_x64-setup.exe` for x64 PCs, or `Octoboard_<version>_arm64-setup.exe` for
  Windows 11 on Arm, an NSIS installer, which installs Octoboard for the current user only. Later versions arrive through the application's own updates, each of which runs the new version's
  installer.
- **The installer's language** is the system's display language where it is one of the application's languages, with
  Portuguese the Brazilian one; otherwise one with the same primary language (Portuguese from Portugal takes the
  Brazilian, and Chinese from Hong Kong, Macao or Singapore the Simplified); and English for a system language that
  matches none of them. No language is asked for. Arabic is laid out right to left. **The uninstaller** speaks the
  language the installation was made in, which the installer saves, and so does the list of what the clean-up before
  uninstalling left (see "The Windows uninstaller's clean-up" in `docs/product/clean-up-before-uninstalling.md`).
- **Windows 10 version 21H2 or later, on x64; Windows 11 on Arm with the arm64 build.** The application, its daemon and
  its console host are built for each architecture. An x64 installation on an Arm PC runs under the system's emulation
  until an in-app update installs the arm64 build over it: the next update for a version that knows the arm64 build,
  the one after the next for an older version, whose next update is the new x64 build (see "Where updates come from" in
  `docs/product/in-app-updates.md`). Everything these
  docs describe of the Windows application holds alike on Windows 10 21H2 and on Windows 11, and on x64 hardware as on
  Arm: the installer, the notification-area icon (see "The notification-area icon on Windows" in
  `docs/product/menu-bar-icon.md`), the Notifications row of Settings and its **Open Windows Settings** button (see
  "Opening the system's notification settings" in `docs/product/notification-settings.md`), the WebView2 runtime's installation below, and in-app updates.
- **WebView2**: the window is drawn with Microsoft Edge WebView2. Where the system lacks that runtime, the installer
  downloads Microsoft's WebView2 bootstrapper and installs the runtime, which needs an internet connection at that
  point; where the runtime is present, nothing is downloaded.
- **The agents**: Octoboard installs no agent. The user installs the CLIs of the agents they want to use, as on macOS;
  on Windows, Claude Code and Codex also need Git for Windows. Octoboard can update an installed CLI when the user asks
  (see `docs/product/agent-cli-updates.md`).
- **Sharing the default configuration** with additional accounts needs Windows' Developer Mode turned on; without it,
  sharing is unavailable and each additional account keeps a configuration of its own (see "The sharing choice" in
  `docs/product/sharing-the-default-configuration.md`).
- **The installer is not code-signed.** Microsoft Defender SmartScreen may therefore stop a downloaded installer with
  "Windows protected your PC". **More info**, then **Run anyway**, runs it. Nothing about Octoboard asks for
  SmartScreen to be turned off.

Status: not implemented — signed Windows releases. Once releases are code-signed, SmartScreen can still warn about a
signed installer until its signer has built up a reputation with Microsoft, so the warning and its **Run anyway** may
remain for a while after signing begins.

## Uninstalling

- **macOS**: deleting `Octoboard.app` uninstalls it. Nothing runs when it is deleted.
- **Windows**: the uninstaller is started from Windows' Settings (Apps) or Control Panel, or run directly, interactively
  or silently with `/S` (as a package manager such as winget runs it). Before anything is deleted, it ends the running
  application and has the application run the clean-up before uninstalling without a window, then goes on whatever the
  clean-up came to, and at the end of an interactive uninstall tells what the clean-up could not undo. With the x64
  build on Windows on Arm it cannot end a running Octoboard, whose executable Windows' x64 translation service holds,
  and the uninstall stops with nothing removed, as does a setup run by hand over the running installation: quit
  Octoboard first. The arm64 build has no such limit (see "What it does" in
  `docs/product/clean-up-before-uninstalling.md`). What it clears, its time limits and how its result is told are in "The Windows uninstaller's clean-up" in
  `docs/product/clean-up-before-uninstalling.md`. It then removes the installed application and, before that, what the
  application's features put outside its own folders: both registry values of the Launch at login entry (`Octoboard`
  under the current user's `Run` key, which Tauri's own uninstaller removes, and under `Explorer\StartupApproved\Run`,
  which Octoboard's uninstaller hooks add; see "Where the entry lives" in `docs/product/launch-at-login.md`), and the
  taskbar's jump list. Where the clean-up ran, it has removed both already;
  the removals do not conflict. For the jump list, Tauri's uninstaller removes the list kept under the application's
  identifier, and where the clean-up did not run to its end the hooks start the application's executable with
  `--delete-jump-list dev.octoboard.desktop`, an argument that removes the executable's own list and the one under that
  identifier and does nothing else. The
  application's config directory and local data directory, under `%APPDATA%` and `%LOCALAPPDATA%`, go only when the
  uninstaller's "Delete the application data" box is ticked; the daemon's data directory,
  `%USERPROFILE%\.octoboard`, is never removed. Turning Launch at login off removes the login entry too; the jump list
  has no switch, so besides the uninstaller only the clean-up before uninstalling removes it.
- **Windows, updating**: an upgrade made by Octoboard's own update or by a setup never runs the clean-up, so this
  computer's keys on remote servers and the accounts' sharing of their agent's default configuration outlive it; one
  made by a tool that runs the installed uninstaller plainly first, and then the new setup, does run it (see "When it
  runs" in `docs/product/clean-up-before-uninstalling.md`). The in-app update starts the new setup in update mode,
  which runs no uninstall, so the login entry and the jump list stay too. A newer setup the user runs interactively
  offers "Uninstall before installing", chosen by default; that runs the installed version's uninstaller in place,
  which does not run the clean-up but removes the login entry and the jump list as above, so Launch at login is off
  after the upgrade, and its switch shows it off, until the user turns it on again. An older setup run interactively
  offers the same and does the same; a setup of the installed version offers "Uninstall Octoboard" as its second
  choice, which also runs the uninstaller in place, with no clean-up. A newer or an older setup run in passive mode
  (`/P`) or silently (`/S`) runs no uninstall at all, so the login entry and the jump list stay as they were.
- **Windows, an earlier version installed**: the uninstaller that runs is always the one the installed version came
  with, a newer setup's "Uninstall before installing" included, and the uninstaller of a version from before it ran the
  clean-up runs none. Uninstalling such a version, the row **Clean up before uninstalling** in Settings is the way to
  clear what its features have in effect (see `docs/product/clean-up-before-uninstalling.md`).

What either leaves, in Octoboard's own directories and elsewhere, and how to remove it by hand, is in
`docs/product/what-remains-after-uninstalling.md`.

## The server form

The server form — the daemon `octoboardd` alone, on a Linux server, with the UI inside its binary — is published in the
same releases as the desktop application, for Linux on x86_64 and aarch64, and installed with a script rather than an
installer; what it runs on and how it is installed and upgraded is in `docs/product/installing-the-server-form.md`.
