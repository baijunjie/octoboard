# Logging an account in

Signing an agent account in from Settings' **Agent accounts** section, for the default account and the accounts the
user owns alike. The section itself and its accounts table, where Log in sits, are in
`docs/product/agent-accounts.md`.

## Where Log in appears

**Log in** is always in an account's actions menu, whatever its login state, so an account that is logged in can be
logged in again; it is unavailable there while a login of the account runs. For an account known to be signed out and
not turned off, Log in also appears as an icon button directly after its **Not logged in** badge in the Status column
of the accounts table (see "The accounts table" in `docs/product/agent-accounts.md`); a turned-off account has none
there. Neither account dialog has a Log in button. The icon button's tooltip is "Log in" and the name announced for it
names the account; while a login of the account runs it is unavailable, its tooltip and description saying the account
is logging in. This applies to the default account and the accounts the user owns alike, of an agent Octoboard can log
in; for any other agent Log in appears nowhere (see "Agents Octoboard cannot log in" below).

## How each agent signs in

Log in signs the account in with its agent's own CLI, run by the daemon in the background against the account's config
directory (for the default account, the directory the user's login shell gives the agent), with no visible terminal.
Antigravity CLI uses a private directory and reaches the system credential store from there. Octoboard never writes
the credential, though it may use it: it reads the agent's sign-in record for the
account's email (see "The account's sign-in email" in `docs/product/account-quota-readings.md`), and a GitHub Copilot
CLI token the user keeps in plain text only to hand it to a run of the CLI through its environment. Each agent signs
in its own way:

| Agent | What the user sees |
|---|---|
| Claude Code | Octoboard opens the authorization page in the browser. Authorizing there completes the login. |
| Codex | Octoboard creates the account's config directory first if it does not exist, then opens the authorization page in the browser. Only one Codex login runs at a time, across all accounts. |
| Grok Build | Grok Build opens the authorization page in the browser itself, and Octoboard does not open it again. The login dialog shows the code to confirm on that page. |
| Antigravity CLI | The CLI opens the Google authorization page. The login dialog offers the link and asks for the resulting authorization code in a masked field. |
| GitHub Copilot CLI | Octoboard displays the GitHub device code and opens the authorization page. Where the CLI cannot save into the credential store and asks to save a token in plain text, the login dialog asks for the user’s choice. Copilot saves it in plaintext only on Yes and ends the login on No. |
| OpenCode | Run `opencode auth login` on the daemon host to enter a model provider key. |
| Qwen Code | Octoboard opens Qwen Code's authenticated Web Shell in the browser. The account owner enters the model provider key there, and Qwen Code saves it. |
| Pi | For xAI, Octoboard displays a device code and opens the authorization page. For other model providers and API keys, sign in through Pi's terminal commands. |

Qwen Code’s login supports provider keys. Subscription sign-in is unavailable pending verification with a Qwen subscription or Coding Plan account. Antigravity CLI’s supported dialog route is Google OAuth. The Google Cloud project and API-key routes remain unverified.

Octoboard opens the page once, as soon as the agent has given it, and only in the window where the login was started,
whether or not Settings is still open there. Opening it in the browser failing is shown in an error alert with the
address to open by hand.

## Agents Octoboard cannot log in

Each agent the daemon serves says whether Octoboard can sign an account of it in without a terminal (its `login_flow`
capability). OpenCode has no Log in button. The measured account uses an API key entered through `opencode auth login` on the daemon host. Support for OpenCode OAuth in the dialog is deferred pending verification with an OAuth account.
A window connected to a daemon that does not send the
capability treats every agent as one it can log in. For an agent that cannot:

- **Log in is not offered**, neither in an account's actions menu nor after its **Not logged in** badge, and the daemon
  refuses a login request for it (`start_account_login`) with `provider_unsupported` (operation `sign_in`), running
  nothing.
- **Octoboard takes no API key or other credential** for it: there is no field for one anywhere. The sign-in hint
  below, and the command it names, are all the window offers, for an account that signs in with a key as for any
  other.
- **Where an account of it reads as not logged in, the window says how to log in instead** — the agent's **sign-in
  hint**: the agent's own message naming the command of its CLI that signs an account in, or, for an agent the window
  has no such message for, a generic sentence that Octoboard cannot log in this agent's accounts and that the user is to
  log in with the agent's own CLI, in the environment where sessions run, using the account's configuration directory.
  Either names that directory: a stored account's own, or the default account's as the daemon found it, else the
  agent's usual one under the home directory, written as the window writes the daemon host's paths (see "How paths are
  shown" in `docs/product/settings.md`). Where the directory is not known there is no hint, and the usual advice to log
  in, with its **Manage accounts** button, stays.

The hint stands in these places:

- in the accounts table's Status column, after the **Not logged in** badge, as a "?" help tip named for logging in to
  the account ("About logging in to Work"), whose popover holds the hint (see "The help tip" in
  `docs/product/labels-and-tooltips.md`); a turned-off account has none, as it has no badge there;
- in the account details dialog, under the account's directory (see "The account details dialog" in
  `docs/product/agent-accounts.md`);
- in the account field of the session dialog, the handoff dialog and the forms in which the user approves a session's
  request, when the account chosen is not logged in: the field says the account is not logged in, so a session cannot
  use it, followed by the hint in place of the advice to choose another account or log in;
- in the error alert of a switch confirmed onto an account found not logged in after the confirmation opened (see
  "Confirming a switch" in `docs/product/switching-session-accounts.md`), which says the same and offers no **Manage
  accounts** button;
- in the error alert for the daemon's refusal `account_not_signed_in` (see "When the agent or the account cannot open a
  session" in `docs/product/opening-sessions.md`), where the hint takes the place of the advice to log in and the alert
  offers no **Manage accounts** button; an alert that gathers several failures, such as those of a switch of several
  sessions, words that remedy with the hint too. The window finds the agent and the directory in the refusal's `agent_id` and `config_dir`,
  falling back for the default account to the directory it knows; a refusal that names no `agent_id` keeps the advice
  to log in.

## The login dialog

Log in opens a modal dialog titled "Log in" and starts the login at once. Its first line names the agent and the
account ("Logging in to Codex account Work…"). Under it is the grey spinner, alone while the login starts, then beside it
that the dialog is waiting for the user to authorize in the browser — for Grok Build, the code to confirm instead. Its
buttons are **Open the page again**, while the page's address is known, and **Cancel login**. While the login runs,
where it is is announced to assistive technology as it changes. Its first announcement comes about 1.5 seconds after
the dialog opens, or after **Log in again** (below) starts a new login, so that it follows the announcement of the
dialog itself and of the button focus is on. While the login waits on a question of the agent's own, a question dialog
stands in the login dialog's place (see "A question from the agent" below).

**Closing the dialog cancels the login**: while the login runs, Cancel login, Escape and the dialog's close button
all end it. Only one login can be followed from Settings at a time, since the dialog is modal; the daemon itself
runs at most one login per account, and one Codex login at a time across all accounts. Log in is unavailable for an
account whose login is running.

Where keyboard focus goes once the dialog has closed depends on what opened it. When a Log in icon button opened it, a
login that succeeded puts focus on the row's actions menu button, a control that stays, since the icon button goes away
once the account reads as logged in. When the dialog closes any other way — Cancel login, Escape, the close button,
Close after a failure — focus goes back to the icon button once it is available again, which is after the daemon has
answered the cancel, or to that same control if the button has gone by then: the login turned out to have succeeded
after all, or the account was turned off meanwhile. When Log in in the actions menu opened it, focus goes back to the
row's actions menu button.

When the Agent accounts section opens while a login is already running — after a reload, or started in another window —
the dialog opens on that login (on one of them, when several are running), without starting another, so it can be
followed or cancelled there. A dialog opened this way has no control to return focus to: however it closes, keyboard
focus goes to the Agent accounts tab in Settings' list of sections, its focus ring showing only when the last input was
a key (see "The focus ring after a focus move the window makes on its own" in
`docs/product/moving-focus-between-regions.md`).

## A question from the agent

While it runs, an agent's login may ask the user a question. The window uses the agent's wording for the question
and the controls that answer it. A question can ask for a yes/no choice or a secret line of text.

**A yes/no choice.** While the login waits on a choice, the login dialog gives way to a question dialog titled
"Login question", in the form of a confirmation the user did not ask for (an alert dialog, which
assistive technology is told is described by its body). Its body opens with one sentence
naming the agent and the account — that the agent needs the user's answer to finish logging in to the account — then
the question in the agent's words, any value it names (such as a path) set apart as a monospace reference. It has no
spinner, no device code and no **Open the page again**. Its footer holds exactly the two answers, the "no" answer as the
secondary button and the "yes" answer as the primary one; there is no **Cancel login** button, the close button (×) and
Escape standing for it. It opens with keyboard focus on the dialog itself, not on an answer (see "Where keyboard focus
starts in a confirmation" in `docs/product/dialogs.md`), and focus goes back to the dialog when the login moves on to
another question. While an answer is being sent its button is pending and the dialog busy: neither answer can be
pressed again, and the dialog cannot be closed, until the daemon has replied.

**A secret answer.** The dialog shows a masked text field and a button to send the answer. If the authorization page
is known, the dialog offers a link to reopen it. The field is cleared when the answer is sent. The daemon passes the
answer directly to the CLI's private terminal. It is absent from logs, saved state and updates to other windows.

**Only the user answers it.** The login waits until the user chooses an answer or submits the text. Closing the
question dialog without answering — the close button or Escape —
cancels the login, as closing the login dialog always does (see "The login dialog" above), and keyboard focus then goes
where it goes when the login dialog closes. The login's timeout (10 minutes, or the time the agent's sign-in declares)
stands still while the question is open, however long the user takes, and goes on with what was left once it is
answered or taken back; a login never times out with its question open (see "How a login ends" below).

**After an answer** the agent's login goes on: usually the running login dialog comes back, with **Cancel login**
taking keyboard focus, and the login then ends as any login does; or the agent ends the login on that answer with a
reason of its own, shown in the "Could not log in" failure alert (see "When a login fails" below). The agent may also
take its question back unanswered, and the running login dialog then comes back the same way.

**In several windows.** The question is part of the login's state, so a login dialog that takes up a running login —
after a reload, or one started in another window — shows the question too, and any window showing it may answer. An
answer that arrives after another window has answered, or after the login has ended, is ignored without a word; the
login's own updates show where it is. Any other failure to send the answer is shown in an error alert titled "Could
not send the answer", and the question dialog stays open.

**A question the window cannot word.** A question the window has no wording for — asked by a daemon newer than the
window, or by an agent the window has no copy of the question for — is not offered for an answer, and the login cannot
go on in the window. The login dialog gives way to a failure alert of its own, in the form of an error alert (the
danger icon, a close button, no Escape), titled "Could not show the login question". Its body says that the login to
the agent's account is waiting for an answer, then that the agent asked a question that cannot be shown here, so the
login cannot continue, then what to do: cancel the login and log in with the agent's own CLI in the environment where
sessions run, using the account's configuration directory, which it names as the window writes the daemon host's
paths (left out where the directory is not known). **Cancel login**, its only button, has keyboard focus; it and the
close button cancel the login. Its appearing, with focus moving into it, is what assistive technology hears.

The daemon takes the answer as `answer_account_login_question`, naming the question and supplying `yes`, `no` or
a secret text answer. It refuses an answer with `login_not_running` when no login of the account runs, and with
`login_question_not_open` when the login is not waiting on that question or its answer kind differs.

## How a login ends

A login ends in one of these:

- **Logged in**: the agent confirms the account is signed in. The dialog closes by itself, the account's quota is read
  again at once, and the account's login badge then shows the result (see "The accounts table" in
  `docs/product/agent-accounts.md`). Assistive technology is told once, naming the account, that it is logged in, about
  1.5 seconds after the success; the dialog closes and focus returns to Settings right after the success, so by then
  both have happened. While the user is away from the window — it is minimized or hidden, or another application, such
  as the browser they authorized in, is in front — the announcement waits, and comes about 1.5 seconds after they are
  back in it. A success that had already happened when the section or the agent's tab is shown again is not announced.
