import type { Account, AccountQuota, Agent, AgentAvailability, Console, Project, Settings } from "../../protocol";
import type { Scenario } from "../scenario";
import type { Ui } from "../interact";
import { ALL_AGENTS, SAMPLE, accountSharingOf, minutesAgo, minutesFromNow, projectOf, sessionOf, snapshotState, switchingAccount } from "./builders";
import { quotaOf, quotaWindow } from "./quotas";

const GROUP = "Sidebar sessions";
const FOCUS_GROUP = "Focus mode";

const { console: console_, web, consoleSession: hub } = SAMPLE;

/** Claude Code's accounts: Work (spent), Personal (nearly), Client (no reading) beside the default account, which has
 * plenty left. Codex has a second account, Team; Grok Build only its default one. */
const accounts: Account[] = [
  { id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work" },
  { id: "a-personal", agent: "claude", name: "Personal", config_dir: "/Users/dev/.claude-personal" },
  { id: "a-client", agent: "claude", name: "Client", config_dir: "/Users/dev/.claude-client" },
  { id: "a-team", agent: "codex", name: "Team", config_dir: "/Users/dev/.codex-team" },
];

const settings: Settings = {
  auto_pull_repositories: false,
  auto_delete_unresumable_archives: true,
  added_agents: ALL_AGENTS,
  default_clone_dir: "/Users/dev/Projects",
  accounts,
};

const available = new Map<Agent, AgentAvailability>(
  ALL_AGENTS.map((agent) => [agent, { agent, availability: "available", default_account_dir: `/Users/dev/.${agent}` }]),
);

/** Claude Code's readings: the default account with plenty left in both windows, Work's five-hour window spent (it resets
 * in two hours), Personal with 4% and 12% left, and none for Client, which has not been read yet. */
const claudeQuotas: AccountQuota[] = [
  quotaOf("claude", null, "max", [quotaWindow("session", 22, 190), quotaWindow("weekly", 41, 3 * 24 * 60 + 4 * 60)]),
  quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 125), quotaWindow("weekly", 63, 2 * 24 * 60 + 5 * 60)]),
  quotaOf("claude", "a-personal", "pro", [quotaWindow("session", 96, 40), quotaWindow("weekly", 88, 24 * 60 + 90)]),
];

/** Claude Code's readings as `claudeQuotas` has them, but Work's five-hour window, spent when it was read eight minutes
 * ago, reset a minute ago and no newer reading has arrived. */
const workResetPassedQuotas: AccountQuota[] = claudeQuotas.map((quota) =>
  quota.account === "a-work"
    ? { ...quota, fetched_at: minutesAgo(8), windows: quota.windows.map((window) => window.kind === "session" ? quotaWindow("session", 100, -1) : window) }
    : quota);

/** Codex with both its accounts spent: the default one in its five-hour window, Team in its weekly one. */
const codexSpent: AccountQuota[] = [
  quotaOf("codex", null, "plus", [quotaWindow("session", 100, 95), quotaWindow("weekly", 52, 5 * 24 * 60)]),
  quotaOf("codex", "a-team", "pro", [quotaWindow("session", 30, 200), quotaWindow("weekly", 100, 3 * 24 * 60 + 60)]),
];

/** Grok Build's one credit pool, spent. */
const grokSpent: AccountQuota[] = [quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 100, 2 * 24 * 60)])];

/** Weekly windows only. Default still has most of its quota and resets in six days; Soon has little
 * left and resets in six hours, so a new session opens on Soon rather than the inherited default. */
const soonerWeeklyQuotas: AccountQuota[] = [
  quotaOf("claude", null, "max", [quotaWindow("weekly", 10, 6 * 24 * 60)]),
  quotaOf("claude", "a-soon", "pro", [quotaWindow("weekly", 90, 6 * 60)]),
];

/** Codex's weekly windows, with the default account used up. Team, listed second, resets in five days; Sprint, listed
 * third, resets in two, so a handoff opens on Sprint rather than the default account (used up, though its weekly
 * window would reset first) or Team, the first account with quota left. */
