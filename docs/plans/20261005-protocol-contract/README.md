# Protocol Contract Development Plan

## Problem and approach

The protocol is documented in prose and mirrored by hand in the web client's types. Native iOS and Android clients will be
written in other languages against it, so it has to become a contract that can be consumed mechanically, and one that
cannot drift from the daemon.

## Key design decisions

- **A machine-readable schema** for the control channel's requests, events and records and for the terminal stream's
  control frames, in a top-level `contracts/` directory.
- **Derived from, or checked against, the daemon's own types**, never a second hand-kept copy.
- **The web client consumes it too.**
- **Field meaning lives in the schema's descriptions**, since native clients will be written from it.

## Milestone

> Goal: native clients can be generated from the protocol description, and the description cannot drift.
> Completion criteria: the schema covers every request, event and record; a quality check fails when the daemon's types
> and the schema disagree; the web client's types come from, or are checked against, the schema.

### Technical design

- [ ] The schema and its place in the repository.
- [ ] The way it is produced from, or checked against, the daemon's types.
- [ ] The web client's types switched over to it.

### Implementation plan

- [ ] Add the drift check to the repository's quality checks.
- [ ] Keep the prose protocol document consistent with the schema.

### Notes for the developer

- **Reusable capabilities**: the daemon's protocol module already defines every message as a typed structure.
- **Reference docs**: `daemon/PROTOCOL.md`.

## Open

- The schema format and the tool that produces it; decide when starting.
