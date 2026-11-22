# The clean-up before uninstalling

Settings' Cleanup section ends with the row **Clean up before uninstalling**: the clean-up for the moment before
Octoboard is uninstalled. Its one action removes every modification Octoboard's features have in effect outside
Octoboard's own directories, which deleting the application does not remove, and turns their features off. What stays
after an uninstall, with the clean-up run and without it, is in `docs/product/what-remains-after-uninstalling.md`.

**It is done on the machine itself.** The desktop application does it in its own window's Settings, with this row. On
macOS deleting the application runs nothing, so the row is the only way; on Windows the uninstaller also runs the same
clean-up by itself, before it removes anything (see "The Windows uninstaller's clean-up" below). A server form's
`octoboardd uninstall`, run on the server, does it as its first step, before it removes anything (see "The server
form's uninstall" below). No request or tool offers it: no client connection, a browser's or a remote window's (a window
onto another computer's daemon) included, can list or clear these items, and Octoboard's MCP tools offer none. A shell
on the host can still run `octoboardd uninstall`, as the user could: an agent's shell in a session, or a shell terminal
opened from a browser or a remote window. That is a local command run as the user, and nothing prevents it.

The row is no setting: nothing of it travels in a configuration export, and no agent reads or runs it through the
settings tools. A search in Settings finds it by its name and by the words of its description, in each wording the
description takes from window to window (see `docs/product/searching-settings.md`).

## Which installation it speaks for

The clean-up is run only on the machine itself, never from another one, so the action exists in one window alone:

- **The desktop application's main window**: the row with its status and its action (see "The row" below), for the
  application's own items and those of this computer's daemon, the accounts that share its agent's default
  configuration. The window asks the desktop application for both, which answers them in one list and clears them in
  one go; the window sends the daemon no request of its own for this. The application asks its own daemon for the
  daemon's items on two local routes, with the launch key it already holds (`POST /system-changes`, and
  `POST /system-changes/clear` to clear), never through the window's connection.

Everywhere else the row is a single sentence, with no status and no button, and nothing is listed, read or cleared from
there:

- **A remote window of the desktop application**: the sentence says that this window shows a remote daemon, that the
  clean-up before uninstalling Octoboard from this computer is done in Settings of this computer's own window, and that
  a daemon on a server is cleaned up when it is uninstalled, on that server (see "The server form's uninstall" below). A
  remote daemon's items are therefore never counted or listed in another computer's window, whatever kind of
  daemon it is: uninstalling the application here does not uninstall Octoboard there.
- **A browser on a desktop application's own daemon**: the sentence says that the clean-up is run where Octoboard is
  being uninstalled, in Settings of the application's own window on that computer.
