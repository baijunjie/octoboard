# Automatic account switch

With **Automatically switch accounts at the usage limit** on (see that section in `docs/product/settings.md`), Octoboard
switches a session to another account of its own agent by itself when the session's turn stops because its account
reached the plan's usage limit, and the session carries on there. It is the same switch the user's **Switch account**
and an owner's `switch_session_account` make — the conversation comes along, and the session reads as switching its
account meanwhile (see "What a switch does", "Carrying the conversation" and "While a session is switching" in
`docs/product/switching-session-accounts.md`) — made without anyone asking for it. It is off by default because it
ends the session's process and spends another account's quota without asking.

The setting governs only this. **Automatically select an account** decides only which account a new session takes (see
"Which account a session uses" in `docs/product/opening-sessions.md`), and **Automatically approve account switches**
only whether an owner's request for a switch asks the user (see "The user's approval of an owner's switch" in
`docs/product/switching-session-accounts.md`). Octoboard never hands a session off to another agent by itself (see
`docs/product/handing-off-sessions.md`).

## When a session is switched

**Only a turn that actually stopped on the usage limit triggers it.** At the end of a turn that failed — Claude Code's
and Grok Build's `StopFailure` hook event, or a Codex turn its rollout shows as failed — Octoboard reads the session's
conversation record, and switches only when the record says the turn stopped on the plan's usage limit, by the same
rules that decide whether a switch continues a turn (see "Continuing after a switch" in
`docs/product/switching-session-accounts.md`). A failed turn that did not stop on the limit, an overload or a network
error say, switches nothing and never shows the session as switching.

- **The record is read again for up to 4 seconds** when it does not say so yet, since an agent can report the failed
  turn before it writes why. That wait comes before the switch takes hold of the session; the record is read once more
  once it has, before the session's process is ended.
- **A used-up quota reading never triggers it.** A fresh successful reading can later continue a turn already
  stopped by the usage limit, as described in "When no account qualifies" below; it never resumes an unrelated idle
  session.
- **Each such turn is a trigger of its own**: nothing holds off a second switch after the first.

**Which sessions.** A session is switched only while it has a process that is *working* or *awaiting instructions*,
nothing is launching, resuming or switching it, it is still on the account whose limit stopped the turn, and no change
of the sharing of the default configuration runs on its account or on the target (see "When a change is carried out"
in `docs/product/sharing-the-default-configuration.md`). A turn that stops on the limit while a launch, a resume or a
switch still holds the session — such as the turn a switch continues, which can stop within the few seconds the
switch keeps its hold after the relaunch — is not dropped: it waits up to **15 seconds** for that hold to end and is
judged then. It is dropped if the session was moved to another account during that wait, and the session is left
alone if it is still held when the wait is over. A session waiting for the user, by whatever route — a
permission prompt, a question, a trust confirmation, a dialog of its agent, a request of its own — is left alone, since
switching it would end what it waits at. The one exception is the dialog Grok Build shows when its turn stops on the
usage limit: ending that dialog is the point of the switch, so it does not count as waiting for the user here. A
session switched or moved to another account meanwhile is left alone. A locked session is switched like any other:
the lock keeps a session from being archived or deleted (see "Locking a session" in `docs/product/sessions.md`), and a
switch does neither. A session left alone for any of these reasons is left as it was, and nobody is told.

## Which account it is switched to

The account a new session of that agent would be given while **Automatically select an account** is on — the account
whose quota is to be spent first, ranked as in "Which account a session uses" in `docs/product/opening-sessions.md` —
whether or not that setting is on, with these left out:

- the account the session is on;
- any account the session was switched off for a usage limit in the last **30 minutes**, so that two accounts both at
  their limit are not swapped back and forth turn after turn;
- any account that clearly cannot take the session: used up (see "A used-up account" in
  `docs/product/account-quotas.md`), turned off (see "Turning an account off" in `docs/product/agent-accounts.md`), not
  logged in (see "An account that is not logged in" in `docs/product/account-quotas.md`), with a change of its sharing
  running, not one of the agent's accounts, or whose quota cannot be read now or has not been read yet — its last
  reading failed, or none is held.

