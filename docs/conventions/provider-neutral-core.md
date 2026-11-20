# Provider-neutral core conventions

Octoboard drives several agents (Claude Code, Codex, Grok Build), and everything that differs between them lives in that
agent's provider, so the rest of the code stays the same when an agent is added or changes. This doc states the rule,
how to add a per-agent difference that has no home yet, and how to check a change against it. Where each file goes is in
the structure docs (`docs/structure/daemon.md` and `docs/structure/ui.md`, "Where new code goes"); how a provider's
codes are declared and worded is in `docs/conventions/error-codes.md`.

## The rule

- **Anything that differs between agents lives in that agent's provider.** Daemon: the descriptor
  `apps/daemon/src/providers/<id>.rs` holds the agent's facts and declared capabilities, the adapter
  `apps/daemon/src/adapter/<id>/` its behaviour. UI: the presentation module `packages/ui/src/providers/<id>.ts` holds
  its names, mark, colour and `provider.<id>.*` copy. Why: a new agent is then a descriptor, an adapter directory and a
  presentation module, with one line in each registry, and nothing keyed by provider changes.
- **Generic code never branches on which agent it is.** It reads the descriptor's fields and branches on declared
  capabilities, or calls the provider's interface (`AgentProvider` and the traits it hands out). Why: a branch on an id
  is a second, hidden description of that agent, and it is missed when the agent changes or another is added.
  Not: `if agent == AgentId::CODEX { ... }`, `agent === "codex" ? ... : ...`, a `match` over agent ids in generic code.
- **Generic code never hard-codes an agent's CLI flags, URL schemes, files, directories or environment variables.**
  They are fields of the descriptor (`layout`, `instruction_file`, `tool_name_prefix`, `detection`,
  `Capabilities::native_app_scheme`, ...), or arguments the adapter builds. Why: they are the agent's facts, and the
  descriptor is where a reader looks them up.
- **A code only one agent raises is that agent's code**, not a core code named after it. How it is declared, named and
  worded is in `docs/conventions/error-codes.md`, "Provider codes".
- **"No general solution exists" and "one agent supports the general mechanism badly" are no licence to special-case
  in generic code.** When a feature involves several agents, prefer a design that works for every agent; where one
  agent needs different handling, the special handling still lives in that agent's provider, behind a capability or an
  interface method that generic code calls. Why: the exception is a fact about the agent, so it belongs where the
  agent's facts are, and generic code stays free of it.
  Not: an `if` on the agent id in the generic message path because only Codex delivers through a private server;
  the generic path asks `MessageTransport`, which Codex's adapter provides and the others leave at its default.
- **A new provider is a descriptor, an adapter directory and one line in each registry** (`REGISTRY` in
  `apps/daemon/src/providers/mod.rs`, `PROVIDERS` in `apps/daemon/src/adapter/mod.rs`), plus a presentation module
  added to `BUILT_IN` in `packages/ui/src/providers/registry.ts`. If adding it needs a change anywhere else, that place
  was keyed by agent id and is a violation to fix first.

## Daemon

- **Read a provider's facts through its descriptor** (`AgentId::descriptor`, `providers::descriptors`) and its files
  through `AgentLayout` (the descriptor's `layout`), not from literals at the call site.
- **Reach a provider's behaviour through `adapter::provider(agent)`**, which gives the `AgentProvider`; its accounts
  through `AccountOps`, its models through `ModelCatalogue`, its hooks through `HookInterpreter`, its terminal screens
  through `ScreenReader`. Why: each trait is the one place the agent's way of doing that thing is written.
- **A value held for every provider is a `protocol::PerAgent<T>`**, indexed by `AgentId`, so a new provider is covered
  without a change.
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
- **Open a session in an agent's desktop application only through `coordinator::native_app::open_session_in_app`**;
  what each agent's application is asked is `AgentProvider::native_app`, declared by `Capabilities::native_app_scheme`.
- **Deliver a message through a provider's own server via `MessageTransport` and `PrivateServer`**, and run that server
  through `ServerHost`. A provider's own files stay behind its adapter (Codex's app-server handshake and thread
  database are reached only through `adapter/codex/app_server.rs` and `adapter/codex/threads.rs`, the latter through
  `ThreadRegistry`).
- **What a provider offers one made compatible with it goes through `CompatibilitySource`**
  (`LaunchSpec::compatibility_source`), and consumers use `adapter/compatibility_offer.rs`.
- **A provider's CLI install and update are `AgentProvider::cli_packaging`**, read only by `src/agents/cli/`.

## UI

- **Provider facts are read through `src/agents/providers.ts`** over the facts the daemon serves (`useProviders()`:
  `providerLabel`, `providerShortName`, `providerInstallUrl`, `providerConfigDir`, `detectionMarkers`), and branch on
  the capabilities the descriptor declares, not on an id.
- **A provider's name, mark, colour, plan names and messages are in its presentation module.** Components read them
  through `src/providers/registry.ts` (`providerMark`, `providerColour`, `providerPlanName`) and `AgentIcon`. Text that
  names a provider sets the name apart with `ProviderName`.
- **Quota window names and model display follow the rules the daemon serves in each descriptor** (`QuotaNaming`,
  `modelDisplayName`); reset credits and the account sharing switch show only for a provider that declares the
  capability.
