# 07 Provider Settings, Compatibility and Error Codes

> Goal: Make provider-specific settings, Claude compatibility and error codes generic: each provider declares them,
> and core code, the protocol, the store, MCP settings tools and UI handle them without naming a provider.
> Completion criteria:
> - These carry no provider-named field, code or branch:
>   - the settings protocol and store
>   - the configuration document
>   - the MCP settings tools
>   - the daemon and UI error-code catalogs
>   - the UI settings components and the configuration import views
> - Existing settings survive the migration.
> - Configuration export and import keep today's rule that approval settings are not exported.
> - A setting choice that needs confirmation is still confirmed before it is saved.
> - Claude compatibility behaves as before, including its default per consuming provider.
> - The existing tests pass, including the test that every daemon code has a UI classification.

## Technical design

- [ ] `ProviderDescriptor` gains a settings schema: `&[SettingDescriptor]`. Each entry has:
  - an id
  - typed choices
  - a default
  - whether a configuration export carries it
  - for each choice, whether saving it needs a confirmation

  The provider injects its settings into launch through `plan`.
- [ ] Provider settings on the wire, in the store and in the configuration document are a generic map from provider id
      to setting id to value. This replaces the approval-mode, approval-reviewer and Claude-compatibility fields.
- [ ] Compatibility source interface on `AgentProvider`, offered by a provider that declares itself a source:
  - its MCP servers
  - its instruction files
- [ ] Claude compatibility becomes a setting of a consuming provider, read through the compatibility source interface.
      Its default is a rule over the added providers, so today's defaults hold: Grok on, Codex on only while Claude
      Code is added.
- [ ] Provider-namespaced error codes, `provider.<id>.<code>`:
  - The descriptor declares them, and the daemon's code catalog includes them.
  - Each provider's presentation module owns their UI classification record and copy.

## Implementation plan

- [ ] Migrate the stored provider-named settings into the generic provider settings.
- [ ] Carry provider settings through the configuration document's export and import by each setting's export flag.
- [ ] Replace the per-agent settings components and configuration import views with renderers driven by the settings
      schema, including the confirmation step.
- [ ] Move Codex's and Grok's reading of Claude Code's configuration onto the compatibility source interface.
- [ ] Make the MCP settings tools serve provider settings generically.
- [ ] Move per-agent error codes and per-agent i18n keys under the provider namespaces. Keep the UI test that checks
      every daemon code has a classification, extended to provider codes.

## Notes for the developer

- **Reusable capabilities**:
  - the existing settings store and settings coordinator
  - the existing configuration document and its version upgrades
  - the existing error-code and i18n catalog mechanisms
- **Development notes**:
  - Renaming or removing settings fields and error codes changes the protocol, so raise the protocol generation.
  - Changing the configuration document's settings raises its version, with an upgrade step for documents from
    released builds.
  - The protocol types are generated for the UI, so regenerate them.
  - The change check includes the UI check and the copy conventions. Gallery changes go through the gallery skill.
- **Reference docs**:
  - `docs/product/configuration-export-and-import.md`
  - `docs/memory/adding-a-daemon-error-code.md` and `docs/memory/adding-a-user-setting.md`
  - the protocol's "Versions and compatibility" section
  - the `knowledge-i18n-copy` and `agent-gallery` skills