- **A browser on a daemon that runs on its own** (the server form): the row says that the clean-up is part of
  uninstalling on the server itself, not done from a browser, with `octoboardd uninstall` (see "The server form's
  uninstall" below). It runs nothing.

A browser tells the two daemons apart by the daemon's snapshot, whose `host` says whether the daemon is a desktop
application's own (`application`), and shows no row until the daemon has said what host it is on.

The sections from "The row" to "The result" describe the row in the desktop application's main window.

## The row

The row's description says that it removes what the app's features added outside its own folders, for when Octoboard
is being uninstalled, and that a single one is undone at its own feature's switch. Under it, a status line says:

- **how many items are in effect**, the application's and the daemon's together;
- **that there is nothing to remove**, once everything answered in full and nothing is listed;
- **that not everything in effect could be read**, whatever the rest listed, where one of the application's features
  could not tell what it has, the application did not answer, or the daemon's part could not be read in full: the
  daemon did not answer (it could not be asked or what it answered could not be read, the application's daemon
  not running, say), or it listed an item of a kind the application does not know (see "The clean-up before
  uninstalling" in `docs/product/values-a-newer-daemon-sends.md`).

Until the first read has answered there is no status line. Nothing is listed on the page, and no item, alert or special
case appears there: the items are listed in the confirmation (see "Remove all" below). The row is read each time the
section is shown, and again after every clean-up.

The action is **Remove all**, a dangerous button. It is disabled until the first read has answered, and while nothing
is in effect (everything answered in full and nothing is listed), with no reason of its own: the status line says why.
Where something could not be read it stays available, even with nothing listed, so that the confirmation reads again.

**What has nothing to undo is not an item**: the conversation an account switch copied, the conversation history
sharing moved into the default account's directory, Grok Build's settings and plugin paths written back from a session,
agent records moved to the Trash, the entries the agents record in their own trust lists for consoles' directories, and
the empty directories turning sharing on created. Each is the user's or the agent's own once written, and removing it
would lose data or undo nothing (see "What stays among the agents' files" in
`docs/product/what-remains-after-uninstalling.md`).

## Remove all

**It asks first**, in a confirmation of a destructive action titled as removing all before uninstalling. As it opens,
everything is read again, its confirm button held meanwhile, so that it counts what will be removed now; where
everything answered in full and nothing is left to remove, it closes without removing anything. Cancelled while a read
is still out, as it opens or after **Try again**, it stays closed: that read's answer does not open it again. From top
to bottom it holds:

- **one sentence**: that, before Octoboard is uninstalled, this removes what the app's features added outside its own
  folders and turns those features off;
- **a compact error alert for each part that could not be read**: a title and the failure behind its Details; where
  something is listed, one sentence saying that this part's items are not listed and that **Remove all** still tries
  to remove them. Both alerts are recovered from by the same read, so there is one **Try again**, on the last alert,
  which reads everything again. A read that fails leaves nothing of that part's earlier answer in the list, and what
  the rest listed stands beside the alert. The alerts are:
  - **Could not read what the app added on this computer**, where one of the application's features could not tell what
    it has (its other features are listed all the same), or the application did not answer;
  - **Could not read what the daemon added**, where the daemon's part could not be read in full (see "The row" above);
- **the count of what will be removed** ("7 items will be removed"), where something is listed: a disclosure with a
  triangle before its words, as an error alert's Details, closed when the confirmation opens, that opens the list of
  the items, one item line each (see "The items" below). The line of an account that shares ends in an
  information mark, which opens the note that this account's idle sessions are restarted when it stops sharing and
  their conversations continue; the result's lines carry no such mark (see "When a change is carried out" in
  `docs/product/sharing-the-default-configuration.md`);
- **where the daemon's part could not be read and something is listed**, one muted sentence, said once for every
  account that may share, which no line is there to carry the mark for: that an account that is sharing stops sharing,
  and that its idle sessions are restarted and their conversations continue;
- **one muted line on what is kept**: the app's own data, and, only where an account shares, or where the daemon's part
  could not be read and something is listed, the conversation history that sharing moved into the default account's
  directory, which removing would lose the user's conversations;
- **the field the action's word is typed into**, then **Cancel** and the confirm button, **Remove all**. It is enabled
  only once the word for "remove" in the current language has been typed, as deleting a console asks for the word for
  "delete" (see "Deleting a console" in `docs/product/consoles.md`), and only while something is listed.

### The items

An item is shown as one **item line**, the same wherever it is listed: the confirmation's list, the result's items not
removed and the result's list of removed items. The line holds, in this order, the item's kind, then the one fact that
tells it from the others in secondary text, then, for two special cases, its qualifier as a badge. Every line names its
kind; items are not grouped under one name. A line never wraps: a fact too long for it is clipped, a path at its start
and any other fact at its end, and carries its full text as a tooltip while it is clipped; the qualifier stays whole.
Where an item is and how it is undone alone are not on its line; they are stated here.

| Item | Listed while | Its fact | Where it is | Undone alone by |
|---|---|---|---|---|
| **This computer's key**, one per saved remote daemon | the saved daemon has this computer's key installed | the key's user and host, as `user@host` | a line in the authorized keys of its user on its host, and its private key in this computer's credential store | turning the key off at Remote devices > the saved daemon's name > This computer's key (see "Taking the key back" in `docs/product/remote-daemons.md`) |
| **This computer's key**, private key only, one per saved remote daemon | the saved daemon no longer signs in with this computer's key, but the credential store still holds the key's private half, which it did not remove when the key was taken back | `user@host`, with the qualifier **Private key only** | this computer's credential store, kept for the saved daemon's user on its host | nothing: it has no switch of its own |
| **Launch at login** | the system holds the login entry: on, waiting for approval, or (Windows) turned off in Task Manager's Startup apps with its entry still there | none; the qualifier **Turned off, still there** where it is turned off in the system's own list | the system's list of apps opened at login | General > Launch at login (see `docs/product/launch-at-login.md`); an entry turned off in the system's own list by turning the switch on and then off |
| **Command-line tool** | a link of the application's is there, whatever copy it leads to | the link's path | the link's path | Remove in General > Command-line tool (see "Command-line tool" in `docs/product/general-settings.md`) |
| **Shared default configuration**, one per account | the account shares its agent's default configuration, or its last change of sharing is incomplete | the account's agent and the account's name | the account's directory, whose entries link into the default account's directory | the account's Share default configuration switch in Agent accounts (see `docs/product/sharing-the-default-configuration.md`) |

The first four are the desktop application's (the command-line tool only in the macOS application, which alone has
it); the last is this computer's daemon's, which the application asks that daemon to list and clear.