No minimum of quota left is asked for. When no account qualifies, nothing is switched and nobody is told; the
session waits as described below.

**A reset that is about to happen is waited for.** A switch relaunches the agent and carries the conversation over,
which costs more than waiting a few minutes. So when the window that keeps the session's own account from running a
turn (the one that resets last, if several are used up) resets within **5 minutes** (exactly 5 included), going by the
reading held, nothing is switched, whatever other accounts are available: the session waits as when no account
qualifies. Without a trustworthy reading there is no such wait: no reading held, one whose last attempt to read the
account failed, a used-up window that gives no reset time, or an account that is not used up all leave the switch as
described above.

While the session is held, only a reading of its **own** account resumes it; another account's successful reading
does not move it. Octoboard reads an account again about 30 seconds after a held reset time and the poller looks every
15 seconds, so the session continues on its own account roughly 30 to 45 seconds after the reset, plus the time the
reading takes. If its own account has not been read as recovered **3 minutes** after the reset (the re-read failed
and is backing off, or the window that blocked it was out of date and another still does), the hold ends and the
session is judged as any other: switched to another account if one qualifies, else left waiting as below, where a
successful reading of any account of the agent can resume it. A hold is never taken twice for the same failed turn.

When the hold ends, a fresh successful reading of the original account takes priority: the session resumes there,
including when recovery is confirmed while a switch to another account is being prepared, before the process stops.

## When no account qualifies

The session stays stopped on its limit. While the setting remains on, Octoboard remembers that failed turn and can
continue it after an account of the same agent has a fresh successful quota reading. It uses the existing readings
in "When quotas are read" in `docs/product/account-quotas.md`; waiting adds no separate timer or quota requests.

- **The reading must begin after the failed turn was captured**, report at least one account-wide window, and give
  a finite used percentage below **100% in every account-wide window**: the five-hour window, account-wide weekly
  window or credit pool. A model-specific window alone, a failed or unavailable reading, an older held reading, or
  merely passing a reset time does not prove recovery.
- **Only the account just read is considered**, and it must still be usable when the session is prepared to restart.
  A fresh reading meeting these rules allows an account left for a usage limit in the last 30 minutes to be used
  again. Other accounts' held readings do not trigger a switch.
- **The original account can recover too.** Octoboard restarts the session on that account without copying its
  conversation. The session is sent a message from Octoboard saying its account's usage quota has recovered, followed by
  the request to continue its task (see "Continuing after a switch" in `docs/product/switching-session-accounts.md`).
  Its owner is told it resumed and whether it was sent a continuation prompt; the user gets no account-switched toast.
  Moving to a different account has the success notices in "When the switch succeeds" below.
- **The wait belongs to this failed turn and its running process.** New input or a queued message, a new turn, a
  manual resume, restart, account switch, model or effort change, an interruption, archiving, deleting, or the process
  exiting invalidates it. Changing the session's account configuration also invalidates it. A session waiting for
  the user is left alone, including when that wait appears while a restart is being prepared.
  A later failed turn replaces an earlier wait; overlapping work on that earlier wait cannot restore it over the
  newer one.
- **Turning the setting off cancels these waits.** Turning it back on does not recover them. The waits are held only
  while the daemon runs and do not survive its restart.

A restart refused before stopping the process can wait for another qualifying reading while the same failed turn
is still eligible. The same wait is never restarted twice by overlapping readings. If restarting on the original
account fails after stopping its process, Octoboard tries to bring it back up there without a prompt, as for a failed
switch. If that also fails, it stays interrupted: the user gets the usual launch-failure alert, and its owner is told
that it could not resume and its work stopped. Failures moving to another account follow "When the switch fails"
below.

## When the switch succeeds

