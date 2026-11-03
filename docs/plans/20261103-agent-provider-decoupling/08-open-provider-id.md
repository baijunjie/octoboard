# 08 Open Provider ID

> Goal: Replace the closed agent enum with an open provider id, so adding a provider needs only its own modules and a
> registration line in each registry.
> Completion criteria:
> - The closed agent enum, the per-provider struct fields and the per-provider SQL columns are gone.
> - Existing data migrates without loss: accounts, account-default flags, and both launch pairs per console.
> - A test-only provider, registered with one line in each registry, runs through:
>   - every daemon test that iterates the registries
>   - the UI's provider-rendering tests
>
>   No other code changes for it.
> - The full checks pass.

## Technical design

- [ ] `AgentId`: a string provider id, validated against the descriptor registry where it enters (protocol input,
      stored rows).
- [ ] Per-provider structs become maps keyed by `AgentId`.
- [ ] The per-provider columns on consoles and projects move to child tables keyed by owner and provider, holding:
  - the account
  - the project's account-default flag
  - each launch pair (model and effort), with which session kind it is for (console sessions or project sessions)
- [ ] The reading of per-provider column groups by column offset goes away with the columns.
- [ ] The generated TS agent type becomes a string id. UI `Record<Agent, …>` types become registry lookups.
- [ ] Tests that iterate every registered provider through each sub-interface. They replace the enum's compile-time
      exhaustiveness (user decision: exhaustiveness is given up).

## Implementation plan

- [ ] Write the store migration that moves the per-provider columns into the child tables.
- [ ] Switch the protocol, the store rows and the core types to `AgentId` and maps.
- [ ] Add the registry-iterating tests and the test-only provider that proves the bar.
- [ ] Reword the docs and comments that assume a fixed number of agents ("three agents", "the other two").

## Notes for the developer

- **Reusable capabilities**: the existing store migration mechanism; the existing protocol type generation.
- **Development notes**:
  - Keep the wire shape unchanged. Provider ids stay the same strings, and a map keyed by them serializes like
    today's per-provider structs, provided every served map holds an entry for every registered provider. A new
    provider id is growth within a generation, so this milestone does not raise the protocol generation.
  - Stored data may name a provider that is no longer registered. Loading does not fail:
    - Per-provider rows, sessions and accounts of that provider are kept in the store and left out of what is loaded.
    - A console's or project's default agent naming it falls back to the first registered provider. The console or
      project itself is not hidden.
    - A console's project-session agent naming it falls back to following the default agent.
  - Update the docs on the daemon structure and the protocol contract.
- **Reference docs**: `docs/structure/daemon.md`; `docs/architecture.md`.
