import type { Agent, AgentAvailability } from "../../protocol";
import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { ALL_AGENTS, SAMPLE, branchItemOf, minutesAgo, sessionOf, snapshotState, worktreeItemOf } from "./builders";
import { DEFAULT_GIT_WORKFLOW } from "../../gitWorkflow";

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
const hubOwned = [
  sibling,
  sessionOf("s-handoff-hub-old", console_.id, api.id, "Earlier release check", "archived", { bound_to: hub.id, started_at: minutesAgo(180) }),
];

// Codex has two named accounts beside its default one, which the dialog's joined picker offers.
const settings = {
  auto_pull_repositories: false,
  auto_delete_unresumable_archives: true,
  added_agents: ALL_AGENTS, default_clone_dir: "/Users/dev/Projects", git_workflow: DEFAULT_GIT_WORKFLOW,
  accounts: [
    { id: "a-codex-work", agent: "codex" as const, name: "Work account", config_dir: "/Users/dev/.codex-work" },
    { id: "a-codex-personal", agent: "codex" as const, name: "Personal account", config_dir: "/Users/dev/.codex-personal" },
  ],
};

const state = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [hub, lead, ...owned, ...hubOwned, bare, working],
  settings,
});

// Detection has settled and finds no agent but Claude Code on the machine.
const onlyClaude = new Map<Agent, AgentAvailability>(
  (["claude", "codex", "grok"] as const).map((agent) => [
    agent,
    { agent, availability: agent === "claude" ? "available" : "unavailable", default_account_dir: `/Users/dev/.${agent}` },
  ]),
);

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
    description: "Three dividers separate Lock and Pin, Rename and Copy session ID, account and binding actions, and Archive, with no group headings. Copy session ID stays directly after Rename, with the ID in smaller muted monospace text beneath its label inside the same clickable item. Hand off is enabled and Unbind follows it directly, before the divider and Archive; Unbind is offered because Hub owns the session. Unbind opens a confirmation first (see the Unbind session dialog); the session keeps its binding badge, the dot in Hub's colour, until that is confirmed.",
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
    id: "sidebar-session-menu-hand-off-disabled",
    group: GROUP,
    category: "normal",
    title: "Menu of a bound session in the middle of a turn, Hand off disabled",
    description: "Hand off is greyed out while the session is in the middle of a turn, while Unbind right after it and Archive stay available.",
    state,
    steps: [openMenu(working.title)],
  },
  {
    id: "sidebar-session-menu-hand-off-no-agent",
    group: GROUP,
    category: "normal",
    title: "Menu of an idle session, no other agent to hand off to",
    description: "Codex and Grok Build are not installed, so no other agent could take the session over: Hand off is greyed out although the session is idle, and Archive stays available.",
    state: { ...state, agentAvailability: onlyClaude },
    steps: [openMenu(bare.title)],
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
    description: "A console session is responsible for no branches and reports to no one, so the only line is the three sessions it owns, an archived one included, and nothing says it is checking branches.",
    state,
    steps: openHandoff(hub.title),
  },
  {
    id: "dialog-hand-off-nothing-moves",
    group: GROUP,
    category: "normal",
    title: "Hand off a session that moves nothing",
    description: "Under Moves to the new session the one line starts with Nothing and says why.",
    state,
    steps: openHandoff(bare.title),
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
    description: "Under \"Moves to the new session\", a danger Alert titled \"Could not check which branches it is responsible for\" says under it that something unexpected went wrong. Details, which holds the raw message, stands on its right with Try again after it at the trailing edge, beside the sessions and the place under Hub; the failure is not read as none, so there is no Nothing line.",
    state,
    sessionBranches: { code: "internal_error", params: { detail: "git could not be run" }, message: "git could not be run" },
    steps: openHandoff(lead.title),
  },
  {
    id: "dialog-hand-off-no-agent",
    group: GROUP,
    category: "exceptional",
    title: "Hand off dialog open as the other agents go missing",
    description: "Reached only when the agents change under an open dialog (the menu's Hand off is disabled without another agent): Codex and Grok Build stop being installed meanwhile. The dialog closes itself and a plain error alert says no other agent can take the session over; OK is its only button.",
    state,
    steps: [...openHandoff(bare.title), (ui) => ui.setAgents({ installed: ["claude"] })],
  },
  {
    id: "dialog-unbind-session",
    group: GROUP,
    category: "normal",
    title: "Unbind a session from its live owner",
    description: "A plain confirmation titled Unbind session, not a danger one, with Cancel then the primary Unbind. The body names the session and its owner, Hub, each as a chip, and says what Hub can no longer do with the session (send it messages, archive it, give it to another owner), that archiving Hub will not archive it, that it will answer only to you, and that only the session itself can bind it to an owner again.",
    state,
    steps: openUnbind(lead.title),
  },
  {
    id: "dialog-unbind-session-changing",
    group: GROUP,
    category: "exceptional",
    title: "Unbind refused while the session is changing",
    description: "Unbind was confirmed and refused because the session is being started, handed off or otherwise changing. An error alert titled \"Could not unbind the session\", with OK alone, says the session is changing so its owner cannot change right now, then the line on waiting for its current turn or operation before retrying; it shows no session ID, raw daemon text, Report issue or Error details. The Unbind session confirmation lies fully hidden beneath it and shows again, still open, once OK is pressed.",
    state,
    sessionReleaseError: {
      code: "session_changing",
      params: { session: lead.id },
      message: `\`${lead.id}\` is being started, resumed, handed off, switched to another account, having its branches handed over or having its owner changed, so its owner cannot change now: look again once that is done`,
    },
    steps: [...openUnbind(lead.title), (ui) => ui.confirm()],
  },
  {
    id: "dialog-unbind-session-owner-changing",
    group: GROUP,
    category: "exceptional",
    title: "Unbind refused while the owner is changing",
    description: "Unbind was confirmed and refused because Hub, the owner, is being started, handed off or otherwise changing. An error alert titled \"Could not unbind the session\", with OK alone, opens with Hub as a chip at the start of its sentence, which says sessions cannot be bound to or moved from it right now, then the line telling you to wait for the owner's current start, resume, handoff, account switch or change of owner to finish; it shows no ID, raw daemon text, Report issue or Error details. The Unbind session confirmation lies fully hidden beneath it and shows again, still open, once OK is pressed.",
    state,
    sessionReleaseError: {
      code: "session_owner_changing",
      params: { session: lead.id, owner: hub.id },
      message: `\`${hub.id}\` is being started, resumed, handed off, switched to another account or having its owner changed, so sessions cannot be bound to or moved from it now: look again once that is done`,
    },
    steps: [...openUnbind(lead.title), (ui) => ui.confirm()],
  },
  {
    id: "dialog-unbind-session-binding-changed",
    group: GROUP,
    category: "exceptional",
    title: "Unbind refused, the session got another owner meanwhile",
    description: "Unbind was confirmed on Hub, but the session had been given to another owner (Update the dependencies) before the release landed. An error alert titled \"Could not unbind the session\", with OK alone, says the session's owner changed while it was being released, then the line saying the confirmation now names the session's current owner and to unbind again if you still want to; it shows no ID, raw daemon text, Report issue or Error details. Once OK is pressed the Unbind session confirmation is still open and now names Update the dependencies, not Hub, as the owner.",
    state,
    sessionReleaseError: {
      code: "session_binding_changed",
      params: { session: lead.id },
      message: `the binding changed while it was being released — \`${lead.id}\` was bound elsewhere; look again before retrying`,
    },
    sessionReleaseMovedTo: sibling.id,
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
    description: "The daemon found the session in the middle of a turn after the window listed it as waiting. The alert says the session cannot be handed off yet, and the dialog stays behind it for another try.",
    state,
    sessionHandoffs: {
      [bare.id]: { code: "session_working", params: { session: bare.id }, message: "The session is in the middle of a turn" },
    },
    steps: confirmHandoff(bare.title),
  },
  {
    id: "dialog-hand-off-session-archived",
    group: GROUP,
    category: "exceptional",
    title: "Hand off refused, the session was archived",
    description: "The daemon archived the session after the window listed it. The alert says an archived session can be neither switched to another account nor handed off, and to reopen it first.",
    state,
    sessionHandoffs: {
      [bare.id]: { code: "session_archived", params: { session: bare.id }, message: "An archived session cannot be handed off" },
    },
    steps: confirmHandoff(bare.title),
  },
  {
    id: "toast-hand-off-messages-dropped",
    group: "Trust prompts and toasts",
    category: "exceptional",
    title: "Messages dropped during a handoff",
    description: "The notice arrives as an error alert titled \"Handoff messages dropped\", whose body opens with the session line, \"Session: Rework the result ranking — Website\", with the session's title and its project each a chip, then, as plain text rather than a chip, what was dropped, followed by the recovery of checking the session they were meant for or the one that took over.",
    state,
    toasts: [{ kind: "error", code: "handoff_messages_dropped", params: {}, message: "", session: lead.id }],
  },
  {
    id: "toast-sharing-messages-dropped",
    group: "Trust prompts and toasts",
    category: "exceptional",
    title: "Messages dropped during a sharing change",
    description: "The notice arrives as an error alert titled \"Messages dropped during a sharing change\", whose body opens with the session line, \"Session: Rework the result ranking — Website\", with the session's title and its project each a chip, then, as plain text rather than a chip, what was dropped, saying the session was not restarted, followed by its own recovery line: the messages were never written to a terminal and the session may not be running, so resume it if still needed and send them again.",
    state,
    toasts: [{ kind: "error", code: "sharing_messages_dropped", params: {}, message: "", session: lead.id }],
  },
];
