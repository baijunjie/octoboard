# Protocol and contract conventions

The daemon's wire protocol, the generated bindings and schema, the MCP contract, and the versions of the protocol, the
store and the configuration document. This doc states the rules a change to any of them follows; the full contract (each
request, event and record) is `apps/daemon/PROTOCOL.md` and the committed schema in `contracts/protocol/`. How a code is
declared and worded is in `docs/conventions/error-codes.md`; file placement is in `docs/structure/daemon.md` and
`docs/structure/ui.md`, "Where new code goes".

## Wire types and generated files

- **A wire type is added in the daemon's Rust types (`apps/daemon/src/protocol/`) and documented in
  `apps/daemon/PROTOCOL.md`; its handler is under `src/coordinator/`, or for a browse request in `src/browse/`, served
  through `browse::lane`.** The UI never writes a wire type by hand:
  `packages/ui/src/protocol.generated.ts` is regenerated and never edited (ESLint ignores it as test-written).
- **The body of an HTTP route that only the daemon's own subcommands read (`POST /restart-check`,
  `POST /revert-gate/:session`, `POST /git-ref/:session`) is defined in the module that handles it, without the `TS` and `JsonSchema` derives, and is
  documented in `apps/daemon/PROTOCOL.md`.** The rule above covers what clients read.
- **The committed JSON Schema (`contracts/protocol/`) and TypeScript bindings are regenerated after the Rust change**
  with `OCTOBOARD_WRITE_BINDINGS=1 pnpm test:rust -p octoboardd protocol::schema` and `... bindings`. The same tests
  fail when the files are stale.
- **A wire type derives `JsonSchema` beside `TS` (both under `cfg(test)`)**, and a doc comment on the type or its field
  is the description native clients read. A field whose Rust type says less than the protocol is narrowed in both
  renderings; `protocol::crosscheck` fails when only one is.
- **Generated wire types are imported only through `packages/ui/src/protocol.ts`**, which imports nothing but
  `protocol.generated` (enforced by `GENERATED_PROTOCOL` in `packages/ui/eslint.layers.mjs`). A UI rule about a wire
  value (a predicate, a list of codes, a parser of a cause) goes in the layer that owns it, not in `protocol.ts`.
- **A new request names its reply in `REPLY_TYPES` (`packages/ui/src/requests.ts`)**; the type check fails until it
  does. Requests go through `Daemon.request`; a stand-in for the daemon (fixture daemon, preview, `testDaemon`) wraps
  its handler in `looseRequestSender`.
- **Structure goes in tool arguments, natural language in content.** Agents are not asked to emit JSON in reply text;
  fields a program acts on are arguments, validated against a schema.

## The envelope and field conventions

The envelope, the tri-state update fields, instants, the canonical wire form of paths, browse requests and the key are
specified in `apps/daemon/PROTOCOL.md`, "What every client implements"; this doc does not restate them. A change to a
request or a field keeps them (no request field named `id`; a new update field is tri-state in Rust, the schema and the
bindings). What goes beyond it:

- **A request answered with `ack` and then events (the content search) goes through `Lane::submit_stream`.** Every
  event carries the client's correlation field (`search`), whose length is bounded and refused before the `ack`; after
  the `ack` the request ends with its last event, or with nothing when superseded or disconnected, and never with an
  `error` carrying its id (why: the `ack` already answered the id, and a client takes one answer per id). A streamed
  request holds its read slot for as long as any of its threads read. Where it gives the slot back while it waits on
  the reply queue, its readers pause until the slot is taken back.
- **Derived state reaches clients as an event of its own carrying the whole record**, replayed inside `snapshot`
  (console and project state), or, for one session, as a `Session` field the store never reads or writes, filled in by
  `AppState::wire_session`, through which every `Session` record a client receives passes. Why: every `*_upserted`
  event carries the whole record, so a field some write path has to fill is blanked at the next upsert. Where the state
  is held is in `docs/conventions/daemon.md`, "State".
- **A record a form edits reaches clients only through `AppState::publish_settings` (or `broadcast_settings`),
  `publish_console` and `publish_project`**, which read it under the publication lock and stamp its field versions;
  sending those events through `AppState::broadcast` fails a debug assertion.
- **A new field of a form-edited record gets a versioned path in the same change** (`src/state/field_versions/paths.rs`;
  for the settings, `SETTINGS_FIELDS` and the flag in `SettingsChanges::paths` at the same position), and its control
  sends `base_versions` for it. Why: without the path everything compiles and passes, and the field is written
  unchecked, so two clients overwrite each other. Add it to the versions in `packages/ui/src/fixtures/builders.ts` too.
  A provider setting needs none (its path comes from its descriptor).
