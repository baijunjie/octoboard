# Installing the desktop app

What the desktop application runs on, how it is distributed and how it is installed, on macOS and on Windows. Both are
in beta. Once installed, the application updates itself from the project's releases, on the user's request: on macOS in
place, on Windows by running the new version's installer (see `docs/product/in-app-updates.md`). How the application
starts, quits and keeps its files is in `docs/product/application-lifecycle.md`.

## Where releases are published

Every version is published as a release of the public GitHub repository, `desktop-v<version>`, holding the macOS disk
image and the Windows installer; which files a release holds, and how a version can reach one platform before the
other, is in "Where updates come from" in `docs/product/in-app-updates.md`. Octoboard is installed from there; it is not
offered through an application store.

## macOS

- **macOS 11 or later**, on Apple silicon or Intel — the release build is one universal bundle holding both
  architectures, the bundled daemon included.
- On macOS, Octoboard is distributed as a `.dmg` disk image. It cannot be distributed through the Mac App Store: its translucent
  window (see "The window chrome and the content panel" in `docs/product/window-layout.md`) relies on a private macOS
  interface, which the store does not admit. The release build, the bundled daemon included, is signed with a Developer
  ID and notarized, so Gatekeeper admits the `.app` and the `.dmg` as built, a copy carrying the quarantine attribute
  included; that has been confirmed only on the machine that built it, not yet by installing on a clean one.

## Windows

- **The installer**: `Octoboard_<version>_x64-setup.exe`, an NSIS installer, which installs Octoboard for the current
  user only. Later versions arrive through the application's own updates, each of which runs the new version's
  installer.
- **Windows 10 version 21H2 or later, on x64.** The application is built for x64 only; there is no native ARM64 build.
  On Windows 11 on Arm the x64 application, its daemon included, runs under the system's emulation. Everything these
  docs describe of the Windows application holds alike on Windows 10 21H2 and on Windows 11, and on x64 hardware as
  under emulation: the installer, the notification-area icon (see "The notification-area icon on Windows" in
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

## The server form

The server form has no requirements or distribution settled: the daemon is a single binary meant to be put on the
server as it is, and its Linux build is not established yet.
