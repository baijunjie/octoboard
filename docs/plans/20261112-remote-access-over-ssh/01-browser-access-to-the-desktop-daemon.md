# 01 Browser access to the desktop daemon

> Goal: the owner opens the daemon of their macOS or Windows application in a browser on another machine, through
> their own SSH port forward, and works with the same daemon, sessions and settings as the application's window. The
> case it serves: a Mac mini at home runs only the desktop application, and the owner uses it from a browser while away,
> without installing the server form.
> Completion criteria: on macOS and on Windows, browser access is off by default; turning it on asks for an explicit
> confirmation of the security trade-off; once on, the daemon serves the UI at a fixed loopback port with the owner key,
> and a browser on another machine, through `ssh -L`, opens the address shown in Settings and sees the same consoles
> and sessions as the window; rotating the key in Settings stops the old address opening the daemon; turning it off
> stops the browser address working; the application tells whether the computer's SSH server is on and guides the
> owner to turn it on; `octoboardd address` answers for the application's daemon with the browser address; the daemon
> listens on loopback only throughout; the user docs say the desktop application has to keep running for remote use.

## Technical design

- [ ] Settings gains one **Remote access** section, shown in the application's window; later milestones add to the
      same section rather than opening others.
- [ ] An **Allow browser access** switch in it, off by default. Turning it on first shows a confirmation that states
      the trade-off: whoever reaches the address with its key can do everything the owner can on this computer,
      including running any command; the key lasts until rotated and travels with the browser's history, bookmarks
      and sync; reaching it from another machine goes through an SSH login to this computer.
- [ ] While it is on, the application's daemon serves the UI the application itself bundles at its own address,
      preferably straight from the application's own files rather than a second copy of the UI built into the
      daemon.
- [ ] While it is on, the daemon accepts the persistent owner key, as a daemon without the application does today.
- [ ] While it is on, the daemon is reached at a fixed port, which the owner can change in the section; still
      loopback only.
- [ ] The section shows the browser address with its key, with a copy action, and an action that rotates the key.
- [ ] The section explains how to connect from another computer: an `ssh -L` port-forward command for this port and
      this computer, then the address to open.
- [ ] Detecting whether the computer's SSH server is on (macOS Remote Login, Windows OpenSSH Server), shown in the
      section, with guidance to the system setting that turns it on.
- [ ] The owner command (`octoboardd address`) answering for the application's own daemon on macOS and Windows,
      with the browser address while browser access is on.
- [ ] Features only the application's window has (installing updates, the native directory picker and the like) are
      absent in the browser, as they already are for the server form's browser.
- [ ] Quitting the desktop application still stops its daemon, unchanged; the user docs say plainly that remote use
      needs the desktop application kept running.

## Notes for the developer

- **Reusable capabilities**: the owner key (persistence in the data directory, rotation, acceptance by the access
  guard); the server form's serving of the UI without a key and its `Host` / `Origin` checks; the page's handling of a
  refused key; the browser platform adapter, which already leaves out what only the application provides; the
  application's settings pages and their guidance to system settings; the shell's way of passing settings it holds to
  the daemon at start.
- **Development notes**: today the application's daemon serves no UI, its port is assigned by the operating system
  and its launch key changes at every start, and the owner key is enabled only for a daemon that does not follow the
  application; this milestone makes the owner key switchable by the setting. The window keeps loading the UI the
  application bundles, unchanged (settled by the user). The application bundles its UI inside its executable, not as
  separate files, which bears on how the daemon can serve it (see "Open" in `README.md`).
- **Reference docs**: `docs/product/server-form.md`, `docs/product/who-can-reach-the-daemon.md` ("The owner key"),
  `docs/product/files-octoboard-owns.md`, `docs/product/application-lifecycle.md`, `docs/structure/daemon.md`,
  `docs/structure/desktop.md`.
