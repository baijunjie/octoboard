# 01 Storing and editing agent tags

> Goal: the user can put tags on each added agent in Settings, and the daemon keeps them application-wide.
> Completion criteria:
> - Tags set on an agent in one window appear in another at once and survive a daemon restart.
> - A tag already used on any added agent is offered for the others.
> - Empty and case-insensitive duplicate tags are dropped as for project tags.

## Technical design

- [ ] The tags of each agent, stored by the daemon with the application settings, sent in the connect snapshot and
      broadcast on change.
- [ ] A request to set an agent's tags, normalised by the project-tag rules.
- [ ] A Tags row in the agent's tab, placed and offering tags as the overview sets out.

## Notes for the developer

- **Reusable capabilities**:
  - The project Tags field and the picked-tag appearance.
  - The project-tag normalisation rules.
  - The per-agent settings rows in the agent's tab.
  - The application settings storage and broadcast.
- **Development notes**:
  - A new application setting normally joins the agent settings tools in the same change. Whether this one does is
    open (see the overview).
  - The field follows the UI standard and the copy conventions. The new row's states go into the UI state gallery.
- **Reference docs**: `docs/product/project-tags.md`, `docs/product/per-agent-settings.md`,
  `docs/product/agent-accounts.md` ("Added agents", "The accounts table"), `docs/product/settings.md`,
  `docs/product/agent-settings-tools.md` ("Who reads and changes what"),
  `docs/product/configuration-export-and-import.md`, `apps/daemon/PROTOCOL.md`.
