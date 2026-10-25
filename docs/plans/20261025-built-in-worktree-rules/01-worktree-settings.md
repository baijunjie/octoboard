# 01 Worktree settings

> Goal: the four worktree settings exist application-wide and per project, and can be read and changed everywhere
> application settings can.
> Completion criteria: daemon tests show the global defaults (on, on, ask, on), a project override per item that falls
> back to the global value when following it, the effective value resolved for a project, the settings carried through
> the protocol, the configuration export and import, and the agents' settings tools; nothing in the UI yet.

## Technical design

- [ ] Global settings: use worktrees (bool), rebase before merging (bool), delivery (`ask` / `merge` /
      `pull_request`), revert gate (bool), with the defaults in the overview.
- [ ] Per-project override: each of the four items either follows the global value or holds its own.
- [ ] One resolution of the effective settings for a project, used by every consumer (02, 04).
- [ ] Protocol: read and change the global settings and a project's override; documented in the daemon protocol
      reference.
- [ ] Configuration export and import carry the global settings and each project's override.
- [ ] Agents' settings tools: console sessions read and change the global settings and a project's override as they do
      the other application and project settings; `get_project` returns the project's override and effective values.

## Notes for the developer

- **Reusable capabilities**: the application settings store and the path **Automatically pull repositories** takes
  through protocol, configuration export and import, and the settings tools; the project record and its editable
  fields.
- **Development notes**: every application setting that is not user-only goes through the agents' settings tools
  too; a new setting is added to the configuration document's versioned format.
- **Reference docs**: `docs/product/settings.md`, `docs/product/projects.md`, `docs/product/agent-settings-tools.md`,
  `docs/product/configuration-export-and-import.md`, `apps/daemon/PROTOCOL.md`.
