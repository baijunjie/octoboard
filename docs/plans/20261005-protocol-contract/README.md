# Protocol Contract Development Plan

## Problem and approach

The protocol is documented in prose and mirrored by hand in the web client's types. Native iOS and Android clients will be
written in other languages against it, so it has to become a contract that can be consumed mechanically, and one that
cannot drift from the daemon. Once clients built against one release meet a daemon of a later one, the contract also
has to stop a change that would break them.

## Key design decisions

- **A machine-readable schema** for the control channel's requests, events and records and for the terminal stream's
  control frames, in a top-level `contracts/` directory.
- **Derived from, or checked against, the daemon's own types**, never a second hand-kept copy.
- **The web client consumes it too.**
- **Field meaning lives in the schema's descriptions**, since native clients will be written from it.
- **The protocol only grows between releases.** A check compares the current schema with a snapshot of the last
  release's:
  - inputs may only accept more;
  - outputs may only give more fields, and never a new kind of value or a lost type or required field;
  - added requests and events are allowed;
  - a real break needs a deliberate new protocol generation.

## Milestones

- [01 Machine-readable schema](01-machine-readable-schema.md)
- [02 Protocol compatibility guard](02-protocol-compatibility-guard.md) — needs 01's language-neutral schema, not the
  web client's switch-over to it

## Open

- The schema format and the tool that produces it; decide when starting.
- Whether error and notice codes are exempt from the rule that output values only shrink, given that clients already
  handle an unknown code from a newer daemon.