The credential store is asked about each saved remote daemon, so a private key it kept for a remote daemon that is no
longer saved (one removed while the store refused to delete its key) is neither listed nor removed; `docs/product/what-remains-after-uninstalling.md` says
where to delete it by hand.

### Once confirmed

It removes every item, whatever becomes of the others, and an item that cannot be removed does not stop the rest. A
part whose items could not be read, the daemon's included, is still asked to remove them. Each item is removed as its
own feature's switch removes it:

- **every installed key is taken back**, the hosts spoken to side by side, and each saved daemon's key is turned off,
  the saved daemon itself staying; **a private key left behind** is removed from the credential store (see "Taking every
  key back at once" in `docs/product/remote-daemons.md`);
- **Launch at login is turned off**, which removes the entry also where it was turned off in the system's own list;
- **the command-line link is removed**, which may raise the system's administrator prompt;
- **sharing is turned off for each listed account**, one after another, each as its switch turns it off: refused while
  the account's sessions work (see "When a change is carried out" in
  `docs/product/sharing-the-default-configuration.md`).

The application's items are removed first, then the daemon's, by asking that daemon to clear everything it has in
effect; where the daemon's part fails as a whole, the application's own items
are removed all the same. The action also removes the Windows taskbar's jump list, which is not an item: it is part of
the application's own presence and has no switch. The application writes it no more until it next starts.

## The result

Once it has run, a dialog titled as the result of the clean-up takes the confirmation's place, with one button,
**Close**. From top to bottom it holds:

- **one summary line**, led by a status glyph: a check with that everything was removed; or a warning triangle with
  how many items were removed and how many could not be ("2 removed, 1 could not be removed."), only how many could not
  be where none was removed ("1 item could not be removed.", never "0 removed"), or, where no listed item was left but
  a part failed as a whole (the application not answering the clearing at all among them), that it could not be
  confirmed that everything was removed. Focus is put on this line as the dialog opens, so a screen reader reads the
  dialog's title and then the line; when the window is not active then (a system prompt the clearing raised, such as
  an administrator's password, still has focus or was just cancelled), focus is put there once the window is active
  again, and again whenever the user comes back to the window while the result is open;
