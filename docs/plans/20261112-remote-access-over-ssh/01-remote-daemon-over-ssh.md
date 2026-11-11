# 01 Connecting to a remote daemon over SSH

> Goal: the macOS and Windows applications connect to a daemon on another machine over SSH and are used as they are
> with their own daemon.
> Completion criteria: from the macOS application and from the Windows application, given a Linux server's host, port,
> user and the user's own password or key, the owner runs a console session and project sessions on the server's
> daemon, the raised hand notifying them through the application; a host whose key does not match the pinned
> fingerprint is refused; a dropped connection is reconnected, the UI showing it as a lost daemon connection meanwhile;
> a server with no daemon running is reported as such; the daemon listens on loopback only throughout.

## Technical design

- [ ] A remote daemon entry: host, port, user, pinned host key fingerprint, and how to authenticate.
- [ ] An SSH connection with password or key authentication, verifying the host key against the pinned fingerprint.
- [ ] Obtaining the remote daemon's address and key by running the owner command over that connection.
- [ ] A local port forwarded to the remote daemon's loopback address, and the UI pointed at it with the key, as it is
      at the application's own daemon.
- [ ] Reconnecting after a dropped connection, reusing the existing lost-connection handling.
- [ ] The application tells the UI its daemon is remote, so nothing that assumes the daemon shares this machine (the
      native directory picker, opening the file manager or a system terminal) is offered, though the daemon is reached
      at a loopback address.

## Implementation plan

- [ ] Add the SSH connection and forward first, then the remote daemon entry and its UI.

## Notes for the developer

- **Reusable capabilities**: how the shell hands the UI its daemon's port and key; the UI's lost-connection handling;
  the desktop shell's notifications.
- **Development notes**: the UI today takes any loopback daemon address for a daemon on this machine. Through the
  forward the page reaches the daemon at a loopback `Host`, which the existing `Host` / `Origin` check admits unchanged.
- **Reference docs**: `apps/daemon/PROTOCOL.md` ("Who may connect"), `docs/product/application-lifecycle.md`
  ("Losing the daemon connection").
