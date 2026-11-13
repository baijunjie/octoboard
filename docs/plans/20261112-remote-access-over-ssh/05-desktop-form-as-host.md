# 05 The desktop form as a host

> Goal: another device of the owner's connects, through the application, to the daemon of their macOS or Windows
> application.
> Completion criteria: on macOS and on Windows, the application shows the QR code and text details in its Remote
> access section; another machine's application connects through them and uses the desktop daemon while the desktop
> application keeps running.

## Technical design

- [ ] The QR code and text details shown in the Remote access section milestone 01 adds.
- [ ] Another machine's application connecting to the desktop daemon through them, with the owner command and the SSH
      server guidance milestone 01 provides.

## Notes for the developer

- **Reusable capabilities**: the Remote access section, SSH server detection and the owner command's answer for the
  application's daemon (milestone 01); the connection details and their QR code (milestone 04).
- **Development notes**: a desktop daemon follows its application, so it is reachable only while the application
  runs; the guide says so.
