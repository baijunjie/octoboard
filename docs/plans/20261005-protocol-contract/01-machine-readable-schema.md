# 01 Machine-readable schema

> Goal: native clients can be generated from the protocol description, and the description cannot drift.
> Completion criteria: the schema covers every request, event and record; a quality check fails when the daemon's types
> and the schema disagree; the web client's types come from, or are checked against, the schema.

## Technical design

- [ ] The schema and its place in the repository.
- [ ] The way it is produced from, or checked against, the daemon's types.
- [x] The web client's types switched over to it.

## Implementation plan

- [ ] Add the drift check to the repository's quality checks.
- [ ] Keep the prose protocol document consistent with the schema.

## Notes for the developer

- **Reusable capabilities**: the daemon's protocol module already defines every message as a typed structure.
- **Reference docs**: `apps/daemon/PROTOCOL.md`.

## Progress

The drift check and the web client's switch to the generated types are delivered; the machine-readable schema for
native clients is not.

- **Delivered**: the daemon renders every wire type reachable from its requests and events as TypeScript with `ts-rs`
  (a dev-dependency, derived only under `cfg(test)`) into `packages/ui/src/protocol.generated.ts`, which is committed. A
  daemon test fails when the file is stale; the terminal stream's control frames are covered too. The web client reads
  its wire types from that file: `packages/ui/src/protocol.ts` re-exports it and adds only the two unions a client
  sends with, the UI's own rules about wire values live in the layers that own them, and an ESLint rule keeps
  `protocol.ts` the one importer of the generated file.
- **Remaining**: the schema in `contracts/` for the Swift and Kotlin clients (the generated file is TypeScript and
  carries Rust doc comments, not a language-neutral description); and keeping `PROTOCOL.md` consistent with it.
