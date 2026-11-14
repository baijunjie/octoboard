# 01 Remote access switch and browser access on macOS

> Goal: the owner turns remote access on in their macOS application and opens its daemon in a browser on another
> machine, through their own SSH forward of the daemon's socket, working with the same daemon, sessions and settings
> as the application's window. The case it serves: a Mac mini at home runs only the desktop application, and the
> owner uses it from a browser while away, without installing the server form.
> Completion criteria: on macOS, remote access is off by default; turning it on asks for an explicit confirmation of
> the security trade-off; once on, with nothing restarted, the daemon serves the UI on its socket with the owner key,
> and a browser on another machine, through the forward that Settings and `octoboardd address` give, opens the
> address and sees the same consoles and sessions as the window; rotating the key in Settings cuts off the pages
> opened with the old one; turning the switch off stops the address working and disconnects those pages; the setting
> is absent from the agents' settings tools and from the configuration document; the application tells whether
> Remote Login is on and guides the owner to turn it on; `octoboardd address`, run from the application's own files,
> answers for the application's daemon; a daemon that cannot open its socket says why in Settings; the daemon's TCP
> port refuses the owner key and serves no UI throughout, on Windows too, where the application's daemon now embeds
> the UI as well; the user docs say the desktop application has to keep running for remote use.

## Technical design

- [ ] The daemon the desktop application bundles embeds the UI, as the server builds do.
- [ ] The embedded UI files are stored compressed and served so, in every build that embeds them.
- [ ] The TCP port of a daemon that follows the application serves no UI on any system, Windows included, where a
      daemon that embeds the UI serves it on its port today.
- [ ] An allow remote access setting, off by default, kept by the daemon, and left out of the agents' settings tools
      and of the configuration document.
- [ ] A daemon that follows the application opens its owner socket, serves the UI on it and holds and accepts the
      owner key while the setting is on: at its start, and from the moment the setting is turned on while it runs.
- [ ] Turning the setting off closes the socket, stops accepting the owner key and closes every connection that key
      opened.
- [ ] Settings gains one **Remote access** section, shown in the application's window and not in a browser; later
      milestones add to the same section rather than opening others.
- [ ] An **Allow remote access** switch in it. Turning it on first shows a confirmation that states the trade-off:
      whoever reaches the address with its key can do everything the owner can on this computer, including running
      any command; the key lasts until rotated and travels with the browser's history, bookmarks and sync; reaching
      it from another machine goes through an SSH login to this computer.
- [ ] The section shows the browser address with its key and the SSH forward command, each with a copy action, and
      an action that rotates the key.
- [ ] The section says so, with the reason, when the daemon could not open its socket.
- [ ] Detecting whether Remote Login is on, shown in the section, with guidance to the system setting that turns it
      on.
- [ ] `octoboardd address` answering for the application's own daemon: the browser address and the forward while
      remote access is on, and that remote access is off otherwise.
- [ ] Features only the application's window has (installing updates, the native directory picker and the like) are
      absent in the browser, as they already are for the server form's browser.
- [ ] Quitting the desktop application still stops its daemon, unchanged; the user docs say plainly that remote use
      needs the desktop application kept running.

## Notes for the developer

- **Reusable capabilities**: the owner key (persistence in the data directory, rotation that closes the connections
  the old key opened, acceptance by the access guard); the browser socket with its checks that the data directory is
  private; the embedded UI, its serving without a key and the `Host` / `Origin` checks; the page's handling of a
  refused key; the browser platform adapter, which already leaves out what only the application provides; the
  `address` command and its notes; the application's settings pages and their guidance to system settings.
- **Development notes**: today the browser socket, the owner key and the embedded UI belong only to a daemon that
  does not follow the application, and are decided once as the daemon starts; this milestone makes them follow the
  setting, while the daemon runs too. A new user setting is as a rule exposed to the agents' settings tools and, by
  the section it is in, carried by the configuration export; this one is deliberately in neither. The window keeps
  loading the UI the application bundles, unchanged. On Windows the section is not offered until milestone 02.
- **Reference docs**: `docs/product/server-form.md` ("The browser socket", "The UI the daemon serves"),
  `docs/product/who-can-reach-the-daemon.md` ("The owner key"), `docs/product/daemon-command-line.md`,
  `docs/product/files-octoboard-owns.md`, `docs/product/application-lifecycle.md`, `docs/structure/daemon.md`,
  `docs/structure/desktop.md`, `docs/memory/adding-a-user-setting.md`.
