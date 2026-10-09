# Documentation Index

Long-lived documentation under `docs/`. Development plan docs and bug tickets are temporary and are not listed here.

## Product

- [Consoles and projects](product/consoles-and-projects.md) — consoles, their working directory, agent defaults and
  per-agent config directories; the three ways a project is associated, what is editable afterwards, and what deleting
  either one does.
- [Sessions](product/sessions.md) — console and project sessions, agent selection, the five session statuses, their
  glyphs and their transitions, the raised hand and its notification, how a declined Claude Code prompt lowers the
  hand from the agent's own transcript and what that costs to keep working, archiving, interruption and resuming,
  switching a session to another account of its agent, how a console session's archive, reopen and delete follow the
  sessions bound to it, where archived sessions are kept, and the terminal.
- [Sidebar](product/sidebar.md) — one console at a time, picked on the rail's console switcher with each console's
  activity badge, previewing another console from the rail while the sidebar is hidden, the console header, the
  console sessions section and the project list, project and session rows and their menus, the binding badge a bound
  session carries, keyboard focus on rows, the order of projects and sessions and pinning, filtering the project list
  and folding every project from its heading, how the sidebar follows the selected session, how its view changes are
  animated, the archive view, and right-clicking a row, header or rail avatar for its action menu (and where focus
  goes when the menu closes).
- [Focus mode](product/focus-mode.md) — a project's and a console session's focus mode: what each view lists, the
  switch strip of a console session's focus mode (its fixed chip order, the activity each chip carries, a click versus
  ⌃Tab / ⌃⇧Tab and which of them resumes an interrupted console session), leaving focus mode and what is remembered,
  the focus mode shortcut, and where keyboard focus lands on entering and leaving.
- [Navigation history](product/navigation-history.md) — Back and Forward in the top bar: what counts as a place, how a
  move skips places that no longer exist and never resumes a session, what the history keeps, and the ⌘[ / ⌘]
  shortcuts in the macOS app.
- [Project git status](product/project-git-status.md) — the branch badge on a project's row and in focus mode's header:
  its glyphs for a branch, a detached `HEAD`, a check in flight and a fast-forward in flight, the ahead and behind
  counts, the marker a failed check leaves and what it tells assistive technology; when and how often the current
  console's projects are checked against their remotes and what one check runs; and the Automatically sync
  repositories switch — what it fast-forwards, what it never does, where it is kept, and the immediate
  fast-forward pass turning it on runs.
- [Orchestration](product/hub-orchestration.md) — how an owner (a console session, or an unbound project session in
  its own project) starts and drives sessions: each one's tools and a bound session's `report`, the brief a task is
  handed over as, the reporting loop and what happens when a session stops without reporting, automatic archiving,
  which sessions an owner drives, and the information sessions of one project share with each other.
- [Report panel](product/report-panel.md) — what a console session shows in the right pane: pushing a page with
  `show_page` and what the page id is for, paging back through the kept history, why a history page is read-only and
  where that is enforced, why a page is a static document whose scripts never run and what is stripped from it, why it
  has no route to the network, submitting a native form and how its fields reach the console session, Escape and F6
  inside a page, the window shortcuts (⌘[ / ⌘], ⌃Tab) a page passes on to the window, the light surface a page renders
  on whatever the window's appearance is, and right-clicking inside a page.
