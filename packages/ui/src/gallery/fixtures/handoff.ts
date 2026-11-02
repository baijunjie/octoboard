import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import type { Ui } from "../../fixtures/interact";
import type { Scenario } from "../scenario";
import { accountOf, ALL_AGENTS, SAMPLE, branchItemOf, minutesAgo, sessionOf, snapshotState, worktreeItemOf } from "../../fixtures/builders";

const { console: console_, web, api, consoleSession: hub } = SAMPLE;

// A project session that sits under the hub and takes the whole place along: it owns three sessions,
// one of them archived, and is responsible for two branches, one of them checked out in a worktree.
const lead = sessionOf("s-handoff-lead", console_.id, web.id, "Rework the result ranking", "idle", { bound_to: hub.id, started_at: minutesAgo(50) });
const owned = [
  sessionOf("s-handoff-owned-1", console_.id, web.id, "Tidy the changelog", "waiting_user", { bound_to: lead.id, started_at: minutesAgo(40) }),
  sessionOf("s-handoff-owned-2", console_.id, api.id, "Update the search fixtures", "idle", { bound_to: lead.id, started_at: minutesAgo(35) }),
  sessionOf("s-handoff-owned-old", console_.id, web.id, "Earlier ranking experiment", "archived", { bound_to: lead.id, started_at: minutesAgo(120) }),
];
// A project session bound to no one that owns none and is responsible for no branch: nothing moves.
const bare = sessionOf("s-handoff-bare", console_.id, api.id, "Add idempotency keys", "waiting_user", { started_at: minutesAgo(30) });
// What the hub owns besides the lead: a live session and an archived one.
const sibling = sessionOf("s-handoff-sibling", console_.id, web.id, "Update the dependencies", "idle", { bound_to: hub.id, started_at: minutesAgo(45) });
// A bound session the menu cannot hand off now, though it can still be unbound.
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
  added_agents: ALL_AGENTS,
  default_clone_dir: "/Users/dev/Projects",
  accounts: [
    accountOf("a-codex-work", "codex", "Work account", "/Users/dev/.codex-work"),
    accountOf("a-codex-personal", "codex", "Personal account", "/Users/dev/.codex-personal"),
  ],
};

const state = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [hub, lead, ...owned, ...hubOwned, bare, working],
  settings,
});

// The session a handoff of `bare` created before archiving the original failed, which the failure names.
const received = sessionOf("s-handoff-received", console_.id, api.id, "Add idempotency keys", "idle", { agent: "codex", started_at: minutesAgo(1) });
const receivedState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [hub, lead, ...owned, ...hubOwned, bare, working, received],
  settings,
});

// The same sessions plus an idle one the user locked, which Hand off and Archive refuse; kept out of `state` so
// the lock mark does not appear in the other scenarios.
const locked = sessionOf("s-handoff-locked", console_.id, api.id, "Review the rate limits", "idle", { locked: true, started_at: minutesAgo(25) });
const lockedState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [hub, lead, ...owned, ...hubOwned, bare, working, locked],
  settings,
});

// The same sessions plus the one still starting, kept out of `state` so it does not appear in the other scenarios.
const startingState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [hub, lead, ...owned, ...hubOwned, bare, working, starting],
  settings,
});

