# Provider-neutral core conventions

Octoboard drives several agents (Claude Code, Codex, Grok Build, Antigravity CLI, GitHub Copilot CLI, OpenCode, Qwen Code and Pi), and everything that differs between them lives in that
agent's provider, so the rest of the code stays the same when an agent is added or changes. Where each file goes is in
the structure docs (`docs/structure/daemon.md` and `docs/structure/ui.md`, "Where new code goes").

## General principles

- **Anything that differs between agents lives in that agent's provider.** Daemon: the descriptor
  `apps/daemon/src/providers/<id>.rs` holds the agent's facts and declared capabilities, the adapter
  `apps/daemon/src/adapter/<id>/` its behaviour. UI: the presentation module `packages/ui/src/providers/<id>.ts` holds
  its names, mark, colour and `provider.<id>.*` copy. Why: a new agent is then a descriptor, an adapter directory and a
  presentation module with its served-info fixture, with one line in each registry, and nothing keyed by provider
  changes.
- **Generic code never branches on which agent it is.** It reads the descriptor's fields and branches on declared
  capabilities, or calls the provider's interface (`AgentProvider` and the traits it hands out). Why: a branch on an id
  is a second, hidden description of that agent, and it is missed when the agent changes or another is added.
  Not: `if agent == AgentId::CODEX { ... }`, `agent === "codex" ? ... : ...`, a `match` over agent ids in generic code.
- **Generic code never hard-codes an agent's CLI flags, URL schemes, files, directories or environment variables.**
  They are fields of the descriptor (`layout`, `instruction_file`, `tool_name_prefix`, `tool_name_separator`, `detection`,
  `Capabilities::native_app_scheme`, ...), or arguments the adapter builds. Why: they are the agent's facts, and the
  descriptor is where a reader looks them up.
- **A code only one agent raises is that agent's code**, not a core code named after it, and generic recovery copy
  names no agent. How it is declared, named and worded is in `docs/conventions/error-codes.md`, "Provider codes".
- **"No general solution exists" and "one agent supports the general mechanism badly" are no licence to special-case
  in generic code.** When a feature involves several agents, prefer a design that works for every agent; where one
  agent needs different handling, the special handling still lives in that agent's provider, behind a capability or an
  interface method that generic code calls. Why: the exception is a fact about the agent, so it belongs where the
  agent's facts are, and generic code stays free of it.
  Not: an `if` on the agent id in the generic message path because only Codex delivers through a private server;
  the generic path asks `MessageTransport`, which Codex's adapter provides and the others leave at its default.
- **A per-agent difference with no home yet gets one first: a fact becomes a capability field, a behaviour a trait
  method.** A yes/no fact or small value that clients are served, or that launch planning or a refusal reads, is a field
  of `Capabilities` (or of the descriptor), set in every descriptor; a fact about how the agent's terminal, hooks or
  reports behave, read only by the daemon code handling that behaviour, is a default method of the trait handing out
  that behaviour (or of `AgentProvider`); something the agent does its own way is a method of `AgentProvider` or the trait it hands out for that kind of thing,
  implemented in each adapter that has the behaviour. A fact the UI needs travels in what the daemon serves, never in a
  table keyed by id. Do not branch on the id meanwhile.
- **A new provider is a descriptor, an adapter directory and one line in each registry** (`REGISTRY` in
  `apps/daemon/src/providers/mod.rs`, `PROVIDERS` in `apps/daemon/src/adapter/mod.rs`), plus a presentation module
  added to `BUILT_IN` in `packages/ui/src/providers/registry.ts` with its served-info fixture beside it
  (`packages/ui/src/providers/<id>.fixture.ts`, the `ProviderInfo` the daemon serves for it, which the UI tests serve).
  If adding it needs a change anywhere else, that place was keyed by agent id and is a violation to fix first.

- **Apply the same capability bar to each provider.** Measure what its CLI supports, implement the feature when
  the CLI provides a way to do it, and declare it unavailable when a measured blocker prevents it. The product doc
  for the feature records the blocker.

## Daemon

- **Generic code exports or reads an agent's configuration-directory variable only through
  `AgentLayout::config_dir_export` and `config_dir_of_export`**, never through `config_dir_var` alone. Why: the
  variable may name the directory's parent (`ConfigDirVarNames::Parent`), and a helper written by hand exports the wrong
  directory for such an agent.
