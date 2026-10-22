# Error alerts

Runtime failures are presented in a dedicated **error alert** that requires the user's acknowledgement. This applies
to actions from menus and dialogs, clipboard and file operations, daemon and connection failures reported as errors,
session launch failures, and failed pages, panes or viewers. Errors do not use timed toasts or an action-failure line
inside the originating dialog. Field-specific validation remains under the affected field; see "Losing the daemon
connection" in `docs/product/application-lifecycle.md`. Notices that are not errors may use toasts; see
`docs/product/toasts.md`.

The error alert uses the standard danger icon and a danger acknowledgement button. It appears above any open dialog
and takes keyboard focus. It identifies the failure and preserves available details, including the affected
session's location when the failure carries one. It has one acknowledgement button. Escape, a click outside and a
close button cannot dismiss it, and it never dismisses itself. Acknowledgement does not retry the failed operation.
The originating dialog remains available for correcting inputs or retrying unless that operation's own contract
closes it, or a recovery action navigates Settings and closes its account or agent forms (see "Opening and closing
Settings" in `docs/product/settings.md`).

Errors are queued and normally shown one at a time in arrival order. An identical failure already waiting for
acknowledgement is not queued again: its message, affected session, original diagnostic message, recovery category
and available daemon code and parameters must all match. Pending errors survive changes between the application's
screens until acknowledged. Startup and rendering failures use the same acknowledgement flow.

When the last pending alert is acknowledged and keyboard focus has nowhere to return to — the control that had it is
gone, as when a dialog closed itself before the alert appeared — focus goes to the dialog still open, if any.
Otherwise it goes to the selected session's terminal. With no session selected, or no console at all, neither the
terminal's empty panel nor the welcome screen takes it, as neither takes focus while it is on nothing (see "With no
session selected" in `docs/product/session-terminal.md` and "Keyboard focus" in `docs/product/welcome-screen.md`):
it goes to the sidebar instead, else, when the sidebar is not shown or has no control to take it, to the rail. Each
region lands focus where `F6` would (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`).

A failed page, pane or viewer may also retain its explanation, fallback content and recovery controls after the error
alert is acknowledged. That durable state remains useful for retrying or inspecting the failure; it does not replace
the alert. Its inline notice keeps a short explanation; diagnostic blocks, lists, remedy paragraphs and recovery
controls appear outside that notice, without repeating the explanation. The acknowledgement dialog announces the
failure; the retained notice does not announce it a second time. Background states that are not reported as errors,
such as the daemon's reconnecting banner, keep their own status and recovery presentation.

## Names and diagnostic details

An error that references a console, project, session, shell terminal or account by its record uses the current name
when that record is available. The name appears as an inline name chip. If the record is missing, the supplied ID
remains visible as a technical code chip; a report page, which has no display name, is identified by its ID. Names
supplied directly by the operation remain names, even when their text happens to match another record's ID. Agent
and product names referenced in the explanation use name chips too. Each name keeps its own text direction;
technical values keep left-to-right order. Accessible labels, system notifications and other text-only surfaces
retain usable plain wording with the same names or fallback IDs.

Known raw output from Git and agent commands appears separately from the short explanation as selectable code,
including when it is the cause of a launch or trust-confirmation failure, an import failure, or a model or quota
reading failure. The output retains its line breaks, indentation and left-to-right order. Long lines scroll
horizontally, with a labelled region reachable from the keyboard. A short ordinary reason remains part of the
explanation; it is not turned into a diagnostic block simply because it accompanies an error. Unrecognized daemon
messages retain the daemon's own readable wording.

A failure attached to one field stays there, with its short reason under the field and any diagnostic or recovery
content following separately. This includes project detection, the Model field's unavailable list, and conversation
lookup or refusal on Session ID. These field-bound reasons do not open an additional acknowledgement dialog.

## Recovery and unexpected errors

The alert distinguishes failures with a known user remedy from unexpected failures. Both use the danger presentation
and require acknowledgement.

**An actionable failure** includes a specific next step alongside the original reason. References to controls such as
Retry, Browse and Unlock appear as inline chips rather than quoted text. For example:

- A full disk asks the user to free space on the disk used by Octoboard and the project, then retry.
- A locked session asks the user to choose Unlock in that session's action menu, then retry.
- An invalid or unavailable path asks the user to correct it or browse to an existing, accessible location.
- An account name already in use asks for an unused name for that agent; an occupied destination asks for a different
  destination while preserving the existing folder.
- A clipboard failure explains granting clipboard access or using the terminal's keyboard copy and paste shortcuts.

Known domain failures, the causes the daemon names for some of them (such as why a session's conversation could not
be moved to another account; see "When a switch fails" in `docs/product/switching-session-accounts.md`), and
recognized operating-system causes provide the remedy; a recognized operating-system cause takes precedence over the
others, except for a switch's copy that failed and could not put back an older item: that is classified by the item
put aside, and the operating-system cause's remedy is given as well. A failure whose named cause is not one the window
knows stays unexpected. An unfamiliar failure does not get generic retry advice merely because it occurred during a
familiar operation.

**A remedy that lives in Settings' Agent accounts section has a button** in the footer, a secondary action before the
acknowledgement button, instead of the step naming where to go:

| Failure | Button | What it opens |
|---|---|---|
| The agent is not added (`agent_not_added`) | **Add agent…** | Settings on Agent accounts, with the Add agent dialog in front (see "Adding an agent" in `docs/product/agent-accounts.md`) |
| An account problem (`unknown_account`, `grok_home_not_initialized`) | **Manage accounts…** | Settings on Agent accounts |
| The added agents not being as the change expected (`agent_already_added`, `agent_order_invalid`) | **Manage accounts…** | Settings on Agent accounts |

Pressing the button acknowledges the alert and opens Settings there, above whatever dialog was open; when Settings is
already open, it moves to that section instead (see "Opening and closing Settings" in `docs/product/settings.md`). The
step in the alert's body then says only what to do, such as adding the agent and trying again.

**A plain alert** says only what happened, with no next step and nothing but its acknowledgement button. It is used
when a dialog closes itself because what it would choose from went away while it was open (see "Opening a session" in
`docs/product/sessions.md` and "Handing off from the window" in `docs/product/handing-off-sessions.md`).

**An unexpected failure** explains that it may be a bug the user cannot resolve in the current interface. The alert
keeps the error explanation and offers expandable diagnostic details, **Error details**, when the original message or
stack trace adds information to its summary. This includes diagnostic strings and errors from another rendering
context, even when they have no stack trace. It asks the user to take a screenshot and file an Issue describing the
steps that led to the failure.

**Report issue** is a secondary action in the footer, before the acknowledgement button, with a trailing
external-link icon. The guidance names it with a non-interactive inline chip; only the footer action is a link. It
opens the explicit `issueUrl` configured in `config/app.json` when the user activates it. Repeated activation is
blocked while opening is pending. It does not create an Issue or attach diagnostics or a screenshot automatically.
The error stays open until acknowledged.
If opening the page fails, a separate actionable error immediately appears above the original error, with the
address displayed as a selectable chip and instructions to open it manually. Acknowledging that failure returns to
the original pending error. Where the desktop platform cannot open external links, the guidance includes the
address as a selectable chip for manual use.

**A batch of failures** uses one acknowledgement dialog listing each failed item. Where the remedies stand depends on
the operation:

- **A configuration import** puts recovery guidance beside each failed item; each actionable item keeps its own remedy
  (see `docs/product/configuration-export-and-import.md`).
- **A set of account switches** gives each failed session only its short reason, and says each remedy once after the
  list: a code's own remedy once for that code, and one sentence per kind of remedy for the rest (see "The alert for
  failed switches" in `docs/product/switching-session-accounts.md`).

If any item is unexpected, the dialog adds one shared screenshot and Issue-reporting instruction and the same Issue
entry or manual address described above; it does not replace the other items' remedies with a common instruction.
The footer carries a remedy's button (see above) only when every failure in the batch has that same remedy. A batch
whose items have raw text of their own, such as a set of account switches, sets **Error details** apart by item, a
heading naming the item over its own block, in place of the details worked out from the errors: it shows the
disclosure whether or not a failure is unexpected, and leaves out an item whose raw text adds nothing to its reason.
When no item of such a batch has raw text, the alert has no **Error details** at all, even when a failure is
unexpected. The result or retry controls remain available after acknowledgement under the operation's own rules.
