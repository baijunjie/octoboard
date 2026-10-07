# Choosing an account where an agent is chosen

> Goal: the console's per-agent directory fields become account pickers, and opening a session settles the agent and
> the account in one control.
> Completion criteria: a console saved from the dialog refers to the accounts picked; a session opened with an entry
> of the grouped control launches under that agent and that account, and a session opened without touching the
> control launches under the defaults it would have used before this change — for a console that pinned nothing,
> with nothing pinned still; an unavailable agent's entries cannot be picked; the account a session runs under is
> named wherever its agent's name is already written. The controls are usable by keyboard alone and translated in
> every offered language, and `docs/product/sessions.md`'s account of opening a session matches the new control.

## Technical design

- [ ] **The console dialog.** Where it shows a directory input for each currently selected agent, it shows a picker
      of that agent's accounts instead, the agent's default account first. The rule for which agents get a row is
      unchanged: the console's console-session agent and its default agent. A reference stored for an agent that is
      not currently selected is kept as it is, as the path is today.
- [ ] **The session dialog.** One grouped list replaces the agent picker: each group is an agent, headed by its name
      and icon, and the group's entries are that agent's accounts, the default one first. Picking an entry settles
      the agent and the account together, so no combination that does not exist can be chosen.
- [ ] The grouped list's initial entry is the resolution the dialog already performs for the agent — the project's
      default, else the console's — paired with the account that agent resolves to for this console.
- [ ] An entry names the account; the agent is carried by its group heading rather than repeated in every entry.
- [ ] An unavailable agent's group is shown, labelled as not installed, with none of its entries selectable. An
      account is never unselectable for its directory being missing.
- [ ] **While availability is not yet determined** — the window at the start of a run before discovery has landed
      (milestone 5) — every group is selectable and none is labelled as not installed, so the user is never told an
      agent is missing on the strength of not having looked yet. A session opened in that window is launched and
      refused by the launch itself if the binary is not there, which is what happens today.
- [ ] **On a first-ever start** there are no stored accounts, so until discovery lands each agent's group holds its
      default account alone — the account that pins nothing, which needs no discovery to exist. Nothing is empty and
      nothing has to be waited for.
- [ ] **Where the account is shown once the dialog is closed.** No new surface is introduced for it; the account is
      named wherever the session's agent is already *named* rather than only drawn as an icon:
      - a project's focus mode, whose session cards name the agent — the only place a *running* session's agent is
        named on screen, and so the one that matters most here;
      - the archive view's rows, which name the agent beside when the session was archived. Only those rows: the
        submenus that lead to the archive view show the agent as an icon alone, and so stay as they are;
      - the description a session row gives assistive technology, and a console session's row's, both of which name
        the agent.
- [ ] A session on its agent's default account is named by that account rather than by a path. A session whose
      account has since been removed is named by the directory it recorded, or by its agent alone when it recorded
      none.
- [ ] A console session holds an account exactly as any other session does — its agent is the console's
      console-session agent — so wherever a console session's agent is named, its account is too.
- [ ] For the selected session nothing is added: the Switch account submenu of milestone 8 marks the current
      account, which is where a user checks it.

## Implementation plan

- [ ] Replace the console dialog's directory rows with account pickers, keeping the dialog's existing rule that an
      untouched field is left as it is when the console is saved.
- [ ] Replace the session dialog's agent picker with the grouped list, and send the chosen account alongside the
      chosen agent on the open request.
- [ ] Resolve the account for a session as the choice made for this session, else the console's account for that
      agent, else the agent's default account. The project contributes the agent alone, as it does today, so a
      session opened without a choice behaves as it does today.
- [ ] Carry the account through to the launch by the path milestone 4 established, changing nothing about how a
      directory reaches the agent.
- [ ] Name the account on the surfaces listed above, and nowhere that today shows the agent as an icon alone.
- [ ] Bring the product docs along:
      - `docs/product/sessions.md` — "Opening a session" lists Agent as the dialog's first field, and "Which agent a
        session uses" gives the resolution chain the account's own chain now sits beside;
      - `docs/product/sidebar.md` — it enumerates what a focus-mode session card shows, what an archive row shows,
        and what each row tells assistive technology, all three of which gain the account;
      - `docs/product/consoles-and-projects.md` — "Editing a console" and "Agent config directories" describe one
        directory input per selected agent, with the agent's default as its placeholder, which this milestone
        replaces with a picker. Milestone 4 rewrote that section in account terms; this milestone replaces the
        control it describes.
- [ ] Add every string to the message catalogues of all offered languages.

## Notes for the developer

**Reusable capabilities**

- The session dialog's existing agent picker and the console dialog's two agent pickers are the controls being
  changed; the agent label and icon are already shared.
- The agent resolution chain for a session already exists in the daemon; the account chain mirrors it rather than
  introducing a second shape.
- Milestone 5 already refuses to open a session whose agent is unavailable; the picker's non-selectable entries are
  the visible half of that rule, not a second rule.

**Development notes**

- A session's agent is fixed for its lifetime and a session record belongs to one agent. The grouped control must
  not suggest that an existing session's agent can be changed — it is a control for opening a session, and
  milestone 8's switch stays within one agent.
- Which agent a session uses, and the dialog's fields, are fixed by the product doc; this milestone changes how the
  choice is made, not what the priorities are.
- Whether the grouped drop-down should become a list with filtering once an account list grows is in the overview's
  Open section; build the drop-down first.

**Reference docs**

- `docs/product/sessions.md` — "Opening a session" and "Which agent a session uses".
- `docs/product/consoles-and-projects.md` — "Agent config directories" and "Editing a console", for the rules the
  pickers inherit.
- `docs/memory/writing-ui-components.md`, `docs/memory/verifying-the-desktop-ui.md`.
