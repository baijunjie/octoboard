# Writing daemon code

The rules (where session state lives, derived state, store transactions, timing and claims, provider defaults) are in
`docs/conventions/daemon.md` and `docs/conventions/provider-neutral-core.md`, "Daemon"; versions and migrations in
`docs/conventions/protocol-and-contracts.md`, "Versions and compatibility"; platform rules in
`docs/conventions/daemon-processes-and-io.md`, "Platform differences"; lint in `docs/conventions/tooling.md`, "Running
tests, lint and checks". Two failures they lead to that look like defects and are not:

## A developer database left behind by an amended unreleased schema step is moved aside, and the user told

Amending an unreleased version in place leaves any developer database an earlier build already brought to that version
failing at the first read of what the amendment added ("no such column"): the store opens a database at its own
`SCHEMA_VERSION` as it is. That is expected, not a defect to fix in code; the database is moved aside and created
afresh. Say so to the user when the branch carrying the amendment replaces a build they have run.

## Clippy warnings that surface late are test code a plain run never linted

A warning in test code shows up only once something makes that target rebuild under `--all-targets`. Do not read that
delay as cargo withholding cached warnings: warnings are replayed for an unchanged target, so neither `cargo clean` nor
touching a file is the fix.