- [Window layout](product/window-layout.md) — the window chrome (the top bar and the left rail, over the translucent
  macOS material or an opaque colour in a browser) and the opaque content panel it frames, what the top bar holds and
  how it doubles as the macOS titlebar, what the rail holds (the console switcher, the waiting count, the connection
  status it shows only on trouble, the notifications bell, the right pane's toggle, Settings), the three panes and what
  each is allowed to give up, what the right pane shows (a console session's report or a project's files) and how that
  changes on selecting a session, Browse files, switching consoles, Back and Forward and deleting its owner, resizing
  the sidebar and the right pane, hiding either and floating it back in on hover, the macOS window's 1148×600 minimum
  and the arithmetic behind it, the narrow layout a plain browser gets below 1148 px, where the sidebar and the right
  pane become drawers over the terminal, which right-click menus the desktop app and a browser show, and how all of it
  mirrors under a right-to-left language (and what never does).
- [Moving focus between regions](product/moving-focus-between-regions.md) — F6 and Shift+F6: the order of the window's
  regions, which ones are skipped, and where focus lands in each, the right pane included.
- [Window size and position](product/window-size-and-position.md) — how the macOS window's size, position and maximized
  state are remembered and restored across launches.
- [Project pane](product/project-pane.md) — a project's files and Git review (uncommitted changes and branch comparison)
  in the right pane: its Files and Git modes and what is remembered of the mode, opening it with Browse files or by
  selecting a project session without starting or resuming anything, the file tree (what is listed, its order, rows and
  the keyboard, the selected file and a removed one), when the tree is listed again, what is remembered per project, the
  read-only file viewer (what it shows, Previous/Next and the arrow keys, closing it, a file that changes while open),
  its errors, and a pane that fails to load.
- [Project pane: Git mode](product/project-pane-git-mode.md) — the project pane's read-only review of a repository in
  two views — Uncommitted (a worktree's uncommitted changes: the worktree selector and a worktree that goes away, the
  Staged, Conflicted, Unstaged and Untracked list, a subdirectory project and renames across its boundary, states and
  refreshes) and Compare (a tip-to-tip comparison of two local branches: the From and To selectors, Swap, fixed commits,
  when a comparison is made, a branch that moves or goes away) — that it never writes to the repository and how it
  differs from the branch badge, opening a change in the file viewer, errors, and what is kept for the window's
  lifetime.
- [Toasts](product/toasts.md) — failures and notices that need no answer: where they appear, how one about a session
  is titled, when they dismiss themselves, how an identical one replaces rather than stacks, copying their text
  without taking focus off the terminal, and reaching them with F6.
- [Labels and tooltips](product/labels-and-tooltips.md) — the tooltip every icon-only control of the top bar, the rail
  and the sidebar carries, and how a name too long for its space is faded and offered in full.
- [Appearance](product/appearance.md) — the light, dark and follow-the-system choice and which of them is the default,
  where it is chosen, what follows it (down to the terminal's palette, its 4.5:1 minimum contrast, and the native
  window's own appearance), where the choice is kept, and why a report page stays on a light surface either way.
- [Settings](product/settings.md) — the Settings dialog: how it opens (the rail, and the macOS menu's Settings… /
  ⌘, and when that is ignored), how it closes and where focus goes, its General (Appearance and Language), Git, Agent
  accounts, Trusted folders and Notifications sections.
- [Language](product/language.md) — the 17 offered languages and why they are ordered by tag, English as the fallback,
  the default picked from the system's languages on the first launch and kept from then on, the language chosen in
  Settings, how the system's languages map onto the list, what follows the current language and what does not (report
  pages, text aimed at agents), where the choice is kept, and what is translated so far.
- [Launching agents](product/launching-agents.md) — the guarantee that Octoboard installs nothing into a project and
  never writes the user's agent configuration (the one exception being the conversation record a switch of a session's
  account copies), and the one thing it touches a project directory for, once the user turns it on; the three things
  injected per launch and the console session's generated instruction file, the launch environment, Claude Code's
  workspace-trust prompt, how Octoboard answers it and trusted folders, and the per-agent specifics.
- [Application lifecycle](product/application-lifecycle.md) — what Octoboard runs on and how it is distributed, startup
  and the single-instance rule, who can reach the daemon (any local program, but no web page in a browser), losing the
  daemon connection, closing the window into the background (out of the Dock, everything kept running) and every way
  the window comes back, the quit confirmation and which gestures it covers, how to quit a window that has stopped
  responding, crash behaviour, and the files Octoboard keeps under `~/.octoboard` (and the one it keeps outside it),
  including the database that also holds the settings the daemon stores.