- **Could not log in**: the dialog becomes a failure alert (see "When a login fails" below). When the CLI said it
  succeeded but the agent still reports the account not signed in, its cause says so; when the agent ended the login
  on an answer to its question, its cause is the agent's own reason.
- **Timed out**, 10 minutes after the login started, or the time the agent's sign-in declares in their place, not
  counting the time a question of the agent's stayed open: the dialog becomes a failure alert saying the login timed
  out.
- **Cancelled**, including from another window: the dialog closes.
- **Lost**, when the daemon restarts or the connection is re-established while the login runs: the dialog closes.

A login that cannot start is shown the same way as one that could not log in, with the daemon's reason: for example
while another Codex account is logging in (`provider.codex.login_busy`), or while the account is being removed with its directory moved to the Trash
(`account_being_removed`; see "Logins and agent reads during the removal" in `docs/product/removing-accounts.md`),
where the cause says only that the account is being removed, with no remedy (see "Recovery and unexpected errors" in
`docs/product/error-alerts.md`). A start refused because this account's login is already running is not
shown as a failure; the dialog takes up that login instead.

### When a login fails

After a failure, a timeout or a refusal, the running dialog gives way to a failure alert of its own, in the form of an
error alert (see "The error alert" in `docs/product/error-alerts.md`), though it is not queued with them: the danger
icon, the title "Could not log in" or, for a timeout, "Login timed out", a close button (×) that does what **Close**
does, and neither Escape nor a click outside closes it.
Its appearing, with keyboard focus moving into it, is what assistive technology hears of the failure.

- Its body opens with one sentence naming the agent and the account: that logging in to the account failed, or that
  the login to it timed out. Where the failure has a cause beyond failing, the same sentence goes on with it — the
  daemon's reason for a refusal, that the CLI finished but the account is still not logged in, and for a failure
  Octoboard has no message of its own for, the sentence such a failure opens with (see "Failures shown on the screen"
  in `docs/product/error-alerts.md`). A login the agent's CLI simply reported as failed has no further cause there.
- Then the main remedy, where the failure has one; for a failure Octoboard has no message of its own for, the next step
  of its kind, or, when it is unexpected, the request to report it, with **Report issue** in the footer.
- Then any further explanation in Octoboard's words.
- What the agent's CLI or the system said is only under a collapsed **Error details** disclosure at the end.

Its footer holds two buttons: **Close** and **Log in again**, which has keyboard focus; for an unexpected failure,
**Report issue** takes Close's place, and the close button stands for Close. Log in again brings the running dialog
back on a new login; Close, and the close button, close it, as above.

Whatever the outcome, the account's quota is read again within about 15 seconds (see "When quotas are read" in
`docs/product/account-quota-readings.md`).
