# What remains after uninstalling

What is left on the user's system once Octoboard is uninstalled, for each of its three installations: the desktop
application on macOS, the desktop application on Windows, and the server form (see `docs/product/server-form.md`). For
each: what uninstalling removes, what stays in Octoboard's own directories, and everything else that stays, with where
it is, whether it has any effect, and how to remove it by hand. What every installation leaves among the agents' own
files is in "What stays among the agents' files" at the end.

Nothing Octoboard does relies on an uninstall hook: deleting the application on macOS runs none, an application
deleted without its uninstaller gets none on Windows either, and the Windows uninstaller's own clean-up does not run in
every uninstall (see "The clean-up before uninstalling" below). Each item that has an effect outside Octoboard's own
directories is undone by turning its feature off, but for three that have no switch: the Windows taskbar's jump list,
which the uninstaller removes (see "Before uninstalling" below), and the server form's systemd user unit and the
lingering its install turned on, which only `octoboardd uninstall` undoes (see "The server form" below).
What an uninstall leaves in Octoboard's own directories is static files, which no longer do anything once no daemon
runs. The clean-up before uninstalling undoes all of the features' modifications at once (see "Before uninstalling"
below).

A statement followed by a mark such as `(not verified on Windows: …)` describes behaviour that depends on a platform it
has not yet been checked on: a Windows machine, a Linux user session with systemd, or an installed macOS application.
The statement says what the code does; the mark says it has not yet been seen to happen there, and, where only part of
it has not, which part.

## Before uninstalling

These items are each undone at their own feature's switch, before the application is uninstalled:

- **Launch at login**: turning the switch off removes the login entry (see `docs/product/launch-at-login.md`).
- **The command-line tool** (macOS): **Remove** in Settings removes the link (see "Command-line tool" in
  `docs/product/general-settings.md`).
- **This computer's key on a remote host**: turning a saved remote daemon's key off, or removing the saved daemon,
  takes the key's line off the host and its private half out of the credential store (see "Taking the key back" in
  `docs/product/remote-daemons.md`).
- **Sharing the default configuration**: turning an account's sharing off removes its links into the default directory
  and restores what the account held (see "Turning sharing off" in
  `docs/product/sharing-the-default-configuration.md`).

The Windows taskbar's jump list has no switch: the uninstaller removes it, and so does the
clean-up below. It is part of the
application's own presence, as its taskbar button is, not an exception Octoboard registers with the system, so no
switch is owed for it. The server form's unit and lingering have no switch either; `octoboardd uninstall` removes them. What each of these leaves when it was not undone first is listed under the installation below.

### The clean-up before uninstalling

The clean-up before uninstalling removes all of the items above that are in effect at once and turns their features
off, for the moment before Octoboard is uninstalled (see `docs/product/clean-up-before-uninstalling.md`). It is done on
the machine itself, never from a browser or a remote window, and acts on the installation being uninstalled:

