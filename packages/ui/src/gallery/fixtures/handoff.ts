import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import { FIXTURE_AGENTS } from "../../fixtures/fixtureProviders";
import type { Ui } from "../../fixtures/interact";
import type { Scenario } from "../scenario";
import { accountOf, SAMPLE, branchItemOf, minutesAgo, processTreeOf, sessionOf, snapshotState, worktreeItemOf, WINDOWS_MACHINE } from "../../fixtures/builders";
import { WINDOWS_HOST } from "../../test/hosts";

const { console: console_, web, api, consoleSession: hub } = SAMPLE;

// A project session that sits under the hub and takes the whole place along: it owns three sessions,
// one of them archived, and is responsible for two branches, one of them checked out in a worktree.
const coordinator = sessionOf("s-handoff-coordinator", console_.id, web.id, "Rework the result ranking", "idle", { bound_to: hub.id, started_at: minutesAgo(50) });
const owned = [
  sessionOf("s-handoff-owned-1", console_.id, web.id, "Tidy the changelog", "idle", { bound_to: coordinator.id, started_at: minutesAgo(40) }),
  sessionOf("s-handoff-owned-2", console_.id, api.id, "Update the search fixtures", "idle", { bound_to: coordinator.id, started_at: minutesAgo(35) }),
  sessionOf("s-handoff-owned-old", console_.id, web.id, "Earlier ranking experiment", "archived", { bound_to: coordinator.id, started_at: minutesAgo(120) }),
];
// A project session bound to no one that owns none and is responsible for no branch: nothing moves.
const bare = sessionOf("s-handoff-bare", console_.id, api.id, "Add idempotency keys", "idle", { started_at: minutesAgo(30) });
// What the hub owns besides the coordinator: a live session and an archived one.
// It has a conversation on record, which its agent's desktop application can open although the session is at rest with a process.
const sibling = sessionOf("s-handoff-sibling", console_.id, web.id, "Update the dependencies", "idle", {
  bound_to: hub.id,
  agent_session_id: "5c2e9a44-7b0d-4e6f-8a31-0d4b1c7e92aa",
  started_at: minutesAgo(45),
});
// A bound session at work, which keeps its owner (`sibling`) from being handed off while it can still report to it; it
// can itself still be unbound.
const working = sessionOf("s-handoff-working", console_.id, web.id, "Fix the summary layout", "working", { bound_to: sibling.id, started_at: minutesAgo(20) });
// A bound session whose launch has not finished: the menu cannot hand it off or unbind it yet, though it can be archived.
const starting = sessionOf("s-handoff-starting", console_.id, web.id, "Set up the search index", "starting", { bound_to: sibling.id, started_at: minutesAgo(1) });
const hubOwned = [
  sibling,
  sessionOf("s-handoff-hub-old", console_.id, api.id, "Earlier release check", "archived", { bound_to: hub.id, started_at: minutesAgo(180) }),
];

// Codex has two named accounts beside its default one, which the dialog's joined picker offers.
const settings = {
  ...DEFAULT_SETTINGS,
  added_agents: [...FIXTURE_AGENTS],
  default_clone_dir: "/Users/dev/Projects",
  accounts: [
    accountOf("a-codex-work", "codex", "Work account", "/Users/dev/.codex-work"),
    accountOf("a-codex-personal", "codex", "Personal account", "/Users/dev/.codex-personal"),
  ],
};

const state = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [hub, coordinator, ...owned, ...hubOwned, bare, working],
  settings,
});

// The session a handoff of `bare` created before archiving the original failed, which the failure names. It carries the
// original's title and, both being idle, is listed after it (newest first), so the original's menu is the first one named so.
const received = sessionOf("s-handoff-received", console_.id, api.id, "Add idempotency keys", "idle", { agent: "codex", started_at: minutesAgo(31) });
const receivedState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [hub, coordinator, ...owned, ...hubOwned, bare, working, received],
  settings,
});

