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

## A new setting of General, Git, Agent behavior, Keyboard shortcuts or Agent accounts travels in a configuration export

Every setting of those Settings sections travels and is applied by an import, except one whose turning on itself
changes files outside `~/.octoboard/` (today only an account's sharing of the default configuration), which the user
turns on again by hand (`docs/product/configuration-export-and-import.md`, "What is not configuration and never
travels"). None of these is a reason to leave a setting out: its value is a path (the default clone directory travels,
written `~/`-relative), it lets agents act without asking or spend quota, the agents' settings tools hide it, or it has
Octoboard act on files outside `~/.octoboard/` later as it runs. If you are unsure whether turning it on itself changes
outside files, ask. In the same change, by where the setting sits:

- An application setting the daemon holds (General, Git, Agent behavior): a field of `DocumentSettings` in
  `apps/daemon/src/protocol/configuration.rs`, filled in `build` in `apps/daemon/src/config_document/export.rs`; its
  document and current values in `InspectedSettings`, filled by the inspection in
  `apps/daemon/src/config_document/import/mod.rs`, and the `coordinator::settings::update_settings` call there that
  applies the document's settings.
- A setting of the Agent accounts section: in the accounts region, `DocumentAgentSettings` and
  `InspectedAgentSettings`, inspected and applied in `apps/daemon/src/config_document/import/accounts.rs` — never in
  `DocumentSettings`.
- A setting only the client holds (kept in the client, like the appearance, the Markdown style, running in the
  background or the shortcuts): an optional field of `DocumentSettings` that the daemon neither builds nor reads and
  passes back as it came, handled on the client beside the others in
  `packages/ui/src/configuration/clientSettings.ts`. The client adds it to a document it saves and applies it to
  itself only from the import reply's `settings_applied`, never from the file it read, so it is not applied when the
  daemon failed to apply the settings.

A field added to the document is an `Option` with `#[serde(default)]`, applied by an import only when present: a
document from a released build, the beta releases included, may lack it, and a required field would make it
unreadable. It may be required only while the current `CONFIGURATION_VERSION` is in no release yet and that version's
upgrade step in `apps/daemon/src/config_document/upgrade.rs` writes it into every older document. Moving, renaming or
removing a field goes by the version rule in `apps/daemon/PROTOCOL.md`, "Configuration export", its "Version"
paragraph.

In the window, the document's fields come from regenerating `packages/ui/src/protocol.generated.ts` after the Rust
change (`docs/structure/daemon.md`, "Where new code goes"), never written by hand. A switch gets its entry in `COPY` in
`packages/ui/src/configuration/booleanSettings.ts`, which the type check demands: its Settings section and place there,
its settings-page label and on / off descriptions, an optional help tip, and a
`configuration.import.settings.consequence.<name>` message saying what turning it on then does, added to every message
catalog. The import review, the import result and the settings page all render from that entry; on the settings page
the switch is `<DaemonSwitchSetting field="…" />` (`packages/ui/src/settings/DaemonSwitchSetting.tsx`), not a
component of its own. A setting that is not a switch gets its own line in the card the review and the result share:
`packages/ui/src/configuration/SettingsGroups.tsx` for an application setting, `AgentSettingsCard.tsx` beside it for an
Agent accounts setting; an optional one is shown only when the document carries it.

## A new provider setting is declared only in its provider's schema, with its flags set by whether it is a permission setting

Applies to a setting of one agent provider (a row of that agent's tab in Settings' Agent accounts section), not to an
application, console or project setting. Where its declaration, its reading at launch and its copy go is in
`docs/structure/daemon.md` and `docs/structure/ui.md`, section "Where new code goes" of each; add no field, request,
store column, document field, MCP field or UI component for it. Decide its flags in its `SettingDescriptor`
(`apps/daemon/src/providers/mod.rs`):

- A permission setting, one that decides what the agent's sessions may do without asking the user (like
  `approval_mode` and `approval_reviewer`), is `shown_to_agents: false`, and a choice of it that lets sessions run
  every action without asking has `confirm: true`. It is still `exported: true`, like any other.
- Any setting is `exported: true` unless turning it on itself changes files outside `~/.octoboard/`, and any that is
  not a permission setting is `shown_to_agents: true`. Agents never change a provider setting, whichever flags it has.
  If you are unsure whether it is a permission setting, ask.
- Its copy, `provider.<id>.setting.<setting>.heading` at the least, in every language of the provider's presentation
  module, lands in the same change: a setting without a heading is silently left out of Settings and of the import
  review and result, and no test catches it.
