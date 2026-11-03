# 02 Provider Descriptor and Registries

> Goal: Establish the provider descriptor, the behavior trait, their registries and the UI presentation modules.
> Move each provider's static facts into them.
> Completion criteria:
> - The daemon reads the facts listed below only from descriptors:
>   - labels, install URLs, binaries and config dirs
>   - the records layout and detection markers
>   - the instruction-file name, the tool-name prefix and the ask-question tool name
> - The UI reads install URLs and config dirs only from served descriptors. For providers with a presentation module,
>   it reads marks, colours, labels and short names only from that module.
> - Capability flags are declared (their use is switched by the milestone owning each concern).
> - The daemon's layering test passes, and a test iterates both daemon registries and checks every descriptor is
>   complete.
> - A UI test checks that every built-in provider has a presentation module with messages in every locale.
> - Behavior is unchanged and the existing tests pass.

## Technical design

- [ ] `ProviderDescriptor`, registered below the wire layer and iterable in a fixed order. It carries:
  - id, label and short name, for daemon-written text
  - install URL
  - binary and config dir
  - the records layout (the existing layout descriptor)
  - detection markers
  - the instruction-file name, the tool-name prefix and the ask-question tool name
  - capability flags
  - whether it is a compatibility source

  It is looked up by the current agent kind; the enum stays in this milestone.
- [ ] Behavior trait `AgentProvider: Send + Sync`, registered in the agent sessions layer, absorbing the launch
      adapter. Its core:
  - `descriptor() -> &ProviderDescriptor`
  - launch planning: `plan(LaunchSpec) -> LaunchPlan`, `installed_version`, `refuse_unlaunchable_account`
  - title parsing, as the adapter has it today

  Later milestones add the sub-interface accessors, each with a default.
- [ ] The protocol serves the registered providers' descriptor data: id, label, install URL, default config dir and
      capability flags. Later milestones add the rest.
- [ ] A served provider with no presentation module in the UI shows by its served label, also used as its short name,
      with a generic mark.
- [ ] One UI presentation module per provider: its mark, its colour, and its `provider.<id>.*` messages (label and
      short name for now). The messages are merged into the catalogs. A UI registry lists the modules by provider id.

## Implementation plan

- [ ] Turn the launch adapter into the behavior trait, and register the existing providers in both registries.
- [ ] Move the per-agent static constants, the detection marker list and the instruction-file names into
      descriptors. Lower layers that use provider facts today read them from the descriptor registry instead of
      methods on the enum.
- [ ] Replace UI per-agent tables and default-agent literals with served descriptors and presentation-module lookups.
      The default is the first registered provider.
- [ ] Have gallery mocks serve descriptor data the way the daemon does.

## Notes for the developer

- **Reusable capabilities**:
  - the existing launch adapter and its per-agent modules
  - the existing records layout descriptor
  - the existing i18n catalog mechanism
- **Development notes**:
  - The `Agent` enum stays closed until milestone 08. Keep exhaustive matches compiling, but add no new ones outside
    provider modules.
  - Today's module docs state that per-agent knowledge lives where it is used. Update them to the new rule: it lives in
    the provider.
  - Served descriptors arrive like other daemon data. Until they arrive, the UI shows what it shows for other
    not-yet-loaded daemon data, not a provider-specific fallback.
  - Gallery changes go through the gallery skill. The change check includes the UI check.
- **Reference docs**:
  - `docs/structure/daemon.md`, section "Dependency direction", and the daemon's layering test
  - `docs/agent-cli-reference.md`
  - the `agent-gallery` skill
