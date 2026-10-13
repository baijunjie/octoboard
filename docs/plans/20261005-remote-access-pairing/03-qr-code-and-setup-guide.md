# 03 QR code and setup guide

> Goal: the owner can set up access from a phone or from another machine of theirs by following the docs and scanning a
> QR code or typing what it carries.
> Completion criteria: the QR code appears in the macOS application and in the Linux terminal and, scanned by a device
> that can reach the address in it, pairs it; the same address and code typed by hand, on a machine without a camera,
> pair it too; the setup guide covers the server on a plain network end to end, and an overlay such as Tailscale for
> reaching a machine from outside its network.

## Technical design

- [ ] The QR payload: the address the daemon is reached at and the one-time code.
- [ ] The same address and code shown as text beside the QR code, for a machine without a camera to type in.
- [ ] Rendering in the application and in the terminal.
- [ ] A user-facing setup guide.

## Notes for the developer

- **Development notes**: choosing the address to put in the code is the awkward part when the server has several
  interfaces.
- **Reference docs**: `README.md`.
