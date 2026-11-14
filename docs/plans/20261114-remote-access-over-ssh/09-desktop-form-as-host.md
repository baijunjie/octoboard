# 09 The desktop form as a host

> Goal: another device of the owner's connects, through the application, to the daemon of their macOS or Windows
> application.
> Completion criteria: on macOS and on Windows, with remote access on, the application shows the connection details
> as text and as a QR code in its Remote access section; another machine's application connects through them, finds
> `octoboardd` among the host application's own files, and uses the desktop daemon in a window of its own while the
> host's application keeps running; the proxy mode and the machine-readable `octoboardd address` of milestone 03
> work for the application's daemon on both systems; on Windows the application's way in is a named pipe only the
> daemon's user can open, never a TCP port; a command run through Windows' SSH server is shown to carry binary data
> unchanged, with the command prompt and with PowerShell as the server's default shell, before anything is built on
> it.

## Technical design

- [ ] On Windows, the daemon's owner endpoint: a named pipe that only the daemon's user can open and that refuses
      remote clients, open while remote access is on, serving what the socket serves on Linux and macOS.
- [ ] The proxy mode and the machine-readable `octoboardd address` (milestone 03) working against that pipe.
- [ ] The connection details and their QR code shown in the Remote access section milestone 01 adds, with the path
      of the application's own `octoboardd`.
- [ ] Another machine's application connecting to the desktop daemon through them, with the SSH server guidance
      milestones 01 and 02 provide.

## Implementation plan

- [ ] First, verify on Windows that the proxy mode's bytes pass unchanged through the SSH server, whichever default
      shell it is set to.

## Notes for the developer

- **Reusable capabilities**: the Remote access section, SSH server detection and `octoboardd address` answering for
  the application's daemon (milestones 01 and 02); the proxy mode (milestone 03); the connection details and their
  QR code (milestone 08).
- **Development notes**: a desktop daemon follows its application, so it is reachable only while the application
  runs; the guide says so. On Windows the browser's port of milestone 02 stays the browser's alone: the application
  never sends the key to it.
- **Reference docs**: `docs/product/who-can-reach-the-daemon.md`, `docs/memory/verifying-changes-for-windows.md`.