const handoffWeeklyQuotas: AccountQuota[] = [
  quotaOf("codex", null, "plus", [quotaWindow("weekly", 100, 20 * 60)]),
  quotaOf("codex", "a-team", "pro", [quotaWindow("weekly", 44, 5 * 24 * 60)]),
  quotaOf("codex", "a-sprint", "pro", [quotaWindow("weekly", 81, 2 * 24 * 60)]),
];

const sprintAccount: Account = { id: "a-sprint", agent: "codex", name: "Sprint", config_dir: "/Users/dev/.codex-sprint" };

const soonAccount: Account = { id: "a-soon", agent: "claude", name: "Soon", config_dir: "/Users/dev/.claude-soon" };

const website = { ...web, default_agent: "claude" as const };
const searchApi = projectOf("p-api", console_.id, "Search API", { default_agent: "codex" });
const chat = projectOf("p-chat", console_.id, "Chat Bot", { default_agent: "grok" });
/** A project whose Claude Code sessions name Work, the spent account, to run under. */
/** A project whose Claude Code sessions name Client to run under. */
const onClient = { ...website, claude_account_id: { kind: "account" as const, id: "a-client" } };
const onWork = { ...website, claude_account_id: { kind: "account" as const, id: "a-work" } };

/** The console whose sessions run on Codex, for the buttons that create one. */
const codexConsole: Console = { ...console_, default_agent: "codex" };

/** A window with `projects` and the readings `accountQuotas`, whose one console is `owner`: Hub, and an idle session of
 * each project's default agent beside it. */
function launchState(projects: Project[], accountQuotas: AccountQuota[], owner: Console = console_) {
  return {
    ...snapshotState({
      consoles: [owner],
      projects,
      sessions: [hub, ...projects.map((project) => sessionOf(`s-${project.id}`, console_.id, project.id, `${project.name} work`, "idle", { agent: project.default_agent ?? "claude" }))],
      settings,
      accountQuotas,
    }),
    agentAvailability: available,
  } satisfies Scenario["state"];
}

const openSessionDialog = (ui: Ui) => ui.press(ui.t("sidebar.project.openSession"));
const accountField = (ui: Ui, agent: string) => ui.t("dialog.agentSettings.account", { agent });

/** A Claude Code session that can be switched, with its sibling, on the default account. */
const switchState = (accountQuotas: AccountQuota[], onAccount?: Account) => ({
  ...snapshotState({
    consoles: [console_],
    projects: [website],
    sessions: [
      sessionOf("s-switch-hub", console_.id, undefined, "Release coordinator", "idle"),
      sessionOf("s-switch-work", console_.id, website.id, "Review the release checklist", "idle", onAccount && { account_id: onAccount.id, config_dir: onAccount.config_dir }),
      sessionOf("s-switch-bound", console_.id, website.id, "Verify the migration", "waiting_user", { bound_to: "s-switch-hub" }),
    ],
    settings,
    accountQuotas,
  }),
  agentAvailability: available,
}) satisfies Scenario["state"];
const openSwitchTo = (account: string) => [
  (ui: Ui) => ui.press(ui.t("sidebar.session.actions", { title: "Review the release checklist" })),
  (ui: Ui) => ui.press(ui.t("sidebar.session.switchAccount")),
  (ui: Ui) => ui.press(account),
];

/** Client is turning sharing of the default configuration on, 2 entries of 9 done: while that runs the daemon refuses a
 * session on it, whatever its quota. */
const clientChangingSharing = accountSharingOf([{ account: "a-client", progress: { share: true, phase: "changing", done: 2, total: 9, item: "skills/" } }]);

/** Client's last sharing change did not finish and none runs now, so starting a session on it asks first. */
const clientIncomplete: Settings = {
  ...settings,
  accounts: accounts.map((account) => (account.id === "a-client" ? { ...account, share_default_config_incomplete: true } : account)),
};

/** Team's, Codex's second account, instead: the account whose sharing switch Settings is asked to show, on a tab other
 * than the first. */
const teamIncomplete: Settings = {
  ...settings,
  accounts: accounts.map((account) => (account.id === "a-team" ? { ...account, share_default_config_incomplete: true } : account)),
};
const onTeam = { ...searchApi, codex_account_id: { kind: "account" as const, id: "a-team" } };

