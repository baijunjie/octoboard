# Provider-neutral repairs

> Goal: remove every place in generic code, generic tests and the provider-neutral check that describes the first
> three agents, so that registering a provider needs only its own files and its registry lines.
> Completion criteria: a throwaway provider added on a scratch branch as a descriptor, an adapter with only a launch
> plan, a presentation module with its fixture, and the registry lines passes `pnpm test:rust`, `pnpm clippy:rust`,
> `pnpm --filter @octoboard/ui test` and `node scripts/verify/provider-neutral.mjs` with no other file changed, whatever
> capabilities it declares; and with the throwaway provider gone, the same commands pass and the three existing agents
> behave as before.

The overview is `README.md` in this directory. This milestone changes no behaviour: it moves knowledge, and loosens
tests that encode "every real agent is like the first three".

## Technical design

### A hook event's name and meaning belong to the provider

- [ ] `HookInterpreter` gains `announces_session_start(&self, event: &str, payload: &Value) -> bool`, defaulting to
  `false`: whether the event is the agent saying its session has started, past its start-up screens. Each of the three
  existing interpreters answers it for its own event.
- [ ] Applying a hook event asks the session's interpreter that question where it compares the event's name with the
  literal `SessionStart` today, before noting the session's start for the trust hold.
- [ ] The hook forwarder (`octoboardd hook`) takes an optional `--event <name>`, and the daemon's hook route takes it
  as an optional `event` query parameter: the name of the event for a payload that names none. A name in the payload
  wins. The three existing agents pass none, so one constant command still serves all their events.
- [ ] The helpers that build a hook command and write a hook script take the optional event name, for an adapter that
  has to register one command per event.

Why the second pair: the generic layer reads an event's name only from the payload, which holds for the three agents
and is unmeasured for GitHub Copilot CLI's native payload. The route's contract is the place to say that a payload
need not name its event, not a provider's branch.

### Generic tests follow declarations, never a list of agents

- [ ] The descriptor test that every provider's launch overrides are exactly its compatibility switches derives the
  expectation from the declarations (`SettingDescriptor::compatibility_with`), and each provider's own descriptor tests
  pin its own list. No generic test matches on agent constants with a panic for the rest.
- [ ] The adapter test that every real agent has a status integration goes: a provider without one is a supported
  shape. Each provider's own tests assert what it declares.
- [ ] The adapter test that every non-test provider reads a context share through its own record store follows the
  store's own declaration (`RecordStore::recognises_records`), so a provider that ships before its records is not a
  failure.
- [ ] The quota probe tests take each probing provider's request marker and reply fixture from the provider, not from
  a generic match on the agent: the generic tests run against a stand-in probe, and each provider's quota module tests
  its own reply.
- [ ] Every other generic test helper that matches on an agent and panics on the rest (the record placing of the
  handoff tests, the account ids of the shared-configuration tests, the turn events of the hook application tests, the
  running titles of the interrupt tests) is reached only for agents its test names; any reached through `AgentId::ALL`
  or through a capability filter moves its fixture to the provider.

### UI tests take providers from the registry

- [ ] Each built-in presentation module has a served-info fixture beside it: the `ProviderInfo` the daemon serves for
  that provider. The fixture daemon's provider list and the test provider list are assembled from those fixtures.
- [ ] The registry tests that check a module's copy (names in every catalog, the copy of each declared setting and
  choice in every language, placeholders, the codes' remedies) run over every built-in module with its own fixture,
  not over a hand-written list of three.

Why: today a fourth module would be registered and never checked, and checking it would mean editing the shared
fixture list.

### The provider-neutral check learns its agents from the registry

- [ ] The check derives the registered provider ids from the descriptor files (`apps/daemon/src/providers/<id>.rs`,
  leaving out the registry, the layout and the test provider), and builds the id patterns (`AgentId::<ID>`, the quoted
  id) from them.
- [ ] The other spellings a provider is known by (its product names, dot-directories, instruction file names,
  environment variable prefixes, CLI flags, URL schemes, hook event names) are one table entry per provider id in the
  check. The three existing entries hold exactly what the single expression holds today.
- [ ] The registration exceptions are derived the same way: the adapter registry line of each registered id, with or
  without a `cfg` attribute before it, and nothing else.
- [ ] The check's own test covers a provider added by a descriptor file alone: its id is guarded in generic code and
  its registration line is excepted, with no edit to the check.

## Implementation plan

- [ ] Add the interpreter method and move the three agents' answers into their interpreters; replace the literal in
  hook application. Verified by the existing trust and hook application tests, plus a test that a provider whose
  interpreter does not answer never starts the trust hold's release on an event of that name.
- [ ] Add the optional event name to the forwarder, the route and the hook helpers. Verified by a route test: a
  payload with no name is applied under the given name, and a payload's own name wins.
- [ ] Rewrite the generic daemon tests listed above. Verified by registering the throwaway provider with every
  capability off and no record store, then with a status integration and a record store, and running the suite both
  ways.
- [ ] Add the per-provider served-info fixtures and move the UI registry tests onto the registry. Verified by the
  throwaway presentation module: a setting declared in its fixture with no heading copy fails the suite.
- [ ] Make the check's vocabulary per provider and its ids derived. Verified by the check's own test and by running
  the check on the target branch before and after: the same files pass.
- [ ] Carry out the dry run of the completion criteria, then remove the throwaway provider. Whatever else had to
  change to make it pass is a further repair of this milestone, listed in the delivery report.

## Notes for the developer

**Reusable capabilities**

- The test-only provider on both sides (the daemon's descriptor and behaviour, the UI's presentation module): every
  test that walks the registries already runs it. It is the model for "a provider with none of the thing".
- The per-session ordered hook queue and its route: the event name travels beside the body, not inside a rewritten
  body.

**Development notes**

- A default method claims nothing (`docs/conventions/provider-neutral-core.md`, "Daemon"): the new interpreter method
  defaults to `false`, and no default may be written as what the three agents share.
- The hook route is part of the daemon's documented surface; its description changes with it.
- Search the whole of generic code for further event names and payload field names of the three agents before
  declaring the repair complete: the check's vocabulary names only some of them, which is how the `SessionStart`
  literal went unnoticed.
- Tests, fixtures and the gallery are outside the provider-neutral rule, but a generic test that fails when a provider
  is registered still breaks "one line in each registry".

**Reference docs**

- `docs/conventions/provider-neutral-core.md`
- `docs/conventions/tests.md`
- `docs/conventions/tooling.md`
- `docs/structure/daemon.md`, "Agent records, hooks, adapters and MCP"
- `docs/structure/ui.md`
- `cli-fact-sheets.md` in this directory, "GitHub Copilot CLI"
