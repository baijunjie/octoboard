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

## A new portable application setting travels in a full configuration export

An application setting that describes how Octoboard should behave, rather than something about this machine or its
accounts, is carried by a full ("everything") export and applied on import; a single console's export carries no
application settings, and machine-local settings (such as the default clone directory) and account settings never
travel (`docs/product/configuration-export-and-import.md`, section "The application settings a full export carries").
So a new portable application setting is added, in the same change, to:

- `DocumentSettings` in `apps/daemon/src/config_export.rs`, as an `Option` with `#[serde(default)]`, so a document
  exported before the setting existed still imports and applying it leaves the setting as it is; filled in `build` for
  the full export only;
- the import inspection in `apps/daemon/src/config_import.rs`, with the document's value and the current one, and the
  `coordinator::update_settings` call that applies the document's settings;
- the window: a line in the apply-settings note of `packages/ui/src/configuration/ImportReview.tsx` and in the result
  of `packages/ui/src/configuration/ImportResult.tsx`, each shown only when the document carries the setting.

Do not take the default clone directory being absent from the document as precedent for a new setting: it stays out
because it names a path on one machine.
