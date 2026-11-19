# Descriptor facts for new agent shapes

> Goal: give the descriptor and the layout the facts the five new agents differ in from the first three — how an MCP
> tool is named, what the configuration-directory variable names, whether an agent has accounts beyond the default,
> whether it keeps record files or has a marker of its own — so that no provider has to bend generic code to say them.
> Completion criteria: each new fact is declared by every registered descriptor, read by generic code through the
> descriptor alone, and shown by a test that serves the test-only provider declaring the side the three agents do not
> take; the generated bindings and schema are regenerated; `pnpm test:rust`, `pnpm clippy:rust`, the UI tests and
> `pnpm check:windows` pass; the three existing agents declare today's behaviour and nothing about them changes.

The overview is `README.md` in this directory; the facts behind each item are in `cli-fact-sheets.md`. This milestone
follows `01-provider-neutral-repairs.md`.

## Technical design

### How an MCP tool's name is assembled

- [ ] `ProviderDescriptor` gains `tool_name_separator: &'static str`, beside `tool_name_prefix`: what the provider
  puts between the server's name and the tool's. The qualified name the role text and the tool catalogue use becomes
  prefix, server, separator, tool.
- [ ] Claude Code, Codex and Grok Build declare `__`.

Why: the double underscore is written into the generic name builder, and Gemini CLI presents `mcp_<server>_<tool>`,
OpenCode `<server>_<tool>`. A role text that names a tool the model cannot see sends it looking for nothing.

### What the configuration-directory variable names

- [ ] `AgentLayout` gains `config_dir_var_names`, one of two values: the variable names the configuration directory
  itself, or it names the directory's parent, the configuration directory being the entry of a fixed name inside it
  (the last component of `default_dir`).
- [ ] The layout offers the two conversions generic code needs, and generic code uses only them: the variable and
  value that make the CLI use a given configuration directory, and the configuration directory that an exported value
  of the variable stands for.
- [ ] The helpers that resolve the default account's directory and that export an account's directory to a short CLI
  query, and the quota kick, go through those conversions.
- [ ] An account's directory stays the configuration directory in every case. A provider whose variable names the
  parent can only use a directory of the agent's fixed name; refusing another is the provider's own
  (`AgentProvider::refuse_unlaunchable_account`), with its `dirHint` copy.
- [ ] The three existing layouts declare that the variable names the directory itself.

Why: Gemini CLI's `GEMINI_CLI_HOME` names the parent of `.gemini`, and OpenCode's data directory moves only with
`XDG_DATA_HOME`, the parent of `opencode`. One fact covers both.

### Accounts beyond the default

- [ ] `Capabilities` gains `owned_accounts: bool`: the provider's accounts beyond the default can each be given a
  directory of their own. It is served to clients with the other capabilities.
- [ ] The daemon refuses to add, import or point an account of a provider that does not declare it, with
  `CodedError::require_offered`. The default account is unaffected.
- [ ] The window offers no Add account for such a provider: the agent's actions and the Add account form's agent
  picker leave it out, and its section shows only the default account.
- [ ] Claude Code, Codex and Grok Build declare `true`.

Why: OpenCode cannot be pointed at a second data directory without changing a variable every program it starts would
inherit, and the user decided it has the default account only (`README.md`, "An account's directory is always the
agent's configuration directory").

### An agent without record files, or without a marker of its own

- [ ] `AgentLayout::records_dir` may be empty: the agent keeps no record files under its configuration directory. The
  generic record walk, removal and relocation have nothing to do for such a layout, and the layout test accepts it
  together with a record level and search depth of zero.
- [ ] `DetectionMarkers::own` may be empty: no name at a project's top level points at this provider alone. Project
  detection already skips a provider with nothing present; the descriptor test stops requiring one.

Why: OpenCode's conversations are rows in a database, and nothing at the top of a repository is GitHub Copilot CLI's
alone.

### Leading activity glyphs

- [ ] The set of glyphs a terminal title may start with to show activity gains U+270B (raised hand) and U+23F2
  (timer), so a title that starts with one reaches the provider's `ScreenReader::title_activity` with its glyph and is
  stored without it.

Why: Gemini CLI's titles start with ✦, ◇, ✋ or ⏲; the first two are already in the set.

## Implementation plan

- [ ] Add the separator, set it for the registered providers and build the qualified name from it. Verified by the
  existing role-text test that every tool is named with its agent's prefix, extended to the separator, with the
  test-only provider declaring one that differs.
- [ ] Add the layout fact and the two conversions, move the generic helpers onto them. Verified by layout tests for
  both values and by the account-command tests run for the test-only provider declaring the parent form.
- [ ] Add `owned_accounts`, the refusal and the UI's omission. Verified by a daemon test that adding an account of a
  provider declaring `false` is refused with `provider_unsupported` and by a UI test and gallery scenario of the
  Agent accounts section for such a provider.
- [ ] Allow an empty records directory and an empty own-marker list. Verified by the layout and descriptor tests and
  by a record-walk test over a layout with no records directory.
- [ ] Extend the glyph set. Verified by the title parser's tests for the two new glyphs, alone and before text.
- [ ] Regenerate the client bindings and the protocol schema, and mirror the new capability in the UI's served-info
  fixtures.

## Notes for the developer

**Reusable capabilities**

- `CodedError::require_offered` and the code `provider_unsupported` for refusing what a provider does not offer.
- The capability-agreement tests of the provider registry (a capability agrees with what the provider hands out) as
  the pattern for a new capability's test.
- The UI's reading of served provider facts (`useProviders` and the helpers over it) for the new capability.

**Development notes**

- A new fact follows "Adding a per-agent difference that has no capability or interface method yet" in
  `docs/conventions/provider-neutral-core.md`: a doc comment that says what the value means, set in every descriptor,
  tested through a test provider.
- `Capabilities` is a wire type: follow `docs/conventions/protocol-and-contracts.md`, "Wire types and generated
  files", and its rules on compatibility when a field is added.
- The glyph set is specified in `docs/product/session-titles.md`, "Leading activity glyphs"; the product doc changes
  with it.
- Which of the test-only provider's declarations change is this milestone's call: it must keep "a provider with none
  of the thing" readable, and many existing tests add accounts of it.

**Reference docs**

- `docs/conventions/provider-neutral-core.md`
- `docs/conventions/protocol-and-contracts.md`
- `docs/conventions/daemon.md`, "Agent accounts"
- `docs/product/agent-accounts.md`, "The accounts the user owns" and "Adding, editing and removing an account"
- `docs/product/session-titles.md`, "Leading activity glyphs"
- `cli-fact-sheets.md` in this directory
