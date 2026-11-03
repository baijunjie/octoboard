# 06 Accounts, Models and Session Tools

> Goal: Move each provider's models and efforts, accounts, quotas and login, and its status line behind the provider
> interface. Make the session tools and the UI around them provider-agnostic.
> Completion criteria:
> - None of the following contains a branch on the agent kind or names a provider:
>   - model listing and launch settings
>   - quotas, login, shared configuration and account switching
>   - role text and console instructions
>   - MCP tool descriptions and schemas
>   - the status-line command
>   - the UI's quota, account, trust-prompt and session-model components
> - The existing tests pass, with behavior unchanged.

## Technical design

- [ ] `AgentProvider::catalogue()`: models, efforts, the fallback model and the default effort.
- [ ] `AgentProvider::accounts() -> Option<&dyn AccountOps>`. It covers:
  - the login flow
  - the quota probe and kick
  - identity (email)
  - reset and use credits
  - shared-configuration items
  - the provider's own thread registry, where it has one
- [ ] Status line as an optional capability. The provider owns the status-line command's handling and its context
      reporting, and core only dispatches to it.
- [ ] Session tools name no provider. Milestone 02 already switched their reads of the tool-name prefix, ask-question
      tool and instruction-file name. Here, the text that lists providers is generated from the registries: role
      text, console instructions, and MCP tool descriptions and schemas (provider ids and quota-window naming).
- [ ] Served descriptors add quota-window naming and the model display rule. The provider's presentation module adds
      its trust caution. The UI shows reset credits and account sharing by capability.

## Implementation plan

- [ ] Implement the catalogue, account operations and status line for each existing provider from the current code.
- [ ] Route model listing, launch settings, quotas, login, shared configuration, account switching and the
      status-line command through the provider.
- [ ] Generate the provider-dependent parts of role text, console instructions and MCP tool descriptions from the
      registries.
- [ ] Replace the UI's per-agent branches in these places with descriptor data, capabilities and presentation-module
      messages:
  - quotas
  - account dialogs and account sharing
  - trust prompts
  - session model display

## Notes for the developer

- **Reusable capabilities**:
  - the existing account and quota registries
  - the existing isolation of agent CLI probes
- **Development notes**: gallery changes go through the gallery skill. The change check includes the UI check.
- **Reference docs**:
  - `docs/product/agent-accounts.md` and `docs/product/per-agent-*.md`
  - `docs/memory/isolating-agent-cli-probes.md`
  - the `agent-gallery` skill
