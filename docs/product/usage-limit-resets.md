# Usage limit resets

Codex can grant an account **usage limit resets** (Codex's own name for them; its app server calls them rate limit
reset credits): each one, redeemed, resets the account's eligible usage windows at once. Octoboard shows how many an
account holds and lets the user redeem one. Only Codex has them: Claude Code and Grok Build expose nothing comparable,
so their accounts never show any.

## How resets are read

An account's resets come with its quota reading (see "How a quota is read" in `docs/product/account-quotas.md`): the
reply to Codex's `account/rateLimits/read` carries them beside the windows, so they are read whenever the quota is,
with no request of their own. A reading holds them only while the account has at least one reset it can use; a reply
that says nothing of them, or says it in a form that cannot be read, gives none and never fails the reading. From the
reply Octoboard keeps:

- **how many** resets the account can use;
- **the resets themselves**, when Codex lists them: only those whose status is available, each with Codex's own title
  and description (either can be missing), when it was granted and when it expires (or that it never does). They are
  kept in the order they would be used: the one that expires first, a reset that never expires last. Codex can give the
  count alone, with no list, and its list can hold fewer resets than the count.

The **next reset** is the first of the listed resets that has not expired, the one a redeem names (see "How a reset is
redeemed" below). When every listed reset has expired, the first listed stands in for it in what is shown, as expiring
now, while the redeem names none and leaves Codex to pick. With only the count known there is no next reset.

A later reading that fails keeps the last good resets together with the windows (see "Quota states" in
`docs/product/account-quotas.md`).

## Where resets are shown

A Codex account holding at least one reset shows, beneath its usage meters, one line and a button:

- the line says how many resets are available and when the next reset (see "How resets are read" above) expires:
  "1 usage limit reset available · expires in 6d 3h", the countdown written as a window's is (see "How an account's
  quota is shown" in `docs/product/account-quotas.md`) and moving by itself, "expires now" once that time has passed and
  a newer reading has not arrived yet. When there is no next reset, or it never expires, the line gives the count alone;
- a compact **Redeem reset** button, which opens the confirmation below. Assistive technology hears it named for the
  account ("Redeem reset for Work").

The line and button appear in two places:

- the account's row in the rail's account quota panel (see "What the panel shows" in
  `docs/product/account-quota-panel.md`), under its meters. Pressing **Redeem reset** closes the panel and opens the
  confirmation by itself; the press moves no keyboard focus into the closing panel;
- the account details dialog in Settings (see "The account details dialog" in `docs/product/agent-accounts.md`), under
  its **Usage** meters. The confirmation opens over the details dialog.

They are not shown in Settings' accounts table, the account pickers, the selected session's quota indicator or the
macOS menu bar icon and its menus, and a session's `get_account_quotas` tool says nothing of them (see "Reading quotas
from a session" in `docs/product/account-quotas.md`).

## Only the user redeems a reset

A reset is redeemed only when the user confirms it in the confirmation below. No session can redeem one: no agent
tool does it. The automatic account switch never redeems one either, however its accounts stand (see
`docs/product/automatic-account-switch.md`), and Octoboard redeems none in the background.

## The confirmation

The confirmation is a modal dialog titled **Redeem reset**. It shows:

- that one usage limit reset will be used on the account, named;
- the next reset (see "How resets are read" above) as Codex describes it: its title, its description and "Expires in …"
  ("Expires now" once that time has passed), each where Codex gives it. With no next reset, none is named;
- that redeeming resets the account's eligible usage windows now;
- the account's usage meters, drawn as in the rail's panel (see "How an account's quota is shown" in
  `docs/product/account-quotas.md`);
- **Cancel** and **Use reset**. While the request is under way the button reads "Resetting usage…".

Until **Use reset** is pressed the dialog follows the account's reading as it changes, so a reset used or gone
meanwhile shows. When the reading no longer holds any reset, the dialog says instead that the account has no usage limit
resets available, which assistive technology is told, names no reset, drops the line on what redeeming does, and holds
**Use reset** disabled. From the press on, what the dialog asks and says stays as it was at the press, until the
request fails and the dialog follows the reading again.

## How a redeem ends

What happens after **Use reset** depends on Codex's answer:

- **The usage was reset.** The dialog stays open, titled **Usage reset**, saying that the usage of the account has been
  reset, with **Done** as its only button. Its meters keep the values they had at the press, under the line "Updating
  usage…", until the account's next reading arrives (Octoboard takes one after every redeem; see "How a reset is
  redeemed" below):
  - when that reading succeeds, the bars move to the new values over 300 ms (at once when the system asks for reduced
    motion) and the line reads "Usage updated.";
  - when it fails, or no newer reading arrives within 60 seconds, the line reads "Usage could not be updated yet." and
    the meters keep the values from the press; a failed reading adds the out-of-date mark beside the line (see "Quota
    states" in `docs/product/account-quotas.md`).

  Assistive technology is told that the usage was reset as soon as the dialog says so, and again with the outcome of
  the reading, the new values included when it succeeded; the visible line is not read separately. Keyboard focus stays
  on **Done**, which closes the dialog. An answer that the request had been redeemed already is shown the same way.
- **Nothing needed a reset**: no window of the account is eligible for a reset now. The dialog closes and a notice says
  that the account's usage does not need a reset right now. Whether Codex keeps the reset then has not been measured.
- **No reset was left** to redeem. The dialog closes and a notice says that the account has no usage limit resets
  available.
- **The request failed or was refused.** An error alert titled "Could not reset usage" opens over the dialog, which
  stays open on its question:
  - the account is not logged in: says so, with the remedy to log in to the account and redeem the reset again, and a
    **Manage accounts** button (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`);
  - Codex did not answer in time: says so, and that the reset may have been used all the same, so the account's usage
    is to be checked once it updates;
  - Codex could not redeem the reset: gives what Codex said, with the remedy to check that Codex runs in the login shell
    and is logged in to the account and try again, or, when Codex reports a problem with the reset itself, to try again
    later;
  - a redeem is already running on this account: says so;
  - Octoboard is quitting: the plain alert of every request refused for it (see "Quitting" in
    `docs/product/application-lifecycle.md`);
  - Codex is not available, its binary was not found, or the login shell's environment could not be read: the alert
    those failures always give.

However the dialog closes — Cancel, Done, Escape, a notice — keyboard focus, when it would otherwise be left on nothing,
goes back to where the redeem started, its focus ring showing: from the rail's panel, which has closed, to the rail's
**Account usage** button; from the account details dialog, to its **Redeem reset** button while it is still there, and
otherwise to where that dialog places focus when it opens.

## How a reset is redeemed

The daemon redeems a reset by running `codex app-server` for the account the way a quota reading runs it (see "How a
quota is read" and "What reading a quota leaves behind" in `docs/product/account-quotas.md`) and sending it
`account/rateLimitResetCredit/consume`:

- it names the next reset of the held reading (see "How resets are read" above), or names none — leaving Codex to pick
  — when every listed reset has expired, the reading holds only the count, or its latest attempt failed;
- each redeem sends a new idempotency key, so pressing **Use reset** again is a new request, never a retry of the last;
- Codex is given **60 seconds** to answer, as a reading is, after which the redeem fails as timed out;
- one redeem runs on an account at a time; a second is refused while the first runs. A redeem counts towards the four
  readings, kicks and redeems that may run at once across the daemon (see "When quotas are read" in
  `docs/product/account-quotas.md`);
- none starts once the daemon has begun shutting down, and one running when it does is stopped.

When Codex answers that the usage was reset, the daemon takes the redeemed reset out of the reading it holds (the count
goes down by one; with no reset named, the list is dropped and only the count kept) and sends the changed reading to
every window before it replies, so no window offers that reset again. After every run of Codex, whatever its outcome,
the timed-out and failed ones included, the account's quota is read again straight away, since a reset may have been
used even when no answer came; that reading says what is left.
