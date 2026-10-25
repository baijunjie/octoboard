# Documentation Index

Long-lived documentation under `docs/`. Development plan docs and bug tickets are temporary and are not listed here.

## Product

- [Account quotas](product/account-quotas.md) — each agent account's plan quota: which accounts have one, how it is
  read through the agent's own CLI and when, the windows per agent and their labels, how a quota is shown, the
  reading / unavailable / out-of-date states and their reasons, the two-layer 5-hour / weekly meter and the account
  picker's usage bar, what a reading leaves in the account's directory, the rail's quota panel, and when an account
  counts as used up and what that refuses, and the `get_account_quotas` tool that gives a session the readings and refreshes them on request.
- [Collection motion](product/collection-motion.md) — entry, exit and position transitions for dynamic collections, including
  horizontal and wrapping items, virtualized rows, keyboard focus and reduced motion.
- [Consoles](product/consoles.md) — consoles: their fields and working directory, the two default agents, agents
  that are not installed, the avatar, console session instructions, per-agent config directories (accounts and how
  sessions hold their directory), and editing and deleting a console.
- [Projects](product/projects.md) — what a project carries and its trust permission, the three ways a project is
  associated, its name, description, default agent and per-agent account, model and effort, how its git remote URL
  is recorded and kept current, what is editable afterwards, the directory browser, and hosts.
- [Project tags](product/project-tags.md) — what a project tag is, how tags are normalised and offered, and how a
  picked tag in the project dialog's Tags field looks and behaves.
- [Removing a project](product/removing-projects.md) — what goes with a removed project, the confirmation listing its
  running sessions, refusals, and moving the project folder to the Trash.
- [Configuration export and import](product/configuration-export-and-import.md) — carrying configuration to another
  machine as a versioned JSON document: where the window offers it (Settings' Configuration section and the welcome
  screen's Import configuration…, all through the window's own file dialogs),
  the export dialog's scope choice and its warning about the paths the file carries, the import dialog's three steps
  (the file, the review of every project with where it stands on the daemon's host and where it goes, the result by
  outcome); then the two export scopes, what a console and a project carry and what is deliberately left out, the
  project path as the one machine-local path in it; the import's two steps — the four directories a project can be
  placed in and which is the default, the stricter rule on a URL a clone may be made from, account references rebound
  by name — and what applying does: always a new console, projects through the ordinary association and untrusted
  like any other, the opt-in application setting, and what is refused before anything is created against what is
  reported as failed afterwards.
- [Model and reasoning effort](product/model-and-reasoning-effort.md) — the model and reasoning effort a console's
  console sessions and a project's sessions launch with: the console's two per-agent pairs, a project session's fallback
  from the project to the console's project-session pair, persistent session overrides, what unset means, how the model
  list is asked of the agent itself for the account in question, how the effort levels follow the chosen model, and what
  the pickers do while that list is out or cannot be had.
- [Sessions](product/sessions.md) — console and project sessions, their shared creation dialog, optional names,
  agent and account selection, Advanced launch settings, the session statuses, their
  glyphs and their transitions, the raised hand and its notification, how a declined Claude Code prompt lowers the
  hand (read from the agent's own transcript) and a cancelled Claude Code turn returns to awaiting instructions (read
  from its transcript and terminal title), and what that costs to keep working.
- [Archiving and resuming sessions](product/archiving-and-resuming-sessions.md) — archiving and interruption, how a
  console session's archive and reopen follow the sessions bound to it, resuming a dormant session (the
  `conversation_unresumable` and `codex_unarchive_failed` refusals, an archived owner whose conversation is gone
  relaunched fresh), when a conversation counts as resumable per agent, and how long archived sessions are kept.
- [Switching a session's account](product/switching-session-accounts.md) — moving a session to another account of its
  agent: the confirmation's two scopes (the session with its sub-sessions, or the whole console), the background
  batch of up to four switches at once and the session's switching state, an owner's `switch_session_account` tool,
  what is copied per agent (Claude Code sidecars and rewind checkpoints, Codex rollouts compressed or not and every
  history segment of a reverted thread, subagent rollouts left behind), the target's Codex thread row pointed at the
  copied head, the all-or-nothing copy, and when a switch fails.
- [Handing a session off to another agent](product/handing-off-sessions.md) — the session row's Hand off entry and its
  dialog, an owner's `handoff_session` tool (its arguments, result and refusals), and the daemon's `hand_off_session`:
  when a session can be handed off, the handoff package built from its records, how the receiving session takes over
  its owner, branches and owned sessions, who is told, messages held during the handoff, and when it fails.
- [Deleting sessions](product/deleting-sessions.md) — deleting archived sessions one at a time or all at once (an
  archived owner takes its bound archived sessions along), deleting a never-used session without confirmation, and
  moving agent records to the Trash automatically for unused sessions or optionally for used sessions, and the
  daemon's automatic deletion of archived sessions whose conversation is gone.
- [Session terminal](product/session-terminal.md) — the selected session's terminal: what its area shows with no
  session selected, typing and the mouse, output, scrollback and resizing, the reconnect and loading covers,
  interrupted, archived and dormant sessions (an interrupted one's saved preview, an archived one's history in its
  place), links in the output (web addresses and file paths: click to open, the OSC 8 target tooltip, which paths
  link), and the creation offer while a console has nothing in it.
- [Session history](product/session-history.md) — what an archived session shows in the terminal's place: the user's
  messages and the agent's final replies read from the agent's own record, with Octoboard messages hidden, per-agent
  differences, the loading / empty / unavailable / failed states, and keyboard focus.
- [Session titles](product/session-titles.md) — the task title alongside created session names: where it is
  displayed, what renaming edits, where it comes from (the agent's records, its decorated terminal title, the first
  prompt), how titles are retained, and the accepted terminal sequences.
- [Session context size](product/session-context.md) — the session's current context size (the main conversation's
  latest request): per-agent sources, Claude Code's context window from its status line and when it is missing, when the
  size is unknown or drops, the context-size chip (a badge, led by a ring of the share used when a window is known) and
  its Context window card, refresh, and what happens on archiving, upgrading and account switches.
