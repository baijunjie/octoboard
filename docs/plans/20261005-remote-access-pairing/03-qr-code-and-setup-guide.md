# 03 QR code and setup guide

> Goal: a user can set up access from a phone by following the docs and scanning a QR code.
> Completion criteria: the QR code appears in the macOS application and in the Linux terminal and, scanned by a device on
> the overlay network, pairs it; the setup guide covers the Tailscale-based setup end to end.

## Technical design

- [ ] The QR payload: the host's overlay-network address and the one-time code.
- [ ] Rendering in the application and in the terminal.
- [ ] A user-facing setup guide.

## Notes for the developer

- **Development notes**: choosing the address to put in the code is the awkward part when a host has several interfaces.
- **Reference docs**: `README.md`.