- **an error alert for each part that failed as a whole**, titled **Could not remove what the app added on this
  computer** (one of the application's features could not tell what it has, or the application did not answer) or
  **Could not remove what the daemon added** (the daemon's part failed: the daemon could not be asked or what it
  answered could not be read, one of its features could not tell what it has, or it answered an item of a kind the
  application does not know), saying that some of its items may have been removed and to open this again to see what
  is still in effect, with what failed behind its Details. That part's items are not among the lines;
- **the items not removed**, only where any was left, with no label over them: each a block of its own, outlined and
  padded, the blocks stacked. A failure glyph leads the block and the item's line (see "The items" above) is its
  title, its kind in the weight of a row's name; under the title's text hang why, in one short plain sentence, and what
  to do about it or where to remove what stays (see below), then a **Details** disclosure, closed when the result
  opens, that holds the rest: what stays of a key in full, and the system's own words;
- **the count of the removed items** ("3 removed"), where any was: a disclosure, closed when the result opens, that
  opens their list, one item line each, as in the confirmation, but each led by a success glyph standing in the same
  column as the blocks' glyphs.

Every item line of the result thus carries the glyph of its outcome, and a screen reader hears "Removed" or "Not
removed" before each line; the confirmation's list carries no glyph.

For an item not removed, the sentence and what its Details hold are:

- **This computer's key**: one sentence for each part that stays, both shown where two parts stay: that the line could
  not be removed from the server and may still be there; that the server's authorized keys may be incomplete; that
  this computer's credential store did not remove the private key. Its Details hold each part that stays, as the remote
  daemon's own alerts word it (see "Taking the key back" in `docs/product/remote-daemons.md`): a line that may be left
  on the host, named with its mark and its file where the application knows it, for the owner to delete on the server,
  adding that the private key was removed where it was; the server's authorized keys possibly left incomplete, naming
  the copy of their earlier content the server kept, to restore them from; a private key the credential store kept,
  named by its service and account, for the owner to remove there. Only where the line was removed and the private key
  was not do they say that the line was removed from the server. For a private key only, the sentence and the Details
  name only the private key the store kept.
- **Launch at login**: the sentences say that the system did not remove it from the apps opened at login, so it is
  still in that list, and where to remove it, as one path: on macOS in System Settings, under General > Login Items; on
  Windows under Task Manager > Startup apps. Its Details hold the system's own words, where it gave any.
- **Command-line tool**: the sentence says that the link could not be removed, and under it why, as the setting's own
  row says it (see "Command-line tool" in `docs/product/general-settings.md`), where that names more than that the
  change could not be made. Its Details hold the system's own words; for a link that leads elsewhere there are none,
  the explanation naming its target.
- **Shared default configuration**: the sentence is the daemon's own reason the account still shares (its sessions
  working, say), and under it what to do about it, where the daemon gives that. Its Details hold the daemon's further
  detail, where it gives any; otherwise the block has no Details.
  The result does not name which entries of the conversation history stay; the confirmation has said that it stays in
  the default account's directory.

The row is read again after every clean-up and shows the new state. However the confirmation or the result closes,
focus returns to **Remove all**, or, where it is then disabled (nothing left in effect), to Cleanup's entry in
Settings' list of sections; where the row is still being read again as the dialog closes, which of the two is chosen
once that read has answered, waiting a few seconds at most.

## The Windows uninstaller's clean-up

The desktop application's Windows uninstaller runs the clean-up **Remove all** runs, by itself, before it removes
anything of the application. The row stays in Settings on Windows all the same: it shows what is
in effect before uninstalling, and the result per item, in the application's language.

### When it runs

On an uninstall proper: the uninstaller started from Windows' Settings (Apps) or Control Panel, or run directly,
interactively or silently (`/S`, as a package manager such as winget runs it). The uninstaller tells an uninstall
proper from one a setup makes on the way to installing by one thing alone, NSIS's `_?=` option, with which a setup
starts it in place; and it errs toward not cleaning, since cleaning on an upgrade would take the user's keys and
sharing away. So it does not run where a setup starts the uninstaller, and this computer's keys on remote servers and
the accounts' sharing outlive those:

- **the in-app update** starts the new setup in update mode, which runs no uninstall at all;
- **a newer setup run interactively** offers "Uninstall before installing", chosen by default, which runs the
  installed version's uninstaller in place; that uninstall runs no clean-up. It still removes the login entry and the
  jump list (see "Uninstalling" in `docs/product/installing-the-desktop-app.md`). A newer setup run in passive mode
  (`/P`) or silently (`/S`) runs no uninstall at all, and so leaves the login entry and the jump list too;
- **an older setup run interactively**, a downgrade, offers the same choice and runs the installed version's
  uninstaller the same way: no clean-up;
- **a setup of the installed version run by hand** offers "Uninstall Octoboard" as its second choice, which runs the
  uninstaller in place as well: no clean-up, and nothing says so. Where the user then cancels the setup at its next
  page, Octoboard is uninstalled with nothing cleaned. To uninstall with the clean-up, uninstall
  from Windows' Settings, or run **Remove all** in Octoboard's Settings first;
- **anything else that starts the uninstaller as a setup does**, with `_?=` (as some deployment tools do, to wait for
  it), gets no clean-up either. What then stays is what `docs/product/what-remains-after-uninstalling.md` lists.

**An upgrade made by uninstalling first does run it.** A package manager or a script that upgrades by running the
installed uninstaller plainly, without `_?=`, and then the new setup (winget where a package's manifest asks for the
previous version to be uninstalled first, say) is an uninstall proper to the uninstaller: the clean-up runs, and the
keys and the sharing are gone after the upgrade. Octoboard's own in-app update and its setup never
upgrade this way.

