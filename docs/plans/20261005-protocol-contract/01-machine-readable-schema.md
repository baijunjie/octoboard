# 01 Machine-readable schema

> Goal: native clients can be generated from the protocol description, and the description cannot drift.
> Completion criteria: the schema covers every request, event and record; a quality check fails when the daemon's types
> and the schema disagree; the web client's types come from, or are checked against, the schema.

## Technical design

- [ ] The schema and its place in the repository.
- [ ] The way it is produced from, or checked against, the daemon's types.
- [ ] The web client's types switched over to it.

## Implementation plan

- [ ] Add the drift check to the repository's quality checks.
- [ ] Keep the prose protocol document consistent with the schema.

## Notes for the developer

- **Reusable capabilities**: the daemon's protocol module already defines every message as a typed structure.
- **Reference docs**: `apps/daemon/PROTOCOL.md`.

## Progress

The drift check is delivered; the machine-readable schema for native clients is not.

- **Delivered**: the daemon renders every wire type reachable from its requests and events as TypeScript with `ts-rs`
  (a dev-dependency, derived only under `cfg(test)`) into `packages/ui/src/protocol.generated.ts`, which is committed. A
  daemon test fails when the file is stale, and `packages/ui/src/protocol.contract.ts` makes `tsc` fail when a type of
  `protocol.ts` differs from its generated counterpart, optional key and `null` included; the terminal stream's control
  frames are covered too. Both run under `pnpm verify` for a change to either side.
- **Remaining**: the schema in `contracts/` for the Swift and Kotlin clients (the generated file is TypeScript and
  carries Rust doc comments, not a language-neutral description); the web client importing the generated types instead
  of mirroring them (`protocol.ts` would keep its helpers and constants); and keeping `PROTOCOL.md` consistent with it.
