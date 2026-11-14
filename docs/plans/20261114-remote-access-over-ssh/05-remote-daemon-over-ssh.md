# 05 Connecting to a remote daemon over SSH

> Goal: the macOS and Windows applications connect to a daemon on another machine over SSH and show it in a window of
> its own, used as the local daemon's window is.
> Completion criteria: from the macOS application and from the Windows application, given a Linux server's host,
> port, user and the user's own password or key, the owner opens a window on the server's daemon and runs a console
> session and project sessions there, the raised hand notifying them through the application; a host whose key does
> not match the pinned fingerprint is refused; a host given with no fingerprint is connected to only once the owner
> has confirmed the fingerprint shown, which is pinned from then on; a dropped connection is reconnected, the window
> showing it as a lost daemon connection meanwhile, and a key rotated meanwhile is followed; a server with no daemon
> running is reported as such, with an action that starts it; a daemon with no owner endpoint is reported as such; a
> daemon of another protocol generation is refused before its window loads, naming both versions and the ways out;
> an `octoboardd` that cannot be found is reported, asking for its path; the application's local port opens nothing
> without the key, and stays the application's from before the window opens until it is closed, reconnections
> included; the daemon listens on loopback only throughout.

## Technical design

- [ ] A remote daemon entry, kept by the application: a name, host, port, user, the pinned host key fingerprint, how
      to authenticate, and the path of `octoboardd` where it is not found by itself. No password is stored.
- [ ] An SSH connection made by the application itself, with password or key authentication, verifying the host key
      against the pinned fingerprint, or showing it for confirmation at a first connection that has none.
- [ ] Finding `octoboardd` on the remote machine: the entry's path, else the places Octoboard installs to.
- [ ] Reading the owner key, the protocol generation and the release version through the machine-readable
      `octoboardd address`, at every connection and reconnection.
- [ ] Refusing a daemon of another generation before a window is opened, offering to update the server's daemon over
      the connection or to update the application.
- [ ] A loopback port the application listens on, assigned by the system and held for as long as the window lives,
      through every reconnection; each connection it accepts is carried by one run of the proxy mode over the SSH
      connection.
- [ ] A window (milestone 04) opened on that port with the key, marked remote with the entry's name.
- [ ] Reconnecting after a dropped connection behind the same local port, the window's existing lost-connection
      handling covering the gap; the window is pointed at the daemon again only where the key has changed.
- [ ] For a server with no daemon running, an action that starts it over the connection.
- [ ] Closing the window, or quitting, ends the connection and nothing on the remote daemon.

## Notes for the developer

- **Reusable capabilities**: the proxy mode and the machine-readable `octoboardd address` (milestone 03); the window
  per daemon (milestone 04); the shell's way of pointing a window at a new address; the UI's lost-connection
  handling; the desktop shell's notifications; the server form's `start` and `update` commands.
- **Development notes**: the shell carries no daemon traffic today, and from this milestone it carries the tunnels,
  which the structure docs state. Through the proxy a request reaches the owner socket with a loopback `Host` and the
  application's own origin, which the existing checks admit unchanged. A window of the application is not a page the
  daemon served, so it does not ask whether its key is refused; the application re-reads the key instead. The window
  retries its daemon's address by itself after a lost connection, which is why the application may never let the
  local port go while the window lives: the owner key is kept off any port another user could take. Until milestone
  07 a password is asked for at every connection.
- **Reference docs**: `apps/daemon/PROTOCOL.md` ("Who may connect", "Versions and compatibility"),
  `docs/product/application-lifecycle.md` ("Losing the daemon connection", "A daemon of another version"),
  `docs/product/who-can-reach-the-daemon.md` ("Other users of the same machine"),
  `docs/product/server-form-updates.md`, `docs/memory/verifying-the-daemon-on-linux.md`.