The uninstaller that runs is always the one the installed version came with, so a version from before the uninstaller
ran the clean-up runs none when it is uninstalled, by hand or by a newer setup; there the row is the way. On macOS there
is no uninstall process, and the row is the clean-up.

### What it does

First it ends the running application, with the prompt Tauri's uninstaller shows for it: OK ends the application,
Cancel stops the uninstall; a silent uninstall ends it without asking. The x64 build on Windows on Arm is the
exception: there the uninstaller cannot end a running Octoboard, since Windows' service that translates x64 code holds
its executable, so ending it fails and the uninstall stops before the clean-up, with nothing cleared or removed; quit
Octoboard first. The arm64 build, which an x64 installation on an Arm PC moves to through the in-app update (see
"Where updates come from" in `docs/product/in-app-updates.md`), ends it as described. Its progress details then
say that it is undoing what Octoboard changed outside its own folders, while the application's executable, started without a window,
clears what **Remove all** clears, in the same order and each as described in "Once confirmed" above:

1. this computer's keys on remote servers, each key's line in the server's authorized keys and its private key in the
   credential store, and the private keys left from keys taken back earlier;
2. Launch at login;
3. the command-line link, which the Windows application never has;
4. this computer's daemon's items, the accounts that share their agent's default configuration. The bundled daemon is
   started for this, for a clean-up run (below), and stopped again afterwards with an ordinary stop, so nothing of
   Octoboard runs on once the uninstall is through; sessions an ended application left running in their holders end
   with that stop;
5. the taskbar's jump list.

**The daemon started for it does nothing but the clearing**. A clean-up run takes over what the
ended application left running in its holders, so that an account whose session is working is refused as its switch
refuses it and the stop that follows ends those sessions; and has what the previous run's agents left running
stopped. It leaves out everything else a daemon does at its start: no scheduled task runs or is caught up on, no
interrupted session is resumed, the idle sessions the sharing switch ends around its change are not started again
and what their processes had started is stopped with them (so the note on an account's line that its idle sessions
are restarted does not hold here: they end), no session is switched to another account when its own runs out, no
agent's command line is run in the background to read quotas, no archived session is deleted, no agent's records
are collected, no project's repository is fetched or pulled, the messages a planned stop left queued for a session
are not delivered, remote access is not opened, and the machine is not kept awake. It answers every request a
daemon answers on its loopback port, since the agents it took over still report to it, but it **launches no
agent**, whoever asks (a session's tool, a hook): a new session, a resume or a relaunch is refused as a stopping
daemon refuses it. Where that daemon ends by itself before it has answered (a crash, or ended from outside), none is
started in its place: its items are reported as not known. Once the clearing is through it is asked to stop and
given **30 seconds** to end what it took over before it is ended.

**It runs only as the uninstaller starts it**. Started by hand, it refuses, clearing nothing and
ending as a clean-up that could not run, where the file for what is left is named anywhere but in the installation's
own folder, and while Octoboard is running, for which it waits up to 10 seconds: the uninstaller has ended the
application just before, and a running one's keys and login entry are not cleared behind its back.

Nobody is asked anything during it, and an item that cannot be removed is reported as left. It is bounded:

- **clearing is given 120 seconds in all**. A remote server that does not answer costs about 40 seconds, the servers
  being asked side by side;
- **the daemon is given up to 45 seconds to start**, tried again a second apart, since the daemon of the application
  just ended may hold the data directory for a while yet; one that does not start leaves its items reported as not
  known;
- **past the 120 seconds** the clearing is left where it stands, and what is then still in effect is read, in up to
  15 seconds, and reported as left; a daemon not yet started by then is not started for the reading, and its items
  are reported as not known. Where that reading finds nothing in effect, nothing is left;
- **at 240 seconds** the clean-up is ended, whatever it is doing, its daemon with it. What it had found left by then
  is told as below; where it had not got that far, it counts as one that could not run.

Whatever it came to, the uninstall goes on.

**Before the files are removed, the uninstaller waits up to about 10 seconds for Windows to let go of Octoboard's
executable**, then checks again that Octoboard is not running, with the same prompt as at the
start. A process of the executable that has just ended, the clean-up's own, can be counted as running for a moment
after it ends; without the wait that check would ask to end an application that is not running and, unable to, stop
the uninstall with the clean-up already done. Where nothing holds the executable there is no wait, and where Octoboard
has been started again meanwhile the wait runs out and the prompt asks to end it; while it waits, its progress details
say "Waiting for Windows to release Octoboard's executable...". Every uninstall waits so, the one a
setup runs in place included, which runs no clean-up but has the executable remove the jump list.

### How its result is told

- **Where it left nothing**, nothing is shown and no list is written. The list an earlier clean-up left is removed as
  the clean-up begins, so the file is this clean-up's or is not there.
- **Where it left something**, at the end of an interactive uninstall, once everything else is removed, a message box
  says that Octoboard is uninstalled but could not undo everything it had changed outside its own folders, followed by
  a line for each item left, saying where it is: for a key, its line, with its mark, in the named authorized keys file
  of its user on its host, to be removed there, or that file possibly left incomplete, to be restored from the copy
  the server kept, and its private key in this computer's credential store under its service and account; for Launch
  at login, that the entry is still in the system's startup list, with the system's reason; for an account, that it
  still shares, with the daemon's reason; for a part that could not tell what it has, why (this computer's daemon not
  starting, say, or the file of the saved remote daemons being unreadable, in which case no key is taken back).
  Where clearing ran out of time, a line says so, and that what follows was still in effect then and some of it may
  have been removed since it was read; what was still in effect follows, or that it could not be read. The box holds
  as many lines as fit with its heading, then says how many more there are, and ends with the path of the file that
  holds the full list:
  `uninstall-clean-up.log` in the `logs` folder of the daemon's data directory
  (`%USERPROFILE%\.octoboard\logs\uninstall-clean-up.log`). The uninstaller does not remove that file: its "Delete the
  application data" box removes the application's folders under `%APPDATA%` and `%LOCALAPPDATA%`, not
  `%USERPROFILE%\.octoboard`. Each clean-up that leaves something replaces it.
