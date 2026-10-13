# Remote Access and Pairing Development Plan

## Problem and approach

The daemon binds to loopback with no authentication, which is safe only because nothing else can reach it. In the
server form (see "Two deployment forms" in `docs/architecture.md`) every client is on another machine: the daemon runs
on a Linux server without a display and its owner reaches it from a browser on their own machine — so the external
surface is the only surface that form has. The desktop form has the same shape as a secondary case, the owner's own
phone or second machine reaching their Mac. Both need an authenticated surface and a way to let a device in.

One user, any number of their devices. The daemon does not tell clients apart: whoever holds a credential is taken for
the owner and can do everything the owner can, so a credential is to be kept like a password, and nothing here gives
a second person a place of their own. That is this version's boundary, not an omission.

The daemon is a remote shell by nature — it can start agents, type into them and list directories — so being on the same
network is not authorisation, and the surface has to stand on its own on a plain network. It is not left to an overlay:
the owner's other machine on the same network should not have to join a Tailscale network first to reach the server,
which costs more than it protects. An overlay such as Tailscale stays the answer for reaching a machine from outside
its network; Octoboard builds no network layer of its own either way.

A browser is the client, and that settles TLS: a browser grants the Notifications API and the Clipboard API only to a
secure context, which over plain `http` is a loopback address alone — from a network address the raised hand never
notifies anyone, and the whole point of the orchestration is an agent raising its hand and a person answering. The UI
also opens `wss:` only from an `https:` page, and an `https:` page may not open `ws:`. So the external surface is HTTPS,
and the UI is served from that same origin.

## Key design decisions

- **Two listening surfaces**: an internal one on loopback carrying only what the agent processes use (hook callbacks,
  the MCP endpoint), and an external one for clients, with authentication on every route, the UI and every WebSocket
  included.
- **TLS on the external surface**, for the browser's reasons above; how its certificate is provisioned is open.
- **Pairing**: a short-lived one-time code, carried in a QR code with the daemon's address, is exchanged for a
  long-lived per-device credential. Devices are listed and revocable, so a lost or retired device is revoked on its own
  without re-issuing the rest; each stands for the owner, never for a different person.
- **WebSocket upgrades check the origin.**
- **Off by default in the desktop form**: whether the external surface exists at all is a setting there. The server
  form has no other surface, so it is on from the start there.
- **The QR code** is shown in the macOS application and printed in the Linux terminal.
- **This reverses the "loopback only" premise** stated in the README, the protocol document,
  `docs/product/application-lifecycle.md` and `docs/architecture.md`; those are updated in the same change.

## Milestones

- [01 Authenticated surface](01-authenticated-surface.md)
- [02 Pairing and devices](02-pairing-and-devices.md)
- [03 QR code and setup guide](03-qr-code-and-setup-guide.md)

## Open

- The bind policy: every interface, or one chosen at start (an overlay's, where one is used).
- How the certificate for the external surface is provisioned — one the server's administrator supplies, a private CA,
  or a name with a public certificate — and what a browser is told to trust.
