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
close button cannot dismiss it, and it never dismisses itself. Acknowledgement does not retry the failed operation. The originating dialog remains available for correcting inputs or retrying
unless that operation's own contract closes it.

Errors are queued and normally shown one at a time in arrival order. An identical failure already waiting for
acknowledgement is not queued again: its message, affected session, original diagnostic message, recovery category
and available daemon code and parameters must all match. Pending errors survive changes between the application's
screens until acknowledged. Startup and rendering failures use the same acknowledgement flow.

A failed page, pane or viewer may also retain its explanation, fallback content and recovery controls after the error
alert is acknowledged. That durable state remains useful for retrying or inspecting the failure; it does not replace
the alert. Background states that are not reported as errors, such as the daemon's reconnecting banner, keep their
own status and recovery presentation.

## Recovery and unexpected errors

The alert distinguishes failures with a known user remedy from unexpected failures. Both use the danger presentation
and require acknowledgement.

**An actionable failure** includes a specific next step alongside the original reason. References to Retry, Browse and the Settings sections
appear as inline chips rather than quoted text. For example:

- A full disk asks the user to free space on the disk used by Octoboard and the project, then retry.
- A locked session asks the user to choose Unlock in that session's action menu, then retry.
- An invalid or unavailable path asks the user to correct it or browse to an existing, accessible location.
- An account name already in use asks for an unused name for that agent; an occupied destination asks for a different
  destination while preserving the existing folder.
- A clipboard failure explains granting clipboard access or using the terminal's keyboard copy and paste shortcuts.

Known domain failures and recognized operating-system causes provide the remedy. An unfamiliar failure does not get
generic retry advice merely because it occurred during a familiar operation.

**An unexpected failure** explains that it may be a bug the user cannot resolve in the current interface. The alert
keeps the error explanation and offers expandable diagnostic details when the original message or stack trace adds
information to its summary. This includes diagnostic strings and errors from another rendering context, even when
they have no stack trace. It asks the user to take a screenshot and file an Issue describing the steps that led to
the failure.

**Report issue** is a secondary action in the footer, before the acknowledgement button, with a trailing
external-link icon. The guidance names it with a non-interactive inline chip; only the footer action is a link. It
opens the explicit `issueUrl` configured in `config/app.json` when the user activates it. Repeated activation is
blocked while opening is pending. It does not create an Issue or attach diagnostics or a screenshot automatically.
The error stays open until acknowledged.
If opening the page fails, a separate actionable error immediately appears above the original error, with the
address displayed as a selectable chip and instructions to open it manually. Acknowledging that failure returns to
the original pending error. Where the desktop platform cannot open external links, the guidance includes the
address as a selectable chip for manual use.

**A batch of failures**, such as account switches or a configuration import, uses one acknowledgement dialog with
recovery guidance beside each failed item. Each actionable item keeps its own remedy. If any item is unexpected,
the dialog adds one shared screenshot and Issue-reporting instruction and the same Issue entry or manual address
described above; it does not replace the other items' remedies with a common instruction. The result or retry controls remain available after
acknowledgement under the operation's own rules.
