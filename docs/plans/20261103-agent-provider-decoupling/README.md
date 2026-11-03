# Agent Provider Decoupling Development Plan

## Problem and approach

Octoboard runs agents from several providers (Claude Code, Codex, Grok Build). Only a few things sit behind a
per-agent interface today:

- launching, MCP/hook/role injection and resuming, which go through the launch adapter
- title parsing, also through the launch adapter
- the records layout, which has a separate per-agent descriptor

Everything else a provider differs in is hard-coded where it is used, in the daemon and in the UI:

- how its hook events are read
- message delivery quirks
- how its terminal screens and dialogs are recognized
- how its records and history are read
- its models and efforts
- its accounts, quotas and login
- the MCP tool descriptions
- its settings
- its error codes and copy

The agent kind is a closed enum, and per-provider values are kept in named struct fields and per-provider SQL
columns. Adding a provider means editing core types, the wire protocol, the store schema and dozens of modules.

The goal is complete decoupling. **Adding a provider means two things:**

- writing the provider's own modules: its daemon descriptor and behavior, and its UI presentation module
- adding a registration line for it in each provider registry

No core daemon, hooks, records, accounts, MCP, store, protocol, UI component or settings code changes. The marketing
website and the website preview terminals in the UI package are outside this bar. They are promotional content,
updated by hand when a provider is to be shown there (user decision).

The terminology changes with it. What the website and docs call agent "brands", and in places "vendors", becomes
"providers".

## Key design decisions

- **One unified provider interface, split by layer (user decision).** The daemon's layering test forbids the wire
  and storage layers from depending on the agent sessions layer, yet both need provider facts and must validate
  provider ids. So:
  - A static `ProviderDescriptor` is registered below the wire layer, where every layer can read it.
  - The behavior trait `AgentProvider` is registered in the agent sessions layer.
  - `AgentProvider` absorbs the launch adapter and gathers the behavior sub-interfaces:
    - hook interpretation and delivery
    - screen reading
    - records
    - model catalogue
    - account operations
  - Every method beyond the core has a default, a no-op or "unsupported". A provider implements only what it has.
- **Capabilities are declared, not inferred from the agent kind.** Core code and UI branch only on the capability
  flags and data a provider declares, never on which provider it is. Examples of such flags:
  - preallocated session id
  - hook script
  - status line
  - reset credits
  - per-session trust
  - message delivery quirks
- **A compatibility source exposes an interface (user decision).** Codex and Grok reuse Claude Code's configuration
  (Claude compatibility).
  - A provider may declare itself a compatibility source. It then exposes its MCP servers and instruction files
    through an interface on itself.
  - A consuming provider reads them through that interface, never through the source's file layout.
  - Whether compatibility is on by default is a rule over the added providers. For example: on only while the
    source is added.
- **The daemon serves descriptor data, and the UI keeps only presentation (user decision).**
  - The daemon serves each provider's descriptor data over the protocol, defined once on the daemon side:
    - install URL and default config dir
    - capability flags
    - quota-window naming
    - its setting choices
    - its model display rule
  - The UI keeps one presentation module per provider: its mark and colour, and its own i18n messages under a
    `provider.<id>.*` namespace merged into the catalogs. The messages cover:
    - its label and short name
    - its trust caution
    - its setting copy
    - its error copy

    It also holds the UI classification record of each of its error codes.
  - The daemon descriptor's label is used in text the daemon writes itself, such as MCP tool descriptions and role
    text. It is also served to the UI.
  - A UI of the same protocol generation may be served a provider it has no presentation module for. It shows that
    provider by the served label with a generic mark, and its features follow the served capabilities as usual (user
    decision).
  - Components render generically from both.
- **The provider id becomes open (user decision).** The closed `Agent` enum is replaced by a string provider id.
  - What replaces it:
    - per-provider struct fields become maps
    - the per-provider SQL columns move to child tables keyed by owner and provider
    - the generated TS type becomes a string
  - The cost: the enum's compile-time exhaustiveness is given up. Today adding an agent fails to compile everywhere it
    is unhandled, on purpose.
  - What covers for it: interface defaults, and tests that run every registered provider through each sub-interface.
- **Per-provider wire and store shapes become generic, keeping today's product rules.**
  - Provider-specific settings (Claude Code's approval mode, Codex's approval reviewer, Claude compatibility) become
    provider settings, declared by each provider's settings schema.
  - Each setting declares whether a configuration export carries it. Each choice may require a confirmation before it
    is saved. This keeps approval settings out of exports and keeps the confirmation before "approve everything".
  - Per-provider error codes become provider-namespaced (`provider.<id>.<code>`).
- **The migration keeps the enum closed until the last milestone.** Each earlier milestone moves one concern behind
  the interface while the enum still exists. Each merges on its own with behavior unchanged.
- **The terminology rename covers all user-facing copy and docs (user decision).**
  - What changes:
    - website copy in every locale, in each locale's own word for "provider"
    - docs, the README and code comments
    - "vendor" in the sense of an agent's maker
    - "brand colour", "brand mark" and "brand icon" for a provider's visual identity, which become "provider colour",
      "provider mark" and "provider icon"
  - What stays:
    - the comment noting that provider product names are trademarks and so not translated
    - uses unrelated to agents: the app's own branding, the slogan, git hosts, browser and platform brands, styling
      classes, vendored code and "brand-new"
- **Chinese wording (user decision).**
  - zh-Hans: 提供商 / 提供商智能体.
  - zh-Hant: 供應商, phrased as 各供應商的代理 or 另一供應商的代理 where the agent is meant. 供應商代理 is never
    written as one word, since it reads as a supplier's distributor.
- **The Claude-only status-line command moves into the provider as part of the status line capability (user
  decision).** The bar leaves no provider-specific command in core.

## Milestones

- [01 Provider terminology](01-provider-terminology.md)
- [02 Provider descriptor and registries](02-provider-descriptor-and-registries.md)
- [03 Hooks and message delivery](03-hooks-and-message-delivery.md)
- [04 Records and history](04-records-and-history.md)
- [05 Screen reading](05-screen-reading.md)
- [06 Accounts, models and session tools](06-accounts-models-and-session-tools.md)
- [07 Provider settings, compatibility and error codes](07-provider-settings-compatibility-and-error-codes.md)
- [08 Open provider id](08-open-provider-id.md)
