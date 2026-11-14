# 02 Browser access on Windows

> Goal: the owner turns remote access on in their Windows application and opens its daemon in a browser on another
> machine, through their own SSH port forward (`ssh -L`) to a fixed loopback port.
> Completion criteria: on Windows, the Remote access section and its switch are offered, and the confirmation names
> the cost particular to Windows beside the trade-off milestone 01 states; while the switch is on, the daemon serves
> the UI and accepts the owner key at a fixed loopback port, which the owner can change in the section; a browser on
> another machine, through `ssh -L` to that port, opens the address shown and sees the same consoles and sessions as
> the window; rotating the key and turning the switch off behave as in milestone 01; a port already in use is said in
> the section, with browser access not working until the port is changed and the daemon otherwise running; the
> application tells whether OpenSSH Server is on and guides the owner to turn it on; `octoboardd address` answers for
> the application's daemon on Windows; the port setting is absent from the agents' settings tools and from the
> configuration document, as the switch is; the daemon's own port still refuses the owner key and serves no UI, and
> on Linux and macOS no TCP port accepts the owner key; the product docs state the Windows exception and what it
> costs.

## Technical design

- [ ] On Windows, while remote access is on, a loopback listener at a fixed port takes the socket's place: it serves
      the UI and accepts the owner key. The daemon's own port, which the application and the agents' helpers use,
      stays as it is and keeps refusing the owner key.
- [ ] The port is a setting beside the switch, kept by the daemon and left out of the agents' settings tools and of
      the configuration document like it, and a change takes effect at once.
- [ ] The confirmation on Windows adds: on a computer other people also log in to, one of them can take this port
      whenever the daemon is not holding it — while the application is not running, or while its daemon is
      restarted — and be sent the key by a browser that reconnects.
- [ ] The section shows the browser address with its key and the `ssh -L` command for this port, each with a copy
      action, and the rotate action, as on macOS.
- [ ] The section says so when the port is already in use.
- [ ] Detecting whether OpenSSH Server is on, shown in the section, with guidance to the system setting that turns it
      on.
- [ ] `octoboardd address` on Windows printing the browser address and the `ssh -L` command while remote access is
      on.

## Notes for the developer

- **Reusable capabilities**: everything milestone 01 adds (the setting, the section, the switch and its
  confirmation, turning off and rotation); the browser router the socket serves on Linux and macOS.
- **Development notes**: this is the one place the owner key is accepted on a TCP port, and it exists because
  Windows' SSH server cannot forward to a socket or a named pipe; nothing else may come to rely on it, and the
  application's own way in to a Windows host is the named pipe of milestone 09. No daemon on Windows reads or holds
  an owner key today, and its key file is trusted there through the data directory's access control list, not
  through the ownership checks Linux and macOS make. Changing the port frees the old one, which pages opened on it
  keep reconnecting to.
- **Reference docs**: `docs/product/who-can-reach-the-daemon.md` ("Other users of the same machine", "The owner
  key"), `docs/product/server-form.md` ("The browser socket"), `docs/product/files-octoboard-owns.md`,
  `docs/memory/verifying-changes-for-windows.md`.
