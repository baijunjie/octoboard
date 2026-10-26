# Adding a user setting

## A new setting goes into the agents' settings tools unless only the user may change it

When you add a setting the user can change — application-wide, a console's or a project's — expose it to agents in the
same change: the read tool returns it and the update tool takes it (for an application setting, `get_settings` and
`update_settings` in `apps/daemon/src/mcp/settings.rs`, their schema and description in `apps/daemon/src/mcp/mod.rs`,
and the MCP field list in `apps/daemon/PROTOCOL.md`). Leave it out only when it falls under what must be done by the
user personally, as decided in `docs/product/agent-settings-tools.md`, section "Who reads and changes what"; if you are
unsure whether it does, ask rather than leave it out.

Do not take a neighbouring setting that the tools leave out as precedent: accounts, trusted folders and the like are
left out because they are the user's own, not because settings are opt-in to the tools.

## A new application setting travels in a full configuration export

Every application setting is carried by a full ("everything") export and applied by an import that is asked to apply
settings, except one that is only valid on this machine — a path such as the default clone directory, a system
permission, this machine's accounts — since it would mean nothing elsewhere; a single console's export carries no
application settings (`docs/product/configuration-export-and-import.md`, section "The application settings a full
export carries"). If you are unsure whether a new setting is valid only on this machine, ask. Whether the agents'
settings tools may read or change a setting is a separate decision and no reason to leave it out of the export: the
settings hidden from agents because they let them act without asking or spend quota (approving requests automatically,
starting usage windows automatically) travel all the same. So a new application setting that is not machine-local is
added, in the same change, to:

- `DocumentSettings` in `apps/daemon/src/config_export.rs`, as an `Option` with `#[serde(default)]`, so a document
  exported before the setting existed still imports and applying it leaves the setting as it is; filled in `build` for
  the full export only;
- the import inspection in `apps/daemon/src/config_import.rs`, with the document's value and the current one, and the
  `coordinator::update_settings` call that applies the document's settings;
- the window: for a setting that is a switch, one entry in `BOOLEAN_SETTINGS` in
  `packages/ui/src/configuration/importParts.tsx` (its settings-page label, its document field, and a
  `configuration.import.settings.consequence.<name>` message saying what turning it on then does, added to
  `packages/ui/src/i18n/messages/en.ts` and `zh-Hans.ts`), which both the review and the result render from; the field
  and its `current_` counterpart must exist in `DocumentSettings` and `InspectedSettings` in
  `packages/ui/src/protocol.ts`. A setting that is not a switch (like the git workflow settings or keyboard
  shortcuts) instead gets its own line in both `packages/ui/src/configuration/ImportReview.tsx` and `ImportResult.tsx`,
  shown only when the document carries the setting.
