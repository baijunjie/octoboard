# The Agent accounts section in Settings

> Goal: the user can see, add, rename, repoint and remove accounts from Settings.
> Completion criteria: the section lists every account grouped by agent, with each agent's default account first,
> an unavailable agent marked as not installed and an agent whose availability is not yet determined marked as
> neither; adding, renaming, repointing and removing an account the user owns all take effect and are reflected in
> other open clients; a path that is not absolute is reported in place with nothing saved, while a directory that
> does not exist yet is accepted; a name that collides within its agent is reported in place; the section is
> reachable by keyboard alone and is translated in every language the application offers. The section is listed in
> `docs/product/settings.md`.

## Handoff

From milestone 04, accounts: storage and protocol:

- **The default account's name takes part in the uniqueness comparison, and the daemon can only do half of
  it.** The comparison is the daemon's, but the default account's name is Octoboard's own word for it and is
  translated, and the daemon carries no message catalogue. Milestone 04 therefore compares against a fixed
  English key (`DEFAULT_ACCOUNT_NAME` in `apps/daemon/src/protocol.rs`), documented as a comparison key and
  not for display. **This section's add and edit forms have to compare a typed name against the translated
  default-account name of the current language as well**, and report the collision in place the way they
  report a collision with a stored account — otherwise a user whose language calls it something other than
  "Default" can create an account the picker shows twice over.

## Technical design

- [ ] A new Settings section, Agent accounts, in the dialog's list of sections.
- [ ] The section explains what an account is — a named config directory of an agent, the directory that agent keeps
      its login and conversation history in — and that an agent has to be on the user's `PATH` for its accounts to
      be usable.
- [ ] Accounts are grouped by agent, in the order the agents are listed elsewhere, each group headed by the agent's
      name and icon. An agent that is not available is headed as not installed and its accounts are shown as not
      usable; one whose availability is not yet determined is headed plainly, with no claim either way.
- [ ] Within a group, the agent's default account comes first, said to be the setup the session runs under when
      nothing is pinned, with the directory it resolves to shown. Nothing about it is editable and it cannot be
      removed — its name is Octoboard's own and it stores no path (milestone 4). The accounts the user owns follow.
- [ ] An entry shows the account's name and its directory. A path too long for the row is cut from its start, the
      way a trusted folder's path is, so the directory's own name stays visible, with the full path as the tooltip.
- [ ] Adding an account asks for the agent, a name and a directory. Editing one offers the name and the directory;
      the agent cannot be changed, since a directory belongs to one agent's layout. A directory that does not exist
      is accepted, so the form says that Octoboard does not require it to exist rather than refusing it. The form
      does not promise that the agent will create it: that is measured for Claude Code only.
- [ ] A **Grok Build** account's directory has to be a Grok home that Grok has already been run against, or its
      sessions keep neither their login nor their conversation. The form for a Grok account says so, and says that
      Octoboard checks it when a session launches rather than here.
- [ ] Removing an account asks for confirmation and says what it affects: consoles referring to it go back to the
      agent's default account, and sessions already open are not affected, since each launches from the directory it
      recorded.
- [ ] A name already taken for that agent is reported in the add and edit forms, in place, naming what it collides
      with. The default account's name counts as taken.

## Implementation plan

- [ ] Register the section in the dialog's section list, after Git and before Trusted folders.
- [ ] Build the section from the account list in the settings record and the per-agent availability the daemon
      derives, and act through the account requests from milestone 4.
- [ ] Report a path that is not absolute in the add and edit forms, in place. Do not check whether the directory
      exists.
- [ ] Bring `docs/product/settings.md` along: its list of sections is General, Git, Trusted folders, Notifications,
      and this milestone inserts one between Git and Trusted folders.
- [ ] Keep the dialog's keyboard rules: a control that disappears hands focus back to the section's own tab.
- [ ] Add every string to the message catalogues of all offered languages.

## Notes for the developer

**Reusable capabilities**

- The Trusted folders section is the closest existing model for a list of entries with a per-entry action, a path
  shown truncated from its start, and an empty state that says how an entry comes to exist.
- The Git section is the minimal end-to-end example of a section reading and writing one settings field.
- The console dialog's directory field and the directory browser already exist for choosing a directory; reuse them
  rather than offering a bare text field if the project's dialogs do.
- The agent label and icon are already shared between the UI's agent pickers.

**Development notes**

- Settings in SQLite reach the UI through the settings record; UI-only preferences live in local storage instead.
  An account is the former, while per-agent availability is derived state that arrives separately (milestone 5).
- Section order and the dialog's own keyboard behaviour are fixed by the product doc; do not invent new behaviour
  for this section.
- Milestone 8's switch submenu ends in an entry that opens Settings here, so the dialog has to be openable at a
  named section rather than only at the one it opens on.

**Reference docs**

- `docs/product/settings.md` — the dialog's structure and the existing sections.
- `docs/memory/writing-ui-components.md` and `docs/memory/verifying-the-desktop-ui.md` — UI conventions and how to
  verify a change in the running application.
- The i18n copy conventions skill, for the section's wording.