// The same sessions plus an idle one the user locked, which Hand off and Archive refuse; kept out of `state` so
// the lock mark does not appear in the other scenarios.
const locked = sessionOf("s-handoff-locked", console_.id, api.id, "Review the rate limits", "idle", { locked: true, started_at: minutesAgo(25) });
const lockedState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [hub, coordinator, ...owned, ...hubOwned, bare, working, locked],
  settings,
});

// The same sessions plus the one still starting, kept out of `state` so it does not appear in the other scenarios.
const startingState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [hub, coordinator, ...owned, ...hubOwned, bare, working, starting],
  settings,
});

// An independent project session whose status is one a newer daemon sent that this client does not know; kept out of
// `state` so its row does not appear in the other scenarios.
const unknownStatus = sessionOf("s-handoff-unknown", console_.id, api.id, "Reindex the archive", "paused_for_review", { started_at: minutesAgo(30) });
const unknownStatusState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [hub, coordinator, ...owned, ...hubOwned, working, unknownStatus],
  settings,
});

// A bound session and its owner whose titles run to paragraphs, as a pasted brief does: the unbind confirmation
// names each of them once, and the chips have to wrap inside the dialog.
const longCoordinator = sessionOf(
  "s-unbind-long-coordinator",
  console_.id,
  web.id,
  "Rework the result ranking so that exact title matches outrank fuzzy body matches and then re-tune the recency decay and add the per-project boost described in the planning notes and update every search fixture and snapshot that the new ordering changes",
  "idle",
  { bound_to: hub.id, started_at: minutesAgo(50) },
);
const longHub = {
  ...hub,
  title:
    "Release coordinator for the search overhaul that keeps the ranking and indexing and autocomplete work in step and hands the finished branches to review one at a time and reports anything that blocks the Friday cut",
};
const longTitleState = snapshotState({ consoles: [console_], projects: [web, api], sessions: [longHub, longCoordinator], settings });

// A project session that has stopped (interrupted) with its conversation on record, which its agent's desktop
// application can open; kept out of `state` so it does not appear in the other scenarios.
const stopped = sessionOf("s-handoff-stopped", console_.id, api.id, "Migrate the search index", "interrupted", {
  agent_session_id: "0b1f6c7e-5d1a-4c52-9a77-3f0d2f9a1c10",
  started_at: minutesAgo(70),
});
const stoppedState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [hub, coordinator, ...owned, ...hubOwned, bare, working, stopped],
  settings,
});
// An idle project session at its prompt with a conversation on record, the dev server and the watcher it started still
// running, and a worker bound to it that is at work: opening it in its agent's application archives it with the worker,
// so the confirmation lists the worker and the processes first. Kept out of `state` so it does not appear in the other
// scenarios.
const resting = sessionOf("s-handoff-resting", console_.id, web.id, "Preview the ranking change", "idle", {
  agent_session_id: "e3a8d1f0-2c64-4b9e-9f57-6a0c8b21d4e7",
  started_at: minutesAgo(60),
});
const restingWorker = sessionOf("s-handoff-resting-worker", console_.id, web.id, "Check the ranking fixtures", "working", { bound_to: resting.id, role: "worker", started_at: minutesAgo(20) });
const restingState = snapshotState({ consoles: [console_], projects: [web, api], sessions: [hub, coordinator, ...owned, ...hubOwned, bare, working, resting, restingWorker], settings });
const restingTrees = [
  processTreeOf("700:1", resting, "pnpm dev --port 5173", 45, { process_count: 3, memory_bytes: 142_000_000, ports: [5173] }),
  processTreeOf("701:1", resting, "vitest --watch --reporter=dot", 30, { process_count: 2, memory_bytes: 96_000_000 }),
];
// An independent project session beside what could own it: two console sessions, and two running coordinators of its
// project, one of them with a title that runs past the menu. The sessions that cannot own it stay in the sidebar: a
// worker and a coordinator of another project.
const bindable = sessionOf("s-bind-free", console_.id, api.id, "Add idempotency keys", "waiting_user", { started_at: minutesAgo(30) });
const bindConsoleSessions = [
  hub,
  sessionOf("s-bind-hub-codex", console_.id, undefined, "Release coordinator", "working", { agent: "codex", ordinal: 2, colour: "blue", started_at: minutesAgo(70) }),
];
const bindCoordinators = [
  sessionOf("s-bind-coordinator", console_.id, api.id, "Rework the search index", "idle", { bound_to: hub.id, role: "coordinator", started_at: minutesAgo(50) }),
  sessionOf(
    "s-bind-coordinator-long",
    console_.id,
    api.id,
    "Migrate every endpoint to the new pagination scheme and update the clients, the fixtures and the documentation",
    "working",
    { agent: "codex", started_at: minutesAgo(20) },
  ),
];
const bindExcluded = [
  sessionOf("s-bind-worker", console_.id, api.id, "Update the search fixtures", "idle", { bound_to: "s-bind-coordinator", started_at: minutesAgo(40) }),
  sessionOf("s-bind-other-project", console_.id, web.id, "Tidy the changelog", "idle", { started_at: minutesAgo(35) }),
];
const bindState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [...bindConsoleSessions, ...bindCoordinators, ...bindExcluded, bindable],
  settings,
});
// No running console session and no other coordinator: nothing can own the session.
const bindNoneState = snapshotState({ consoles: [console_], projects: [web, api], sessions: [bindable], settings });