- [Sidebar](product/sidebar.md) — the sidebar's frame: one console at a time, the console header, the console sessions
  section and the project list, filtering the project list and folding every project from its heading, the shared
  shape of the empty row a section with nothing to list takes, row names and keyboard focus, right-clicking a row,
  header or rail avatar for its action menu (and where focus goes when the menu closes), how the sidebar follows the
  selected session, and how its view changes are animated.
- [Console switcher](product/console-switcher.md) — the rail's console avatars and each console's activity badge,
  the current console and how it is remembered, the session a switch brings back into the terminal, and previewing
  another console from the rail while the sidebar is hidden.
- [Sidebar project rows](product/sidebar-project-rows.md) — a project's row: expanding and collapsing, its branch
  badge and activity marker, its action menu and items, the sessions nested under it (a session bound to another of
  its sessions listed under its owner), and opening the system file manager from it.
- [Sidebar session rows](product/sidebar-session-rows.md) — a session's row: its glyph, agent tooltip, context chip,
  action menu and Delete/Archive shortcut, the binding badge a bound session carries, and the session lock controls.
- [Order of projects and sessions](product/order-of-projects-and-sessions.md) — how the sidebar orders projects and
  sessions, the order when ordering by status is turned off, and pinning.
- [Archive view](product/archive-view.md) — the full archive the sidebar's menus and focus mode lead to, and archive
  previews: scope, rows, Reopen, Delete and Delete all, nesting, closing, animation and focus.
- [Focus mode](product/focus-mode.md) — a project's and a console session's focus mode: what each view lists, where a
  lead session and the sessions under it are listed and where they are only counted, the switch strip of a console
  session's focus mode (its fixed chip order, the activity each chip carries, a click versus ⌃Tab / ⌃⇧Tab and which
  of them resumes an interrupted console session), leaving focus mode and what is remembered, the focus mode
  shortcut, and where keyboard focus lands on entering and leaving.
- [Navigation history](product/navigation-history.md) — Back and Forward in the top bar: what counts as a place, how a
  move skips places that no longer exist and never resumes a session, what the history keeps, and the ⌘[ / ⌘]
  shortcuts in the macOS app.
- [Project git status](product/project-git-status.md) — the branch badge on a project's row and in focus mode's header:
  its branch and detached `HEAD` glyphs, the shared in-flight spinner, the ahead and behind counts and their
  directional arrow animations, the marker a failed check leaves and what it tells assistive technology; when and how
  often the current console's projects are checked against their remotes, what one check runs and the remote URL it
  records for the project; how the branch and counts are re-read locally, without a fetch, when the repository's refs
  change; which of the user's own git settings a check's `git` is given and which are filtered out; and the
  Automatically pull repositories switch — what it fast-forwards, what it never does, where it is kept, and the
  immediate fast-forward pass turning it on runs — and Pull current branch: an immediate check and fast-forward of one
  project.
- [Project git actions](product/project-git-actions.md) — the project menu's Git actions: Push current branch (which
  branch is published, when an upstream is recorded, the branch read sent before the push, and what is refused);
  Switch branch (the third-level local branch menu, immediate switching, and failures); and Restore file changes (the
  confirmation, what is restored, and what is refused).
- [Opening a project terminal](product/project-terminal.md) — Open in → Terminal in a project's action menu: the system
  terminal at the project's directory on the machine the daemon runs on, Terminal.app on macOS and the first installed
  emulator on Linux, when the request finishes, and when it is refused.
- [Shell terminals](product/shell-terminals.md) — the plain login shells the daemon runs for a project: who owns them
  and how long they live, where they start and with what environment, their title, when one counts as busy, a shell
  that exits on its own, and what closes or ends them.
- [Orchestration](product/hub-orchestration.md) — how an owner (a console session, or an unbound project session in
  its own project) starts and drives sessions: each one's write tools, lead sessions (a project session bound to a console
  session that keeps its own sessions, two levels deep), the brief a task is handed over as, which sessions an owner
  drives, and a session that reports to nobody archiving itself once everything under it is archived.
- [What every session reads](product/what-every-session-reads.md) — the read-only tools every session has whatever its
  role, with one scope, and why: session records and their fields, listing consoles and a console's projects, reading
  a session's history page by page, the models an agent offers, and what stays narrower than the reads.
