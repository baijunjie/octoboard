# 04 The desktop form as a host

> Goal: another device of the owner's connects to the daemon of their macOS or Windows application.
> Completion criteria: on macOS and on Windows, the application tells whether the computer's SSH server is on and, if
> not, guides the owner to turn it on; it shows the QR code and text details; another machine's application connects
> through them and uses the desktop daemon while the desktop application keeps running.

## Technical design

- [ ] Detecting whether the SSH server is on (macOS Remote Login, Windows OpenSSH Server), and guidance to the
      system setting that turns it on.
- [ ] The QR code and text details shown in the application.
- [ ] The owner command answering for the application's own daemon, whose port the operating system assigns, on
      macOS and Windows.

## Notes for the developer

- **Reusable capabilities**: the per-user port file the daemon writes; the application's settings pages and their
  guidance to system settings.
- **Development notes**: a desktop daemon follows its application, so it is reachable only while the application
  runs; the guide says so.