const GROUP = "Sidebar sessions";

const openMenu = (title: string) => (ui: Ui) => ui.press(ui.t("sidebar.session.actions", { title }));
const openHandoff = (title: string) => [
  openMenu(title),
  (ui: Ui) => ui.press(ui.t("sidebar.session.handoff")),
];
const openUnbind = (title: string) => [
  openMenu(title),
  (ui: Ui) => ui.press(ui.t("sidebar.session.unbind")),
];
// The dialog's confirming button, pressed once the dialog is up.
const confirmHandoff = (title: string) => [...openHandoff(title), (ui: Ui) => ui.confirm()];

const coordinatorBranches = [
  branchItemOf(web, coordinator, "feat/ranking-rework"),
  worktreeItemOf(web, coordinator, "feat/ranking-weights", "/Users/dev/worktrees/website-ranking-weights"),
];

export const handoffScenarios: Scenario[] = [
  {
    id: "sidebar-session-menu-handoff",
    group: GROUP,
    category: "normal",
    title: "Menu of an idle project session, Hand off offered",
    description: "The selected project session's menu offers Focus mode beside Lock and Pin, showing the current global shortcut rather than a single-letter key. Choosing it enters Hub's focus mode, since the session is bound to Hub. Dividers separate that group, Copy session ID, the account and binding actions, Rename, and Archive, with no group headings. Copy session ID has its own group, with the ID in smaller muted monospace text beneath its label inside the same clickable item. Change model, with a chip icon, is enabled and leads the account and binding actions, directly before Hand off. Hand off, which is enabled, has Unbind directly after it, then a divider, then Rename, then a divider, then Archive; Unbind is offered because Hub owns the session. Unbind opens a confirmation first (see the Unbind session dialog); the session keeps its binding badge, the dot in Hub's colour, until that is confirmed.",
    state,
    steps: [(ui) => ui.press(ui.session(coordinator.title)), openMenu(coordinator.title)],
  },
  {
    id: "sidebar-session-menu-windows",
    group: GROUP,
    category: "normal",
    title: "Menu of a project session on Windows",
    description: "The selected project session's menu offers Focus mode with the global shortcut written as text key caps, Ctrl, Alt and →, in place of the symbols of an Apple keyboard. The caps are wider than the symbols: look that the menu's width holds them beside the label and that no item's label is cut. Choose Spanish, Italian or Russian with the language control to see the longest labels (\"Modo de concentración\", \"Modalità concentrazione\", \"Режим фокусирования\") fit, and Arabic to see the caps keep their writing order with a gap between them.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions: [bare], ...WINDOWS_MACHINE }),
    host: WINDOWS_HOST,
    steps: [(ui) => ui.press(ui.session(bare.title)), openMenu(bare.title)],
  },
  {
    id: "sidebar-session-menu-unbound",
    group: GROUP,
    category: "normal",
    title: "Menu of an unbound project session, Bind to in place of Unbind",
    description: "The selected independent project session's Focus mode entry shows the current global shortcut and enters Search API's focus mode. Hand off is enabled and is followed by Bind to, with a chevron and the link icon, in the place a bound session shows Unbind, then a divider, then Rename, then a divider, then Archive: with no owner to be released from, there is no Unbind entry.",
    state,
    steps: [(ui) => ui.press(ui.session(bare.title)), openMenu(bare.title)],
  },
  {
    id: "sidebar-session-menu-bind",
    group: GROUP,
    category: "normal",
    title: "Bind to submenu of an independent project session",
    description: "The Bind to submenu is open on the independent session, in the place a bound session's menu shows Unbind. It lists two groups: first the console sessions under the heading Console sessions, then a divider, then the project sessions under the heading Project sessions. Each row is the session's title with its agent icon, newest first; the long title is clipped with a fade, not wrapped. The two headings are small muted text that cannot be chosen or focused. A worker and a coordinator of another project are in the sidebar but are not listed. Choosing a row binds the session at once, with no confirmation, and it moves under that owner in the sidebar, after which its menu offers Unbind here.",
    state: bindState,
    steps: [openMenu(bindable.title), (ui) => ui.press(ui.t("sidebar.session.bind"))],
  },
  {
    id: "sidebar-session-menu-bind-none",
    group: GROUP,
    category: "exceptional",
    title: "Bind to submenu with no session able to own it",
    description: "The Bind to submenu is open on an independent project session that no running console session or coordinator could own, and holds one greyed-out item, No possible owners, with no headings.",
    state: bindNoneState,
    steps: [openMenu(bindable.title), (ui) => ui.press(ui.t("sidebar.session.bind"))],
  },
  {
    id: "sidebar-console-session-menu-handoff",
    group: GROUP,
    category: "normal",
    title: "Menu of an idle console session, Hand off offered",
    description: "Dividers separate Lock and Pin, Colour, Copy session ID, Hand off, Rename, and Archive, with Copy session ID in its own group and no group headings. Colour is its own group after Lock and Pin. Rename sits immediately above Archive, with a divider between them. This console session is not selected, so its menu has no Focus mode entry. Hand off is enabled. A console session is never bound and never bound to an owner, so there is neither Unbind nor Bind to.",
    state,
    steps: [openMenu(hub.title)],
  },
  {
    id: "sidebar-session-menu-starting",
    group: GROUP,
    category: "normal",
    title: "Menu of a bound session that is still starting, Hand off and Unbind disabled",
    description: "Change model, Hand off and Unbind are all greyed out with no reason under their names, since the session is only briefly unavailable, where a busy session would say \"Session busy\" under Hand off; Archive stays available.",
    state: startingState,
    steps: [openMenu(starting.title)],
  },
  {
    id: "sidebar-session-menu-open-in-app",
    group: GROUP,
    category: "normal",
    title: "Menu of a stopped session, Open in the agent's app with a note",
    description: "The menu of an interrupted session offers Open in Claude app, with the window icon and the W shortcut, directly after Hand off. The item is enabled, and under its label a short muted line, \"The agent's app may not see this conversation\", warns that the default directory the app reads lacks the conversation's latest record; the line only warns, and the item still opens the app. Without the line the item is a single row like its neighbours.",
    state: stoppedState,
    nativeApps: ["claude", "codex"],
    nativeAppsUnseen: [stopped.id],
    steps: [openMenu(stopped.title)],
  },
  {
    id: "sidebar-session-menu-unknown-status",
    group: GROUP,
    category: "exceptional",
    title: "Menu of a session with a status this client does not know",
    description: "The row reads Status unknown with the muted dashed empty circle. Its menu keeps Lock, Pin, Copy session ID, Rename and Archive, but offers no Change model, Switch account, Hand off, Open in app, Bind to or Unbind, since nothing that depends on the session's status is offered for one the client cannot name.",
    state: unknownStatusState,
    steps: [openMenu(unknownStatus.title)],
  },
  {
    id: "sidebar-session-menu-hand-off-blocked",
    group: GROUP,
    category: "normal",
    title: "Menu of an idle session with a child at work, Hand off disabled, Open in the agent's app offered",
    description: "Hand off is greyed out with its reason on a short line under the label, in place of a description: \"Child sessions working\", because a session under this one can still report to it (a session that is itself busy, waiting for the user or running work in the background says \"Session busy\"). Look at how much room the reason line takes in the menu. Open in Claude app is enabled with no reason line, although the session is idle and still has a process: a session at rest can be opened in the agent's app.",
    state,
    nativeApps: ["claude", "codex"],
    steps: [openMenu(sibling.title)],
  },
  {
    id: "sidebar-session-menu-open-in-app-working",
    group: GROUP,
    category: "normal",
    title: "Menu of a session at work, Open in the agent's app disabled",
    description: "Open in Claude app is greyed out with \"Session busy\" on a short line under the label, in place of a description, and Hand off says the same. Look at how much room the two reason lines take in the menu.",
    state,
    nativeApps: ["claude", "codex"],
    steps: [openMenu(working.title)],
  },
  {
    id: "sidebar-session-menu-handoff-locked",
    group: GROUP,
    category: "normal",
    title: "Menu of a locked session, Hand off disabled",
    description: "Hand off and Archive are both greyed out with \"Session locked\" under their names although the session is idle; Archive shows the reason in place of its menu shortcut. Unlock stays available. A session that is busy (\"Session busy\"), has a child at work (\"Child sessions working\", see the menu of a session with a child at work) or has no other agent to hand off to (\"No other agent\") greys Hand off the same way, with its own reason.",
    state: lockedState,
    steps: [openMenu(locked.title)],
  },
  {
    id: "dialog-handoff",
    group: GROUP,
    category: "normal",
    title: "Hand off a project session with everything that moves",
    description: "All three lines of Moves to the new session: two branches, three sessions (archived ones counted) and the place under Hub. The Hand off to field opens on Codex, the first other agent, and offers its three accounts in one picker beside the agent. Advanced is open, with Model and Reasoning effort on the agent's own setting and, under them, Claude compatibility, the switch Codex has, reading Same as project followed by a grey Off badge.",
    state,
    sessionBranches: coordinatorBranches,
    steps: [...openHandoff(coordinator.title), (ui) => ui.press(ui.t("dialog.agentSettings.advanced"))],
  },
  {
    id: "dialog-handoff-console-session",
    group: GROUP,
    category: "normal",
    title: "Hand off a console session",
    description: "A console session is responsible for no branches and reports to no one, so the only line is the three sessions it owns, an archived one included, and nothing says it is checking branches. A session that moves nothing has a single line too, starting with Nothing.",
    state,
    steps: openHandoff(hub.title),
  },
  {
    id: "dialog-handoff-checking",
    group: GROUP,
    category: "normal",
    title: "Hand off while the branches are being checked",
    description: "Checking which branches it is responsible for… stands among the lines already known, the sessions and the place under Hub. The branch line joins them once the inspection answers.",
    state,
    sessionBranches: "pending",
    steps: openHandoff(coordinator.title),
  },
  {
    id: "dialog-handoff-branches-failed",
    group: GROUP,
    category: "exceptional",
    title: "Hand off when the branches could not be checked",
    description: "A small danger mark sits right after the \"Moves to the new session\" heading; there is no Alert under the list. The sessions and the place under Hub are listed as usual, and the failure is not read as none, so there is no Nothing line.",
    state,
    sessionBranches: { code: "internal_error", params: { detail: "git could not be run" }, message: "git could not be run" },
    steps: openHandoff(coordinator.title),
  },
  {
    id: "dialog-handoff-branches-failed-details",
    group: GROUP,
    category: "exceptional",
    title: "Hand off when the branches could not be checked, mark opened",
    description: "The danger mark after the heading is hovered, and its popover says \"Could not check which branches it is responsible for.\", then the application's generic sentence about it, the daemon's raw message under that, and Try again.",
    state,
    sessionBranches: { code: "internal_error", params: { detail: "git could not be run" }, message: "git could not be run" },
    steps: [...openHandoff(coordinator.title), (ui) => ui.hoverStatusMark(ui.t("dialog.handoff.movesBranchesFailed.mark"))],
  },
  {
    id: "dialog-handoff-branches-no-git",
    group: GROUP,
    category: "exceptional",
    title: "Hand off when the branches could not be checked, Git missing, mark opened",
    description: "The danger mark after the heading is hovered, and its popover says \"Could not check which branches it is responsible for.\", then the sentence that Git is not installed or not on PATH and the remedy to install it where sessions run and try again, with no raw message, and Download Git (external-link icon) then Try again.",
    state,
    sessionBranches: { code: "binary_not_found", params: { binary: "git" }, message: "git was not found on the login shell's PATH" },
    steps: [...openHandoff(coordinator.title), (ui) => ui.hoverStatusMark(ui.t("dialog.handoff.movesBranchesFailed.mark"))],
  },
  {
    id: "dialog-handoff-no-agent",
    group: GROUP,
    category: "exceptional",
    title: "Hand off dialog open as the other agents go missing",
    description: "Reached only when the agents change under an open dialog (the menu's Hand off is disabled without another agent): Codex and Grok Build stop being installed meanwhile. The dialog itself turns into a failure modal: a danger icon in the header, the title Cannot hand off the session, a sentence saying no other agent can take the session over, and OK as its only footer button, with a close button in the header that does what OK does (Escape and a click outside do nothing). No second dialog opens over it.",
    state,
    steps: [...openHandoff(bare.title), (ui) => ui.setAgents({ installed: ["claude"] })],
  },
  {
    id: "dialog-unbind-session",
    group: GROUP,
    category: "normal",
    title: "Unbind a session from its live owner",
    description: "A warning confirmation titled Unbind session, with the warning icon leading its header, then Cancel and the primary Unbind. Both titles here are paragraph-long, and each appears once, as a chip that wraps inside the dialog: one paragraph that runs on from the first sentence into the next (the session will no longer report to the owner; it will answer only to you, until it binds itself to an owner or you bind it to one again), then a separate paragraph of consequences (the owner can no longer instruct it, archive it or give it to another owner, and archiving the owner will not archive it; the two can still send each other messages). All body text is in the foreground colour, none of it muted.",
    state: longTitleState,
    steps: openUnbind(longCoordinator.title),
  },
  {
    id: "dialog-unbind-session-changing",
    group: GROUP,
    category: "exceptional",
    title: "Unbind refused while the session is changing",
    description: "Unbind was confirmed and refused because the session is being started, handed off or otherwise changing. An error alert titled \"Could not unbind the session\", with OK alone, says the session is changing so its owner cannot change right now, then the line on waiting for its current turn or operation before retrying; it shows no session ID, raw daemon text, Report issue or Error details. The Unbind session confirmation lies fully hidden beneath it and shows again, still open, once OK is pressed. A refusal because the owner is changing, or because the session got another owner meanwhile, reads alike.",
    state,
    sessionReleaseError: {
      code: "session_changing",
      params: { session: coordinator.id },
      message: `\`${coordinator.id}\` is being started, resumed, handed off, switched to another account, having its branches transferred or having its owner changed, so its owner cannot change now: look again once that is done`,
    },
    steps: [...openUnbind(coordinator.title), (ui) => ui.confirm()],
  },
  {
    id: "sidebar-session-open-in-app-failed",
    group: GROUP,
    category: "exceptional",
    title: "Open in the agent's app refused",
    description: "Open in Claude app was chosen from the menu of a stopped session and confirmed (the daemon archives it and opens the agent's app), but the daemon could not open the app. An error alert titled \"Could not open the agent's app\", with OK alone, names the session and its project, says that the app may not be installed, may not have started, or has no conversation to open, then, on the line after it, to see the details for the cause and try again; the daemon's own reason (here that the application is not installed) is only under Error details, open here.",
    state: stoppedState,
    nativeApps: ["claude"],
    sessionAppOpenError: {
      code: "native_app_unavailable",
      params: { detail: "the Claude application is not installed" },
      message: "The application could not be opened: the Claude application is not installed",
    },
    steps: [openMenu(stopped.title), (ui) => ui.press(ui.t("provider.claude.openInApp")), (ui) => ui.confirm(), (ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "dialog-open-session-in-app",
    group: GROUP,
    category: "normal",
    title: "Open an idle session with a bound session and background processes in the agent's app",
    description: "A warning confirmation titled Open in Claude app, with the session named in the first sentence, which says it will be archived and then opened in the agent's app, and the message that its branches and worktrees are kept and that it should be reopened from the archive once the app is done with the conversation. Under it a section headed \"Archived with it (1)\" lists the worker bound to the session, then a Background processes section under a divider, as the archive confirmation has them: the line \"Selected processes will be stopped. Uncheck any to keep them running.\" leads two ticked checkboxes labelled by their commands, and \"2 processes will be stopped.\" follows them. The confirming button reads Open in Claude app.",
    state: restingState,
    nativeApps: ["claude", "codex"],
    processes: { trees: restingTrees },
    steps: [openMenu(resting.title), (ui) => ui.press(ui.t("provider.claude.openInApp"))],
  },
  {
    id: "dialog-handoff-pending",
    group: GROUP,
    category: "normal",
    title: "Hand off request running",
    description: "Hand off shows its pending spinner ahead of the label while the daemon works, and the dialog stays open: its Cancel and close button are disabled, and Escape and a click outside do nothing.",
    state,
    sessionHandoffs: { [hub.id]: "pending" },
    steps: confirmHandoff(hub.title),
  },
  {
    id: "dialog-handoff-session-working",
    group: GROUP,
    category: "exceptional",
    title: "Hand off refused, the session is mid-turn",
    description: "The daemon found the session in the middle of a turn after the window listed it as idle. The alert says the session is still working or waiting for the user and to try again once it is waiting for instructions, and the dialog stays behind it for another try. A session archived meanwhile is refused in the same alert.",
    state,
    sessionHandoffs: {
      [bare.id]: { code: "session_working", params: { session: bare.id }, message: "The session is in the middle of a turn" },
    },
    steps: confirmHandoff(bare.title),
  },
  {
    id: "dialog-handoff-archive-failed",
    group: GROUP,
    category: "exceptional",
    title: "Hand off done, archiving the original failed on the application's data",
    description: "The handoff counts as done: the dialog has closed and the new session (the receiving one) is selected in the sidebar and open in the main area, with the original still listed. Over it, an error alert titled \"Could not finish the handoff\": the work was handed off to the new session (a chip), but archiving the original failed, and the cause the sentence ends with is \"Octoboard could not read or write its own data.\" Then the remedy for the application's data folder (writable, its disk available, access granted in system privacy settings) with no step to retry, and a line saying the original session is still open and is to be archived by the person once this is resolved. SQLite's text is only in the Error details disclosure, open here.",
    state: receivedState,
    sessionHandoffs: {
      [bare.id]: {
        code: "handoff_archive_failed",
        params: {
          receiving_session: received.id,
          reason: "Octoboard could not read or write its own data: attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY",
          reason_code: "store_write_failed",
          cause: "access",
          detail: "attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY",
        },
        message: `the work was handed off to ${received.id}, but archiving the original session failed: Octoboard could not read or write its own data: attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY`,
      },
    },
    steps: [...confirmHandoff(bare.title), (ui) => ui.press(ui.t("error.diagnostics"))],
  },
];
