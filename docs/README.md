# Documentation Index

Pointers to the long-lived documentation under `docs/`, grouped by area; open the one whose question matches yours.
Development plan docs and bug tickets are temporary and are not listed here.

Where to start, by question:

- What is it built with, and where is the code? The [project map](project-map.md), then the end's structure doc under
  "Code" below.
- Where does new code go, and which helper must it use? The end's structure doc.
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
- [Session titles](product/session-titles.md) — created names, displayed names and how a title follows the agent's.
- [Session context size](product/session-context.md) — how the context size is measured, refreshed and shown as a chip.
- [Session history](product/session-history.md) — the read-only history shown for an archived session.
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
- [Handing a session off to another agent](product/handing-off-sessions.md) — moving a session's work to a new session
  of a different agent.
- [Changing a session's parent](product/changing-session-owners.md) — transferring to a parent, working independently
  and setting a parent, and how those transitions affect the role.
- [Interrupting a session's turn](product/interrupting-turns.md) — how a parent stops a child session's running turn.
- [Message delivery](product/message-delivery.md) — how messages are written into a running session and held when it
  cannot take them.

### Orchestration and agent tools

- [Orchestration](product/hub-orchestration.md) — coordinator and worker capabilities, parent and child relationships,
  tools, briefs and routing.
- [Session requests](product/session-requests.md) — a session asking the user to start an independent session.
- [Requests to resume a conversation](product/requests-to-resume-a-conversation.md) — a session asking the user to
  resume a conversation by its ID.
- [Reporting to a parent session](product/reporting-to-owners.md) — how a project session with a parent reports a round
  of work, when it archives itself, and what is kept for a parent that is interrupted.
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
- [Launching agents](product/launching-agents.md) — what every agent launch injects, what is never modified, and the
  launch environment.
- [Per-agent launch specifics](product/per-agent-launch-specifics.md) — what Claude Code, Codex and Grok Build each do
  differently at launch.
- [Model and reasoning effort](product/model-and-reasoning-effort.md) — the Advanced section's model and effort pickers
  and what a session shows running.
- [Agent accounts](product/agent-accounts.md) — Settings' Agent accounts section: added agents, the default account,
  owned accounts, the accounts table and the account details and settings dialogs.
- [Per-agent settings](product/per-agent-settings.md) — settings that apply to every account of one agent: the settings
  each provider declares (permission approval modes, Claude compatibility) and agent tags.
- [Removing an account](product/removing-accounts.md) — what removing an account clears, its confirmation, and the option
  to move the account directory to the Trash.
- [Logging an account in](product/logging-accounts-in.md) — the login dialog and how each agent signs in.
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
  shown, and redeeming one from the rail's panel or the account details dialog.
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
- [Project Git actions](product/project-git-actions.md) — pushing, switching branch and restoring changes from a
  project's menu.
- [Git workflow settings](product/git-workflow-settings.md) — where a project session develops and the development
  rule.
- [The revert gate](product/revert-gate.md) — the guard against quietly undoing changes already on a branch.
- [Branch and worktree cleanup](product/branch-and-worktree-cleanup.md) — removing a session's branches and worktrees
  when it is archived or deleted.
- [Configuration export and import](product/configuration-export-and-import.md) — the configuration document and its
  regions (consoles and projects, application settings, the account structure) and what an import does with each.
- [The configuration export and import dialogs](product/configuration-export-and-import-dialogs.md) — the export
  dialog's region choices and the import dialog's file step, region-tabbed review and result.

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
  resizing and maximizing.
- [The archive view](product/archive-view.md) — the full archive of a project, console session or console.
- [Navigation history](product/navigation-history.md) — the Back and Forward buttons and what counts as a place.
- [Moving focus between regions](product/moving-focus-between-regions.md) — F6 navigation between the window's
  regions.
- [Keyboard shortcuts](product/keyboard-shortcuts.md) — the changeable shortcuts, how they are matched and shown.
- [Settings](product/settings.md) — the Settings dialog and each of its sections: General, Git, Agent accounts, Agent
  behavior, Trusted folders, Permissions, Notifications, Keyboard shortcuts, Configuration and About.
- [Appearance](product/appearance.md) — light, dark and follow-the-system, and what follows the choice.
- [Language](product/language.md) — the offered languages, how the choice is made and stored.
- [Right-to-left layout](product/right-to-left-layout.md) — what is mirrored under a right-to-left language.
- [Dialogs](product/dialogs.md) — dialog titles, initial focus, what Enter and confirm buttons do, and where an unasked question stands among other dialogs.
- [Error alerts](product/error-alerts.md) — where a failure, warning or note is shown, by how it arises.
- [Issue reports](product/issue-reports.md) — what Report issue prefills into a new Issue and what is removed from
  it first.
- [Toasts](product/toasts.md) — the notices that need no answer.
- [Empty states](product/empty-states.md) — how a view with nothing to show says so.
- [Labels and tooltips](product/labels-and-tooltips.md) — tooltips, help tips and handling of names that do not fit.
- [Scrolling surfaces](product/scrolling-surfaces.md) — edge fades and keyboard scrolling.
- [Collection motion](product/collection-motion.md) — transitions for items entering, leaving and moving in
  collections.

### Terminals, files and the project pane

- [The session terminal](product/session-terminal.md) — the terminal showing the selected session: typing, copying,
  scrollback, reconnecting and states.
- [Links in terminal output](product/terminal-links.md) — how web addresses and file paths in terminals become links.
- [Terminal panel](product/terminal-panel.md) — the right pane's Terminal panel: tabs, actions and the shell terminal.
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
- [File viewer](product/file-viewer.md) — the read-only dialog showing one file: header, stand-ins, wrapping,
  finding and navigation.
- [Markdown documents in the file viewer](product/markdown-documents.md) — how a Markdown file renders as a document.
- [Finding in the file viewer](product/finding-in-the-file-viewer.md) — Find in file: opening the find bar, what is
  searched, its switches and count, how matches are highlighted and stepped through, and closing it.
- [Report panel](product/report-panel.md) — the panel where a console session, or a project session that faces the
  user, pushes, withdraws and pages through its pages and receives form submissions.

### Desktop app and website

- [Application lifecycle](product/application-lifecycle.md) — requirements, startup, daemon connection and its key,
  restarting a daemon that dies, closing, quitting, crashes, logs and owned files.
- [In-app updates](product/in-app-updates.md) — update channels, where updates come from, checking, notice, download
  and restart.
- [Menu bar icon](product/menu-bar-icon.md) — the macOS menu bar icon, its dot and menu.
- [Website](product/website.md) — the official site: publishing, pages and languages, the homepage and navigation,
  platform availability, motion and missing pages.
- [Website search indexing](product/website-search-indexing.md) — the sitemap, robots.txt, canonical and language
  alternates, share images, structured data and the Markdown and `llms.txt` copies.
- [Website product preview](product/website-product-preview.md) — the embedded and standalone product preview: scenes,
  sample data, layout and loading failures.
- [Website privacy](product/website-privacy.md) — what the privacy page discloses and the Google Analytics consent
  prompt and cookie.

## Reference

- [Architecture](architecture.md) — what Octoboard is, its layers and technology choices, orchestration design
  decisions, known pitfalls and the data model.
- [Agent CLI reference](agent-cli-reference.md) — facts about the three agent CLIs themselves: hooks, payloads,
  injection, config directories and plan quota.
- [UI standard](ui-standard.md) — the bar `packages/ui` is held to: accessibility, the confirmed visual and
  interaction conventions, and the rules that follow from them.
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
- [Tooling structure](structure/tooling.md) — the workspaces and toolchains, `pnpm verify` scoping, git hooks, CI
  workflows and repository configuration, and where new scripts and checks go.

## Development memory

- [Adding a daemon error code](memory/adding-a-daemon-error-code.md) — classifying a new daemon error code in the UI in
  the same change, and sending a condition the daemon finds on its own as a coded notice about its session.
- [Adding a dependency](memory/adding-a-dependency.md) — which `package.json` group a package belongs in when the UI
  bundle ships it.
- [Adding a reason an account cannot be used](memory/adding-a-reason-an-account-cannot-be-used.md) — every place that
  picks or accepts an account must check a new reason.
- [Adding a user setting](memory/adding-a-user-setting.md) — exposing a new setting to agents' settings tools and the
  configuration export, and the flags of a new provider setting.
- [Building and launching the app for verification](memory/building-and-launching-the-app-for-verification.md) —
  building, launching and isolating the real app and its daemon sidecar to verify a change.
- [Building on HeroUI and react-aria](memory/building-on-heroui-and-react-aria.md) — pitfalls of HeroUI 3 and react-aria
  components: state attributes, context, tabs, lists and tooltips.
- [Changing the agents' MCP tools](memory/changing-the-agents-mcp-tools.md) — adding, changing or auditing the MCP tools
  and role texts from the agent's side: descriptions against results, parity with the UI, the agents' typical tasks.
- [Catching a branch up with its target](memory/catching-a-branch-up-with-its-target.md) — which branch is the target,
  and why a guarded pushed branch is merged, not rebased.
- [Committing alongside other agents](memory/committing-alongside-other-agents.md) — several agents in one worktree
  share one index: stage your paths only right before committing and check the index holds nothing else.
- [Driving and reading the real app window](memory/driving-and-reading-the-real-app-window.md) — scripting and
  inspecting the live window without touching the user's work, and what a probe can prove.
- [Fading the edge of a scroller](memory/fading-the-edge-of-a-scroller.md) — the single module every faded scroller
  takes its fade from.
- [Focus and key handling in the UI](memory/focus-and-key-handling-in-the-ui.md) — keeping focus from dropping, Escape
  and key handlers, top layers and the shortcut registry.
- [Handling paths and files in the daemon](memory/handling-paths-and-files-in-the-daemon.md) — treating hook-reported
  paths as untrusted, destructive acts, reading project and git-directory files safely, and telling directories apart
  by device and inode.
- [Isolating agent CLI probes](memory/isolating-agent-cli-probes.md) — keeping a probe of an agent CLI or a live session
  off the user's configuration, accounts and data.
- [Keeping the UI responsive in WebKit](memory/keeping-the-ui-responsive-in-webkit.md) — avoiding document-wide style
  cost, writing cheap CSS rules, and measuring latency in the system WKWebView.
- [Line wrapping](memory/line-wrapping.md) — the widths committed text is wrapped at, and why an edit re-wraps the whole
  paragraph.
- [Measuring build and test speed](memory/measuring-build-and-test-speed.md) — judging a speed change by CPU time, and
  what to avoid when making checks cheaper.
- [Probing the agent CLIs](memory/probing-agent-clis.md) — establishing what an agent CLI actually does: which install,
  oracles, positive controls and out-of-band proof.
- [Raising an error alert in the UI](memory/raising-an-error-alert-in-the-ui.md) — passing a title, raising
  UI-detected failures as `ActionableError`, the remedy of a failed write to the application's own data, and settling
  what a daemon failure arrives as against a real daemon.
- [Reading daemon state in the UI](memory/reading-daemon-state-in-the-ui.md) — selectors, effect dependencies, polling
  deduplication and event ordering when reading daemon state.
- [Running git from the daemon](memory/running-git-from-the-daemon.md) — building `git` commands in the daemon:
  environment, deadlines, flags, ref names and test fixtures.
- [Sending browse requests from the UI](memory/sending-browse-requests-from-the-ui.md) — bounding the window's
  outstanding browse requests under the daemon's budget.
- [Styling the UI](memory/styling-the-ui.md) — keeping HeroUI's look and the Tailwind, class-name, spacing and divider
  rules that bite, and stopping transitions inside a collapsed disclosure.
- [UI text and reading direction](memory/ui-text-and-reading-direction.md) — routing user-read strings through the
  catalog and laying out by reading direction.
- [Verifying behaviour in the desktop app](memory/verifying-the-desktop-ui.md) — how narrow to keep a verification, and
  gallery, fixture and capture pitfalls.
- [What goes on the window material](memory/what-goes-on-the-window-material.md) — which chrome may sit on the
  translucent window material.
- [Writing automated tests](memory/writing-automated-tests.md) — keeping tests lean, checking a test by breaking its
  rule, the test-only fourth provider (never hard-code the provider list), replaying committed agent captures, and the
  jsdom test pitfalls.
- [Writing daemon tests](memory/writing-daemon-tests.md) — fixtures that are real processes: sharing one exec'd script,
  bounding waits by `PATIENCE` and waiting for the condition itself, signal windows in fixtures, stand-in markers,
  seeded model lists, keeping tests off the real agent CLI, testing budgets by lowering them.
- [Writing daemon code](memory/writing-daemon-code.md) — which session state is stored and which lives in memory, where
  live state lives, store writes, raising versions only between releases, timing, threads, process cleanup, linting,
  and provider interface defaults that claim nothing.
- [Writing desktop shell code](memory/writing-desktop-shell-code.md) — main-thread queuing, native hooks into Tauri
  internals and native menu width.
- [Writing text into a session](memory/writing-text-into-a-session.md) — how the daemon adds text or keys to a session's
  input safely and diagnoses unsent messages.
- [Writing UI code under the React Compiler](memory/writing-ui-code-under-the-react-compiler.md) — what a render may
  read, checking the compiler's log and keeping list-row identity.
- [Writing UI dialogs and fields](memory/writing-ui-dialogs-and-fields.md) — confirmations, error blocks, field
  validation, placeholders and dialog scrolling.