- [Changing a session's owner](product/changing-session-owners.md) — how a binding changes while sessions run: who can
  own a session, handing a session to another owner with `transfer_session`, an unbound session binding itself with
  `bind_to_session`, releasing a session from its owner (`release_session` or the sidebar's Unbind), what a session is
  told when its binding changes — at once, or at its next launch if it is not running — and how its tools follow.
- [Reporting to an owner](product/reporting-to-owners.md) — how a bound session reports to its owner with `report`, the
  report synthesised when a session stops without reporting, and automatic archiving on a `done` report.
- [Information between sessions](product/information-between-sessions.md) — listing the running sessions of every
  console with `list_sessions` and writing information into any of them with `share_info`: the filters that narrow the list, why it reaches a session
  as information rather than an instruction, the copy its owner is sent, and information kept for an interrupted
  session until it is resumed (an archived one is refused).
- [Message delivery](product/message-delivery.md) — how a message written into a session is delivered: what is
  sanitized, when it is queued until the session can take it, the hold while a trust confirmation may be up, the holds
  behind screens an agent raises that only the user can answer and after a Grok Build error, a Claude Code relaunch
  instruction passed as the launch prompt, what the owners' tools report as held, a message a dialog discarded or a view
  of background agents took, and a write that only partly lands.
- [Session requests](product/session-requests.md) — how any session asks with `request_session` for a session that has
  no owner, in any project or as a console session, and what it shares with `request_resume_session`: who is offered
  the tool, its arguments and the refusals before the user is asked, the setting that approves requests automatically
  without a dialog, the 8-minute wait and the raised hand, each agent's own time limit, how a request ends (approval,
  refusal, time limit, withdrawal), what approval does, handing the requester's branches over to the new session with
  `branches`, the outcome message, and the confirmation dialog.
- [Requests to resume a conversation](product/requests-to-resume-a-conversation.md) — how a session asks the user with
  `request_resume_session` to resume a conversation by its session ID, with what the Resume conversation tab can and
  cannot do: who has the tool, its arguments, the refusals before the user is asked, what approval does (reopened,
  resumed, opened on an agent's record, already running) and its result, approvals that fail, and the confirmation
  dialog's variants with the warning for an account whose sharing change is incomplete.
- [Settings tools for agents](product/agent-settings-tools.md) — which settings a console session can read and change
  through its tools (projects, its own console, the application-wide settings), the read-only `get_project` every
  project session has, the `accounts` and `launch` argument shapes, when a change takes effect, and what is never
  exposed to an agent.
- [Report panel](product/report-panel.md) — what a console session shows in the right pane: pushing a page with
  `show_page` and what the page id is for, reading back the pages pushed with `list_pages`, paging back through the kept history, why a history page is read-only and
  where that is enforced, why a page is a static document whose scripts never run and what is stripped from it, why it
  has no route to the network, submitting a native form and how its fields reach the console session, Escape and F6
  inside a page, the window shortcuts (⌘[ / ⌘], ⌃Tab) a page passes on to the window, the light surface a page renders
  on whatever the window's appearance is, and right-clicking inside a page.
- [Welcome screen](product/welcome-screen.md) — what the window shows with no console at all: the screen that fills the
  content panel, its New console button and the quieter Import configuration… beside it where the window can open a file,
  the permanent supported-tool list with brand icons, detection statuses and a PATH hint, why New console stays enabled, what
  is not on screen in that state (the sidebar, the right pane, the terminal's empty state, the top bar's sidebar
  toggle), the pane visibility and widths that are kept rather than reset, and where keyboard focus goes as the first
  console is made and as the last one goes away.
- [Window layout](product/window-layout.md) — the window chrome (the top bar and the left rail, over the translucent
  macOS material or an opaque colour in a browser) and the opaque content panel it frames, what the top bar holds and
  how it doubles as the macOS titlebar, what an open dialog, menu or popover puts out of reach and the connection
  banner's strip that stays usable through it, what the rail holds (the console switcher, the waiting count, the
  notifications bell, the right pane's toggle, Settings), the three panes and what each is allowed to give up, what the
  right pane shows (a console session's report or a project's files) and how that changes on selecting a session, Browse
  files, switching consoles, Back and Forward and deleting its owner, resizing the sidebar and the right pane, hiding
  the sidebar and floating a pane in on hover, the macOS window's 1168×600 minimum and the arithmetic behind it, the narrow
  layout a plain browser gets below 1168 px, where the sidebar becomes a drawer over the terminal and the right pane stays
  docked,
  which right-click menus the desktop app and a browser show, and how all of it mirrors under a right-to-left language
  (and what never does).
- [Moving focus between regions](product/moving-focus-between-regions.md) — F6 and Shift+F6: the order of the window's
  regions, which ones are skipped, and where focus lands in each, the right pane included.
- [Window size and position](product/window-size-and-position.md) — how the macOS window's size, position and maximized
  state are remembered and restored across launches.
- [Project pane](product/project-pane.md) — a project's files and Git review (one branch's uncommitted and committed
  changes) in the right pane: its Files and Changes tabs and what is remembered of the mode, opening it with Browse
  files or by selecting a project session without starting or resuming anything, the file tree, always by folder (what
  is listed, Git-ignored entries shown dimmed, its order, rows and the keyboard, the selected file and a removed one),
  Collapse all folders, recursive file-name filtering with results shown as a tree and ignored files left out unless the
  include-ignored toggle is on, the button and right-click menus of file and change rows (opening live paths with the system,
  revealing them in the file manager and copying paths; change rows use the branch's worktree or fall back to the
  project directory), when the tree is listed again, what is remembered per project, its errors (the file viewer's
  included), and a pane that fails to load.
- [File viewer](product/file-viewer.md) — the read-only file viewer opened from Files rows, Git changes and terminal
  path links: its header (Copy path, the view choices, File manager and Default application), what it shows, including
  a Markdown document's diagrams, formulas, project images and links to other project files, notices and stand-ins,
  wrapping long lines, selecting and copying, Previous/Next and the arrow keys, closing it, a file that changes while
  open, and a file opened from the terminal (the click-time choice between the viewer and the host, opening at a line,
  viewer-only files).
- [Right pane panels](product/right-pane-panels.md) — the right pane as a stack of inset panel cards (a project's
  Files and Terminal, a console session's Report): how the cards look, the panel header, opening and closing panels,
  the per-kind toggles in the top bar, floating a panel in, switching projects under an open panel, resizing,
  maximizing, what is remembered, and keyboard focus.
- [Terminal panel](product/terminal-panel.md) — a project's shell terminals as tabs in the right pane: when a shell
  starts, tabs and which is shown, rename and close and the busy-close confirmation, Escape, F6 and focus in a shell
  terminal, links, an exited shell, and reconnecting.
- [Project pane: Git mode](product/project-pane-git-mode.md) — the project pane's read-only review of one local branch
  in the Changes tab: the branch selector (the default branch from the worktree holding the project's folder, a choice
  that stands, a branch that goes away), the repository and its worktrees, following the selected session, that it
  never writes to the repository and how it differs from the branch badge, and what is remembered.
- [Project pane: Git change list](product/project-pane-git-change-list.md) — the Git mode's change list: its
  uncommitted changes (Conflicted, Staged, Unstaged, Untracked) from the worktree that has the branch checked out, what
  it has committed since forking from the main working copy's branch (one Committed section, its changes together or
  under a foldable row per commit with Group by commit, at most 100 commits), a subdirectory project and renames across
  its boundary, rows and the keyboard, each row's added and removed line counts, the lines saying what the list is made
  of and what failed, folding sections and commits, grouping by folder, filtering, and refreshes and the comparison
  following the branches.
- [Project pane: Git change viewer](product/project-pane-git-change-viewer.md) — opening a Git change in the file
  viewer: its tags and the subtitle naming the commits a committed change is read from, what a change shows (diffs,
  images, renames, type changes, conflicts, no differences), expanding the lines a diff collapses, moving between
  changes, closing it, when the open change is read again, and its errors.
- [Branch and worktree cleanup](product/branch-and-worktree-cleanup.md) — which git branches and linked worktrees go
  when sessions are archived or deleted, or a project's folder is moved to the Trash: what counts as bound to a
  session, what is removed without asking, what the confirmations offer to force (uncommitted or unmerged items, and
  locked worktrees on their own), when the removal runs, and how leftovers are reported to the user and to agents.
- [Which session is working on a branch](product/branch-responsible-session.md) — the responsible-session badge on a
  branch's row in the Git mode's branch selector list: the title it names the session by (the session's own while it
  exists, the recorded one once it is gone) and the filled ring a branch of the selected session takes; the
  responsible session, which starts as the session whose own git reported creating the branch through git's
  reference-transaction hook and moves only by an approved handover, a project session's `new_session` (and what the
  new session's opening prompt says), an owner's `handover_branches` (who may give and take, the refusal while the
  giver is at work, what is reported per worktree, who is told), or a handoff, a branch no session's git reported
  staying unmarked; what counts as a creation (renames keep the record, forced updates and copies do not, a report from an
  ended session's background job counts); what attribution needs (git 2.36 or later); the hook dispatchers a
  session's git runs, which still run the user's own hooks, and their visible effects and cost; how a record is
  scoped to a repository, when it changes or goes and the 1,000 cap; that nothing is written to the repository; and
  where a branch has no responsible session.
- [Empty states](product/empty-states.md) — the two shapes a view, list or panel with nothing to show takes: the full
  panel's glyph, title, explaining line and action, the compact row for a list nested in another, what each part says,
  and how each is announced.
- [Error alerts](product/error-alerts.md) — where a failure is shown: validation under a field, the error alert that
  requires acknowledgement (its queue, duplicate suppression, focus and retrying the original action), failures shown
  on the screen (an empty-state failure in place of a whole subject, or a compact alert with Details among other
  content), and failures marked on a row or on a part of a pane; names and diagnostic details, recovery guidance,
  unexpected errors and Issue reporting.
- [Toasts](product/toasts.md) — notices that need no answer: where they appear, how one about a session
  is titled, when they dismiss themselves, how an identical one replaces rather than stacks, copying their text
  without taking focus off the terminal, and reaching them with F6.
- [Labels and tooltips](product/labels-and-tooltips.md) — how every tooltip in the window behaves (the wait before one
  opens, one at a time, what closes it, how it wraps) and the two places that keep the browser's own instead, the
  tooltip every icon-only control of the top bar, the rail and the sidebar carries, the tooltips the indicators carry,
  and how names, menu items and selector labels too long for their space fade and run as a hover marquee.
- [Scrolling surfaces](product/scrolling-surfaces.md) — what the window does wherever a surface holds more than it
  can show: the fade on the edge that hides the content, which edge it sits on and that it is a mark alone, the
  surfaces it holds for and the scrolling it leaves alone (a document of its own, a menu's list of choices), where no
  scrollbar is drawn and the fade is the only sign, and which surfaces the keyboard scrolls by becoming a named region
  of their own.
- [Appearance](product/appearance.md) — the light, dark and follow-the-system choice and which of them is the default,
  where it is chosen, what follows it (down to the terminal's palette, its 4.5:1 minimum contrast, and the native
  window's own appearance), where the choice is kept, and why a report page stays on a light surface either way.
- [Settings](product/settings.md) — the Settings dialog: how it opens (the rail, and the macOS menu's Settings… /
  ⌘, and when that is ignored), how it closes and where focus goes, its General (Appearance, Language, the default
  clone directory and, in the macOS application alone, keeping Octoboard running in the background), Git, Agent
  accounts, Trusted folders, Permissions, Agent behavior (automatically approving session requests and resume
  requests, each its own switch, and asking questions as choices), Notifications, Keyboard shortcuts and
  Configuration (exporting and importing the configuration document, where the window can save and open files)
  sections.
- [Worktree settings](product/worktree-settings.md) — the four worktree settings (use worktrees, rebase before merging,
  delivery, revert gate), their values and defaults, the other three applying only while use worktrees is on, a project
  following each application-wide value or holding its own and how the effective values are worked out, and how they
  are read and changed in Settings, in the project dialog, through the daemon's requests, the agents' settings tools
  and the configuration export and import.
- [The revert gate](product/revert-gate.md) — the built-in gate on a project session's git: which branches are guarded
  and how a session's new branch gets its target branch and fork point, what local updates and pushes it refuses and
  how a deliberate revert passes, what the agent sees, the switch read every time it runs, where it runs and where
  not, failing open, and what it costs.
- [Keyboard shortcuts](product/keyboard-shortcuts.md) — every shortcut the window lists and its default, which can be
  changed and which are fixed, how a combination is written and matched by physical key, Settings' Keyboard shortcuts
  section (changing, recording, resetting, restoring the defaults, what assistive technology is told), which
  combinations are refused and why, where the shortcuts show, that they are kept per browser or device, and their
  form in a configuration document.
- [Agent accounts](product/agent-accounts.md) — Settings' Agent accounts section: the added agents list (how it is
  first filled, the undecided state, where it applies), the agent strip and its not-installed warning, Add agent,
  reordering and removing an agent, the empty state, the default account and renaming it, owned accounts and their
  actions menu, the accounts table and the account settings dialog, turning an account off with its Disable switch,
  sharing the default configuration, the per-agent permission approval, and adding, editing and removing an account.
- [Logging accounts in](product/logging-accounts-in.md) — signing an agent account in from Settings → Agent accounts:
  where Log in appears, how each agent signs in, the login dialog and its focus behaviour, how a login ends, and the
  quota re-read afterwards.
- [Sharing the default configuration](product/sharing-the-default-configuration.md) — a non-default account sharing the
  agent's default directory through links: the per-account choice, what each agent shares and what is never shared,
  turning it on (backups, history moved or merged, Codex rows carried) and off (backups restored, the account's own
  conversations taken back), a change carried out at once or refused, the account locked while it runs, its progress
  and the account's sharing state, how a failed one is reported and retried, the repair before every launch, Claude
  Code's auto memory, and removing or repointing a sharing account.
- [Language](product/language.md) — the 17 offered languages and why they are ordered by tag, English as the fallback,
  the default picked from the system's languages on the first launch and kept from then on, the language chosen in
  Settings, how the system's languages map onto the list, what follows the current language and what does not (report
  pages, text aimed at agents), where the choice is kept, and what is translated so far.
- [Launching agents](product/launching-agents.md) — the guarantee that Octoboard installs nothing into a project and
  never writes the user's agent configuration (the five narrow exceptions being the conversation a switch of a session's
  account copies, the Codex thread row that switch points at the copied head, the Grok Build trust entry carried to the
  user's own store, an account set to share the default configuration, and selected Grok settings a session saved,
  carried on exit into the file a linked Grok configuration leads to), and that a fast-forward and a push are the ways
  it changes a project repository, both only at the user's word; the three things injected per launch (plus Claude
  Code's status line, which runs the user's own and reports the context window) and the console's one generated
  instruction file shared by every agent, ending with the console's own instructions, its refresh on each launch and
  which sessions an edit reaches, the launch environment, and the per-agent specifics.