- **A write that takes `base_versions` goes through `AppState::checked_write` with a `RecordKey`** and holds its guard
  until the change is published. In the UI, a settings control that saves a daemon-versioned field sends its write
  through `useSettingsWrite` (code outside components: `createSettingsWriter`); a form dialog for a console, a project
  or an account uses `useCheckedEdit` (`docs/conventions/ui-components.md`, "Dialogs and fields"); a form field's path
  is one of the daemon's versioned paths.
- **A setting whose effect happens on the daemon's machine, and that not every such machine can carry out, is offered by
  the daemon's host facts** (`snapshot.host`; add the fact to `HostInfo` in `src/protocol/host.rs`), never by the
  client's `PlatformAdapter` capabilities. Why: the window may run on another machine than the daemon, and a browser
  client has no native capabilities yet must still offer it. Not: copying the platform gating of the client's own
  settings (launch at login, the menu bar icon) onto it.

## Names

Requests, replies, events and MCP tools follow `apps/daemon/PROTOCOL.md`, "Names" (one name per operation, the verb
vocabulary, the field names, the older exceptions). What goes beyond it: **settle an MCP tool's name when the tool is
added**, because released names are frozen; a tool that runs the same coordinator function as a control request carries
its name exactly.

## MCP tools and the MCP contract

- **The MCP server key `octoboard` (`SERVER_KEY`), the qualified tool names (`mcp__octoboard__<tool>`,
  `octoboard__<tool>` for Grok Build) and the `{"tool", "arguments"}` call shape are frozen.** Role descriptions and
  notices name a tool qualified as the receiving session's agent shows it; the console instruction file and tool
  descriptions use bare names.
- **A new MCP tool is a catalogue entry (`src/session_tools/catalogue/`), a dispatch arm (`src/mcp/dispatch.rs`) and a
  file in `src/mcp/tools/`**, calling the coordinator or reporting function the control socket uses, never
  reimplementing it.
- **A tool that waits in front of the user and must be withdrawn on cancel is named in `WITHDRAWN_ON_CANCEL`**; one that
  runs through `session_request::run_to_the_end` so its caller is told the outcome by message goes into
  `runs_on_the_call` (both in `src/session_tools/catalogue/mod.rs`). Why: otherwise they silently lose their cancel
  path.
- **Judge a tool from the agent's side.** An agent knows only the tool descriptions, its role text and the results, so a
  tool's description matches its real result word by word (every field it names is returned, nothing it tells the agent
  to use is data no tool gives it), and its parameters and result match the same operation in the UI and
  `apps/daemon/PROTOCOL.md`. Check a change by walking the descriptions and role texts alone through what agents do
  (taking over sessions, learning their own id, choosing an account, recovering from an exhausted quota).
- **A new setting the user can change (application, console or project) is exposed in the agents' settings tools in the
  same change:** the read tool returns it and the update tool takes it (`get_settings` and `update_settings` in
  `src/mcp/tools/settings.rs`, their schema in `src/session_tools/catalogue/settings.rs`, the MCP field list in
  `apps/daemon/PROTOCOL.md`). Only the rule in `docs/product/agent-settings-tools.md`, "Who reads and changes what", can
  make it the user's alone, and it goes by the Settings section the setting sits in and whether it is a permission
  setting. A setting the tools leave out is not a precedent for another; when unsure, ask.
- **A new report-page component kind** is a type and a `KINDS` row in `src/protocol/page_content/` (with the accessor
  arms, and `rules.rs` for inputs and submitters) and a topic in `src/session_tools/page_guide.rs`; the UI's
  `report/page/registry.tsx` then fails type checking until it has a renderer. Read pages and submissions only through
  `PageContent::parse` and `check_submission`.

## Versions and compatibility

The compatibility rules (a generation only grows, what raises `GENERATION`, the frozen `hello`, what each side
tolerates) are in `apps/daemon/PROTOCOL.md`, "Versions and compatibility". What code must do to meet them:

- **Never read a request with `deny_unknown_fields`, and never add a required input within a generation.** A change that
  breaks a released generation raises `GENERATION`; `protocol::compatibility` fails against the last release's snapshot
  in `contracts/protocol/released/` (refreshed by `release:version`) until it does.
- **Raise a version, and write its migration, only between released versions.** This holds for `GENERATION`, the
  store's `SCHEMA_VERSION` (with `MIGRATIONS`, `schema/v{N}.sql` and `PAST_SCHEMAS`) and `CONFIGURATION_VERSION`: a
  change to anything no release has carried is made in place, with no bump, no step, no kept past schema and no
  migration test, however many commits it spans. After a release the first change to what it carried raises the version
  once, and later changes before the next release fold into it. A developer database an earlier build brought to the
  amended version is moved aside, never bridged by a second step or a guard that tolerates the missing column.
- **A version is released once a `desktop-v*` tag is on a commit that carries it**; check the remote's tags
  (`git ls-remote --tags origin 'desktop-v[0-9]*'`), not only local ones. A prerelease tag counts.