- [Menu bar icon](product/menu-bar-icon.md) — the macOS menu bar icon: what a left and a right click do, its menu
  (running sessions grouped by status in sidebar order, at most 10 per group with an "N more…" line, "No sessions
  running", Open, Quit), choosing a session from it, and its Quit.

- [Website](product/website.md) — the localized homepage, privacy and error pages, keyboard navigation, motion,
  illustrative workflow, release download links, platform roadmap, privacy disclosures, planned remote host support,
  search indexing, and the agent-readable Markdown copies.

## Reference

- [Architecture](architecture.md) — what Octoboard is and its core concepts, the layers and why the daemon is split from
  the application, the technology choices, the orchestration design decisions (the MCP server as a child process,
  structured arguments, why reporting is not forced), the known pitfalls of the Tauri / Rust approach, and the reasoning
  behind the data model.
- [Agent CLI reference](agent-cli-reference.md) — what the three agent CLIs themselves do, against the versions the
  facts were established on: each one's hook events, the payload fields and the keys a turn can be correlated on, what
  a failing hook costs, how a project's own configuration layers around an injected one, how Octoboard is injected into
  each agent and the conditions that come with it, the rules for writing into a running session, and where each keeps a
  session's conversation record and what moving it to another config directory takes.

## Code

- [Project map](project-map.md) — the monorepo layout (deliverables under `apps/`, the daemon among them, shared code
  under `packages/`), the two workspace roots (pnpm and Cargo) and their common commands, then navigation from the
  code tree to each module's own doc.

## Development memory

- [Catching a branch up with its target](memory/catching-a-branch-up-with-its-target.md) — which branches the revert
  gate guards, why a guarded branch that has been pushed cannot be rebased onto a target branch that has moved, and
  merging as the way through.
- [Line wrapping](memory/line-wrapping.md) — the widths committed text is wrapped at, why nothing in the toolchain
  catches an over-long line (and so why an edit re-wraps the whole paragraph it touched), and why a width is counted
  in characters rather than bytes.
- [Probing agent CLIs](memory/probing-agent-clis.md) — how to establish what the three agent CLIs actually do, and how to
  probe a live Octoboard session that launches them: which session markers to strip and why enumerating them beats
  matching a prefix, why neither `--help`, the binary's own strings, nor a config file's silent acceptance of a
  capability's name can be trusted, how to settle a question without spending a model turn and where that stops being
  yours to decide, how to prove a per-launch injection and an MCP tool out of band rather than through the model, why a
  probe expecting "no" needs a positive control and why a measured silence also has to be bounded by what would have
  ended it, why a permission probe has to be set up against the user's own settings with the mode in effect confirmed
  from the session itself, and the residue a probe leaves — in the user's configuration and in their live
  `~/.octoboard` data — and how a probe that needs no logged-in agent avoids the latter with a throwaway
  `HOME`/`TMPDIR`, using Codex's sign-in screen as session output, and how to put a fake agent CLI on `PATH` for an
  isolated daemon so the real one does not launch instead.
- [Building and launching the app for verification](memory/building-and-launching-the-app-for-verification.md) —
  how to build, launch and isolate the app for a verification: when to launch the real window, building the daemon and
  confirming the running sidecar's binary, `pnpm build:app` (a `tauri build` that cannot sign), `open` rather than
  exec'ing the binary, a throwaway `HOME` and `TMPDIR` and the webview profile they still share (and running under
  other system languages for one launch), reading the wire through a wrapped sidecar, sharing the machine with other
  worktrees' dev apps (fixed port, stopping by PID only), and fully reloading a dev window before judging a defect in
  it.