- **Where providers map their own kinds onto a neutral enum (a tool call's `ToolCategory`), like things get the same
  value in every provider**: a shell command, a file edit or a search is the same category whichever agent ran it.
  Check the other providers' mappings when adding one. Why: clients render and group by the neutral value, so two
  providers that differ on it show the same act two ways.
- **A default method of `AgentProvider` or of a trait it hands out is the answer of a provider with none of the
  thing**: it recognises, hints, proves and acts on nothing. Never write it as what the built-in providers happen to
  share, least of all where the answer leads to a refusal or a deletion. Behaviour several providers share goes in a
  helper their overrides call. Why: a provider that does not override a method gets its default silently.
  Not: a `RecordStore` default that takes a missing record as conclusive, which made every session of a provider whose
  records it cannot recognise count as gone.
  Not: a `HookInterpreter` default that reads a payload field the built-in agents happen to share (`agent_type`,
  `session_id`); reading those fields is a helper (`crate::hooks::is_named_subagent`, `crate::hooks::agent_session_id`)
  that the providers' overrides call.
  Not: a `HookInterpreter::past_startup` default of `true`, which would release the held messages of a provider that
  runs a hook before its trust screen is answered, so that a message's Enter answers the screen.
- **Refuse an operation a provider does not offer with `CodedError::require_offered`** (the code
  `provider_unsupported`), after checking the declared capability, rather than by comparing agent ids.
- **A setting of one provider is declared in its descriptor's `settings`** (a `SettingDescriptor` in
  `apps/daemon/src/providers/mod.rs`) and read from `LaunchSpec::settings` by the provider at launch. Add no field,
  request, store column, document field, MCP field or UI component for it: the store, wire, configuration document,
  MCP tool and UI follow the schema.
- **Set a provider setting's flags by whether it is a permission setting.** A setting that decides what sessions may
  do without asking is `shown_to_agents: false`, and `confirm: true` where a choice lets sessions run every action
  without asking; any setting is `exported: true` unless turning it on itself changes files outside `~/.octoboard/`.
  Its heading copy `provider.<id>.setting.<setting>.heading` lands in the same change: Settings silently leaves out a
  setting with no heading (the import review falls back to its id), and no test catches it.
- **Which settings a console, a project or a session may override is decided only by `SettingDescriptor::overridable`**
  and the queries built on it, never by a list of settings elsewhere.
- **Launch settings are read through `coordinator::launch::launch_provider_settings`**, never
  `Store::provider_settings_of` directly, so the switches a launch level holds are applied.

## UI

- **A provider the daemon serves with no module here is shown by its served label and a generic mark**, so generic code
  never assumes a module exists. `providers/agentMarks.ts` and the website's marks are updated by hand when a provider
  is to be shown there.

## Checking a change against this rule

For a reviewer, and for a change's author before delivery: search the diff of generic code for agent knowledge. Generic
code is everything in the daemon outside `apps/daemon/src/providers/` and `apps/daemon/src/adapter/<id>/` (so everything
under `adapter/` outside an agent's directory, such as `hooks.rs` and `hooks/`, `account_cli.rs` and `account_cli/`,
`files.rs` and `compatibility_offer.rs`, is generic code and names no agent's format), and in the UI everything outside
`packages/ui/src/providers/`. Tests, test fixtures, the UI gallery (`packages/ui/src/gallery/`), the fixture daemon
(`packages/ui/src/fixtures/`) and the website preview (`packages/ui/src/website-preview/`) are outside the search: they
stand in for agents, not for production code.

- Agent ids: `AgentId::CLAUDE`, `AgentId::CODEX`, `AgentId::GROK`, and the string literals `"claude"`, `"codex"`,
  `"grok"` (and `"claude-code"`, `"grok-build"` forms).
- Agent-specific CLI flags, URL schemes, file and directory names (`.claude`, `.codex`, `.grok`, `CLAUDE.md`,
  `AGENTS.md`), and environment variables (`CLAUDE_*`, `CODEX_*`, `GROK_*`).
- Wording that names an agent in a message, code record or recovery text outside its provider's module.

Each hit is a violation unless it is a registry line (`REGISTRY`, `PROVIDERS`, `BUILT_IN`) or the descriptor constant
itself. A change that adds a per-agent difference must show where it lives: a descriptor field or capability, an
interface method with each adapter's implementation, and the provider's presentation module for UI copy.

The search runs as the provider-neutral core step of every `pnpm verify` plan (`scripts/verify/README.md`,
"Provider-neutral core search"). It learns the registered providers from their descriptor files and guards each id; the
other spellings a provider is known by are its entry in the search's `VOCABULARY`. Its only production exceptions are
the adapter registration lines in `apps/daemon/src/adapter/mod.rs`, one per registered id, which the search derives from
the ids; an exception does not exempt adjacent code. The search knows spellings, not intent: reviewers still check new
literals and new provider differences against the rule, and extend its vocabulary when a new spelling needs guarding.