- **A silent uninstall** shows nothing; what it left is in that file alone.
- **Where the clean-up could not run at all** (the executable could not be started or is no longer in the
  installation's folder), the message box says that Octoboard is uninstalled but could not check what it had changed
  outside its own folders, so its device keys on remote servers and the links of accounts that share a default
  configuration may be left; no list is written.
- **Where it was started and did not tell what it left**, the message box says the same, and adds that, if the
  clean-up saved a list of what it left, it is in `uninstall-clean-up.log` at the path above: where it ended with a
  status that is none of its own (it refused to run, it was ended at its 240 seconds before it had told what it
  left, or it was ended from outside, which Windows reports as -1 or 1), and where it said it left something and its
  list is missing or empty (not verified on Windows for a clean-up ended at its 240 seconds, or ended from outside with
  status 1). A clean-up that refused to run has saved none.

The uninstaller's words (the line while the clean-up runs, and the message box's heading and its messages) are in the
language of the installation (see "Windows" in `docs/product/installing-the-desktop-app.md`), and the uninstaller
has the clean-up write the list under the heading and `uninstall-clean-up.log` in the same language. What a feature
gives as its reason (an error's text in the parentheses, the daemon's message after an account that still shares) is
written as the feature gave it, which may be English.

## The server form's uninstall

The server form has no window of its own to clean up from: its clean-up is the first step of `octoboardd uninstall`,
which the user runs on the server. It reads what the daemon's features have in effect, says it among what it will do,
and, once confirmed, has a daemon started for a clean-up run clear it: by the command itself, also where a service
runs the daemon otherwise, a run that does nothing but the clearing (see "What it does" above) and so relaunches no
session that could hold what is being cleared. A daemon that already runs is stopped first, once the command has made
sure that it runs this installation's executable and that its stop would not be refused: neither is left to be found
out after the clearing, which is not taken back. Where something stays in effect (an account whose sessions are
running, say) or the clearing cannot run, it removes nothing, starts the daemon it stopped again as it ran (through
its service, or in the background), so that the server is as it was, and says how to go on; `--keep-changes` uninstalls without clearing, leaving what is in effect in place. The command, in full, is in
"Uninstalling the server form" in `docs/product/daemon-command-line.md`; the install script's `--uninstall` hands over
to it (see "Uninstalling" in `docs/product/installing-the-server-form.md`). The desktop application's own daemon is not
uninstalled this way: it goes with the application, after the row above or the Windows uninstaller's clean-up.
