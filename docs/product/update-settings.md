# Update settings

Settings' **Updates** section, directly above About, holds everything about updating Octoboard, in three rows:
**Version**, **Check for updates automatically** and **Update channel**. It is there wherever something updates itself:

- in the **desktop application**, on macOS and on Windows, where it updates the application (see
  `docs/product/in-app-updates.md`);
- in a **browser open on a daemon that updates itself** — the server form's official release build (see "Which daemons
  update themselves" in `docs/product/server-form-updates.md`) — where it updates the daemon.

Anywhere else, a browser on any other daemon included, the section is absent. The rows are the same in both; where they
differ, it is said below.

## The Version row

**Version** says in a sentence where the updates stand, the first that applies of:

1. in a browser, the daemon running another version than the page was loaded for: that Octoboard now runs that version,
   and to reload the page to use it (see "What the browser shows" in `docs/product/server-form-updates.md`);
2. an update installed, which takes effect after Octoboard restarts — on Windows, an update downloaded, which is
   installed when Octoboard quits or restarts (see "Installed, waiting for a restart" in
   `docs/product/in-app-updates.md`) — naming its version where it is known;
3. a version being downloaded;
4. a check under way;
5. in a browser, a newer version available that the daemon cannot install itself: naming that version and the file the
   daemon cannot replace, and saying that the owner of its folder has to run the install command on that machine;
6. a newer version available, with the version running;
7. the version running being the latest on the channel;
8. otherwise — before any check, or after a check that did not finish — only the version running.

Under the sentence a second line says when the last check ended — "just now" for under a minute, otherwise how long
ago, kept current as time passes — once a check has ended, except while a check runs or while the warning mark below
is shown. In the desktop application that is the last check of this run of the application; in a browser it is the
daemon's last check, the same in every page.

Beside the row one button moves the update on:

- **Reload**, in a browser whose daemon now runs another version (case 1 above), which reloads the page;
- **Restart now** once the update is installed, showing a pending spinner as "Restarting to apply an update" in
  `docs/product/in-app-updates.md` says; in a browser it restarts the daemon (see "Restarting into the update" in
  `docs/product/server-form-updates.md`);
- **Download and install** once a newer version is found that can be installed from here, showing the spinner while a
  check or the download is under way;
- otherwise **Check for updates**, showing the spinner while a check runs. In a browser it shows it from the press until
  the daemon's status shows the check started; a press the daemon lets pass, within 30 seconds of the end of its last
  check or download (see "When the daemon checks" in `docs/product/server-form-updates.md`), keeps the spinner a moment
  longer and then says, to assistive technology too, when the last check ended.

**Installing by hand.** While the daemon cannot install the update itself (case 5), a block under the row, labelled
**Install command**, shows the install command the daemon gives for its executable's directory and channel (see
"Downloading and replacing the executable" in `docs/product/server-form-updates.md`), such as
`curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh -s -- --bin-dir /usr/local/bin`, with a
button that copies it; the row's button stays **Check for updates**, and there is no download.

Before the button, a **What is new** link, drawn as a button, opens in the browser the GitHub release page of the
version the row's sentence is about, `https://github.com/baijunjie/octoboard/releases/tag/desktop-v<version>` (see
"Where updates come from" in `docs/product/in-app-updates.md`): the newer version while one is available or
downloading, the installed version once an update is installed, and otherwise the version running. There is no link
while the version running is not yet known, nor for an installed update whose version is not known. When the link goes
away while it holds keyboard focus — the row turning to an installed update of unknown version — focus moves to the
row's button.

The row's controls are, in order: What is new, the warning mark described below when it is shown, and the row's
button.

**After a check that did not finish**, pressed here or automatic, while no newer version is known and nothing is under
way, an amber warning mark stands in the row before **Check for updates**, named for the last update check; the row's
sentence stays the version running. Its popover says that the last check for updates did not finish, that it is not
known whether a newer version exists, and that pressing **Check for updates** tries again; it has no button of its own,
the row's button being the retry, and it opens as a row's mark does (see "Failures, warnings and notes marked at their
point" in `docs/product/error-alerts.md`). The mark goes once a check starts; keyboard focus on the mark or in its
popover then goes to **Check for updates**. In a browser every page shows it, whichever page or schedule ran the check.

While Settings is open, once the state of the update changes — through a press here, a check that was already running
when Settings opened ending, a download started from the notice finishing, or an automatic check — every change of the
sentence is announced, except the sentence of a check under way, which the button's pending state already says, and
the sentence after a check that did not finish. That the last check for updates did not finish is announced instead,
except when the check that did not finish was the one **Check for updates** started here, whose failure its error
alert says. A failed automatic check is announced even after an earlier check was pressed. What the row said
when Settings opened is not announced.

## Check for updates automatically

A switch, **on to begin with**: whether Octoboard looks for updates by itself. Its line says, while on, that Octoboard
checks for updates in the background and nothing is downloaded or installed until the user chooses to; while off, that
turning it on lets Octoboard check in the background, on the same terms. Off, only the checks the user asks for are made
— **Check for updates** here and, in the desktop application, the menu bar icon's update entry; nothing is ever
downloaded, installed or restarted by itself either way. Turning it on checks at once.

- **In the desktop application** it is the client's own choice, kept in the window's own browser storage as the update
  channel is (see "Update channels" in `docs/product/in-app-updates.md`), and takes effect as it is turned. The daemon
  neither stores nor carries it. When it checks is in "When the application checks" in the same document.
- **In a browser** it is the daemon's setting `daemon_update_check`, shared by every page, and saves as the daemon's
  other switches do (see "How switches save" in `docs/product/settings.md`). Agents read and change it through the
  settings tools (see "Application settings: `get_settings` and `update_settings`" in
  `docs/product/agent-settings-tools.md`). When the daemon checks is in "When the daemon checks" in
  `docs/product/server-form-updates.md`.

Neither is part of a configuration export (see "Which settings travel" in
`docs/product/settings-in-a-configuration.md`).

## Update channel

A select between **Stable** and **Beta**, listed in that order, with a line saying that Stable gets only stable releases
and Beta every release, stable ones included. Neither form's choice is part of a configuration export.

- **In the desktop application** it is the client's own choice, as "Update channels" in
  `docs/product/in-app-updates.md` describes, which also says what a switch does. With nothing chosen it shows the
  channel the running build follows by default (Beta on a prerelease build, Stable otherwise); showing it saves nothing,
  and neither does picking the channel already shown: only switching to the other one saves a choice. So a beta user who
  wants to stay on Beta past a stable release switches to Stable and back.
- **In a browser** it is the daemon's setting `daemon_update_channel`, Stable unless changed, shared by every page, and
  saves as the daemon's other settings do (see "How switches save" in `docs/product/settings.md`); what a switch does is
  in "Channels" in `docs/product/server-form-updates.md`.
