# 02 Protocol compatibility guard

> Goal: a change that would break a client built against the last release fails the checks.
> Completion criteria: with a snapshot of the last release's schema in the repository, the check passes on additions
> and fails on each of the following:
> - a removed request, event or field;
> - a newly required input;
> - an input that accepts less;
> - an output that can carry a new kind of value, or loses a type or a required field.

## Technical design

- [ ] A snapshot of the schema from milestone 01 as last released, refreshed at each release.
- [ ] A structural comparison of the current schema with the snapshot:
  - Inputs may only widen: types widen, enumerations grow, and no new required properties.
  - Outputs may only give more: no lost types or required fields, and enumerations only shrink.
  - Added requests, events and output fields pass.
- [ ] A test of the comparison itself, showing that it catches breaks and allows growth.
- [ ] A protocol generation number that a deliberate break increments.

## Implementation plan

- [ ] Run the check under the repository's existing verification.
- [ ] Add refreshing the snapshot to the release steps.

## Notes for the developer

- **Reusable capabilities**: the schema from milestone 01, and the existing drift check.
- **Reference docs**: `apps/daemon/PROTOCOL.md` (including "Coded messages").
