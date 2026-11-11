# Notification settings

Settings' **Notifications** section (see "Notifications" in `docs/product/settings.md`): whether Octoboard may show a
system notification when a session starts waiting for the user (see "The raised hand" in
`docs/product/session-statuses.md`), together with the rail's **Turn on notifications** button, which asks a browser the
same thing.

## What the Notifications row shows

The section's one row says what the current state is:

| Client | What the row shows |
|---|---|
| The macOS application | That Octoboard has no notification switch of its own: macOS manages its notifications, so they are turned off, or the way they appear changed, in System Settings. Beside it is an **Open System Settings** button (see "Opening the system's notification settings" below). No permission prompt is ever shown. |
| The Windows application | The same, with Windows and its Settings app in place of macOS and System Settings, and an **Open Windows Settings** button (see "Opening the system's notification settings" below). |
| A browser, not yet decided | That notifications are not enabled yet, with an **Enable** button that asks the browser. |
| A browser, allowed | That they are allowed. |
| A browser, blocked | That they are blocked, and that they are allowed again from the browser's own settings for the site. |
| A client with no notifications at all | That notifications are not available there. |

## Asking a browser

A browser is asked only from a press of the user's, never on its own: a browser ignores or refuses an ask made
otherwise. Besides the Enable button here, a browser that has not decided yet gets the same ask as the **Turn on
notifications** button on the rail: a bell icon with a small dot on it, shown only while the browser's answer is
undecided (see "The rail" in `docs/product/window-layout.md`). It cannot be dismissed; pressing it asks the
browser. Once the browser has answered, whichever the answer, the bell is gone, and answering from either place
updates the other. The desktop application never shows the bell, since it asks nothing. A browser's state is read
again whenever the window regains focus, since it can be changed in the browser's own settings meanwhile.

## Opening the system's notification settings

In the macOS application, **Open System Settings** opens Octoboard's own entry in the system's Notifications settings:
in System Settings on macOS 13 and later, in System Preferences' Notifications pane on macOS 11 and 12. When the system
does not open it, an error alert titled "Could not open System Settings" says that System Settings did not open and to
open it by hand and choose Notifications; it has only its acknowledgement button, with no remedy button and no request
to report an issue (see "The error alert" in `docs/product/error-alerts.md`).

In the Windows application, **Open Windows Settings** opens the Notifications page of Windows' Settings (System >
Notifications), which lists every application's notifications, Octoboard's among them. When it does not open, an error
alert titled "Could not open Settings" says that Settings did not open and to open it by hand and choose Notifications,
with the same buttons as macOS's.