The session is relaunched on the new account and carries on with the turn the limit stopped: its relaunch prompt tells
it that Octoboard switched it from the one account to the other by itself, that its conversation came along and that
nothing else changed, and then asks it to continue its task, as any switch that continues a turn does. A session whose
turn the switch's rules do not continue is sent that notice as a message instead, telling it to carry on or to wait
for its next instruction.

- **Its owner**, when it has one that is not archived, is written a message naming the session and both accounts, saying
  the conversation came along and either that the session was sent a prompt to continue its task, so the owner is not to
  send one, or that it is back awaiting instructions and is to be sent its next step. An interrupted owner has the
  message kept for its resume, and one in the middle of an operation is waited for (see "Reports and notices for an
  interrupted owner" in `docs/product/reporting-to-owners.md`). Where the switch is made as the failed turn ends, this
  message, or the one for a session resumed on its own account, takes the place of the report Octoboard would otherwise
  synthesise for that turn (see "When a session does not report" in `docs/product/reporting-to-owners.md`).
- **The user** gets a toast titled with where the session is, saying that the account reached its usage limit, so
  Octoboard switched the session, named by its displayed name, to the other account (see `docs/product/toasts.md`).
  The daemon sends it as a `session_notice` with the code `account_switched_automatically`.

## When the switch fails

- **Before the session's process is ended**, or when the process does not stop in time and may still be running, the
  switch leaves the session as it was. Nobody is told; it is only written to the daemon's log.
- **After the process was ended** — a change of the target's sharing began meanwhile, the conversation could not be
  copied, the relaunch was refused or its process did not come up — Octoboard brings the session back up on the
  account it had, without a prompt, and tells nobody. When another operation holds the session at that moment (a
  handover of its branches, a change of its owner), it waits up to **15 seconds** for it to finish before giving up.
  The restore brings the session back only while it is on the account it had or on the one the failed switch was
  moving it to; one somebody else moved to a third account meanwhile is left as they left it, neither moved back nor
  relaunched, and nobody is told.
- **Only when that restore fails too** is anyone told. The session is left *interrupted* on its own account, for the
  user or its owner to resume, and there is nothing left to retry:
  - the user gets an error alert titled "Could not switch account", whose body says that the account reached its
    usage limit and that Octoboard could neither switch the session to the other account nor bring it back up on the
    one it had, so it is interrupted, followed by why the session could not be brought back up. The alert never
    suggests trying again. Where the conversation could not be copied and an older item could not be put back (see
    "Carrying the conversation" in `docs/product/switching-session-accounts.md`), it advises copying that item out of
    the folder the reason names before it is cleared; where the cause is one the window recognises from what the
    system said, such as a full disk or denied access, it gives that remedy without any step that retries — for denied
    access, to check the folder's permissions and that its disk is writable and to grant Octoboard access in the
    system's privacy settings if needed, without choosing another folder. A cause the window has no words for is
    an unexpected failure, with **Report issue** (see "Recovery and unexpected errors" in
    `docs/product/error-alerts.md`). The daemon sends it as a `session_notice` with the code
    `account_switch_automatically_failed`, carrying the failure's own code and params;
  - its owner, when it has one that is not archived, is written a message saying the session reached its account's
    usage limit and Octoboard could neither switch it nor bring it back up, with the reason, that it is interrupted
    and its work stopped, and to look at it with `get_session` and relaunch it with `reopen_session` if the work
    should go on; it is kept or waited for as the message for a success is.
- **A process that had already exited by itself** before the switch stopped it is not the switch's: the session is
  neither restored nor reported by the switch, and its owner is told it ended on its own (see "Notices to an owner" in
  `docs/product/reporting-to-owners.md`). A process that the switch or the restore starts and that ends within its
  first few seconds is that operation's outcome, and its owner is not also told it ended on its own.
- **A session archived or deleted meanwhile, or a switch or its restore cut short by the daemon shutting down** (see
  "Quitting" in `docs/product/application-lifecycle.md`), is neither told about nor logged.
