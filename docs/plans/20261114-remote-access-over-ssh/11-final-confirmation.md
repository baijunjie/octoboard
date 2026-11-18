# 11 Final confirmation

> Goal: confirm at run time, on packaged builds and real hosts, what the milestones implemented but could only check by
> tests, cross-compilation and the gallery.
> Completion criteria: every item below observed working, or the defect it turns up fixed and observed working.

Run each check on an isolated packaged build (its own identifier, a throwaway HOME), never driving the user's own
Octoboard or screen, and ask the user before a check that installs a device key, since that raises a real keychain or
Credential Manager prompt.

Since the milestone was written, the connect page and the New Remote Window entries were replaced by the Remote devices
page in the main window (`docs/product/remote-devices-page.md`); the items below are worded for it.

## macOS application

- [ ] Allow remote access: the socket opens, a browser reaches the UI through `ssh -L` with the owner key, the TCP
      port refuses the key, turning it off cuts every key-admitted connection; Remote Login detection. Verified, except
      the Open System Settings button shown while Remote Login is off (Remote Login stayed on: turning it off needs an
      administrator).
- [ ] The Remote menu and the Dock menu: Remote Devices, a saved daemon (connects quietly), Open This Mac's Window.
      Verified, except Open This Mac's Window, which no round exercised.
- [x] The Remote devices page against a host: password, first-use fingerprint, changed host key, wrong password,
      unreachable host, no daemon running (Start), application not running on a desktop host, version mismatch
      (older: Open in browser; newer: Go to Updates and Open in browser), Open in browser holding the tunnel.
- [x] A device key installed after the first login and used from then on.
- [x] A dropped connection: the window shows the lost connection, reconnects behind the same port, and is pointed at
      the daemon again only when the key changed; closing the window or quitting ends the connection and nothing on
      the remote daemon. The menu bar tab's state while down was not observed (its native panel could not be opened
      without input to the user's screen); the Remote devices page agreed with the window.
- [x] One window per daemon: a second request for an open daemon focuses its window, including two at once.
- [x] Closing and quitting: a remote window closes for good; the main window hides while a remote window is on screen;
      the last window applies the closing rule; quitting with a remote window open, cancelled and confirmed; a remote
      window never asks its daemon to stop, at a quit or at a restart into an update.
- [x] Restore at launch: the remote windows open at quit come back, an entry needing input is skipped, the main window
      stays hidden when one came back and shows after 15 s when none has; nothing comes back at a launch at login.
- [x] The menu bar icon: one icon, the tabs switched while the menu is open, segment clicks, the raised-hand dot on a
      tab, VoiceOver reading the tabs, Disconnect from a remote tab; the Dock badge as the sum; a notification click
      bringing forward its own window.
- [x] A remote window shows no application settings and no update UI; a self-updating remote daemon's two update rows.
- [x] The main window without its daemon: while nothing has been fetched from the local daemon, the rail holds only
      the Remote devices button and the content panel shows the daemon's state; a daemon lost later keeps the last
      content with the connection banner (`docs/product/remote-devices-page.md`, "Without the local daemon").

## Windows application

- [x] Allow remote access: the browser port and the owner pipe, the port change, port in use, OpenSSH Server detection
      and its Optional features link.
- [x] The system menu (title bar icon, Alt+Space) and the jump list, with no and several saved daemons, in English
      (sentence case) and in Japanese; launch arguments reaching the running instance.
- [x] The flyout: placement from the bottom taskbar, dismissal, keyboard, Narrator, dark mode and High Contrast (the
      status badge's outline), the tabs and Disconnect with a remote daemon connected. Placement against a taskbar on
      the other edges and from the overflow area was skipped by the user.
- [x] An update restart remembering the open remote windows.

## Hosts

- [x] A macOS desktop app reached as a host from another computer's app.
- [x] A Windows desktop app reached as a host by a standard (non-administrator) user, with the device key installed
      into that user's `authorized_keys`, and by an administrator into `administrators_authorized_keys`.
- [ ] A host that offers keyboard-interactive login only (PAM).
- [ ] `octoboardd connection` on a Linux server: the pasted line fills the form; the QR code decodes to the same line.

## Copy

- [ ] A native-speaker look at the machine-written catalog text this work added, the th key path description first.

## Mid-task handoff

Four rounds of run-time verification on packaged builds ran on this Mac and the Windows dev VM; the results, evidence
and the drivers that produced them are in `/private/tmp/m11/` (`RESULTS*.md`, `CHECKLIST-R4*.md`), which a reboot may
clear. Everything ticked above passed there, the defects they turned up fixed and observed working again.

Left, and what each needs:

- **Needs a Linux server** (none was available without an administrator prompt): the PAM-only host and
  `octoboardd connection` with its QR code.
- **Not yet exercised, no obstacle:** the macOS Remote and Dock menus' Open This Mac's Window.
- **Needs the user:** turning Remote Login off to see the Open System Settings button, and the native-speaker review.