- [Folder trust](product/folder-trust.md) — each agent's own folder-trust confirmation and where it records the trust,
  the one auto-trust permission Claude Code, Codex and Grok Build share (a project or a trusted folder), the trust
  dialog, how Octoboard presses each agent's confirmation, carrying Grok Build's trust entry into the user's store, and
  the trusted-folders rules.
- [Application lifecycle](product/application-lifecycle.md) — the desktop form in full and what the server form changes:
  what Octoboard runs on and how it is distributed, startup and the single-instance rule, who can reach the daemon (any
  local program, but no web page in a browser; in the server form anyone who can reach the port, which is why today's
  daemon must stay on loopback, what a reverse proxy would have to rewrite, the SSH port forward as the one way in
  today, the access model the form still lacks, and that whoever holds a credential is taken for the owner), losing the
  daemon connection, what closing the window does — a quit unless the Settings switch for running in the background is
  on, which is off to begin with, and then a hidden window out of the Dock with everything kept running — and every way
  the window comes back, the quit confirmation and which gestures it covers, how to quit a window that has stopped
  responding, crash behaviour, the daemon on its own (who starts and stops it, a browser tab closing ending nothing, a
  restart interrupting every session, the data directory being the owner's), and the files Octoboard keeps under
  `~/.octoboard` (and the two it keeps outside it), including the database that also holds the settings the daemon
  stores.
- [In-app updates](product/in-app-updates.md) — how the macOS app updates itself: the Beta and Stable channels and
  switching between them, automatic and manual checks and what counts as up to date, the update notice strip (available,
  downloading, installed) and its focus and announcements, installing in place with the restart only suggested, which
  failures raise an alert, and the Updates and Update channel rows in Settings > About.
- [Menu bar icon](product/menu-bar-icon.md) — the macOS menu bar icon: what a click does, its menu
  (running sessions grouped by status in sidebar order, at most 10 per group with an "N more…" line, "No sessions
  running", the status and agent pictures, the per-agent quota submenus, Open, Quit), choosing a session from it, and
  its Quit.

- [Website](product/website.md) — the localized homepage's account switching, agent handoff and usage features,
  cross-project workflow illustration, sample application preview, privacy and error pages, keyboard navigation,
  motion, release-page links, platform roadmap (including the planned Linux server), privacy disclosures,
  planned remote host support and how the server form differs from it, search indexing, and the agent-readable
  Markdown copies.

## Reference

- [Architecture](architecture.md) — what Octoboard is, its two deployment forms (the desktop form and the server form:
  the daemon alone on a headless Linux server, reached by its owner from a browser on another machine — one user's
  either way, a credential standing for the owner, and why that form is not a "remote host") and its core concepts,
  the layers and why the daemon is split from the application, the constraints the server form puts on the browser as
  a client (a secure context for notifications and the clipboard, hence TLS) and on access (a credential per device,
  not reachability — none of it built yet) and the known boundary that several users on one daemon are outside the
  design, the technology choices, the orchestration design decisions (the MCP server as a child process, structured
  arguments, why reporting is not forced, why a project session asks for a console session rather than being given
  more reach), the known pitfalls of the Tauri / Rust approach, and the reasoning behind the data model.
- [Agent CLI reference](agent-cli-reference.md) — what the three agent CLIs themselves do, against the versions the
  facts were established on: each one's hook events, the payload fields and the keys a turn can be correlated on, what a
  failing hook costs, how a project's own configuration layers around an injected one, how Octoboard is injected into
  each agent and the conditions that come with it, the handler types each agent runs per event, the dialogs, notices,
  pre-session screens and views an agent raises that a written message would answer (and the launch overrides that keep
  some away), each agent's own folder-trust confirmation (when it appears, how it reads and is accepted, and where the
  agent records the trust), each one's own time limit on a tool call — what it does to the server and to its own model
  when the limit passes, and whether its model takes turns while a call is outstanding — what the model is handed and
  how it reaches an MCP tool, the rules for writing into a running session, where each keeps a session's conversation
  record (Codex's history segments included) and what moving it to another config directory or another account takes,
  the measured risks of two accounts sharing a session directory without the rest of their configuration, and how each
  agent treats a configuration directory whose entries are links into another — which entries it keeps and what they
  hold, whether a linked file survives the agent saving it, what loads through a link, and what loses its automatic
  approval there.
- [UI accessibility standard](ui-accessibility.md) — HeroUI default styles taking precedence, WCAG 2.2 AA for custom
  styles in both appearances, and the application-level accessibility rules: keyboard access (Tab, the exception for a
  react-aria collection that manages its own focus, F6 as the way out of the terminal), visible focus and why it never
  uses CSS `:focus-visible` in the app's WKWebView and which moves made by script have to set the modality themselves,
  and the hand-off for a dialog opener that never took focus, is disabled or is gone, accessible names, including the
  one an icon-only control needs beside its tooltip, roles and states — the status region that has to stay mounted, the
  durable visible copy announced from a status region of the surface's own, a disclosure's status region mounted with
  each opening outside the folded body, a focus restore after a press deciding by whether the pressed control had focus,
  a status whose region sits under an open modal being written after the close and the focus move, a status waiting
  until the window is attended, a visual heading being a heading element, and the one `main` the content panel always
  holds — contrast (thresholds, shared row action menu button states, preserving HeroUI's default colours and the
  confirmed Tabs hover treatment, where a custom-style fix belongs, auditing a token or surface change, a region with a
  surface of its own and a glyph on the window material), independent hover marquees and their permitted single-label
  control scopes, colour never the only carrier, motion, and the shortfalls the user has kept on purpose.
- [VoiceOver verification cases](voiceover-cases.md) — real-device VoiceOver results kept as reusable cases: the rules
  for when a new announcement needs a listening pass (what it takes to match a passing case and the checks a caller must
  still pass), the evidence limits, each case's UI location, mechanism, rendered ARIA wiring, spoken result, environment
  and reuse scope (control names, the `StatusAnnouncer` timings with and without a focus move, the login success
  announcement and focus hand-off, a delayed `role="alert"`, on-demand reading of a confirmation warning and the trust
  caution, and the reading paths into a hover- or Enter-opened popover), the patterns heard failing, the unconfirmed
  ones, and which case each existing `StatusAnnouncer` caller matches.

## Code

- [Project map](project-map.md) — the monorepo layout (deliverables under `apps/`, the daemon among them, shared code
  under `packages/`), the two workspace roots (pnpm and Cargo) and their common commands, then navigation from the
  code tree to each module's own doc.

## Development memory

- [Catching a branch up with its target](memory/catching-a-branch-up-with-its-target.md) — why the target branch to
  compare against is the local `main`, usually ahead of `origin/main`, which branches the revert gate guards, why a
  guarded branch that has been pushed cannot be rebased onto a target branch that has moved, and merging as the way
  through.
- [Line wrapping](memory/line-wrapping.md) — the widths committed text is wrapped at, why format checks do not enforce
  prose width (and so why an edit re-wraps the whole paragraph it touched), and why a width is counted
  in characters rather than bytes.
- [Measuring build and test speed](memory/measuring-build-and-test-speed.md) — judging a speed change by CPU time and
  output size, measured back to back at a similar load and counting every process that does the work, never by
  wall-clock time on a shared machine, against a control worktree at the base commit; why checks are made cheaper by
  doing less and caching, not by parallel steps, CI test runs or a shared `CARGO_TARGET_DIR`; why a shared Rust
  compilation cache (sccache) across worktrees was measured and dropped; and how to profile a slow UI test file and when splitting
  one is worth it (only a file that dominates the run, since splitting does not cut CPU).
- [Probing agent CLIs](memory/probing-agent-clis.md) — how to establish what the three agent CLIs actually do, and how
  to probe a live Octoboard session that launches them: which session markers to strip and why enumerating them beats
  matching a prefix, why neither `--help`, the binary's own strings, nor a config file's silent acceptance of a
  capability's name can be trusted, why an event's arrival has to be seen on every path with the handler type actually
  registered, why the exact behaviour of a check Claude Code runs on an input is settled by finding the code behind the
  symptom's wording in its executable and running it as an oracle, how to settle a question without spending a model
  turn and where that stops being yours to decide — substituting only the inference endpoint with a local stub where the
  question is what the agent does, and driving a probe that goes through the daemon from a client of its own on the
  control WebSocket rather than building the application — how to prove a per-launch injection and an MCP tool out of
  band rather than through the model, why a probe expecting "no" needs a positive control (the kind of login being one
  of its preconditions) and why a measured silence also has to be bounded by what would have ended it, why a permission
  probe has to be set up against the user's own settings with the mode in effect confirmed from the session itself, and
  the residue a probe leaves — in the user's configuration and in their live `~/.octoboard` data — and how a probe that
  needs no logged-in agent avoids the latter with a throwaway `HOME`/`TMPDIR`, using Codex's sign-in screen as session
  output, and how to put a fake agent CLI on `PATH` for an isolated daemon so the real one does not launch instead, and
  which entries a throwaway config directory built as a symlink farm has to own so the probe does not write through them
  into the user's own.
- [Building and launching the app for verification](memory/building-and-launching-the-app-for-verification.md) — how to
  build, launch and isolate the app for a verification: when to launch the real window, building the daemon and
  confirming the running sidecar's binary, `pnpm build:app` (a `tauri build` that cannot sign), `open` rather than
  exec'ing the binary, a build-time app identifier plus a throwaway environment for separate daemon and WebKit data,
  checking the running processes and storage before driving the window, cleanup confined to that identity, running
  under other system languages for one launch, reading the wire through a wrapped sidecar, why nothing the daemon
  logs after
  the application is gone can be read and why putting its log on a file removes the condition under investigation,
  sharing the machine with other worktrees' dev apps (fixed port, stopping by PID only), and fully reloading a dev
  window before judging a defect in it.
- [Verifying the desktop UI](memory/verifying-the-desktop-ui.md) — how to verify terminal and UI behaviour in the
  desktop app: why a verification stays as narrow as the change, why an app-wide UI hunt narrows candidates by reading
  code before rendering any and what a brief for such work must cap, and why a shared component's markup widens that
  scope to every surface already using it, settling a change to the data a fixture answers with on the replies rather
  than on renderings, looking at a UI state in the gallery before staging it through a daemon and what the gallery
  cannot settle, why a check that rests on translated copy is made in `zh-Hans` since the other 15 languages render
  English, holding what a reply carries in a fixture factory's signature first and in the fixture daemon's refusal only
  where no signature can settle it, and where to take that condition from, settling hand-written `git` output against
  real `git` in a throwaway repository, checking in WebKit with real pointer input and with long and CJK text, bisecting
  a symptom against the daemon, where to watch for a report page's blocked navigation, where a network probe's positive
  control comes from, proving that no key reached the terminal with a stand-in agent that echoes every byte, and judging
  a packaged window's geometry from measured pixels and a real click rather than from a capture alone.
- [Driving and reading the real app window](memory/driving-and-reading-the-real-app-window.md) — how to script input
  into the packaged app's real window and read output off it on the user's machine: ruling out a locked screen or a
  window not on screen before trusting a capture, getting an error out of a blank window, what a scripted GUI probe can
  and cannot prove — including why a mark inside a row cannot be aimed at from a capture of the un-hovered row — input
  that leaves the user's work alone, which steps need the user's hands, and simulating a hung page.
- [Building on HeroUI and react-aria](memory/building-on-heroui-and-react-aria.md) — reaching for and wrapping the
  library's own component in `packages/ui`: why one HeroUI 3 provides is used rather than hand-built, where to read
  what it provides and what it actually puts in the DOM, using the project's own wrapper where one exists and what a
  justified hand-built one is built on and where its reason is written, why a compound component's parts take their
  own layout classes and tolerate no element between them, why a control's size follows the window's width and not its
  container's, why a state of your own needs a `data-*` name react-aria does not write itself, why an ancestor's
  context beats the default in an element's own file, where a `ListBox` or `Tree`'s padding goes, that a `Tabs` panel
  is mounted only while its own tab is selected so nothing a section must not lose is held inside it, and the three
  tooltip shapes — `TitledControl`, `IndicatorTooltip`, `AnchoredTooltip` — with why `Tooltip.Trigger` is not one of
  them.
- [Writing UI dialogs and fields](memory/writing-ui-dialogs-and-fields.md) — dialogs and form fields in `packages/ui`:
  a confirmation, typed-word deletions included, being `ConfirmDialog` rather than a dialog of its own, a
  confirmation's options sitting outside its one warning, which sits under them (each ticked option adding its line to it, the items behind a
  Details disclosure that stays as the user left it), a single-line
  field handing its form the trimmed value through `useTrimmedField`, what is wrong with one field
  showing under that field from the first submit on and the two conditions a field built outside `TextInput` has to
  meet to mark itself invalid, a field's note being its `Description` while HeroUI typography inside a field throws,
  which modals are a `Dialog` at all and which keep a frame of their own, and the dialog body being the one scroller,
  fading where more is hidden, with nothing reaching above it.
- [Styling the UI](memory/styling-the-ui.md) — getting a style to land in `packages/ui`: styling a HeroUI component
  through its supported variants and slots, keeping its default appearance while adapting layout and sizing, and the
  per-component stylesheet under `@heroui/styles` as the readable source of what the component imposes, imported one
  by one in `style.css` so a newly used component needs its sheet added (and the `tw-animate-css` pin moved on a
  HeroUI upgrade); why a `className` background on a HeroUI part needs its state fills restored; why a Tailwind class name has to stand in the source as literal text and how to confirm its utility was emitted, why a `docked:`
  utility loses to an `sm:` or `rtl:` twin on the same property and what to write instead, dimming a region with a veil
  rather than `opacity` in the app's WKWebView, and taking a style that follows a layout measurement in an effect with
  no dependency array plus a `ResizeObserver`, since an observer alone misses a box whose content changes without its
  size and an element React swaps underneath it.
- [Fading the edge of a scroller](memory/fading-the-edge-of-a-scroller.md) — every surface that hides content behind
  its own edge taking the fade from one module, including overlays for a pinned heading or row, the three reasons the scrolling element
  itself has to carry it, where padding and a focus ring go around a mask that clips by the border box, and the fade
  being a hint over an ordinary overflow container.
- [UI text and reading direction](memory/ui-text-and-reading-direction.md) — every string the user reads going through
  the message catalog and in what shape a translation can follow it (`useT` over the module-level `t()`, a helper
  taking the translator, `PlainMessageKey` tables, placeholders and plural messages, a HeroUI part's own hardcoded
  English), what must not be cut when an existing message is shortened, and laying out by reading direction with
  logical utilities, mirrored directional icons, two-glyph chevrons and an explicit `dir` where `auto` cannot resolve.
- [What goes on the window material](memory/what-goes-on-the-window-material.md) — what may sit on the translucent
  window material against what belongs on the opaque content panel, the two layouts the user rejected, and a glyph or
  tint there being measured against the material itself rather than only against `--chrome`.
- [Meeting the accessibility bar](memory/meeting-the-accessibility-bar.md) — HeroUI default styles taking precedence and
  the WCAG 2.2 AA bar for custom styles in both appearances, the shortfalls kept on purpose being left alone, and the
  three rules that decide how the code is written: the focus ring from `useFocusVisibleProps`, a change announced
  through `StatusAnnouncer`, and a colour-token change audited over every gallery scenario.
- [Focus and key handling in the UI](memory/focus-and-key-handling-in-the-ui.md) — where keyboard focus goes and who
  receives a key in `packages/ui`: why a HeroUI control pressed with the mouse takes focus off the terminal, when
  `preventFocusOnPress` is needed and why "⋯" menus are built on `ActionMenu`, the other ways focus falls to `<body>` (a
  region hidden with CSS, a reordered list, a wrapper that renders nothing once its last child is gone) and the hooks
  that put it back, including the one to reach for where a region goes away from a state change and there is no removal
  handler to hand focus on from, why a `focusout` with no `relatedTarget` is three different things — so a region that
  hides itself once focus leaves it is gated on `document.hasFocus()` — why a dialog loses its Escape and its Tab
  containment when a focused control unmounts or turns disabled — so its subject is switched with a reset key rather
  than by re-keying it, and an action that takes seconds marks its button pending rather than disabled — and why no
  `blur` fires on that removal either, why nothing in the React tree gets Escape first (a pane's own `window` listener,
  and react-aria's while any tooltip is open), why a key handler goes on a plain DOM element rather than on a HeroUI or
  react-aria component, which swallows every key it is handed a handler for, and how a surface that has to stay usable
  while a dialog is open is marked a top layer and ordered against the backdrop.
- [Reading daemon state in the UI](memory/reading-daemon-state-in-the-ui.md) — why a map read through
  `useDaemonStore` has a new identity after every daemon event of its kind, so an effect or memo meant to fire on one
  change depends on the resolved values it reads and never on the map, and a memo reads only those values, resolved
  into locals first, or the React Compiler skips the component, and the derivations that do take the map; why an
  empty collection stands for "nothing has arrived yet" until `hosts` is set, so a predicate meaning "the daemon has
  none of these" is gated on that rather than on the collection, and why no gallery scenario can catch a missing gate;
  and why the reply to a request arrives after the change it made is already in the store, so code after
  `await request(...)` has to be correct under either order; and why a session's terminal title is never steady text
  (agents animate glyphs at its start), so nothing disruptive is keyed off it as sent.
- [Sending browse requests from the UI](memory/sending-browse-requests-from-the-ui.md) — why the window's browse
  requests are bounded by one count shared across the window rather than one per component (the daemon's per-connection
  bound, and a sent request that is never timed out or cancelled when its component unmounts), sizing a new kind of
  request together with the existing limits, and the spare unit each replaceable slot reserves because the daemon
  counts a replacing request before it gives the replaced one up.
- [Writing automated tests](memory/writing-automated-tests.md) — how lean unit tests are kept (one case per rule,
  table-driven) and how to judge an existing test redundant when trimming, then the fixture conventions this
  project's tests need on macOS: why an executable written fresh per test flakes only under a parallel run, why every wait on a spawned process is bounded by the shared `PATIENCE`
  (exec stalls of seconds on machines with endpoint-security software) and how to tell that stall apart, why a daemon
  test that gives anything a model or an effort seeds the agent's model list first, and how to verify behaviour the daemon derives from an agent's own output by replaying a committed capture rather than staging
  a live session, why a jsdom test must leave nothing running when it ends with the cleanup bound to the test that
  armed it, since stray work crashes a later file and the failure names the wrong one, how a test settles HeroUI's
  module-level toast queue and which two tempting fixes only mask it, why a test that fakes timers has to assert for
  itself that it left none pending, why a surface behind a `React.lazy` boundary has to be settled before a test reads
  it and why a clean whole-file run is no evidence that it was, why a dialog test waits out react-aria's mount refocus
  before a step whose focus it asserts, why a jsdom test cannot tell where keyboard focus ends
  after a session is selected, and why it cannot tell what a selection over the viewer's code copies either, since the
  tests mock the renderer away and the real one draws into a shadow root; and why a daemon budget is tested by lowering
  it through a test-only thread-local override rather than by asserting the constant, an override that reaches only
  code on the test's own thread.
- [Writing desktop shell code](memory/writing-desktop-shell-code.md) — rules for the Tauri shell in
  `apps/desktop/src-tauri/`: why work meant to run after the current AppKit callback returns is queued on the main
  `NSOperationQueue`, since `run_on_main_thread` runs it at once when called on the main thread, and why every
  native hook into Tauri, tray-icon or muda internals is re-checked in the running app after raising them, since such
  hooks fail silently.
- [Running git from the daemon](memory/running-git-from-the-daemon.md) — why every `git` the daemon spawns is built
  from the one shared base in `git_env.rs` and bounded in time there (a `git` that can prompt hangs the daemon with
  nothing naming the stuck call, and a read that rewrites the index fails an agent's `git add` on `index.lock`); why a
  `git` that compares with the worktree passes `--ignore-submodules`, since it otherwise runs a status in every
  checked-out submodule; why a diff between two specific versions names each one `<revision>:<path>` and ends its
  arguments with `--`; what a test proving a read leaves the repository alone has to set up first (a stale-stat
  fixture and a positive control); and why a test fixture is built only from Git 2.36's commands.
- [Adding a dependency](memory/adding-a-dependency.md) — which `packages/ui/package.json` field an npm package goes
  in: anything the UI bundle ships (JS, CSS imported by package name, Tailwind plugins) in `dependencies`, since the
  third-party licenses list reads only that field.
- [Adding a user setting](memory/adding-a-user-setting.md) — the checklist for adding a user setting: exposing it in
  the agents' settings tools unless only the user may change it, and carrying every application setting that is not
  valid only on this machine through a full configuration export and import.
- [Adding a daemon error code](memory/adding-a-daemon-error-code.md) — what the UI needs in the same change when the
  daemon starts returning a new error code (message text, params, recovery group, Settings button), so the refusal is
  not shown as an unexpected bug.
- [Adding a reason an account cannot be used](memory/adding-a-reason-an-account-cannot-be-used.md) — every daemon and
  UI place that must check a new reason an account cannot take new work, the console and project dialog pickers
  included.
- [Writing text into a session](memory/writing-text-into-a-session.md) — why any new notice, brief or opening prompt the
  daemon writes into a session's input needs a fixed opening constant listed in `session_history.rs`'s
  `OCTOBOARD_OPENINGS`, or the archived history shows it as the user's own words.
- [Writing daemon code](memory/writing-daemon-code.md) — conventions for the Rust daemon: live state the daemon derives
  held on `AppState` and published by its own event rather than as a field on a stored record, with the cleanups that
  follow from there being no deletion event for it; why a repeating refresh is timed by the client and has to be bounded
  in the daemon by a drop-guard claim and a completion floor, since an in-flight claim alone never fires across clients
  polling on their own phases; why the Rust crates are
  linted with `cargo clippy --all-targets`, since the plain command lints no `#[cfg(test)]` code and so none of this
  repository's tests; how to type-check Linux-only code from macOS (a scratch crate, since bundled SQLite blocks a cross-check
  of the daemon); why a request handler can never assume it is alone with the daemon's
  state, so it re-checks after a long step or holds a claim; and why macOS host integrations avoid Apple Events
  (`osascript`, a crate's Finder default) in favour of consent-free APIs.
- [Handling paths and files in the daemon](memory/handling-paths-and-files-in-the-daemon.md) — why a path built from a
  hook's values, `Session.agent_session_id` among them, holds to the agent's id shape, a daemon-resolved records
  directory, the exact layout depth and no linked directories; why a path taken from text an agent wrote is opened only
  through `path_links::resolve`; why something counts as gone only on `NotFound`; why a file inside a project is read
  only through `browse::live`, never with `File::open`, since a project's contents can link out, block as a FIFO or be
  swapped mid-read; and why two account directories are compared by `account_dirs::account_dir_key`, never by their
  text.
- [Writing UI code under the React Compiler](memory/writing-ui-code-under-the-react-compiler.md) — why anything a
  render reads besides props, state, context and hook results (the clock, `getState()`, module state) is frozen at its
  first value and what to read instead; what leaves a component silently uncompiled (ref access during render,
  `try/finally`, `??=`, a memo reading more than its deps, an `eslint-disable` of the two hook rules) and how to list
  bailouts with the compiler's logger; and what keeps a prop every row takes stable across daemon events.
- [Keeping the UI responsive in WebKit](memory/keeping-the-ui-responsive-in-webkit.md) — why a style change on
  `<html>`, `<body>` or `#root`, or one flipping `#root`'s `:last-child`, is expensive per interaction in the app's
  WebKit and why each HTML entry keeps a sibling after `#root`; why each synchronous layout read multiplies the
  restyle cost; why every modal overlay still pays one whole-document restyle (React Aria's `inert` on `#root`;
  `isNonModal` is not the fix); keeping a tag, class or id on the rightmost compound of every CSS rule (the
  typography plugin only through `typography.mjs`, and checking the built CSS for rules WebKit cannot file); and
  measuring UI latency with `pnpm bench:wkwebview` in the system WKWebView on the base and on the branch (Playwright's
  WebKit and Chrome under-report restyle cost), never against the user's own daemon.