- **A provider's settings are drawn from the served `ProviderInfo.settings` by `AgentProviderSettings`**, with their
  copy in the provider's module (`provider.<id>.setting.<setting>.heading` at the least).
- **A provider's codes, their wording and their recovery copy are in its presentation module** (declared as
  `docs/conventions/error-codes.md`, "Provider codes", says); generic recovery copy names no agent. Not: a generic
  recovery message that says "Codex" because only Codex raises the failure.
- **A provider the daemon serves with no module here is shown by its served label and a generic mark**, so generic code
  never assumes a module exists. `providers/agentMarks.ts` and the website's marks are updated by hand when a provider
  is to be shown there.

## Adding a per-agent difference that has no capability or interface method yet

When a feature behaves differently per agent and the code has nowhere to say so, add the place first; do not branch on
the id meanwhile.

1. **Name the difference as a fact or as a behaviour.** A yes/no fact about the agent, or a small value, is a
   *capability*; something the agent does in its own way, or that needs code, is an *interface method*.
2. **Daemon, a fact:** add a field to `Capabilities` (or to the descriptor beside the other facts) in
   `apps/daemon/src/providers/mod.rs`, with a doc comment saying what true means, and set it in each provider's
   `DESCRIPTOR` (`providers/<id>.rs`; the compiler lists the ones you missed). Generic code reads
   `agent.descriptor().capabilities.<field>`.
3. **Daemon, a behaviour:** add a method to `AgentProvider` (`apps/daemon/src/adapter/mod.rs`) or to the trait it hands
   out for that kind of thing (`HookInterpreter`, `ScreenReader`, `RecordStore`, `AccountOps`, `ModelCatalogue`,
   `StatusLine`, `CompatibilitySource`, `TrustCarrier`). Give it a default body that claims nothing and does nothing
   (see the rule above), and implement it in each adapter that has the behaviour (`adapter/<id>/`). Generic code calls
   the method on `adapter::provider(agent)`.
4. **A fact the UI needs travels in what the daemon serves** (the provider's served info, generated into
   `protocol.generated.ts`, `docs/conventions/protocol-and-contracts.md`). The UI reads the served capability; it does
   not copy it into a table keyed by id.
5. **UI copy and codes about it** go in the provider's presentation module (`packages/ui/src/providers/<id>.ts`):
   `provider.<id>.*` messages, and the record and wording of a code only that provider raises.
6. **Test it in generic terms**: a test provider (`adapter/test_provider.rs`, `providers/test_provider.rs`, UI
   `providers/testAgent.ts`) that declares the capability or overrides the method shows the generic code follows the
   declaration, not the id.

If the difference is a new kind of per-agent fact that many places will want, add it to the descriptor rather than to
one adapter, so the UI and other consumers can read it.

## What breaks the rule

These breaches show the shapes it takes. Each is described as a pattern; the cure is the same: move the
agent-specific part behind the provider and let generic code ask.

- **An interrupt title gate that branches on Claude Code.** Generic session code decided whether a new prompt hook
  and a newly stopped title proved the interrupted turn had ended by testing for the Claude Code agent. The behaviour
  belongs on the provider's `ScreenReader::prompt_after_title_stop_ends_turn`, which generic code asks.
- **A cross-project guard that parses each agent's tool-call payload.** Generic guard code knew the shape of each
  agent's file-edit tool call. Reading a payload is the provider's `HookInterpreter`'s job; the guard asks it which
  paths a call touches.
- **Codex-only relocation failures worded in generic code.** The causes of a Codex-only relocation failure and the UI's
  recovery copy that names Codex lived in generic code. They are Codex's provider codes (`provider.codex.<code>`,
  declared on its descriptor) with their records and wording in Codex's presentation module.
- **Inherited session marker variables listed in generic code.** The environment variables an agent's CLI sets to mark a
  session, which a nested launch must not inherit, were named in generic code. The cure is a field on each descriptor
  that declares its own, with generic code removing whatever the descriptors list.

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

`scripts/verify/provider-neutral.mjs` searches the complete production core, not only changed lines, as the
**provider-neutral core** step of every non-empty `pnpm verify` plan, quick and release plans included. Run it alone
with `node scripts/verify/provider-neutral.mjs`. It checks Rust, TypeScript, JavaScript and CSS for the ids and known
agent names, file names, environment variables, CLI flags, URL schemes and hook-event names. Comments are ignored;
strings stay visible. Rust items and external modules gated by `test` are excluded, along with conventional test and
fixture files. Adapter directories are recognised by their corresponding provider descriptor file, so a shared
adapter helper remains checked.

The only remaining production exceptions are the three adapter registration lines in
`apps/daemon/src/adapter/mod.rs`: each pairs an id with its implementation in `PROVIDERS`, the registration point the
rule permits. `EXCEPTIONS` in the check lists each exact file and source line with that reason; an exception does not
exempt adjacent code. There are no other production exceptions.

`scripts/verify/provider-neutral.test.mjs` covers these boundaries and runs the command against a temporary core with
a reintroduced agent-id branch, asserting exit status 1. The search recognises known spellings rather than inferring
all possible provider knowledge: reviewers still check new literals and new provider differences against the rule,
and extend the search vocabulary when a new spelling needs guarding.
