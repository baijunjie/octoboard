# Adding a user setting

## A new setting goes into the agents' settings tools unless only the user may change it

When you add a setting the user can change — application-wide, a console's or a project's — expose it to agents in
the same change: the read tool returns it and the update tool takes it (for an application setting, `get_settings` and
`update_settings` in `apps/daemon/src/mcp/tools/settings.rs`, their schema and description in
`apps/daemon/src/session_tools/catalogue/settings.rs`, and the MCP field list in `apps/daemon/PROTOCOL.md`). Leave it
out of the update tool only when the rule in `docs/product/agent-settings-tools.md`, section "Who reads and changes
what", makes it the user's alone, and out of the read tool too only where that section says so. That rule goes by which
section of Settings the setting sits in and whether it is a permission setting, so settle where the setting goes in
Settings before deciding. If you are unsure, ask rather than leave it out.

Do not take a setting the tools leave out as precedent on its own: settings are not opt-in to the tools, and one is left
out only because that rule makes it the user's alone.

## A new application setting travels in the settings region of a configuration export

Every application setting is carried by any export that includes the application settings region — chosen on its own,
whatever consoles and accounts the export holds — and applied by an import that is asked to apply settings, except one
that is only valid on this machine, such as a path like the default clone directory or a system permission, since it
would mean nothing elsewhere (`docs/product/configuration-export-and-import.md`, sections "The application settings an
export carries" and "What is not configuration and never travels"). If you are unsure whether a new setting is valid
only on this machine, ask. Whether the agents' settings tools may read or change a setting is a separate decision and
no reason to leave it out of the export: the settings hidden from agents because they let them act without asking or
spend quota (approving requests automatically, starting usage windows automatically) travel all the same. So a new
application setting that is not machine-local is added, in the same change, to:

- `DocumentSettings` in `apps/daemon/src/protocol/configuration.rs`, filled in `build` in
  `apps/daemon/src/config_document/export.rs` beside the others, which are written only when `Selection.settings` asks
  for the settings region. Until the first release it is a required field (no `Option`, no `#[serde(default)]`), since
  every export writes it and there is no older document to read. From the first release on, a setting added without
  raising `CONFIGURATION_VERSION` is an `Option` with `#[serde(default)]`, applied by an import only when present,
  because an export from a released build lacks it and a required field would make that export unreadable; otherwise
  raise `CONFIGURATION_VERSION` and add the step that upgrades the previous version (its doc comment in
  `protocol/configuration.rs`);
- `InspectedSettings` in `apps/daemon/src/protocol/configuration.rs`, with the document's value and the current one,
  filled by the import inspection in `apps/daemon/src/config_document/import/mod.rs`, and the
  `coordinator::settings::update_settings` call there that applies the document's settings;
- the window: its fields in `DocumentSettings` and `InspectedSettings` come from regenerating
  `packages/ui/src/protocol.generated.ts` after the Rust change (`docs/structure/daemon.md`, "Where new code goes"),
  never written by hand in the UI. For a setting that is a switch, add its entry in `COPY` in
  `packages/ui/src/configuration/booleanSettings.ts` (which the type check then demands): its settings-page label and
  on / off descriptions, an optional help tip, and a `configuration.import.settings.consequence.<name>` message saying
  what turning it on then does, added to `packages/ui/src/i18n/messages/en.ts` and `zh-Hans.ts`. The import review, the
  import result and the settings page all render from that entry; on the settings page the switch is
  `<DaemonSwitchSetting field="…" />` (`packages/ui/src/settings/DaemonSwitchSetting.tsx`), not a component of its
  own. A setting that is not a switch (like the git workflow settings or keyboard shortcuts) instead gets its own line
  in both `packages/ui/src/configuration/ImportSettings.tsx` (the review's Application settings tab) and
  `ImportResult.tsx`: always shown for a required field (like the git workflow line), shown only when the document
  carries the setting for an optional one.

## A new provider setting is declared only in its provider's schema, with its flags set by whether it is a permission setting

Applies to a setting of one agent provider (a row of that agent's tab in Settings' Agent accounts section), not to an
application, console or project setting. Where its declaration, its reading at launch and its copy go is in
`docs/structure/daemon.md` and `docs/structure/ui.md`, section "Where new code goes" of each; add no field, request,
store column, document field, MCP field or UI component for it. Decide its flags in its `SettingDescriptor`
(`apps/daemon/src/providers/mod.rs`):

- A permission setting, one that decides what the agent's sessions may do without asking the user (like
  `approval_mode` and `approval_reviewer`), is `exported: false` and `shown_to_agents: false`, and a choice of it that
  lets sessions run every action without asking has `confirm: true`. This is unlike an application setting, whose
  automatic approvals do travel: imported, a provider's permission choice would turn on silently, skipping the
  confirmation (`docs/product/configuration-export-and-import.md`, "What is not configuration and never travels";
  `docs/product/agent-settings-tools.md`, "Who reads and changes what").
- Any other is `exported: true`, unless it is valid only on this machine, and `shown_to_agents: true`. Agents never
  change a provider setting, whichever flags it has. If you are unsure whether it is a permission setting, ask.
- Its copy, `provider.<id>.setting.<setting>.heading` at the least, in every language of the provider's presentation
  module, lands in the same change: a setting without a heading is silently left out of Settings and of the import
  review and result, and no test catches it.
