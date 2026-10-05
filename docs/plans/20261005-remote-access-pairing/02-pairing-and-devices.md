# 02 Pairing and devices

> Goal: a device can be paired with a one-time code and then holds its own revocable credential.
> Completion criteria: a code works once and expires; a paired device authenticates with its credential; a revoked
> device is refused from then on, including on connections already open.

## Technical design

- [ ] Issuing a one-time pairing code with a short lifetime.
- [ ] Exchanging a code for a per-device credential, stored hashed.
- [ ] Listing and revoking devices, through the protocol.

## Implementation plan

- [ ] Add credential storage to the daemon's store and the pairing exchange to the external surface.

## Notes for the developer

- **Reusable capabilities**: the daemon's existing store.
- **Development notes**: revocation has to close open sockets, not only refuse new ones.
- **Reference docs**: `daemon/PROTOCOL.md`.