- **in the desktop application's main window**, Settings' Cleanup section has a row, **Clean up before uninstalling**,
  that says how many of the items are in effect and has one action, **Remove all**, which counts them in its
  confirmation, on the application's items and on this computer's daemon's: it takes every installed key back from its
  host and from the credential store, removes a private half the credential store kept from a key taken back earlier,
  turns Launch at login off (removing an entry turned off in the system's own list too), removes the command-line link
  and turns sharing off for every account that shares or whose last change of sharing is incomplete, the last through
  this computer's daemon. It also removes the Windows taskbar's jump list, which the application does not write again
  until it next starts;
- **by the Windows uninstaller**, which, on an uninstall proper, has the application clear all of the same before it
  removes anything, with nobody asked, and tells at the end of an interactive uninstall what it left, in a message box
  and in `%USERPROFILE%\.octoboard\logs\uninstall-clean-up.log` (see "The Windows uninstaller's clean-up" in
  `docs/product/clean-up-before-uninstalling.md`);
- **on the server form**, on the server itself, `octoboardd uninstall` does it as its first step, before it removes
  anything (see "What uninstalling removes" under "The server form" below), turning sharing off for every account that
  shares or whose last change of sharing is incomplete. A browser on that daemon has no action: its row says that the
  clean-up is part of uninstalling on the server, with `octoboardd uninstall`.

Deleting the desktop application on macOS runs no clean-up: its items stay unless the row in its Settings was used
first. On Windows they stay, unless the row was used first, where the application is deleted without its uninstaller,
where the uninstaller is the one a setup runs in place (a newer or an older setup's "Uninstall before installing", or a
setup of the installed version's "Uninstall Octoboard") or is otherwise started with NSIS's `_?=` option, where the
clean-up could not run (the uninstaller's closing message says so), and where the installed version's uninstaller is from before it ran the clean-up (see "When it
runs" in `docs/product/clean-up-before-uninstalling.md`).

Where it has removed everything, what the "What else stays" sections below list of those items — the command-line
link, the login entry, this computer's keys on remote hosts and their private halves, private halves left from keys
taken back earlier, and the jump list unless the application was started again — does not stay, and of "What stays
among the agents' files" an account's links into the default directory and its backups do not either. On the server
form, the rest of `octoboardd uninstall`, not the clean-up, removes what "The server form" lists under "What
uninstalling removes". What its result names as not removed (for the Windows uninstaller's clean-up, its message box
and `uninstall-clean-up.log`) stays as those sections describe it, and so does
everything where it was not run (on the server form, where the clean-up leaves something in effect, `octoboardd
uninstall` removes nothing; with `--keep-changes` it runs none: see "What uninstalling removes" under "The server form"
below). Octoboard's own directories and the conversation history sharing moved into the default account's directory stay
whether it ran or not.

## The desktop application on macOS

### What uninstalling removes

The application is uninstalled by deleting `Octoboard.app`, which removes the application and the daemon inside it.
Nothing runs when it is deleted, so nothing else is removed. Quit Octoboard first: a quit ends every session (see
"Quitting" in `docs/product/application-lifecycle.md`).

### What stays in Octoboard's own directories

- **`~/.octoboard`**, the daemon's data directory: consoles, session records, settings, logs and everything else
  `docs/product/files-octoboard-owns.md` lists. To remove it: `rm -rf ~/.octoboard`. A later installation takes it up
  again as it is, so removing it loses every console, session record and setting.
- **`~/Library/Application Support/dev.octoboard.desktop`**, the application's config directory and, on macOS, its
  local data directory too: `window-state.json`, `daemon-window-state.json`, `remote-daemons.json`,
  `remote-windows.json`, `settings.json` and `remote-files/`, the copies of remote files opened on this computer with
  the lock beside them (see "The desktop application's own files" in `docs/product/files-octoboard-owns.md`). To
  remove it, delete the folder.
- **What an earlier build left in the temporary directory**: builds that kept the copies of remote files in
  `dev.octoboard.desktop/` under the system's temporary directory left `remote-files/` and `remote-files.lock` there. A
  later build removes them at its first start once no instance of such a build still runs and holds the lock (one that
  does postpones it to a later start), and the folder only where that leaves it empty: macOS may keep other things of
  its own there, such as `WebKit/` (see "The desktop application's own files" in
  `docs/product/files-octoboard-owns.md`). Where no such start came before the uninstall, quit any earlier build and
  delete `remote-files/` and `remote-files.lock` in `dev.octoboard.desktop/` in the temporary directory (`$TMPDIR` in a
  terminal), and the folder itself only where it is then empty.

### What else stays

- **The command-line link**, where **Remove** was not used first: a symbolic link named `octoboardd` in
  `/usr/local/bin`, or in `~/.local/bin` where installing there was refused. With the application gone it leads
  nowhere: a terminal passes over it, so `octoboardd` runs another file of that name in a later directory of the
  `PATH` where there is one, and otherwise finds nothing to run; it has no other effect. To remove it:
  `rm /usr/local/bin/octoboardd` (which may need `sudo`) or `rm ~/.local/bin/octoboardd`. Installing may have created
  either directory where it was missing; it stays, empty, being a standard directory other tools use too.
- **The login item**, where Launch at login was not turned off first. On macOS 26.2, the entry stayed under
  **System Settings > General > Login Items & Extensions > Open at Login** after signing in again with the
  application still in the Trash and after a separate sign-in with the Trash emptied. Octoboard was not
  running after either sign-in. To remove the entry, select it and click the minus button.
- **This computer's key on each remote host** whose key was not taken back: one line in that user's authorized keys,
  marked `Octoboard on <this computer's name>` (see "Device keys" in `docs/product/remote-daemons.md`). It lets anyone
  holding its private half sign in to that host as that user. To remove it, delete the line with that mark from the file
  on the host (`~/.ssh/authorized_keys` on a Linux or macOS host; on a Windows host the file named in "Device keys").
- **The private half of each such key**, in the login keychain: a generic password whose service is
  `dev.octoboard.desktop.remote-daemon-key` and whose account is the saved daemon's id, one per saved remote daemon
  whose key is installed. When taking a key back, Octoboard asks Keychain to delete the private half even if the
  remote host cannot be reached. If Keychain reports a deletion error, Octoboard reports the item's service and
  account so it can be removed in Keychain Access. Moving the application to the Trash does not perform a Keychain
  clean-up.
- **What macOS keeps under the application's identifier**, `dev.octoboard.desktop`, as for any application: the web
  view's data and caches in `~/Library/WebKit/dev.octoboard.desktop` and `~/Library/Caches/dev.octoboard.desktop`;
  where macOS created them, `~/Library/HTTPStorages/dev.octoboard.desktop`,
  `~/Library/Preferences/dev.octoboard.desktop.plist` and
  `~/Library/Saved Application State/dev.octoboard.desktop.savedState`, any of which may be absent; the privacy
  permissions granted to it (see `docs/product/macos-privacy-permissions.md`) and its notification settings. On
  macOS 26.2, WebKit data, caches, a Desktop Folder grant and the edited notification settings survived moving the
  app to the Trash and emptying it. HTTPStorages, Preferences and Saved Application State were absent throughout
  that check. System Settings can hide an app's permissions and notification entries while the bundle is absent.
  These records have no effect without the application. The folders and files can be deleted. Where their entries
  appear, permissions can be removed in Privacy & Security and notification settings can be changed in Notifications.

## The desktop application on Windows

### What uninstalling removes

The application is uninstalled with its uninstaller (see "Uninstalling" in
`docs/product/installing-the-desktop-app.md`). It removes the installed application, and, before that:

- on an uninstall proper, everything the clean-up before uninstalling clears, which it has the application clear first:
  this computer's keys on remote servers and their private halves, private halves left from keys taken back earlier,
  the login entry, the links of accounts that share their agent's default configuration (with their backups restored)
  and the jump list (see "The Windows uninstaller's clean-up" in `docs/product/clean-up-before-uninstalling.md`). What
  it could not clear stays, as "What else stays" below and "What stays among the agents'
  files" describe it, and is named at the end of an interactive uninstall and in
  `%USERPROFILE%\.octoboard\logs\uninstall-clean-up.log`. The uninstall a newer setup runs in place runs none of it;
- the login entry's two registry values: `Octoboard` under
  `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, which Tauri's own uninstaller removes, and the
  value of the same name under
  `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`, which Octoboard's
  uninstaller hooks add;
- the taskbar's jump list. Tauri's own uninstaller removes the list kept under the application's
  identifier, `dev.octoboard.desktop`; Octoboard's hooks start the application's executable as
  `Octoboard.exe --delete-jump-list dev.octoboard.desktop`, which removes the executable's own list (named by no
  application id) and the one under the identifier, and does nothing else. Windows keeps the list as one file,
  `c553eab74fb82b0f.customDestinations-ms` in `%APPDATA%\Microsoft\Windows\Recent\CustomDestinations\`, which each of
  the two removes.

No upgrade made by Octoboard's own update or by a setup runs the clean-up: this computer's keys on remote servers and
the accounts' sharing stay as they were (an upgrade a tool makes by running the installed uninstaller plainly first
does run it; see "When it runs" in `docs/product/clean-up-before-uninstalling.md`). The
in-app update starts the new version's setup in update mode (`/UPDATE`), which runs no uninstall at all: the login
entry and the jump list stay as they were too. A newer setup the user runs interactively offers
"Uninstall before installing", chosen by default; that uninstall runs in place and does not run the clean-up, but
removes both registry values and the jump list, so after such an upgrade Launch at login is off, and its switch shows
it off, until the user turns it on again. A newer setup run in passive mode (`/P`) or silently (`/S`) runs no uninstall,
and leaves both as they were. The login entry does not rely on the uninstaller: turning Launch at login off removes it too. The jump list
has no switch: besides the uninstaller, only the clean-up before uninstalling removes it (see "The clean-up before
uninstalling" above).

### What stays in Octoboard's own directories

- **`%USERPROFILE%\.octoboard`**, the daemon's data directory, as on macOS: delete the folder to remove it. The
  uninstaller never removes it, "Delete the application data" ticked or not. Where the uninstaller's clean-up left
  something, `logs\uninstall-clean-up.log` in it lists what (see "How its result is told" in
  `docs/product/clean-up-before-uninstalling.md`).
- **`%APPDATA%\dev.octoboard.desktop`**, the application's config directory: `window-state.json`,
  `daemon-window-state.json`, `remote-daemons.json`, `remote-windows.json` and `settings.json`, as on macOS.
- **`%LOCALAPPDATA%\dev.octoboard.desktop`**, the application's local data directory: `remote-files/`, the copies of
  remote files opened on this computer with the lock beside them, cleared at the application's next
  start. It is apart from the config directory on Windows because a roaming profile or a redirected folder carries
  `%APPDATA%` to a server, and the copies have no size limit. The web view's data is in the same folder (see "What else
  stays" below).
- **What an earlier build left in the temporary directory**, as on macOS: `dev.octoboard.desktop\` in `%TEMP%`, removed
  at a later build's first start, or deleted by hand where none came.

Tauri's uninstaller removes the config directory and the local data directory only when its "Delete the application
data" box is ticked; otherwise both stay. Where one stays, delete the folder.

### What else stays

- **The login entry**, where the uninstaller did not run (the application's folder deleted by hand, say) and Launch at
  login was not turned off first: the two registry values above. The `Run` value names an executable that is gone; the
  `StartupApproved\Run` value only records that the entry was turned off in Task Manager. To remove them:
  `reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v Octoboard /f` and
  `reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" /v Octoboard /f`.
  This includes an entry turned off in Task Manager: the switch shows it off while its `Run`
  value is still there, and turning the switch on and then off again, the clean-up before uninstalling, or uninstalling
  removes it.
- **The taskbar's jump list**, where the uninstaller did not run, and the clean-up before uninstalling did not remove it
  (or the application was started again after it did): the tasks Remote devices and Open this PC's window,
  and one line per saved remote daemon (see "Where remote daemons are reached from" in
  `docs/product/remote-daemons.md`). The shell writes it whenever it builds its menu; there is no switch for it.
  Windows keeps it outside the application's folders, as
  `%APPDATA%\Microsoft\Windows\Recent\CustomDestinations\c553eab74fb82b0f.customDestinations-ms`; its lines start the
  application's executable, which is gone. To remove it, delete that file.
- **The installer the in-app updater downloaded**, left in `%TEMP%`. It has no effect; delete it (not verified on
  Windows: no installer an in-app update downloaded has yet been looked for there).
- **The installer's own registry key** under `HKEY_CURRENT_USER\Software`, named after the product (`Octoboard`, below
  a key named after the manufacturer), holding the install location and the installer's language. It has no effect
  without the application. Tauri's uninstaller removes it only when "Delete the application data" is ticked; otherwise
  delete the key in the Registry Editor.
- **This computer's key on each remote host**, as on macOS, where its key was not taken back first and the
  uninstaller's clean-up did not run or could not take it back. Where it could not (a host that did not answer, say),
  its message box and log name the line's mark, file, user and host.
- **The private half of each such key**, where neither took it out, in the Credential Manager: a generic credential named
  `dev.octoboard.desktop.remote-daemon-key:<saved daemon id>`, one per saved remote daemon whose key is installed,
  and one for a saved daemon whose key was taken back where the Credential Manager did not remove its private half
  then. Nothing uses it once the application is gone. To remove it, delete it under Windows Credentials in the Credential
  Manager.
- **The web view's data and caches**, which WebView2 keeps in `%LOCALAPPDATA%\dev.octoboard.desktop`, the
  application's local data directory, its `EBWebView` folder among them. They have no effect without the application.
  They go with that directory when "Delete the application data" is ticked; where it stays, delete it.

## The server form

### What uninstalling removes

It is uninstalled with its own command, run on the server as the user it was installed for, `octoboardd uninstall`;
the install script's `curl … | sh -s -- --uninstall` hands over to it (see "Uninstalling" in
`docs/product/installing-the-server-form.md`). What it does, in order, is in "Uninstalling the server form" in
`docs/product/daemon-command-line.md`. Before anything is cleared, stopped or removed, it removes nothing where the unit
runs another file than this `octoboardd`, where a file to remove does not belong to the user running it, or where the
executable's directory cannot be written. Its first step is the clean-up before uninstalling: where something is in
effect (an account that shares its agent's default configuration), a daemon that runs is stopped and a daemon started
only for the clean-up clears it, and where that leaves something in effect (an account whose sessions are running,
say) it removes nothing and starts the stopped daemon again as it ran.
`--keep-changes` runs no clean-up and leaves what is in effect, with nothing left to clear it (see "What stays among the
agents' files" below). An `octoboardd` from before the command is removed by the install script itself, which clears
nothing first. Where it goes through, it removes:

- the running daemon, stopped as `octoboardd stop` stops it; where that is refused, it removes nothing, and where the
  reason was work that stopping would interrupt, `octoboardd uninstall --force` stops it regardless;
- the systemd user unit `octoboardd.service`, the one the script wrote or the archive's own copied by hand, disabled,
  stopped and deleted, the user's manager reloaded;
- lingering, turned off only where the install turned it on, as `~/.octoboard/install.lingering` records, and no other
  service of the user's is enabled;
- `octoboardd.previous`, every `.octoboardd.*` file and last `octoboardd` itself, in the install directory: what an
  update, a rollback or an interrupted install leaves beside the executable (see `docs/product/files-octoboard-owns.md`).

### What stays in Octoboard's own directories

- **`~/.octoboard`**, the daemon's data directory, which the uninstall names, with `rm -rf ~/.octoboard` as the way to
  remove it. Where lingering was left on because another service of the user's is enabled, or could not be turned
  off, `install.lingering` stays in it, and the uninstall prints `loginctl disable-linger <user>` as the way to turn
  lingering off.

### What else stays

- **The install directory** (`~/.local/bin`, or the one given with `--bin-dir`), and any line the user added to a shell
  profile to put it on the `PATH`. The directory is a standard one other tools use; the `PATH` line names a directory
  that no longer holds `octoboardd`, and is removed by editing the profile.
- **Lingering that was on before the install**, which the install left as it was and the uninstall leaves too;
  lingering an install by a script that kept no `install.lingering` turned on; and lingering the install turned on
  that the uninstall left on because other services of the user's are enabled, which it names: those are the links in
  any `*.wants/`, `*.requires/` or `*.upholds/` folder of `${XDG_CONFIG_HOME:-~/.config}/systemd/user/`, or of the
  generators' directories of the user's runtime directory, other than `octoboardd.service`. Lingering keeps the user's services running after logout and starts them at boot. To turn it
  off: `loginctl disable-linger <user>`, which may need `sudo`.
- **The directory `~/.config/systemd/user`** (under `$XDG_CONFIG_HOME` where it is set), which installing the service
  creates where it is missing and the uninstall leaves, empty or not; and `default.target.wants` in it, which
  installing creates too and which stays where other links are in it; where disabling `octoboardd.service` leaves it
  empty, systemd itself removes it (as systemd 257 does). They are the standard directories systemd reads the user's units from, and
  have no effect; empty ones can be deleted with `rmdir`.
- **The daemon's lines in the user's journal**, which `journalctl --user -u octoboardd` shows. journald keeps them under
  its own retention; they have no effect and age out, or `journalctl --user --vacuum-time=<age>` removes older entries.
- **This server's authorized keys**: a desktop application that connected to it may have added one line, marked
  `Octoboard on <that computer's name>`, to `~/.ssh/authorized_keys`. That application takes it back when its key is
  turned off, the saved daemon removed or its own clean-up before uninstalling run; otherwise delete the line. It is
  that application's, not this server's daemon's, so `octoboardd uninstall` on this server does not touch it.

## What stays among the agents' files

These stay after every installation is uninstalled, on the machine the daemon ran on.

**Nothing Octoboard gave an agent stays**: its hooks, MCP server and role description reach the agent on each launch
only, so an agent run on its own afterwards starts without them (see "What Octoboard never modifies" in
`docs/product/launching-agents.md`).

- **Entries for consoles' directories in the agents' trust lists**: the entry Claude Code records for a console's
  directory, `~/.octoboard/consoles/<console id>`, and, only where a git repository holds that directory, the entry
  Codex records for the repository's root. They stay; Claude Code's names only a directory of Octoboard's own and has
  no effect anywhere else, and Codex's covers its repository, as any Codex trust entry does. Grok Build records none for
  a console, but an earlier version of Octoboard may have carried one into its `trusted_folders.toml`. Where each is
  kept and how to remove it is in "Console sessions' trust confirmations" in `docs/product/folder-trust.md`.
- **Sharing the default configuration**: the conversation history sharing moved into the default account's directory
  stays there, but for the conversations of the account's own sessions that turning sharing off took back; so do the
  empty directories turning sharing on created there, which are directories the agent's client keeps there anyway (see
  "What turning sharing off leaves" in `docs/product/sharing-the-default-configuration.md`). An account still sharing
  when Octoboard is uninstalled keeps its links into the default directory, so its agent goes on reading the default
  configuration, and the backups of its own configuration (`<entry>.bak-<stamp>`) stay in its directory; turning
  sharing off first, at its switch or by the clean-up before uninstalling (which the server form's `octoboardd
  uninstall` does as its first step, and the Windows uninstaller on an uninstall proper), undoes both; once Octoboard is removed, nothing of it does, and the links can be
  deleted by hand. On the server form this is what `octoboardd uninstall --keep-changes` leaves, and what the install
  script's own removal of an `octoboardd` from before the command may leave.
- **Conversations an account switch copied** into the target account's directory stay there, beside the originals,
  as the agent's own (see "Carrying the conversation" in `docs/product/switching-session-accounts.md`).
- **What else Octoboard's operations wrote into the agents' configuration** stays as it was left, the agent's or the
  user's own from then on: Grok Build's carried trust entries for projects, its settings written back from a session and
  the plugin paths rewritten in its registry, and agent records moved to the Trash (see "What Octoboard never modifies"
  in `docs/product/launching-agents.md`).
- **Account directories other than the default one**, which the user named when adding the accounts: each stays with
  its login, settings and conversation history, as removing the account in Octoboard leaves it unless its option to
  move the directory to the Trash is ticked (see `docs/product/removing-accounts.md`). Deleting one by hand loses all
  three.
- **The query homes** Grok Build's background runs use, under `~/.octoboard/agent-query-homes/`, are inside
  `~/.octoboard` and go with it.
- **What the agents' CLIs wrote themselves** — their conversation records, caches and logs, the trust they record when
  a confirmation is answered, a login's credentials, and the first-run set-up Claude Code makes in a config directory
  it has not run in — is theirs, not Octoboard's, and stays as it does after any use of the agent.