- **After a release, a removed, moved or renamed field or column is bridged by the raised version and its step**, never
  by keeping the old one readable beside the new. A renamed stored value (an enum-like string such as a language tag) is
  bridged the same way: by the raised version's step in the daemon store and in the configuration document, while a
  client-side cache maps the old value on read (`RETIRED_TAGS` in `packages/ui/src/i18n/language.ts`).
- **A UI reader of a daemon output enum reads it through that value's normaliser, never the raw value.** The bindings
  type a received enum as `Received<Foo>`, open to values a newer daemon of the same generation adds, and the store
  keeps the value as sent. The normaliser is a `knownValues<Foo>` predicate (`packages/ui/src/values/knownValue.ts`)
  beside the rule that owns the value (`isKnownSessionStatus` in `sessions/sessionStatus.ts`). It yields a known value
  only where `apps/daemon/PROTOCOL.md` states a fallback, the enum has its own `unknown` member, or the reader names
  beside the predicate a neutral reading that offers, enables and counts nothing (a switch off, an `info` tone, a plain
  file, no link); otherwise the record is shown neutrally (how is in `docs/conventions/ui-standard.md`, "A value a newer
  daemon sends"), and nothing is offered, enabled or counted because of it. Not: an `as Foo` cast, a `switch` without a
  default, `Record<Foo, X>[raw]`, a message key built from the raw value, or `.find(...)!` over it. A tagged union (an
  object discriminated by `type`, `kind`, `group` or `state`) is not caught by the types: each of its readers handles an
  unknown tag by hand. Every entry point to an action follows the rule of the menu that offers it: a dialog reachable
  from an alert or a shortcut refuses what the menu hides. Error causes are the case `docs/conventions/error-codes.md`
  sets out for `params.cause`. Why: one unguarded reader throws during render, and the single error boundary takes the
  whole window down.

## Store schema

- **A new table or column while the schema version is unreleased changes `SCHEMA` (`src/store/schema.rs`) in place**;
  changing a released schema raises `SCHEMA_VERSION`, appends to `MIGRATIONS` and keeps `src/store/schema/v{N}.sql`.
  Edit the file of the table's family.
- **What belongs to one provider of a console or a project is a row of a child table keyed by the owner and the
  provider** (`console_providers`, `console_launches`, `project_providers`), not a column. The exception is the JSON
  `compatibility` column, keyed by setting id, on `console_launches`, `project_providers` and `sessions`, so a new
  overridable setting needs no column. Rows of a provider this build does not register are kept and left out of
  loads.
- **A plain boolean application setting is one line of the `bool_settings!` table** (`src/store/settings.rs`).

## The configuration document

- **A setting of General, Appearance, Git, Agent behavior, Keyboard shortcuts, Cleanup or Agent accounts travels in a
  configuration export**, unless turning it on itself changes files outside `~/.octoboard/`. A path value, a permission
  setting or a setting hidden from agents is no reason to leave it out.
- **An application setting the daemon holds is a field of `DocumentSettings`** (`src/protocol/configuration.rs`), filled
  in `config_document/export.rs` and inspected and applied in `config_document/import/mod.rs`; an Agent accounts setting
  is in `DocumentAgentSettings` and `InspectedAgentSettings` (`config_document/import/accounts.rs`), never in
  `DocumentSettings`.
- **A setting only the client holds is an optional `DocumentSettings` field the daemon neither builds nor reads**,
  handled in `packages/ui/src/configuration/clientSettings.ts`, added in `heldClientSettings`, applied only from the
  import reply's `settings_applied`; name it in the `clientLines(...)` of its card in `SettingsGroups.tsx`, or the
  import review leaves it out silently.
- **A field added to the document is an `Option` with `#[serde(default)]`, applied only when present**, unless the
  current `CONFIGURATION_VERSION` is in no release and its upgrade step writes it into every older document. Moving,
  renaming or removing a field goes by the version rule above.
- **A switch the daemon holds gets its `COPY` entry** in `packages/ui/src/configuration/booleanSettings.ts` (the type
  check demands it) and is drawn by `DaemonSwitchSetting`, never a switch of its own.
- **A `COPY` entry carries the switch's section and `order`, its settings-page label and on / off descriptions, an
  optional help tip, and a `consequence`:** a `configuration.import.settings.consequence.<name>` message in every
  catalog saying what turning it on then does, or `null` only where the import line would say no more than "turns on".
  The import review, the import result and the settings page all render from the entry.
- **A switch with rows that depend on it is a component that renders `DaemonSwitchSetting` and adds the rows**
  (`packages/ui/src/settings/KeepAwakeSetting.tsx`), not a switch of its own.
- **A setting that is not a switch gets its own line in the card the import review and result share:**
  `configuration/SettingsGroups.tsx` for an application setting, `configuration/AgentSettingsCard.tsx` for an Agent
  accounts setting; an optional one is shown only when the document carries it.