// A bound session and its owner whose titles run to paragraphs, as a pasted brief does: the unbind confirmation
// names each of them once, and the chips have to wrap inside the dialog.
const longLead = sessionOf(
  "s-unbind-long-lead",
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
const longTitleState = snapshotState({ consoles: [console_], projects: [web, api], sessions: [longHub, longLead], settings });

const GROUP = "Sidebar sessions";

const openMenu = (title: string) => (ui: Ui) => ui.press(ui.t("sidebar.session.actions", { title }));
const openHandoff = (title: string) => [
  openMenu(title),
  (ui: Ui) => ui.press(ui.t("sidebar.session.handOff")),
];
const openUnbind = (title: string) => [
  openMenu(title),
  (ui: Ui) => ui.press(ui.t("sidebar.session.unbind")),
];
// The dialog's confirming button, pressed once the dialog is up.
const confirmHandoff = (title: string) => [...openHandoff(title), (ui: Ui) => ui.confirm()];

const leadBranches = [
  branchItemOf(web, lead, "feat/ranking-rework"),
  worktreeItemOf(web, lead, "feat/ranking-weights", "/Users/dev/worktrees/website-ranking-weights"),
];

export const handoffScenarios: Scenario[] = [
  {
    id: "sidebar-session-menu-hand-off",
    group: GROUP,
    category: "normal",
    title: "Menu of an idle project session, Hand off offered",
    description: "Three dividers separate Lock and Pin, Rename and Copy session ID, account and binding actions, and Archive, with no group headings. Copy session ID stays directly after Rename, with the ID in smaller muted monospace text beneath its label inside the same clickable item. Change model, with a chip icon, is enabled and leads that group, directly before Hand off. Hand off, which is enabled, has Unbind directly after it, before the divider and Archive; Unbind is offered because Hub owns the session. Unbind opens a confirmation first (see the Unbind session dialog); the session keeps its binding badge, the dot in Hub's colour, until that is confirmed.",
    state,
    steps: [openMenu(lead.title)],
  },
  {
    id: "sidebar-session-menu-unbound",
    group: GROUP,
    category: "normal",
    title: "Menu of an unbound project session, no Unbind",
    description: "Hand off is enabled and is followed directly by the divider and Archive: with no owner to be released from, there is no Unbind entry.",
    state,
    steps: [openMenu(bare.title)],
  },
  {
    id: "sidebar-console-session-menu-hand-off",
    group: GROUP,
    category: "normal",
    title: "Menu of an idle console session, Hand off offered",
    description: "Three dividers group Lock, Pin and Focus mode first; Rename, Copy session ID and Colour second; Hand off third; and Archive last, with no group headings. Hand off is enabled. A console session is never bound, so there is no Unbind.",
    state,
    steps: [openMenu(hub.title)],
  },
  {
    id: "sidebar-session-menu-starting",
    group: GROUP,
    category: "normal",
    title: "Menu of a bound session that is still starting, Hand off and Unbind disabled",
    description: "Change model, Hand off and Unbind are all greyed out with no reason under their names, since the session is only briefly unavailable, where a session in the middle of a turn would say \"Session busy\" under Hand off; Archive stays available.",
    state: startingState,
    steps: [openMenu(starting.title)],
  },
  {
    id: "sidebar-session-menu-hand-off-locked",
    group: GROUP,
    category: "normal",
    title: "Menu of a locked session, Hand off disabled",
    description: "Hand off and Archive are both greyed out with \"Session locked\" under their names although the session is idle; Archive shows the reason in place of its keyboard shortcut. Unlock stays available. A session in the middle of a turn (\"Session busy\") or with no other agent to hand off to (\"No other agent\") greys Hand off the same way, with its own reason.",
    state: lockedState,
    steps: [openMenu(locked.title)],
  },
  {
    id: "dialog-hand-off",
    group: GROUP,
    category: "normal",
    title: "Hand off a project session with everything that moves",
    description: "All three lines of Moves to the new session: two branches, three sessions (archived ones counted) and the place under Hub. The Hand off to field opens on Codex, the first other agent, and offers its three accounts in one picker beside the agent. Model and effort follow the project.",
    state,
    sessionBranches: leadBranches,
    steps: openHandoff(lead.title),
  },
  {
    id: "dialog-hand-off-console-session",
    group: GROUP,
    category: "normal",
    title: "Hand off a console session",
    description: "A console session is responsible for no branches and reports to no one, so the only line is the three sessions it owns, an archived one included, and nothing says it is checking branches. A session that moves nothing has a single line too, starting with Nothing.",
    state,
    steps: openHandoff(hub.title),
  },
  {
    id: "dialog-hand-off-checking",
    group: GROUP,
    category: "normal",
    title: "Hand off while the branches are being checked",
    description: "Checking which branches it is responsible for… stands among the lines already known, the sessions and the place under Hub. The branch line joins them once the inspection answers.",
    state,
    sessionBranches: "pending",
    steps: openHandoff(lead.title),
  },
  {
    id: "dialog-hand-off-branches-failed",
    group: GROUP,
    category: "exceptional",
    title: "Hand off when the branches could not be checked",
    description: "A small danger mark sits right after the \"Moves to the new session\" heading; there is no Alert under the list. The sessions and the place under Hub are listed as usual, and the failure is not read as none, so there is no Nothing line.",
    state,
    sessionBranches: { code: "internal_error", params: { detail: "git could not be run" }, message: "git could not be run" },
    steps: openHandoff(lead.title),
  },
  {
    id: "dialog-hand-off-branches-failed-details",
    group: GROUP,
    category: "exceptional",
    title: "Hand off when the branches could not be checked, mark opened",
    description: "The danger mark after the heading is hovered, and its popover says \"Could not check which branches it is responsible for.\", then the application's generic sentence about it, the daemon's raw message under that, and Try again.",
    state,
    sessionBranches: { code: "internal_error", params: { detail: "git could not be run" }, message: "git could not be run" },
    steps: [...openHandoff(lead.title), (ui) => ui.hoverStatusMark(ui.t("dialog.handOff.movesBranchesFailed.mark"))],
  },
  {
    id: "dialog-hand-off-no-agent",
    group: GROUP,
    category: "exceptional",
    title: "Hand off dialog open as the other agents go missing",
    description: "Reached only when the agents change under an open dialog (the menu's Hand off is disabled without another agent): Codex and Grok Build stop being installed meanwhile. The dialog itself turns into a failure modal: a danger icon in the header, the title Cannot hand off the session, a sentence saying no other agent can take the session over, and OK as its only button. No second dialog opens over it.",
    state,
    steps: [...openHandoff(bare.title), (ui) => ui.setAgents({ installed: ["claude"] })],
  },
  {
    id: "dialog-unbind-session",
    group: GROUP,
    category: "normal",
    title: "Unbind a session from its live owner",
    description: "A warning confirmation titled Unbind session, with the warning icon leading its header, then Cancel and the primary Unbind. Both titles here are paragraph-long, and each appears once, as a chip that wraps inside the dialog: one paragraph that runs on from the first sentence into the next (the session will no longer report to the owner; it will answer only to you; only the session itself can bind to an owner again, the app cannot do it for you), then a separate paragraph of consequences (the owner can no longer instruct it, archive it or give it to another owner, and archiving the owner will not archive it; the two can still send each other messages). All body text is in the foreground colour, none of it muted.",
    state: longTitleState,
    steps: openUnbind(longLead.title),
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
      params: { session: lead.id },
      message: `\`${lead.id}\` is being started, resumed, handed off, switched to another account, having its branches handed over or having its owner changed, so its owner cannot change now: look again once that is done`,
    },
    steps: [...openUnbind(lead.title), (ui) => ui.confirm()],
  },
  {
    id: "dialog-hand-off-pending",
    group: GROUP,
    category: "normal",
    title: "Hand off request running",
    description: "Hand off shows its busy state while the daemon works, and the dialog stays open.",
    state,
    sessionHandoffs: { [hub.id]: "pending" },
    steps: confirmHandoff(hub.title),
  },
  {
    id: "dialog-hand-off-session-working",
    group: GROUP,
    category: "exceptional",
    title: "Hand off refused, the session is mid-turn",
    description: "The daemon found the session in the middle of a turn after the window listed it as waiting. The alert says the session cannot be handed off yet, and the dialog stays behind it for another try. A session archived meanwhile is refused in the same alert.",
    state,
    sessionHandoffs: {
      [bare.id]: { code: "session_working", params: { session: bare.id }, message: "The session is in the middle of a turn" },
    },
    steps: confirmHandoff(bare.title),
  },
  {
    id: "dialog-hand-off-archive-failed",
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
