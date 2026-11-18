# Controls unavailable for a reason

What a button, a switch or a checkbox looks like and does while it cannot be used for a reason the user needs to
learn, and where that reason is said.

## How such a control behaves

Such a control is **unavailable** rather than disabled: it stays where it is, in the Tab order and under the pointer,
so that its reason can be reached.

- **It looks faded** and shows no hover or press feedback. A checkbox's box alone is faded; its label keeps its usual
  look.
- **It keeps keyboard focus**: it can be tabbed to, and it keeps focus as it becomes unavailable or available again.
  Assistive technology is told it is unavailable, and its accessible name stays the action it stands for.
- **It does not respond**: a click, `Enter` or `Space` does nothing — no action, no alert, no confirmation, no change
  of value, and focus is not sent anywhere else — and a tap only opens the reason (below). A checkbox stays as it is
  shown.
- **Its tooltip is the reason**, shown on pointer rest and on keyboard focus by the rules in "How a tooltip behaves" in
  `docs/product/labels-and-tooltips.md`. While the control is unavailable the reason takes the place of the control's
  own tooltip, as plain text with no heading above it. For a switch or a checkbox the tooltip belongs to its track or
  box: resting the pointer on the box opens it, resting it on a checkbox's label does not, and keyboard focus on the
  control opens it too.
- **The reason is also its description** for assistive technology, in place of any other description while the control
  is unavailable.
- **On a touch screen a tap opens the reason** in the touch popover (see "How a tooltip behaves" in
  `docs/product/labels-and-tooltips.md`), still with no action and no alert; a tap on the same control while it is
  available runs its action and opens nothing.
- **The reason is said nowhere else**: not as text beside, under or after the control. Text that describes the state
  itself is not such a repeat and stays — the update notice's, or the version line in Settings' Updates section, saying
  that the new version is installed and to restart when the agent update finishes (see "What the browser shows" in
  `docs/product/server-form-updates.md` and "The Version row" in `docs/product/update-settings.md`).

A control that is unavailable only for as long as its own request is unanswered, with no reason to tell, looks and
behaves the same, with no reason as its tooltip or description. A control that is simply disabled — taken out of the
Tab order, as every **+** that opens a session is while no agent can open one — is not one of these.

**An option in a select or an item in a menu that cannot be chosen** is the exception: its reason is secondary text
within the option or item, under its name, and not a tooltip. Choosing it does nothing either (see "Names too long for
their space" in `docs/product/labels-and-tooltips.md`).

## Where it applies

- **Keep** on a Processes panel card whose tree cannot be kept (see "Stop and Keep" in
  `docs/product/processes-panel.md`), and the checkbox of such a tree in an archive confirmation (see "Processes in an
  archive confirmation" in `docs/product/background-processes.md`).
- A sidebar **+** in its quick form while nothing can be opened with defaults (see "Opening a session with defaults" in
  `docs/product/opening-sessions.md`).
- **Restart now** on the rail, in the update notice and in Settings while an update of an agent's CLI runs (see
  "Restarting into the update" in `docs/product/server-form-updates.md`).
- **Import** in a configuration import's review while the file could not be read again (see "When settings change
  during the review" in `docs/product/configuration-export-and-import-dialogs.md`).
- **Open in the agent's app** on an archive view row for a session with no conversation yet (see "When it can be used"
  in `docs/product/opening-a-session-in-its-agents-app.md`).
- An account's **Share default configuration** switch while it cannot be turned (see "Sharing the default
  configuration" in `docs/product/agent-accounts.md`).
- The **Log in** icon button in the accounts table while a login of the account runs (see
  `docs/product/logging-accounts-in.md`).
