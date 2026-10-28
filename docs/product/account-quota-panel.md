# The account quota panel on the rail

The rail's **Account usage** button, a gauge directly above Settings (see "The rail" in
`docs/product/window-layout.md`), opens a floating panel beside the button, on the content panel's side, that shows the
accounts' quotas without opening Settings. The panel is drawn on the same surface as a tooltip (see "How a tooltip
behaves" in `docs/product/labels-and-tooltips.md`), at its own width, with its content laid out to its edges.

## What the panel shows

- **One tab per agent that is added and installed and has an account to list** (below), in the order of the added
  agents (see "Added agents" in `docs/product/agent-accounts.md`), each showing the agent's icon and name, as wide as
  its content, the tabs starting at the strip's start. An agent that is not added, or that has been determined
  unavailable, has no tab; an added agent whose availability is not yet determined has one; an agent whose accounts are
  all turned off has none. Each opening starts on the tab of the selected session's agent while
  the button shows that session's indicator (see "The selected session's quota indicator" in
  `docs/product/account-quotas.md`) and the agent has a tab, otherwise on the first agent's tab; a tab chosen in the
  panel lasts only until it closes. When it opens on the selected session's agent this way, that tab's list is also
  scrolled so that the session's account — the default account for a session that names none — is the first whole row in
  view, clear of the list's top fade, as far as the list can scroll; the row is not marked in any other way. Otherwise a
  list opens at its top.
- **The selected agent's accounts**: its default account, under the name the user gave it or else "Default" (see "The
  default account" in `docs/product/agent-accounts.md`), then the accounts the user owns, leaving out every account the
  user turned off (see "Turning an account off" in `docs/product/agent-accounts.md`). The one exception is the account
  the selected session is running on — starting, working, waiting for the user or awaiting instructions: while it does,
  that account is listed even when it is turned off, with a warning-coloured **Disabled** badge after its neutral,
  bordered plan badge, and its agent keeps its tab for it. When the session stops running there, the row goes, and the
  tab with it when it was the agent's only row; keyboard focus on that tab, or inside the panel, moves to the tab now
  selected, or, with no tab left, to the panel's first button. Each shows its name, its plan badge and its quota
  beneath, in the states of "Quota states" in `docs/product/account-quotas.md`, as "How an account's quota is shown" in
  `docs/product/account-quotas.md` describes; a used-up account recedes, with a **Limit reached** badge after its plan
  badge (see "A used-up account" in `docs/product/account-quotas.md`). The name and its plan badge keep to one line, and
  a name too long for its space fades before the plan badge, as in "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`; what follows them on the name line — the Disabled and Limit reached badges, a
  failure mark, how long ago the reading was taken — moves to the next line when it does not fit beside them. The panel
  shows no directories and offers nothing to edit.
- The list of accounts is at most 384 px high, or 60% of the window's height where that is less, and the panel is as
  tall as the tallest agent's list, so choosing a tab with fewer accounts never moves the tab strip from under the
  pointer (which would close a panel opened by hover). Beyond that height a list scrolls and fades at its edges, and
  while it scrolls it is a tab stop of its own (see "Scrolling a surface from the keyboard" in
  `docs/product/scrolling-surfaces.md`).
- **With no agent that is added and installed** there are no tabs, and an empty state stands in their place. When some
  agent is installed, or not yet determined unavailable, but none of them is added, it carries the bot icon, is titled
  "No agent added", with a line saying to add an agent to see its usage there, and offers an **Add agent…** button,
  which closes the panel and opens Settings on its Agent accounts section with the Add agent dialog in front (see
  "Opening and closing Settings" in `docs/product/settings.md`). When every agent has been determined unavailable, it
  carries the gauge icon, is titled "No agent available", with a line saying an agent's usage is shown once its command
  is found on the user's `PATH`, and offers nothing to press.
- **With agents added and installed but no tab left**, every account of each being turned off, the empty state carries
  the gauge icon, is titled "No enabled account", with a line saying that disabled accounts are not shown there, and
  offers a **Manage accounts…** button, which closes the panel and opens Settings on its Agent accounts section.
- At the panel's foot, **Manage accounts…** closes the panel and opens Settings on its Agent accounts section (see
  "Opening and closing Settings" in `docs/product/settings.md`). It is left out while the panel shows "No agent added"
  or "No enabled account", whose button already leads there.

