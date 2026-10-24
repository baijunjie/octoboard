# Logging an account in

Signing an agent account in from Settings' **Agent accounts** section, for the default account and the accounts the
user owns alike. The section itself, its accounts table and the account settings dialog, where Log in sits, are in
`docs/product/agent-accounts.md`.

## Where Log in appears

**Log in** is always in an account's actions menu, whatever its login state, so an account that is logged in can be
logged in again; it is unavailable there while a login of the account runs. For an account known to be signed out,
Log in also appears as an icon button directly after its **Not logged in** badge — in the Status column of the
accounts table, and in the heading of its account settings dialog (see "The accounts table" and "The account settings dialog" in
`docs/product/agent-accounts.md`). The icon button's tooltip is "Log in" and the name announced for it names the
account; while a login of the account runs it is unavailable, its tooltip and description saying the account is
logging in. This applies to the default account and the accounts the user owns alike.

## How each agent signs in

Log in signs the account in with its agent's own CLI, run by the daemon in the background against the account's config
directory (for the default account, the directory the user's login shell gives the agent), with no terminal and no
window. Octoboard never reads or writes the credential itself. Each agent signs in its own way:

| Agent | What the user sees |
|---|---|
| Claude Code | Octoboard opens the authorization page in the browser. Authorizing there completes the login. |
| Codex | Octoboard creates the account's config directory first if it does not exist, then opens the authorization page in the browser. Only one Codex login runs at a time, across all accounts. |
| Grok Build | Grok Build opens the authorization page in the browser itself, and Octoboard does not open it again. The login dialog shows the code to confirm on that page. |

Octoboard opens the page once, as soon as the agent has given it, and only in the window where the login was started,
whether or not Settings is still open there. Opening it in the browser failing is shown in an error alert with the
address to open by hand.

## The login dialog

Log in opens a modal dialog titled for the agent and the account ("Log in to Codex account Work") and starts the
login at once. While the login runs, the dialog shows a spinner and says that the account is logging in, then that it
is waiting for the user to authorize in the browser — for Grok Build, with the code to confirm instead. Its buttons
are **Open the page again**, while the page's address is known, and **Cancel login**. While the login runs, what the
dialog says is announced to assistive technology as it changes. Its first announcement comes about 1.5 seconds after
the dialog opens, or after **Log in again** (below) starts a new login, so that it follows the announcement of the
dialog itself and of the button focus is on.

**Closing the dialog cancels the login**: Cancel login, Escape and the dialog's close button all end a login that is
still running. Only one login can be followed from Settings at a time, since the dialog is modal; the daemon itself
runs at most one login per account, and one Codex login at a time across all accounts. Log in is unavailable for an
account whose login is running.

Where keyboard focus goes once the dialog has closed depends on what opened it. When a Log in icon button opened it,
a login that succeeded puts focus on a control that stays, since the icon button goes away once the account reads as
logged in: the row's actions menu button for the table's, the first switch of the account settings dialog for the dialog's.
When the dialog closes any other way — Cancel login, Escape, the close button, Close after a failure — focus goes back
to the icon button once it is available again, which is after the daemon has answered the cancel, or to that same
control if the button has gone by then or the login turns out to have succeeded after all. When Log in in the actions
menu opened it, focus goes back to the row's actions menu button.

When the Agent accounts section opens while a login is already running — after a reload, or started in another window
— the dialog opens on that login (on one of them, when several are running), without starting another, so it can be
followed or cancelled there.

## How a login ends

A login ends in one of these:

- **Logged in**: the agent confirms the account is signed in. The dialog closes by itself, the account's quota is read
  again at once, and the account's login badge then shows the result (see "The accounts table" in
  `docs/product/agent-accounts.md`). Assistive technology is told once, naming the account, that it is logged in, about
  1.5 seconds after the success; the dialog closes and focus returns to Settings right after the success, so by then
  both have happened. While the user is away from the window — it is minimized or hidden, or another application, such
  as the browser they authorized in, is in front — the announcement waits, and comes about 1.5 seconds after they are
  back in it. A success that had already happened when the section or the agent's tab is shown again is not announced.
- **Could not log in**: the dialog stays open, says the login to the account did not go through and gives the agent's
  reason. When the CLI said it succeeded but the agent still reports the account not signed in, the reason says so.
- **Timed out**, after 10 minutes of waiting for the user to authorize: the dialog stays open and says the login timed
  out.
- **Cancelled**, including from another window: the dialog closes.
- **Lost**, when the daemon restarts or the connection is re-established while the login runs: the dialog closes.

A login that cannot start is shown the same way as one that could not log in, in the dialog with the daemon's reason:
for example while another Codex account is logging in. A start refused because this account's login is already
running is not shown as a failure; the dialog takes up that login instead. After a failure, a timeout or a refusal,
the dialog offers **Log in again**, which starts a new login in the same dialog, and **Close**.

Whatever the outcome, the account's quota is read again within about 15 seconds (see "When quotas are read" in
`docs/product/account-quotas.md`).