/** When the daemon, which holds a newer reading than the window, refuses: the five-hour window resets in 95 minutes. */
const refusedWithReset = {
  code: "account_quota_exhausted",
  params: { agent: "Claude Code", account: "Work", window: "5-hour", resets_at: String(minutesFromNow(95)) },
  message: "Work has used up its Claude Code quota (5-hour)",
};

/** Hub's own Codex handoff: the default account is spent and Team is not. */
const handoffQuotas: AccountQuota[] = [
  codexSpent[0],
  quotaOf("codex", "a-team", "pro", [quotaWindow("session", 30, 200), quotaWindow("weekly", 44, 3 * 24 * 60 + 60)]),
];

export const quotaGatingScenarios: Scenario[] = [
  {
    id: "dialog-new-session-quota-picker",
    group: GROUP,
    category: "normal",
    title: "New session, account picker with quota bars",
    description: "Website would run on Work, whose quota is used up, so the Claude Code account field opens on Default, the first account with quota left, and the open picker lists every account with a compact usage bar before its name: the five-hour window's bar with the weekly window's thin one right under it. Default has plenty left and is selected; Work is unselectable, its name and bar faded and its reason at full contrast under the name on two lines, \"Used up\" and when it resets; Personal is nearly empty; Client has no reading yet and shows an empty track.",
    state: launchState([onWork], claudeQuotas),
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Claude Code"))],
  },
  {
    id: "dialog-new-session-sharing-picker",
    group: GROUP,
    category: "normal",
    title: "New session, account picker with a sharing change running",
    description: "Website would run on Client, whose sharing change is running, so the Claude Code account field opens on Default instead, with no alert. The open picker lists Default (selected), Work, Personal and Client, each with an empty usage track, none having a reading yet. Client is unselectable, its name and track faded and the short reason \"Changing sharing\" at full contrast under the name; the other accounts can be chosen.",
    state: { ...launchState([onClient], []), accountSharing: clientChangingSharing },
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Claude Code"))],
  },
  {
    id: "dialog-new-session-quota-reset-passed",
    group: GROUP,
    category: "normal",
    title: "New session, account whose spent window has since reset",
    description: "Website runs on Work, whose five-hour window was spent when it was read 8 minutes ago but reset a minute ago, before any newer reading. The Claude Code account field keeps Work selected, not used up, and the open picker lists Work's bar with the five-hour part full over the weekly window's 37%, its summary reading 5-hour 100% remaining; Personal is nearly empty and Client has no reading.",
    state: launchState([onWork], workResetPassedQuotas),
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Claude Code"))],
  },
  {
    id: "dialog-new-session-auto-select-account",
    group: GROUP,
    category: "normal",
    title: "New session, the account whose weekly quota resets sooner",
    description: "Website would run on Default, which has most of its weekly quota left and resets later, but the Claude Code account field opens on Soon, which has little weekly quota left and resets sooner. The open picker shows Soon selected beside Default's fuller bar.",
    state: {
      ...launchState([website], soonerWeeklyQuotas),
      settings: { ...settings, auto_select_account: true, accounts: [soonAccount] },
    },
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Claude Code"))],
  },
  {
    id: "dialog-hand-off-auto-select-account",
    group: GROUP,
    category: "normal",
    title: "Hand off, the account whose weekly quota resets sooner",
    description: "Hub hands off to Codex, whose default account is used up. The Codex account field opens on Sprint, whose weekly quota resets in two days, not on Team, the first account with quota left, which resets in five. The open picker lists the used-up default account unselectable, then Team, then Sprint selected.",
    state: {
      ...launchState([], handoffWeeklyQuotas),
      settings: { ...settings, auto_select_account: true, accounts: [...accounts.filter((account) => account.agent === "codex"), sprintAccount] },
    },
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: hub.title })),
      (ui) => ui.press(ui.t("sidebar.session.handOff")),
      (ui) => ui.press(accountField(ui, "Codex")),
    ],
  },
  {
    id: "dialog-new-session-quota-all-used-up",
    group: GROUP,
    category: "exceptional",
    title: "New session, every account of the agent is used up",
    description: "Both of Codex's accounts are used up (Default in its five-hour window, Team in its weekly one), so none can be preselected: Open is disabled and a stock danger alert at the top of the dialog names the unavailable account in its status-coloured title, with the quota reason and reset time in its muted description.",
    state: launchState([searchApi], codexSpent),
    steps: [openSessionDialog],
  },
  {
    id: "dialog-new-session-quota-single-account",
    group: GROUP,
    category: "exceptional",
    title: "New session, the agent's only account is used up",
    description: "Grok Build has only its default account, and its account picker is still there beside the agent select, offering that one option. Its credit pool is used up: the open picker lists Default unselectable, its name and empty bar faded and its reason at full contrast under the name on two lines, \"Used up\" and when it resets. Open is disabled and a stock danger alert at the top of the dialog names the unavailable account in its status-coloured title, with the quota reason and reset time in its muted description.",
    state: launchState([chat], grokSpent),
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Grok Build"))],
  },
  {
    id: "dialog-new-session-incomplete-launch",
    group: GROUP,
    category: "exceptional",
    title: "New session, the account's last sharing change did not finish",
    description: "Website runs on Client, whose last sharing change did not finish, so Open asks first: a confirmation titled \"Start on Client?\" opens over the session dialog, which stays behind it. It says the configuration and conversations may be incomplete and offers two buttons, View in Settings and, last, the primary Start anyway, which is not destructive and holds the focus; there is no Cancel, and the dialog's close button is the way to dismiss it.",
    state: { ...launchState([onClient], []), settings: clientIncomplete },
    steps: [openSessionDialog, (ui) => ui.confirm()],
  },
  {
    id: "dialog-switch-account-incomplete-launch",
    group: GROUP,
    category: "exceptional",
    title: "Switch account, the target's last sharing change did not finish",
    description: "Switching the session to Client, whose last sharing change did not finish: after Confirm in the switch dialog, the \"Start on Client?\" confirmation opens over it, which stays behind with its Confirm button pending until the question is answered. The question's buttons are View in Settings and the focused, primary Start anyway, with no Cancel; the dialog's close button dismisses it. Nothing has been switched yet.",
    state: { ...switchState([]), settings: clientIncomplete },
    steps: [...openSwitchTo("Client"), (ui) => ui.confirm()],
  },
  {
    id: "dialog-resume-incomplete-launch",
    group: GROUP,
    category: "exceptional",
    title: "Resume by selecting, the account's last sharing change did not finish",
    description: "Selecting Website's interrupted session, which ran on Client, whose last sharing change did not finish, would resume it, so the window asks first: a \"Start on Client?\" confirmation opens over the window, saying the configuration and conversations may be incomplete. Its buttons are View in Settings and the primary Start anyway, with no Cancel (the dialog's close button dismisses it); Start anyway holds the focus and keeps it while the session's terminal attaches. Nothing has been resumed yet.",
    state: {
      ...snapshotState({
        consoles: [console_],
        projects: [website],
        sessions: [
          sessionOf("s-resume-incomplete", console_.id, website.id, "Port the importer to the new parser", "interrupted", { account_id: "a-client", config_dir: "/Users/dev/.claude-client" }),
        ],
        settings: clientIncomplete,
      }),
      agentAvailability: available,
    },
    steps: [(ui) => ui.press(ui.session("Port the importer to the new parser"))],
  },
  {
    id: "settings-accounts-from-incomplete-launch",
    group: "Settings",
    category: "exceptional",
    title: "Settings opened from the question about an account's unfinished sharing change",
    description: "Search API runs on Team, a Codex account whose last sharing change did not finish. Open asked first, and View in Settings was pressed: the launch is cancelled, the question and the session dialog beneath it are gone, and Settings is open on Agent accounts with the Codex tab selected. Team's Share default configuration switch, shown on, its position before the change, is scrolled into view with the focus ring on it, and the warning that the last attempt to turn sharing off did not finish sits right below.",
    state: { ...launchState([onTeam], []), settings: teamIncomplete },
    steps: [openSessionDialog, (ui) => ui.confirm(), (ui) => ui.press(ui.t("dialog.incompleteLaunch.viewSettings"))],
  },
  {
    id: "dialog-new-session-quota-refused",
    group: GROUP,
    category: "exceptional",
    title: "Session launch refused, the account's quota is used up",
    description: "The daemon holds a newer reading than the window: Work's five-hour quota is used up. An error dialog opens over the session dialog, which stays behind it: it names the account and the agent, says the 5-hour quota is used up and in how long it resets, and advises another account or waiting.",
    state: launchState([onWork], []),
    sessionOpenError: refusedWithReset,
    steps: [openSessionDialog, (ui) => ui.confirm()],
  },
  {
    id: "sidebar-session-switch-account-quota-menu",
    group: GROUP,
    category: "normal",
    title: "Switch account menu, an account used up",
    description: "The row's Switch account submenu lists Default (current), Work, Personal and Client; each account shows its compact usage bar in place of the key icon, as in the account picker. Work, whose five-hour quota is used up, is greyed out and cannot be chosen, with its reason on two lines under its name, \"Used up\" and when it resets (arrow keys still reach it, and its accessible name carries the reason); Personal, nearly spent, and Client, not read yet (an empty track), can be chosen.",
    state: switchState(claudeQuotas),
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Review the release checklist" })),
      (ui) => ui.press(ui.t("sidebar.session.switchAccount")),
    ],
  },
  {
    id: "sidebar-session-switch-account-sharing-menu",
    group: GROUP,
    category: "normal",
    title: "Switch account menu, an account whose sharing change runs",
    description: "The row's Switch account submenu lists Default (current), Work, Personal and Client. Client, whose sharing change is running, is greyed out and cannot be chosen, with the short reason \"Changing sharing\" under its name (arrow keys still reach it; its accessible name stays \"Client\" and the reason is its description); the others can be chosen.",
    state: { ...switchState([]), accountSharing: clientChangingSharing },
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Review the release checklist" })),
      (ui) => ui.press(ui.t("sidebar.session.switchAccount")),
    ],
  },
  {
    id: "sidebar-session-switching-menu",
    group: GROUP,
    category: "normal",
    title: "Menu of a session switching its account",
    description: "Verify the migration, bound to Release coordinator, reads interrupted while the daemon holds it for its account switch. In its menu, Hand off and Unbind are greyed out, Hand off with \"Switching account\" under its name; in the Switch account submenu, Default stays marked as current and Work, Personal and Client are greyed out with the same reason under each name and cannot be chosen.",
    state: (() => {
      const base = switchState([]);
      const sessions = new Map(base.sessions);
      sessions.set("s-switch-bound", { ...sessions.get("s-switch-bound")!, status: "interrupted" });
      return switchingAccount({ ...base, sessions }, ["s-switch-bound"]);
    })(),
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Verify the migration" })),
      (ui) => ui.press(ui.t("sidebar.session.switchAccount")),
    ],
  },
  {
    id: "sidebar-session-switch-account-sharing-own-menu",
    group: GROUP,
    category: "exceptional",
    title: "Switch account menu, the session's own account is changing its sharing",
    description: "The row's session is on Client, whose sharing change is running, so the daemon would refuse to move it away or hand it off. Every other entry in the Switch account submenu, Default, Work and Personal, is greyed out with the short reason \"Account changing sharing\" under its name and cannot be chosen; Client stays marked as current. In the menu behind it, Hand off is disabled too, with the same reason under its name.",
    state: { ...switchState([], accounts[2]), accountSharing: clientChangingSharing },
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Review the release checklist" })),
      (ui) => ui.press(ui.t("sidebar.session.switchAccount")),
    ],
  },
  {
    id: "dialog-switch-account-quota-used-up",
    group: GROUP,
    category: "exceptional",
    title: "Switch account, the account turned out used up",
    description: "The menu offered Work, and its reading landed after: Confirm is disabled, and the late danger alert announces the refusal. Its stock status-coloured title says Work cannot be used for now; the muted description gives the quota reason and reset time.",
    state: switchState([]),
    steps: [...openSwitchTo("Work"), (ui) => ui.setAccountQuotas(claudeQuotas)],
  },
  {
    id: "dialog-hand-off-quota-picker",
    group: GROUP,
    category: "normal",
    title: "Hand off, the inherited Codex account is used up",
    description: "Hub hands off to Codex, whose default account is used up: the field opens on Team, the account with quota left, and the open picker lists the default account unselectable, its name and bar faded and its reason at full contrast under the name on two lines, \"Used up\" and when it resets.",
    state: launchState([], handoffQuotas),
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: hub.title })),
      (ui) => ui.press(ui.t("sidebar.session.handOff")),
      (ui) => ui.press(accountField(ui, "Codex")),
    ],
  },
  {
    id: "dialog-hand-off-quota-all-used-up",
    group: GROUP,
    category: "exceptional",
    title: "Hand off, the only Codex account is used up",
    description: "Hub hands off to Codex, which has only its default account, and that is used up: Hand off is disabled and a stock danger alert at the top of the dialog, above the lines about what moves, names the unavailable account in its status-coloured title, with the quota reason and reset time in its muted description. The account picker beside the agent select holds that one account, Default, with its two-layer usage bar, the five-hour bar empty over the part-filled weekly one, and the picker stays closed.",
    state: { ...launchState([], [codexSpent[0]]), settings: { ...settings, accounts: accounts.filter((account) => account.agent === "claude") } },
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: hub.title })),
      (ui) => ui.press(ui.t("sidebar.session.handOff")),
    ],
  },
  {
    id: "sidebar-quick-create-quota-used-up",
    group: GROUP,
    category: "exceptional",
    held: true,
    title: "Quick create, the default agent's accounts used up",
    description: "With the modifier held the new-session buttons are bolts. The console's sessions run on Codex, the default agent, and both Codex accounts are used up, so the bolt in the sidebar header looks disabled but can be focused and hovered: the pointer rests on it and its tooltip is the short reason \"Default agent's quota used up\", while its accessible name stays \"New console session with defaults\". Search API's project row has the same disabled bolt, whose tooltip shows the same reason on hover.",
    state: launchState([searchApi], codexSpent, codexConsole),
    // The "+" is still plain while the steps run: the modifier is pressed after them, and the open tooltip then reads the reason.
    steps: [(ui) => ui.hoverTitledControl(ui.t("sidebar.consoleSessions.new"))],
  },
  {
    id: "sidebar-quick-create-quota-refused",
    group: GROUP,
    category: "exceptional",
    title: "Quick create refused, the default agent's accounts used up",
    description: "The console's sessions run on Codex, the default agent, whose accounts are all used up. A press of the new console session button with the modifier down, before the page noticed it, opens no session: an error dialog says that creating a new console session with defaults is unavailable because the default agent's accounts have all used up their quota, and advises choosing another agent in the new session dialog or trying again once the quota resets.",
    state: launchState([searchApi], codexSpent, codexConsole),
    steps: [(ui) => ui.pressWithModifier(ui.t("sidebar.consoleSessions.new"))],
  },
  {
    id: "focus-project-quick-create-quota-switched",
    group: FOCUS_GROUP,
    category: "normal",
    title: "Quick create when the inherited account is used up",
    description: "Website would run on Work, whose quota is used up, so a quick create (pressed here with the modifier) runs the new session on Default, the account with quota left. Its card is selected and its line beside the agent mark opens with Default.",
    state: launchState([onWork], claudeQuotas),
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${onWork.id}` },
    steps: [
      (ui) => ui.pressWithModifier(ui.t("sidebar.project.openSession")),
      // The new card slides in before the view is settled.
      (ui) => ui.wait(600),
    ],
  },
  {
    id: "focus-console-session-quota-quick-menu",
    group: FOCUS_GROUP,
    category: "exceptional",
    held: true,
    title: "Quick create menu with an agent's accounts used up",
    description: "With the modifier held, the project chooser opened from the focus header is in its quick form: Search API, whose default agent (Codex) has all its accounts used up, is greyed out and cannot be chosen, with the short reason \"Default agent's quota used up\" under its name, while Website, which can fall back to Default, is offered.",
    state: launchState([onWork, searchApi], [...claudeQuotas, ...codexSpent]),
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub.id}` },
    steps: [(ui) => ui.press(ui.t("sidebar.focus.newSession"))],
  },
];
