# Remote Access and Pairing Development Plan

## Problem and approach

The daemon binds to loopback with no authentication, which is safe only because nothing else can reach it. Phones and
other machines must be able to, so the daemon needs an authenticated surface for them and a way to pair a device by
scanning a QR code.

The daemon is a remote shell by nature — it can start agents, type into them and list directories — so being on the same
network is not authorisation. Reaching the host from another device is left to a third-party overlay network such as
Tailscale; Octoboard builds no network layer of its own.

## Key design decisions

- **Two listening surfaces**: an internal one on loopback carrying only what the agent processes use (hook callbacks, the
  MCP endpoint), and an external one for clients, with authentication on every route, the UI and every WebSocket included.
- **Pairing**: a short-lived one-time code, carried in a QR code with the host's address, is exchanged for a long-lived
  per-device credential. Devices are listed and revocable.
- **WebSocket upgrades check the origin.**
- **Off by default**: whether the external surface exists at all is a setting.
- **The QR code** is shown in the macOS application and printed in the Linux terminal.
- **This reverses the "loopback only" premise** stated in the README, the protocol document and
  `docs/product/application-lifecycle.md`; those are updated in the same change.

## Milestones

- [01 Authenticated surface](01-authenticated-surface.md)
- [02 Pairing and devices](02-pairing-and-devices.md)
- [03 QR code and setup guide](03-qr-code-and-setup-guide.md)

## Open

- The bind policy under Tailscale: the overlay interface only, or all interfaces with authentication.
- Whether the connection needs TLS for native clients beyond what the overlay provides (for example `tailscale serve`).
