# Documentation Index

Pointers to the long-lived documentation under `docs/`, grouped by area; open the one whose question matches yours.
Development plan docs and bug tickets are temporary and are not listed here.

Where to start, by question:

- What is it built with, and where is the code? The [project map](project-map.md), then the end's structure doc under
  "Code" below.
- Where does new code go, and which helper must it use? The end's structure doc.
- What rules must code follow? The coding conventions below; they are binding, so read the ones a change touches before
  designing it.
- How does one module work? The `README.md` beside the module, linked from its structure doc.
- What does the product do? The product docs below. Why is it built this way? [Architecture](architecture.md).
- How should I work in this project? The development memory below.

## Product

### Sessions

- [Sessions](product/sessions.md) — console versus project sessions, coordinator and worker roles, reporting
  relationships, where sessions are listed, locking and renaming.
- [Opening a session](product/opening-sessions.md) — the session dialog and how the agent, account, model and effort of
  a new session are chosen.
- [Session statuses](product/session-statuses.md) — what each status and its glyph means (Paused and Dormant included),
  what it is derived from, the top status a project, chip or avatar shows, and the raised hand: the notification, Dock
  badge and rail count for a session waiting on the user.
- [Turns the agents' hooks do not report](product/turns-hooks-do-not-report.md) — the turn endings no hook event
  announces and how the status copes.
- [Program status reports](product/program-status-reports.md) — OSC 7501 reports a program in a session's terminal
  makes about itself, which every session's terminal supports: the support query, how long records last, and how they
  become the status of a session whose agent has no hooks.
- [Session titles](product/session-titles.md) — created names, displayed names and how a title follows the agent's.
- [Session context size](product/session-context.md) — how the context size is measured, refreshed and shown as a chip.
- [Session history](product/session-history.md) — the read-only history shown for an archived session.
- [Live conversation feed](product/live-conversation-feed.md) — what a client following one session's conversation
  receives: entry kinds, tool calls, pages, live changes and pending entries, replacement and end, limits, and what each
  agent supplies.
- [Archiving and resuming sessions](product/archiving-and-resuming-sessions.md) — how archiving or an interruption ends
  a session's process and how an interrupted or archived session comes back.
- [Background processes](product/background-processes.md) — processes started by agents, automatic cleanup, keeping
  processes running, carrying them over on a handoff, account switch, model change or change of an account's sharing,
  moving them between sessions, project processes and process choices when archiving.
- [Processes panel](product/processes-panel.md) — the right pane's Processes panel: the session filter, project
  processes, a process's card, Stop and Keep.
- [Resuming a conversation by its session ID](product/resuming-a-conversation-by-its-session-id.md) — opening a
  project session on an existing conversation found by its ID.
- [Deleting sessions](product/deleting-sessions.md) — deleting a session for good rather than archiving it.
- [Switching a session's account](product/switching-session-accounts.md) — moving a session to another account of its
  agent and carrying its conversation.
- [Automatic account switch](product/automatic-account-switch.md) — switching a session whose turn stopped at its
  account's usage limit, waiting for quota recovery, and restoring it when a switch fails.
- [Opening a session in its agent's app](product/opening-a-session-in-its-agents-app.md) — opening a session's
  conversation in the agent's own desktop app, archiving it first unless it is already archived: which agents have one,
  when it is offered and allowed, the confirmation, what the archive takes along and keeps, the warning when the app
  may not see the conversation, and the failures.
- [Handing a session off to another agent](product/handing-off-sessions.md) — moving a session's work to a new session
  of a different agent.
- [Changing a session's parent](product/changing-session-owners.md) — transferring to a parent, working independently
  and setting a parent, and how those transitions affect the role.
- [Interrupting a session's turn](product/interrupting-turns.md) — how a parent stops a child session's running turn.
- [Message delivery](product/message-delivery.md) — how messages are written into a running session and held when it
  cannot take them.
- [Slash commands and image messages](product/slash-commands-and-image-messages.md) — `send_command` and images on
  `send_message`: the completion check, writing images and text through the terminal or Codex's queue, and why neither
  is ever carried to another process as text.
- [Answering an agent's prompts](product/pending-interactions.md) — how a session's permission prompt or ask-the-user
  questions are published as `Session.interaction` and a client's answer pressed into the terminal, per agent, and
  when the hand comes down.
- [Draft preservation](product/draft-preservation.md) — how an automatic message is kept from being sent with the
  user's half-typed draft: the native stash, the grace period, clearing the draft and handing it back, and which keys
  hold a message.
- [Messages held after a usage limit](product/messages-held-after-a-usage-limit.md) — how Octoboard's messages are
  held once a session's turn stopped on its account's usage limit, the user's own messages, and what releases the hold.

### Orchestration and agent tools

- [Orchestration](product/hub-orchestration.md) — coordinator and worker capabilities, parent and child relationships,
  tools, briefs and routing.
- [Session requests](product/session-requests.md) — a session asking the user to start an independent session.
- [Requests to resume a conversation](product/requests-to-resume-a-conversation.md) — a session asking the user to
  resume a conversation by its ID.
- [Reporting to a parent session](product/reporting-to-owners.md) — how a project session with a parent reports a round
  of work, when it archives itself, and what is kept for a parent that is interrupted.
- [Message headers](product/message-headers.md) — how every text a session or Octoboard writes into a session is headed
  (sender, relationship, nature), the exact header of a parent session's instruction, and what sessions are told about
  headers.
- [Information between sessions](product/information-between-sessions.md) — what a session can see of the other running
  sessions, how it tells them something, and what is kept for an interrupted session.
- [What every session reads](product/what-every-session-reads.md) — the read-only MCP tools open to all sessions.
- [Settings tools for agents](product/agent-settings-tools.md) — the MCP tools that read and change projects, consoles
  and application settings.
- [Which session is working on a branch](product/branch-responsible-session.md) — the responsible-session record for a
  branch and its transfer.

### Agents, accounts and quotas

- [Supported agents](product/supported-agents.md) — the agents Octoboard supports, their ids and order, how an agent
  the window has no presentation for is shown, and what happens to stored data of an agent the build does not support.
- [Octoboard's memory](product/memory.md) — the console and project memory every agent shares: the switch that turns it
  on (and the agents' own memory off), entries and their limits, the session tools, the index a session starts with,
  memory left behind by removed projects, and the memory dialog and Settings section.
- [Launching agents](product/launching-agents.md) — what every agent launch injects, what is never modified, and the
  launch environment.
- [Per-agent launch specifics](product/per-agent-launch-specifics.md) — what Claude Code, Codex and Grok Build each do
  differently at launch.
- [Compatibility between agents](product/compatibility-between-agents.md) — what a launch is given while a
  compatibility switch is on (Claude compatibility for Codex and Grok Build, Codex compatibility for Claude Code and
  Grok Build): MCP servers, skills and prompts, and instructions, with the trust, naming and left-out rules.
- [Compatibility switches in launch settings](product/compatibility-switches-in-launch-settings.md) — the
  compatibility switches a console, a project or a session holds beside its model and effort: the follow / On / Off
  states, which value a launch uses, the pickers in each dialog, and what sessions may change.
- [Model and reasoning effort](product/model-and-reasoning-effort.md) — the Advanced section's model and effort pickers
  and what a session shows running.
- [Agent accounts](product/agent-accounts.md) — Settings' Agent accounts section: added agents, the default account,
  owned accounts, the accounts table and the account details and settings dialogs.
- [Agent CLI versions and updates](product/agent-cli-updates.md) — when an agent CLI's installed and latest versions
  are checked, how its install method and update command are told, and how an update the user starts runs and ends.
- [Per-agent settings](product/per-agent-settings.md) — settings that apply to every account of one agent: the settings
  each provider declares (permission approval modes, Claude and Codex compatibility, simple shell commands) and agent
  tags.
- [Removing an account](product/removing-accounts.md) — what removing an account clears, its confirmation, and the option
  to move the account directory to the Trash.
- [Logging an account in](product/logging-accounts-in.md) — the login dialog and how each agent signs in.
- [Account MCP servers and connectors](product/account-mcp-servers-and-connectors.md) — reading an account's platform
  connectors and local MCP servers (Claude Code, Codex, Grok Build), the Connectors and MCP dialog opened from an
  account's actions menu, and signing a Codex account in to and out of an MCP server.
- [Sharing the default configuration](product/sharing-the-default-configuration.md) — an account sharing the agent's
  default configuration and how a change is carried out.
- [Account quotas](product/account-quotas.md) — which accounts have a plan quota, how it is shown, its states, and when
  it counts as used up.
- [Account quota readings](product/account-quota-readings.md) — how and when Octoboard reads a quota through the
  agent's CLI, the sign-in email a reading carries, what a reading leaves behind, and what the daemon's log records of
  readings.
- [Listing agents from a session](product/listing-agents-from-a-session.md) — the `list_agents` session tool:
  the agents a session could be opened on, their tags and accounts, its fields and refresh rules.
- [The account quota panel](product/account-quota-panel.md) — the rail's Account usage panel.
- [Processes on the rail](product/processes-on-the-rail.md) — the rail's Processes button and its popover: the
  running processes counted per project and console session, and the jump to each one's Processes panel.
- [Starting a usage window](product/starting-usage-windows.md) — kicking a usage window that has not begun counting.
- [Codex usage limit resets](product/usage-limit-resets.md) — how a Codex account's usage limit resets are read and
  shown, redeeming one from the rail's panel or the account details dialog, and why Claude's limit resets are not shown.
- [Folder trust](product/folder-trust.md) — the agents' trust confirmations, the trust permission and trusted folders.
- [macOS privacy permissions](product/macos-privacy-permissions.md) — the Screen Recording and Accessibility
  permissions agents in terminals can need, and the other macOS consents programs in Octoboard's terminals can ask for.

### Projects, consoles and git

- [Consoles](product/consoles.md) — what a console owns: agents, avatar, instructions, config directories, editing and
  deleting.
- [The console switcher](product/console-switcher.md) — picking the console the sidebar shows.
- [Projects](product/projects.md) — what a project carries, how it is associated, named and edited, and hosts.
- [Project tags](product/project-tags.md) — free-form tags on projects and how they are picked and normalised.
- [Removing a project](product/removing-projects.md) — what a removal takes with it and its confirmation.
- [Project git status](product/project-git-status.md) — the branch badge, when a project is checked and automatic
  pulling.
- [Project Git actions](product/project-git-actions.md) — pulling (with the merge confirmation), pushing, switching
  branch and restoring changes from a project's menu.
- [Git workflow settings](product/git-workflow-settings.md) — where a project session develops and the development
  rule.
- [The revert gate](product/revert-gate.md) — the guard against quietly undoing changes already on a branch.
- [Keeping sessions to their own project](product/keeping-sessions-to-their-own-project.md) — the cross-project guard:
  which edits and git updates in another registered project it refuses, what passes, what the agent is told.
- [Branch and worktree cleanup](product/branch-and-worktree-cleanup.md) — removing a session's branches and worktrees
  when it is archived or deleted.
- [Configuration export and import](product/configuration-export-and-import.md) — the configuration document and its
  regions (consoles and projects, application settings, the account structure) and what an import does with each.
- [Settings in a configuration](product/settings-in-a-configuration.md) — which settings a configuration document
  carries in which region, the ones only a client holds, and how an import applies each.
- [The configuration export and import dialogs](product/configuration-export-and-import-dialogs.md) — the export
  dialog's region choices and the import dialog's file step and region-tabbed review.
- [Configuration import progress and result](product/configuration-import-progress-and-result.md) — what the import
  dialog shows while an import runs and the per-region result after it finishes.

### Window and interface

- [Window layout](product/window-layout.md) — the chrome, top bar, rail, panes, minimum size and responsive layout.
- [Pane docking and resizing](product/pane-docking.md) — docking capacity, pane widths, resizing, hiding and hover previews.
- [The floating sidebar](product/sidebar-drawer.md) — the sidebar as a floating overlay when it cannot dock, and at 768 px
  and below the rail coming in with it as one card.
- [Touch swipes](product/touch-swipes.md) — at 768 px and below, finger swipes that open, close and switch the floating sidebar
  and right-pane panels.
- [Window size and position](product/window-size-and-position.md) — where the window opens across launches.
- [Sidebar](product/sidebar.md) — the console header, session and project lists, and how selection works.
- [Project rows](product/sidebar-project-rows.md) — a project row's actions in the sidebar.
- [Session rows](product/sidebar-session-rows.md) — a session row's shortcuts, binding badge and lock controls.
- [Order of projects and sessions](product/order-of-projects-and-sessions.md) — how the sidebar orders rows and how
  pinning works.
- [Focus mode](product/focus-mode.md) — the sidebar replacement showing one project's or console session's view.
- [The welcome screen](product/welcome-screen.md) — what the window shows with no console.
- [Right pane panels](product/right-pane-panels.md) — the stack of panels in the right pane: cards, floating,
  resizing and maximizing, and what clears a panel covering the terminal's area.
- [The archive view](product/archive-view.md) — the full archive of a project, console session or console.
- [Navigation history](product/navigation-history.md) — the Back and Forward buttons and what counts as a place.
- [Moving focus between regions](product/moving-focus-between-regions.md) — F6 navigation between the window's
  regions.
- [Keyboard shortcuts](product/keyboard-shortcuts.md) — the changeable shortcuts, how they are matched and shown.
- [Settings](product/settings.md) — the Settings dialog itself: opening and closing, its sections and moving between
  them, the narrow layout, how switches save, and the short sections (Appearance, Trusted folders, Permissions, Keyboard
  shortcuts, Cleanup, Configuration) with pointers to the docs of the others.
- [General settings](product/general-settings.md) — the General section's rows: language, launch at login, resume
  sessions, background run, menu bar icon, Prevent sleep, closing idle terminals, sorting.
- [Git settings](product/git-settings.md) — the Git section's rows: default clone directory, automatic pull,
  Development and Protection.
- [Agent behavior settings](product/agent-behavior-settings.md) — the auto-approval switches and Ask questions as
  choices.
- [Notification settings](product/notification-settings.md) — the Notifications row per client, asking a browser, and
  opening the system's notification settings.
- [About Octoboard](product/about-octoboard.md) — the About section's rows and the third-party licenses dialog.
- [Searching Settings](product/searching-settings.md) — the Settings search field: what a query matches, how sections,
  rows and groups are filtered, the empty state, and Find and Escape on the field.
- [Appearance](product/appearance.md) — light, dark and follow-the-system, and what follows the choice.
- [Language](product/language.md) — the offered languages, the one language each daemon holds and every window of it
  follows, how it is chosen and stored, and the windows that show no daemon.
- [Right-to-left layout](product/right-to-left-layout.md) — what is mirrored under a right-to-left language.
- [Dialogs](product/dialogs.md) — dialog titles, initial focus, what Enter and confirm buttons do, and where an unasked question stands among other dialogs.
- [Error alerts](product/error-alerts.md) — where a failure, warning or note is shown, by how it arises.
- [Concurrent edits](product/concurrent-edits.md) — the same setting, console, project or account changed from several
  places at once: which writes are checked against field versions, which are not, and how an open form follows a
  change made elsewhere.
- [Issue reports](product/issue-reports.md) — what Report issue prefills into a new Issue and what is removed from
  it first.
- [Toasts](product/toasts.md) — the notices that need no answer.
- [Empty states](product/empty-states.md) — how a view with nothing to show says so.
- [Labels and tooltips](product/labels-and-tooltips.md) — tooltips, help tips and handling of names that do not fit.
- [Controls unavailable for a reason](product/controls-unavailable-for-a-reason.md) — a button, switch or checkbox
  that cannot be used: faded, no response to a press, the reason in its tooltip only.
- [Scrolling surfaces](product/scrolling-surfaces.md) — edge fades and keyboard scrolling.
- [Collection motion](product/collection-motion.md) — transitions for items entering, leaving and moving in
  collections.

### Terminals, files and the project pane

- [The session terminal](product/session-terminal.md) — the terminal showing the selected session: typing, the mouse
  and the macOS editing keys, typing waiting while the session's messages are held, and links; points to the docs
  below.
- [The session terminal with no session selected](product/session-terminal-with-no-session-selected.md) — what the
  terminal's area shows with no session selected, where focus goes, and the creation action in an empty console.
- [Copy, paste and selection in the session terminal](product/terminal-copy-paste-and-selection.md) — the terminal's
  editing menu (Copy, Paste, Quote, Select all, Open in on a path link), what Quote writes into a session's input,
  selecting with the mouse under mouse reporting, and text a program copies with OSC 52.
- [Terminal output, scrollback and size](product/terminal-output-scrollback-and-size.md) — attaching from a first
  paint, the scrollback, a client that falls behind the output, gutters and resizing, and colours.
- [Reconnecting and loading in the session terminal](product/session-terminal-reconnecting-and-loading.md) — automatic
  reconnecting and the disconnected panel, lost input, the cover while the session's process cannot be reached, and
  the loading state.
- [The terminal of interrupted and archived sessions](product/session-terminal-interrupted-and-archived-sessions.md) —
  resuming on selection, the Resume or Reopen card, an archived session's history, and the saved terminal preview.
- [Pasting an image into a session's terminal](product/pasting-images.md) — which pastes paste a clipboard image, the
  saved file's path that is pasted, where the file is kept and how a failure shows.
- [Attaching a file to a session](product/attaching-files.md) — how a client with no shared file system uploads any
  file in parts to a session's scratch directory, its limits, the saved file's name and how a failure shows.
- [Dropping files onto a terminal](product/dropping-files-onto-a-terminal.md) — how dropped files' paths are pasted,
  local paths in the desktop main window and uploads elsewhere, refusals, the upload queue and its notice.
- [Links in terminal output](product/terminal-links.md) — how web addresses and file paths in terminals become links.
- [Answers to a program's queries](product/terminal-queries.md) — which terminal queries Octoboard answers itself, the
  colours and sizes a program reads, and the queries the clients answer.
- [A terminal shown on several clients](product/terminals-on-several-clients.md) — which client counts as most
  recently active, how a terminal's size is decided while several clients show it, how clients are told the size, a
  client that only watches without sizing it, and where its colours come from.
- [Terminal panel](product/terminal-panel.md) — the right pane's Terminal panel: tabs, actions and the shell terminal.
- [Terminal font size](product/terminal-font-size.md) — the terminals' font size: the Appearance row, the shortcuts, how
  every open terminal follows it and where it is kept.
- [Shell terminals](product/shell-terminals.md) — plain shells the daemon runs for a project: ownership, title and
  closing.
- [Opening a project terminal](product/project-terminal.md) — which system terminal Open in › System terminal opens for a
  project and when it refuses.
- [Project pane](product/project-pane.md) — the Files panel and file tree of a project.
- [Project pane Git mode](product/project-pane-git-mode.md) — reviewing one branch's changes: selector, worktrees and
  what is remembered.
- [Project pane Git change list](product/project-pane-git-change-list.md) — the list of uncommitted and committed
  changes.
- [Project pane Git change viewer](product/project-pane-git-change-viewer.md) — the diff view of one change.
- [Finding in files](product/finding-in-files.md) — Find in files: opening it, the query and its switches, the file
  mask, what is searched and why there is no index, results and preview, opening a result, states, refusals and bounds.
- [File viewer](product/file-viewer.md) — the read-only dialog showing one file: header, stand-ins, wrapping,
  finding, navigation, files opened from the terminal and Download and open.
- [Markdown documents in the file viewer](product/markdown-documents.md) — how a Markdown file renders as a document.
- [Finding in the file viewer](product/finding-in-the-file-viewer.md) — Find in file: opening the find bar, what is
  searched, its switches and count, how matches are highlighted and stepped through, and closing it.
- [Zooming images in the file viewer](product/zooming-images-in-the-file-viewer.md) — the zoom bar, keys, wheel,
  pinch and double-click, zoom limits and steps, panning, crisp pixels past 100%, and how an image change's two sides
  zoom and scroll as one.
- [Report panel](product/report-panel.md) — the panel where a console session, or a project session that faces the
  user, pushes, withdraws and pages through its pages and receives form submissions, each page submitted at most once.
- [Report page components](product/report-page-components.md) — what a report page is built from: the component
  catalogue, the daemon's checks on a pushed page, form checks, tables and charts.

### Desktop app, daemon and website

- [Installing the desktop app](product/installing-the-desktop-app.md) — what the macOS and Windows apps run on, where
  releases are published, installing (WebView2, the agents' CLIs, Developer Mode, SmartScreen on the unsigned
  Windows installer) and uninstalling (what the Windows uninstaller removes).
- [The clean-up before uninstalling](product/clean-up-before-uninstalling.md) — the row in Settings' Cleanup section
  and its confirmation: what Octoboard's features have in effect outside its own directories, the one action that
  removes all of it once its word is typed, the result per item, and which installation each window speaks for;
  and the same clean-up as the Windows uninstaller runs it by itself, with how its result is told.
- [What remains after uninstalling](product/what-remains-after-uninstalling.md) — for the macOS and Windows apps and
  the server form: what each switch undoes before uninstalling, what uninstalling removes, what stays in Octoboard's own
  directories and elsewhere (command-line link, login entry, jump list, device keys, the system's records, the agents'
  trust entries, sharing's and account switches' leftovers), and how to remove each by hand.
- [Application lifecycle](product/application-lifecycle.md) — the desktop form from startup to quit: starting the
  daemon and restarting one that dies, losing the daemon connection, a daemon of another version, one window per
  daemon and remote windows, closing, quitting and restarting, restoring remote windows, crashes and forced
  termination.
- [The server form](product/server-form.md) — the daemon alone on a Linux server, reached from a browser over SSH: how
  it starts and stops, the owner-only browser socket and the SSH forward to it, the UI a server build serves and the
  page whose key is refused, `--port`, and the example systemd user unit.
- [Running in the background from the command line](product/daemon-command-line.md) — the
  `octoboardd start/stop/restart/status/address` commands (on Windows too, and against the desktop app's daemon),
  `address --rotate`, `proxy` and `address --json` for another device reaching the daemon over SSH, and `connection`:
  how they find the daemon, waiting out a planned restart caught half-way, exit statuses, and how a successor is
  recognised.
- [Planned restart and planned stop](product/planned-restart-and-planned-stop.md) — handing sessions and shell
  terminals over to their holders: how one is asked for, who starts the next daemon, `--service-managed`, what a
  planned stop keeps, while the terminals are handed over, and an ordinary stop during a planned stop.
- [Installing the server form](product/installing-the-server-form.md) — what the Linux server build runs on, what a
  release holds for Linux (archives, `.sha256`, `.sig`, the Linux manifests), the install script with `--service`, the
  ways to upgrade, and uninstalling with `--uninstall`.
- [Who can reach the daemon](product/who-can-reach-the-daemon.md) — the daemon's access model in both forms: loopback
  only, the launch key and where helpers keep it, the owner key a browser address carries (on the browser's way in
  alone) and its trade-off, SSH as the only remote path, remote access to the desktop app's daemon (Allow remote
  access's macOS socket and Windows owner pipe, Allow browser access's loopback browser port), other users of the
  machine and port takeover, and the origin and key checks.
- [Remote daemons](product/remote-daemons.md) — the desktop app reaching daemons on other machines over SSH: the
  Remote, Dock and Windows system menus and the jump list, the saved-daemon fields, connection details, every
  connection outcome, device keys, the remote window, Disconnect and restoring remote windows at launch.
- [Values a newer daemon sends](product/values-a-newer-daemon-sends.md) — what the window does with an enum value a
  newer daemon of the same protocol generation sends that it does not know: shown neutrally, left out or blank, and
  never acted on.
- [The Remote devices page](product/remote-devices-page.md) — the main window's page for listing, adding, editing,
  removing and connecting saved remote daemons: opening and leaving it and its place in Back/Forward, the device list
  in the sidebar, the settings form and its Paste button, the switch of this computer's key on a saved device,
  questions and failures handed to it, and using it while the local daemon cannot be used.
- [Scheduled tasks](product/scheduled-tasks.md) — having Octoboard start a session at a set time: what a task holds,
  the schedule kinds and the daemon machine's time zone, due runs, the single catch-up after a missed run, Run now, the
  account fallback, run records, the failed-run notification, what saving checks, and what deleting a console, project
  or account does to tasks.
- [The Scheduled tasks page](product/scheduled-tasks-page.md) — the page for listing, adding, editing, running and
  deleting scheduled tasks: the rail button and its running count, the task list in the sidebar, the header, the
  settings form and how it follows a task changed elsewhere, Run history, and its empty and failed states.
- [Leaving a form with unsaved changes](product/leaving-a-form-with-unsaved-changes.md) — the Discard changes question
  asked before a page form (Remote devices, Scheduled tasks) that holds unsaved changes is left: what asks, what does
  not, changes nobody asked for, and what cannot ask.
- [Links and files in a remote window](product/links-and-files-in-a-remote-window.md) — how a remote window forwards
  the remote machine's loopback addresses to this computer, the Forwarded ports button on its rail, and the copies
  Download and open keeps of remote files.
- [Access settings](product/remote-access-settings.md) — Settings' Access
  section: its groups (This computer's browser, Other computers, Key), the two switches and their confirmations,
  failures, the browser port, the local address, the SSH server row, the connection details with QR, the SSH steps for a
  browser on another computer and Rotate key.
- [Files Octoboard owns](product/files-octoboard-owns.md) — every file Octoboard writes under `~/.octoboard` and the
  desktop app's own state files, and the database's schema version check.
- [Process holders](product/process-holders.md) — the processes that run sessions and shell terminals so they outlive
  a daemon that goes down without stopping: what a daemon start takes over, the inexact screen, helpers through the
  holder, holder builds and their copies.
- [Launch at login](product/launch-at-login.md) — opening Octoboard when the user logs in: the system entry it reflects,
  waiting for approval on macOS, and the hidden start at a login launch.
- [Keeping the computer awake](product/keeping-the-computer-awake.md) — Prevent sleep, held by the daemon on its host:
  which hosts can hold it (macOS, Windows, Linux with logind), Always or while sessions work, which sessions count, what
  is held on each system, and how clients learn whether it is held.
- [In-app updates](product/in-app-updates.md) — the desktop application's updates: update channels, where updates come
  from, checking, notice, download and restart.
- [Server form updates](product/server-form-updates.md) — how the Linux server form's daemon updates itself: which
  daemons do, checking, the browser's notices, downloading and replacing the executable, restarting, failures and the
  `octoboardd update` command.
- [Update settings](product/update-settings.md) — Settings' Updates section in both forms: the Version row, Check for
  updates automatically and the update channel.
- [Menu bar icon](product/menu-bar-icon.md) — the macOS menu bar icon, its dot, its red eyes while the computer is kept
  awake, its menu and hiding it.
- [Website](product/website.md) — the official site: publishing, pages and languages, the homepage and navigation,
  platform availability, motion and missing pages.
- [Website download page](product/website-download-page.md) — the download page's tabs and panels, the install command
  and terminal demo, where each download link points, and the fixed `/download/<target>/` addresses.
- [Website search indexing](product/website-search-indexing.md) — the sitemap, robots.txt, canonical and language
  alternates, share images, structured data and the Markdown and `llms.txt` copies.
- [Website product preview](product/website-product-preview.md) — the embedded and standalone product preview: scenes,
  sample data, layout and loading failures.
- [Website privacy](product/website-privacy.md) — what the privacy page discloses and the Google Analytics consent
  prompt and cookie.
- [Documentation site](product/documentation-site.md) — the docs at `/docs/`: addresses and languages, the English
  fallback, the retired Chinese redirects, the sidebar and what each page covers (the Features pages in English and
  Simplified Chinese only), and how the guides and the Features pages are written.

## Reference

- [Architecture](architecture.md) — what Octoboard is, its layers and technology choices, orchestration design
  decisions, known pitfalls and the data model.
- [Agent CLI reference](agent-cli-reference.md) — facts about the three agent CLIs themselves: hooks, payloads,
  injection, config directories and plan quota.
- [VoiceOver verification cases](voiceover-cases.md) — real-device VoiceOver results kept as reusable cases.

## Code

- [Project map](project-map.md) — the technology stack and frameworks of each end, the two workspaces, and which
  structure doc to open for which end; start here to find where code lives.
- [Daemon structure](structure/daemon.md) — `apps/daemon`'s directory map and CLI modes, the test-enforced dependency
  direction between its modules with its allow-list, where new code goes, and the shared helpers new daemon code must
  use.
- [UI structure](structure/ui.md) — `packages/ui`'s directory map, the lint-enforced import layering, where new code
  goes, and the shared UI entry points (errors, terminal, settings, auto-refresh, fixtures).
- [Desktop structure](structure/desktop.md) — `apps/desktop`'s Tauri shell: directory map, IPC surface, where new code
  goes, and its build and release scripts.
- [Website structure](structure/web.md) — `apps/web`'s Nuxt directory map, what it imports from the UI package, where
  new code goes, and its SEO, locale and icon entry points.
- [Documentation site structure](structure/docs.md) — `apps/docs`'s Starlight directory map, why the docs are a
  separate app at `/docs/` in the website's Pages deployment, what it copies from or reads in the website and UI, and
  where pages, snippets and quoted commands go.
- [Tooling structure](structure/tooling.md) — the workspaces and toolchains, `pnpm verify` scoping, git hooks, CI
  workflows and repository configuration, and where new scripts and checks go.

## Coding conventions

Binding rules for code, one home per rule; the structure docs keep placement and the shared entry points.

- [General principles](conventions/principles.md) — the principles every convention builds on and the cases no rule
  writes down are judged by.
- [Provider-neutral core](conventions/provider-neutral-core.md) — agent-specific facts and behaviour live in the
  provider, generic code branches on declared capabilities, provider defaults and settings, and how to check a change
  for agent knowledge in generic code.
- [Daemon](conventions/daemon.md) — Rust and daemon code in general: modules, state, store, timing, concurrency and
  claims, agent accounts.
- [Daemon processes and I/O](conventions/daemon-processes-and-io.md) — processes, blocking work, paths and files, git
  from the daemon, writing text into a session, platform differences.
- [Error codes](conventions/error-codes.md) — coded failures and notices, provider codes, and how the UI maps and words
  them.
- [Protocol and contracts](conventions/protocol-and-contracts.md) — wire types, protocol changes, versions and
  compatibility, store schema and migrations, the configuration document, settings and the agents' MCP tools.
- [UI code](conventions/ui-code.md) — React and TypeScript in `packages/ui`: the platform adapter and the page, the React
  Compiler, reading daemon state, requests and browse budgets, performance in WebKit.
- [UI components](conventions/ui-components.md) — the wrapper layer, HeroUI and React Aria, shared components, dialogs
  and fields, scrollers and their fade, styling mechanisms.
- [UI standard](conventions/ui-standard.md) — the visual and interaction bar `packages/ui` is held to: conformance and
  kept exceptions, semantic colours, badges and references, icons, buttons, fields, tooltips, clipped text, cards, tab
  strips, Settings pages, menus, links, empty states, where an error is shown, and motion.
- [UI accessibility](conventions/ui-accessibility.md) — keyboard reach, focus and key handling in code, accessible
  names and roles, announcements, pointer target size, not by colour alone, and text direction.
- [Internationalization in code](conventions/i18n-in-code.md) — catalogs, keys, `useT`, `Translate` and `<Message>`,
  placeholders, numbers and plurals in code, for the UI and the website.
- [Desktop shell](conventions/desktop-shell.md) — Tauri shell code in `apps/desktop/`: what the shell carries, platform
  modules, entry points with a prohibition, native callbacks and threads, native menu accelerators.
- [Website](conventions/website.md) — Nuxt website code in `apps/web/`.
- [Documentation site](conventions/docs-site.md) — Starlight docs code in `apps/docs/`: quoted values, UI names and
  platform words, sidebar, snippets, links and anchors, Features page sections, screenshots, look and feel.
- [Comments](conventions/comments.md) — the project's comment rules beyond the `agent-docs` skill: machine-local
  content, what a comment may refer to and how `TODO` is written.
- [Tests](conventions/tests.md) — what tests assert, real runs, time, jsdom component tests, daemon and desktop shell
  tests, Windows and Linux runs, and the code-level rules of the UI state gallery and its fixtures.
- [Tooling](conventions/tooling.md) — running cargo outside the heavy-step slot, CI workflows, scripts, dependencies,
  single sources of configuration, line width and local ESLint rules.

## Development memory

- [Adding a reason an account cannot be used](memory/adding-a-reason-an-account-cannot-be-used.md) — the file-by-file
  list of places that pick or accept an account, and how to re-measure an account reading in scratch homes before it
  refuses work.
- [Adding a user setting](memory/adding-a-user-setting.md) — which settings still travel in a configuration export: only
  turning one on that itself changes files outside ~/.octoboard/ keeps it out.
- [Building a feature for several agents](memory/building-a-feature-for-several-agents.md) — agent-specific handling
  lives in the provider behind a capability or interface method, and a design every agent can use comes first.
- [Building and launching the app for verification](memory/building-and-launching-the-app-for-verification.md) —
  building, launching and isolating the real app and its daemon sidecar to verify a change.
- [Building on HeroUI and react-aria](memory/building-on-heroui-and-react-aria.md) — the traps behind the component
  rules: a part's real DOM, `children ??` defaults, viewport sizing, state attributes and selectors, pointer events in
  list items, `ListLayout` insets, `Tabs` panels and the collection's hidden copy, and where each defect can be seen.
- [Changing the agents' MCP tools](memory/changing-the-agents-mcp-tools.md) — auditing tool coverage by walking the
  descriptions through every task agents do, not by matching features to tool names.
- [Catching a branch up with its target](memory/catching-a-branch-up-with-its-target.md) — which branch is the target,
  and why a guarded pushed branch is merged, not rebased.
- [Committing alongside other agents](memory/committing-alongside-other-agents.md) — several agents in one worktree
  share one index and one build: stage your paths only right before committing, and give agents that edit or commit
  at once a worktree each.
- [Driving and reading the real app window](memory/driving-and-reading-the-real-app-window.md) — scripting and
  inspecting the live window without touching the user's work, and what a probe can prove.
- [Fading the edge of a scroller](memory/fading-the-edge-of-a-scroller.md) — declaring scrollers the coverage check
  cannot see, and jsdom's `ResizeObserver` stub.
- [Focus and key handling in the UI](memory/focus-and-key-handling-in-the-ui.md) — where jsdom and the gallery miss
  focus and key defects (modal presses, session selection, handoffs, viewer copy), which press to test a dialog under,
  and reading Escape and top-layer symptoms.
- [Isolating agent CLI probes](memory/isolating-agent-cli-probes.md) — keeping a probe of an agent CLI or a live session
  off the user's configuration, accounts and data, and what a scratch daemon on Windows does not keep apart.
- [Keeping the UI responsive in WebKit](memory/keeping-the-ui-responsive-in-webkit.md) — judging cost as restyle times
  forced passes, counting bare rightmost compounds, the specificity trap in unwrapping `:where()`, and measuring latency
  only in WKWebView with `bench:wkwebview`.
- [Measuring build and test speed](memory/measuring-build-and-test-speed.md) — judging a speed change by CPU time
  measured back to back, what to avoid when making checks cheaper, and profiling a slow UI test file before splitting
  it.
- [Probing the agent CLIs](memory/probing-agent-clis.md) — establishing what an agent CLI actually does: which install,
  oracles, positive controls and out-of-band proof.
- [Raising an error alert in the UI](memory/raising-an-error-alert-in-the-ui.md) — reproducing a daemon failure with a
  real daemon on a throwaway `HOME` before keying a remedy or fixture to it.
- [Reading daemon state in the UI](memory/reading-daemon-state-in-the-ui.md) — proving a component stayed idle by
  counting its executions, what to suspect first when `App.renderCounts.test.tsx` breaks, and rewriting merged code that
  reads the whole `sessions` map.
- [Running git from the daemon](memory/running-git-from-the-daemon.md) — `read` for a status-only diff, symptoms of a
  git run outside `git::run`, and checking git fixtures by removing what they depend on.
- [Styling the UI](memory/styling-the-ui.md) — reading HeroUI's own sheet, styling project content inside a HeroUI
  component, finding the rule that draws a colour, checking a surface and auditing a token change (contrast figures,
  captures, the window material), and diagnosing a transition that never ends or text a pixel off.
- [Verifying behaviour in the desktop app](memory/verifying-the-desktop-ui.md) — how narrow to keep a verification, and
  gallery, preview, fixture, git-output, WebKit, touch and capture pitfalls.
- [Verifying changes for Windows](memory/verifying-changes-for-windows.md) — cross-checking, cross-linking and running
  the tests of Rust changes for Windows from a Mac, confirming Windows behaviour as a standard user on that user's own
  desktop, and measuring what Windows itself does through its own API.
- [Writing automated tests](memory/writing-automated-tests.md) — producing a tool's failure for real, how to
  inject the delay and the shape of the load proof, `fakeTimers()` faking animation frames, and reading a
  `window is not defined` crash.
- [Writing daemon tests](memory/writing-daemon-tests.md) — reading a failure that only shows in a parallel run before
  changing a timeout, and techniques the daemon test rules call for.
- [Writing daemon code](memory/writing-daemon-code.md) — moving aside a developer database left behind by an amended
  unreleased schema, and late clippy warnings in test code.
- [Verifying the daemon on Linux](memory/verifying-the-daemon-on-linux.md) — running the daemon's tests and clippy in a
  Linux container with plain cargo, verifying under systemd and a real SSH login with systemd as PID 1, proving with a
  second user's scan that no key reaches another user, and testing the self-update end to end against a local HTTPS
  origin.
- [Writing desktop shell code](memory/writing-desktop-shell-code.md) — finding and checking every native hook after a
  move of `tauri`, `tray-icon` or `muda`, and checking event state by changing it while a page loads.
- [Writing documentation pages](memory/writing-documentation-pages.md) — checking `#fragment` anchors against the
  built pages under `apps/docs/dist/` after translating or retitling a heading, since no check covers them.
- [Writing screen-reader announcements and focus moves](memory/writing-screen-reader-announcements-and-focus-moves.md) —
  which VoiceOver case rules to read before claiming a match, and what to do when none matches.
- [Writing text into a session](memory/writing-text-into-a-session.md) — the waits the per-message bound leaves
  unbounded, and diagnosing a message left unsent from inside the write.
- [Writing UI code under the React Compiler](memory/writing-ui-code-under-the-react-compiler.md) — running
  `compiler:bailouts` on the changed files and reading its list, not its exit status.
- [Writing UI dialogs and fields](memory/writing-ui-dialogs-and-fields.md) — keeping HeroUI typography out of a field
  and finding which fields throw on it, and checking a `Select` note that appears late.