## Opening the panel

The panel opens by the input the device has, as a status mark's popover does (see "Failures, warnings and notes marked
at their point" in `docs/product/error-alerts.md`):

- **Resting the mouse on the button** for 250 ms floats the panel in; a pointer that only crosses the button shows
  nothing. The panel stays while the mouse is on the button or on the panel, and goes 150 ms after the mouse has left
  both, so crossing the gap between them does not close it. Touch and pen do not open it this way. Floating in moves no
  keyboard focus, and the window's shortcuts go on working beside it.
- **A mouse click on the button opens nothing and pins nothing**: where there is a mouse there is the hover. A click on
  the button of a panel the mouse floated in leaves it as it is.
- **A tap** (touch or pen), **assistive technology** and **Enter or Space** open it with keyboard focus moved into the
  panel, onto the selected agent's tab, or, with no tab, onto the panel's first button (Add agent… or Manage
  accounts…). A panel opened this way is modal, as a menu is: the window's shortcuts and F6 wait until it closes, and a
  tap on a floating panel turns it into this form.

**While the mouse is on the button** and the button shows the selected session's account (see "The selected session's
quota indicator" in `docs/product/account-quotas.md`) and that account's agent has a tab, the open panel makes that account stand out: every other
account's row, on every agent's tab, is dimmed by a veil of the panel's own colour, the same veil the sidebar lays over
inactive projects. The dimming lasts from then until the mouse enters the panel or the panel closes. Keyboard focus on
the button does not bring it. It is a passing cue, separate from a used-up account's look (see "A used-up account" in
`docs/product/account-quotas.md`): the button's account keeps its used-up look undimmed when it is used up, and another
used-up account is dimmed on top of its own look. Dimmed text falls below the usual contrast minimum, by decision, since
the cue is lifted as soon as the mouse enters the panel.

## Closing the panel

The panel closes:

- on **Escape**, which then goes no further: it reaches neither the agent in the terminal nor a drawer or a floating
  pane under the panel, which stay open. A panel the mouse floated in holds no focus, so it leaves the key alone while
  focus is in a terminal (the agent's or a shell's) or in a text field holding text, where the key does what it does
  there and the panel stays, and while an input method is composing. A panel opened by a tap or the keyboard holds focus
  and takes the key. With a mark's popover open inside the panel (see below), the key is that popover's where the
  popover would take it — keyboard focus is in it, or the mouse opened it and focus is not in a terminal or a text field
  holding text — which closes the popover alone, the panel staying open;
- on a **press outside** the button and the panel, which also does whatever it does where it lands (for a panel opened
  by a tap or the keyboard, the press is swallowed, as a menu's);
- when a **modal dialog or a menu opens** by any other way, such as Settings from the application menu.

After it closes by Escape or a press outside, a mouse still resting on the button does not open it again until it has
left the button and come back.

An account's failure or warning mark (see "Quota states" in `docs/product/account-quotas.md`) opens its popover from
inside the panel, by the mouse resting on it or a tap, and that popover counts as part of the panel: a press inside it,
its own opening, and the mouse crossing from the panel into it do not close the panel. How it opens and closes is in
"Failures, warnings and notes marked at their point" in `docs/product/error-alerts.md`; which of it and the panel Escape
closes is in the Escape item above.

On closing, keyboard focus goes back to what held it when the panel opened — the button, after a keyboard opening —
unless the close itself put focus somewhere else. A panel floated in by the mouse moved no focus, so closing it moves
none.

## The panel is not modal while the mouse has floated it in

A panel the mouse floated in does not take the window around it the way a menu or popover does (see "What an open
dialog, menu or popover covers" in `docs/product/window-layout.md`): while it is open the rest of the window stays
usable, and the window's keyboard shortcuts, Back and Forward, `F6` and the floating panes go on working. It is no stop
of its own in the `F6` cycle. Assistive technology is told it is a non-modal dialog named "Account usage", and the
button says whether the panel is expanded and which panel it controls. A panel opened by a tap or the keyboard is modal,
as a menu is: focus is inside it, an outside press is swallowed, and the window's shortcuts and `F6` wait until it
closes.