- [Verifying the desktop UI](memory/verifying-the-desktop-ui.md) — how to verify terminal and UI behaviour in the real
  app: why a verification stays as narrow as the change, looking at a UI state in the gallery before staging it through
  a daemon and what the gallery cannot settle, checking in WebKit with real pointer input and with long and CJK text,
  ruling out a locked screen before trusting a capture, getting an error out of a blank window, bisecting a symptom
  against the daemon, what a scripted GUI probe can and cannot prove and why its setup goes through the daemon's
  protocol (raising a session's hand with a forged hook event, seeding trusted folders), confirming Octoboard is
  frontmost before a scripted keystroke, freezing the app's WebContent process, where to watch for a report page's
  blocked navigation, where a network probe's positive control comes from, proving that no key reached the terminal with
  a stand-in agent that echoes every byte, and which checks need a person (input methods, reduced motion).
- [Writing UI components](memory/writing-ui-components.md) — conventions for `packages/ui` components: why a component
  HeroUI 3 already provides is used rather than hand-built, where to check what it provides, and what a justified
  hand-built one is built on and where its reason is written, why a HeroUI control pressed with the mouse takes keyboard
  focus off the terminal, when `preventFocusOnPress` is needed, and why "⋯" menus are built on `ActionMenu`, why a
  dialog must not lose focus to `<body>` when a focused control unmounts or turns disabled, so its subject is switched
  with a reset key rather than by re-keying it and an action that takes seconds marks its button pending rather than
  disabled, why a Tailwind class name has to stand in the source as literal text, why a state of your own on a
  react-aria-components element needs a `data-*` name it does not write itself, why every icon-only control also gets a
  tooltip through `TitledControl`, how user-facing text goes through the message catalog (`useT` over the module-level
  `t()`, a helper taking the translator, `PlainMessageKey` tables, placeholders and plural messages instead of joined
  fragments), how layout follows the reading direction (logical utilities, mirrored directional icons, two-glyph
  chevrons, `dir` on paths and typed names, `docked:rtl:` twins), why only icon-only chrome sits on the translucent
  window material and text goes on the opaque panel, and the WCAG 2.2 AA bar the UI is held to (keyboard, visible focus,
  names, roles and states (`aria-current` only on hand-built rows, a status region kept mounted so what appears in it is
  announced), contrast — auditing every gallery scenario in both themes, fills against their surface and animations at
  their faintest frame, overriding HeroUI's whole `--default` family on a region with its own surface, which token an
  outline meant to be seen is built from, and why HeroUI's own text colours are measured rather than trusted — colour,
  motion).
- [Sending browse requests from the UI](memory/sending-browse-requests-from-the-ui.md) — why the window's browse
  requests are bounded by one count shared across the window rather than one per component (the daemon's per-connection
  bound, and a sent request that is never timed out or cancelled when its component unmounts), and sizing a new kind of
  request together with the existing limits.
- [Writing automated tests](memory/writing-automated-tests.md) — how lean unit tests are kept (one case per rule,
  table-driven), then the fixture conventions this project's tests need on macOS: why an executable written fresh per
  test flakes only under a parallel run, why every wait on a spawned process is bounded by the shared `PATIENCE`
  (exec stalls of seconds on machines with endpoint-security software) and how to tell that stall apart, and how to
  verify behaviour the daemon derives from an agent's own output by replaying a committed capture rather than staging
  a live session, and why a jsdom test cannot tell where keyboard focus ends after a session is selected.
- [Writing daemon code](memory/writing-daemon-code.md) — conventions for the Rust daemon: live state the daemon derives
  held on `AppState` and published by its own event rather than as a field on a stored record, with the cleanups that
  follow from there being no deletion event for it; why a repeating refresh is timed by the client and has to be bounded
  in the daemon by a drop-guard claim and a completion floor, since an in-flight claim alone never fires across clients
  polling on their own phases; the non-interactive environment and deadline every `git` subprocess needs, because `git`
  and `ssh` ask on a terminal the daemon does not have; why a `git` read in a user's repository needs
  `GIT_OPTIONAL_LOCKS=0` and `-c diff.autoRefreshIndex=false` to keep it from rewriting the index (and failing an
  agent's `git add` on `index.lock`), and what a test proving a read writes nothing has to set up first; and how to
  type-check Linux-only code from macOS (a scratch crate, since bundled SQLite blocks a cross-check of the daemon).
