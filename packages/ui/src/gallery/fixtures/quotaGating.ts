import type { Account, AccountQuota, Agent, AgentAvailability, Console, Project, Settings } from "../../protocol";
import type { Scenario } from "../scenario";
import type { Ui } from "../interact";
import { ALL_AGENTS, SAMPLE, accountSharingOf, minutesAgo, minutesFromNow, projectOf, sessionOf, snapshotState, switchingAccount } from "./builders";
import { quotaFailure, quotaOf, quotaWindow } from "./quotas";
import { DEFAULT_GIT_WORKFLOW } from "../../gitWorkflow";

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
  default_clone_dir: "/Users/dev/Projects", git_workflow: DEFAULT_GIT_WORKFLOW,
  accounts,
};

/** The name of Work in the one menu that shows a disabled account's label clipped by the menu's width. */
const longWorkName = "Work account for the Acme Corporation engagement";
const longWorkNameSettings: Settings = {
  ...settings,
  accounts: accounts.map((account) => (account.id === "a-work" ? { ...account, name: longWorkName } : account)),
};

const available = new Map<Agent, AgentAvailability>(
  ALL_AGENTS.map((agent) => [agent, { agent, availability: "available", default_account_dir: `/Users/dev/.${agent}` }]),
);

/** Claude Code's readings: the default account with plenty left in both windows, Work's session window spent (it resets
 * in two hours), Personal with 4% and 12% left, and none for Client, which has not been read yet. */
const claudeQuotas: AccountQuota[] = [
  quotaOf("claude", null, "max", [quotaWindow("session", 22, 190), quotaWindow("weekly", 41, 3 * 24 * 60 + 4 * 60)]),
  quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 125), quotaWindow("weekly", 63, 2 * 24 * 60 + 5 * 60)]),
  quotaOf("claude", "a-personal", "pro", [quotaWindow("session", 96, 40), quotaWindow("weekly", 88, 24 * 60 + 90)]),
];

/** Claude Code's readings as `claudeQuotas` has them, but Work's session window, spent when it was read eight minutes
 * ago, reset a minute ago and no newer reading has arrived. */
const workResetPassedQuotas: AccountQuota[] = claudeQuotas.map((quota) =>
  quota.account === "a-work"
    ? { ...quota, fetched_at: minutesAgo(8), windows: quota.windows.map((window) => window.kind === "session" ? quotaWindow("session", 100, -1) : window) }
    : quota);

/** Codex with both its accounts spent: the default one in its 5h window, Team in its weekly one. */
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

/** Work and Claude Code's default account turned off, Personal and Client left on: a session on Work, or one that would
 * inherit it, has two accounts to pass over. */
const claudeTurnedOff: Settings = {
  ...settings,
  disabled_default_accounts: ["claude"],
  accounts: accounts.map((account) => (account.id === "a-work" ? { ...account, disabled: true } : account)),
};

/** Codex's Team turned off, its default account left on: with the default account's own window used up, no Codex account
 * can open a session, for two different reasons. */
const codexTeamOff: Settings = {
  ...settings,
  accounts: accounts.map((account) => (account.id === "a-team" ? { ...account, disabled: true } : account)),
};

/** Client turned off as well as the default account and Work: the session's own account, Work, is off too, which only
 * stops new work on it. */
const claudeOnlyPersonalOn: Settings = {
  ...claudeTurnedOff,
  accounts: claudeTurnedOff.accounts.map((account) => (account.id === "a-client" ? { ...account, disabled: true } : account)),
};

/** Every Codex account turned off, the default one and Team. */
const codexTurnedOff: Settings = {
  ...settings,
  disabled_default_accounts: ["codex"],
  accounts: accounts.map((account) => (account.id === "a-team" ? { ...account, disabled: true } : account)),
};

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

/** When the daemon, which holds a newer reading than the window, refuses: the session window resets in 95 minutes.
 * Its wire name stays "5-hour", with its kind and its five-hour length beside it; the app words it "session" for
 * Claude Code. */
const refusedWithReset = {
  code: "account_quota_exhausted",
  params: { agent: "Claude Code", account: "Work", window: "5-hour", window_kind: "session", window_minutes: "300", resets_at: String(minutesFromNow(95)) },
  message: "Work has reached its Claude Code usage limit (5-hour)",
};

/** The daemon refusing a session on Work, which it found not logged in since the window last heard. */
const refusedSignedOut = {
  code: "account_not_signed_in",
  params: { agent: "Claude Code", account: "Work" },
  message: "Work (Claude Code) is not logged in. Choose another account, or log in to it in Settings → Agent accounts.",
};

/** The daemon refusing a session on Work, which it turned off since the window last heard. */
const refusedDisabled = {
  code: "account_disabled",
  params: { agent: "Claude Code", account: "Work" },
  message: "Work is disabled for Claude Code. Choose another account, or turn it back on in Settings → Agent accounts.",
};

/** Claude Code's readings with two accounts not logged in: Work, which never was, and Client, whose config directory
 * does not exist yet. The default account and Personal have theirs as `claudeQuotas` has them. The daemon refuses a session
 * on either, and the pickers list them but do not offer them. */
const signedOutQuotas: AccountQuota[] = [
  claudeQuotas[0],
  quotaOf("claude", "a-work", null, [], { unavailable: quotaFailure("quota_not_signed_in", { agent: "Claude Code" }, "Claude Code is not signed in: Error: not logged in") }),
  claudeQuotas[2],
  quotaOf("claude", "a-client", null, [], { unavailable: quotaFailure("quota_config_dir_missing", { agent: "Claude Code", dir: "/Users/dev/.claude-client" }, "The config directory does not exist") }),
];

/** Grok Build's one account, not logged in. */
const grokSignedOut: AccountQuota[] = [quotaOf("grok", null, null, [], { unavailable: quotaFailure("quota_not_signed_in", { agent: "Grok Build" }, "Grok Build is not signed in") })];

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
    title: "New session, account picker with usage bars",
    description: "Website would run on Work, whose usage limit is reached, so the Claude Code account field opens on Default, the first account with usage left, and the open picker lists every account with a compact usage bar before its name: the session window's bar with the weekly window's thin one right under it. Default has plenty left and is selected; Work is unselectable, its name in the secondary text colour and its bar faded and its reason at full contrast under the name on two lines, \"Limit reached\" and when it resets; Personal is nearly empty; Client has no reading yet and shows an empty track.",
    state: launchState([onWork], claudeQuotas),
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Claude Code"))],
  },
  {
    id: "dialog-new-session-sharing-picker",
    group: GROUP,
    category: "normal",
    title: "New session, account picker with a sharing change running",
    description: "Website would run on Client, whose sharing change is running, so the Claude Code account field opens on Default instead, with no alert. The open picker lists Default (selected), Work, Personal and Client, each with an empty usage track, none having a reading yet. Client is unselectable, its name in the secondary text colour and its track faded, with no reason under the name since the change ends on its own; the other accounts can be chosen.",
    state: { ...launchState([onClient], []), accountSharing: clientChangingSharing },
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Claude Code"))],
  },
  {
    id: "dialog-new-session-disabled-picker",
    group: GROUP,
    category: "normal",
    title: "New session, account picker with accounts turned off",
    description: "Website would run on Work, which is turned off, so the Claude Code account field opens on Personal, the first account that is on, with no alert. The open picker lists only the accounts that are on, Personal (selected) and Client, each with an empty usage track, none having a reading yet: Default and Work, turned off, are not listed at all.",
    state: { ...launchState([onWork], []), settings: claudeTurnedOff },
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Claude Code"))],
  },
  {
    id: "dialog-new-session-disabled-agents",
    group: GROUP,
    category: "normal",
    title: "New session, agents with every account turned off are hidden",
    description: "Chat Bot would run on Grok Build, whose only account is turned off. Codex's Default and Team accounts are also off. The dialog falls back to Claude Code, with no alert, and the open Agent list offers only Claude Code: both agents with no enabled account are absent.",
    state: {
      ...launchState([chat], []),
      settings: { ...codexTurnedOff, disabled_default_accounts: ["codex", "grok"] },
    },
    steps: [openSessionDialog, (ui) => ui.press(ui.t("dialog.session.agent"))],
  },
  {
    id: "dialog-new-session-all-accounts-disabled",
    group: GROUP,
    category: "exceptional",
    title: "New session, every agent's accounts are turned off",
    description: "Every added agent has all its accounts turned off. Opening a new session shows a failure modal saying no agent is available to open a session, with OK as its only button, instead of offering a disabled account.",
    state: {
      ...launchState([website], []),
      settings: { ...settings, disabled_default_accounts: ALL_AGENTS, accounts: accounts.map((account) => ({ ...account, disabled: true })) },
    },
    steps: [openSessionDialog],
  },
  {
    id: "dialog-new-session-signed-out-picker",
    group: GROUP,
    category: "normal",
    title: "New session, account picker with accounts not logged in",
    description: "Website would run on Work, which is not logged in, so the Claude Code account field opens on Default, the first account that can run a session, with no alert. The open picker lists Default (selected), Work, Personal and Client. Work and Client, whose last reading found them not logged in, are unselectable, their names in the secondary text colour and their empty tracks faded, with the short reason \"Not logged in\" at full contrast under each name, styled like the reason of an account at its limit; Personal can be chosen.",
    state: launchState([onWork], signedOutQuotas),
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Claude Code"))],
  },
  {
    id: "dialog-new-session-quota-reset-passed",
    group: GROUP,
    category: "normal",
    title: "New session, account whose spent window has since reset",
    description: "Website runs on Work, whose session window was spent when it was read 8 minutes ago but reset a minute ago, before any newer reading. The Claude Code account field keeps Work selected, not at its limit, and the open picker lists Work's bar with the session part full over the weekly window's 37%, its summary reading Session 100% remaining; Personal is nearly empty and Client has no reading.",
    state: launchState([onWork], workResetPassedQuotas),
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Claude Code"))],
  },
  {
    id: "dialog-new-session-auto-select-account",
    group: GROUP,
    category: "normal",
    title: "New session, the account whose weekly limit resets sooner",
    description: "Website would run on Default, which has most of its weekly usage left and resets later, but the Claude Code account field opens on Soon, which has little weekly usage left and resets sooner. The open picker shows Soon selected beside Default's fuller bar.",
    state: {
      ...launchState([website], soonerWeeklyQuotas),
      settings: { ...settings, auto_select_account: true, accounts: [soonAccount] },
    },
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Claude Code"))],
  },
  {
    id: "dialog-new-session-auto-select-agent",
    group: GROUP,
    category: "normal",
    title: "New session, automatic selection falls back to another agent",
    description: "Search API defaults to Codex, whose Default account has reached its limit and whose Team account is turned off. With automatic account selection on, the dialog opens on Claude Code, the first added agent with an available account, with Default selected and Open enabled.",
    state: {
      ...launchState([searchApi], [codexSpent[0]]),
      settings: { ...codexTeamOff, auto_select_account: true },
    },
    steps: [openSessionDialog],
  },
  {
    id: "dialog-hand-off-auto-select-account",
    group: GROUP,
    category: "normal",
    title: "Hand off, the account whose weekly limit resets sooner",
    description: "Hub hands off to Codex, whose default account has reached its limit. The Codex account field opens on Sprint, whose weekly limit resets in two days, not on Team, the first account with usage left, which resets in five. The open picker lists the default account at its limit unselectable, then Team, then Sprint selected.",
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
    title: "New session, every account of the agent has reached its limit",
    description: "Both of Codex's accounts have reached their limit (Default in its 5h window, Team in its weekly one), so none can be preselected: Open is disabled and the Agent field is marked invalid, with its own error under the agent and account pair naming the unavailable account and giving the limit reason and reset time. No Alert stands above the form.",
    state: launchState([searchApi], codexSpent),
    steps: [openSessionDialog],
  },
  {
    id: "dialog-new-session-quota-single-account",
    group: GROUP,
    category: "exceptional",
    title: "New session, the agent's only account has reached its limit",
    description: "Grok Build has only its default account, and its account picker is still there beside the agent select, offering that one option. Its credit pool has reached its limit: the open picker lists Default unselectable, its name in the secondary text colour and its empty bar faded and its reason at full contrast under the name on two lines, \"Limit reached\" and when it resets. Open is disabled and the Agent field is marked invalid, its error naming the account and giving the limit reason and reset time under the pair, partly behind the open list (the closed picker is the scenario above).",
    state: launchState([chat], grokSpent),
    steps: [openSessionDialog, (ui) => ui.press(accountField(ui, "Grok Build"))],
  },
  {
    id: "dialog-new-session-signed-out-single-account",
    group: GROUP,
    category: "exceptional",
    title: "New session, the agent's only account is not logged in",
    description: "Grok Build has only its default account, which is not logged in, so none can be preselected: Open is disabled and the Agent field is marked invalid, with its own error under the agent and account pair saying that Default, shown as a chip, is not logged in and advising to choose another account or log in to it. The account picker, still there beside the agent select, is closed.",
    state: launchState([chat], grokSignedOut),
    steps: [openSessionDialog],
  },
  {
    id: "dialog-hand-off-all-disabled",
    group: GROUP,
    category: "normal",
    title: "Hand off, skip the agent whose accounts are all turned off",
    description: "Codex's Default and Team accounts are both turned off, so handing off Hub falls back to Grok Build. The open Hand off to list offers only Grok Build: Codex is absent, and Hub's own agent, Claude Code, is excluded as usual. Hand off is enabled and no account error appears.",
    state: { ...launchState([], []), settings: codexTurnedOff },
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: hub.title })),
      (ui) => ui.press(ui.t("sidebar.session.handOff")),
      (ui) => ui.press(ui.t("dialog.handOff.agent")),
    ],
  },
  {
    id: "dialog-new-session-incomplete-launch",
    group: GROUP,
    category: "exceptional",
    title: "New session, the account's last sharing change did not finish",
    description: "Website runs on Client, whose last sharing change did not finish, so Open asks first: a confirmation titled \"Start on this account\" opens over the session dialog, which stays behind it. It names Client and says its last sharing change did not finish, so the configuration and conversations may be incomplete and offers two buttons, View in Settings and, last, the primary Start anyway, which is not destructive and holds the focus; there is no Cancel, and the dialog's close button is the way to dismiss it.",
    state: { ...launchState([onClient], []), settings: clientIncomplete },
    steps: [openSessionDialog, (ui) => ui.confirm()],
  },
  {
    id: "dialog-switch-account-incomplete-launch",
    group: GROUP,
    category: "exceptional",
    title: "Switch account, the target's last sharing change did not finish",
    description: "Switching the session to Client, whose last sharing change did not finish: after Confirm in the switch dialog, the \"Start on this account\" confirmation opens over it, which stays behind with its Confirm button pending until the question is answered. The question's buttons are View in Settings and the focused, primary Start anyway, with no Cancel; the dialog's close button dismisses it. Nothing has been switched yet.",
    state: { ...switchState([]), settings: clientIncomplete },
    steps: [...openSwitchTo("Client"), (ui) => ui.confirm()],
  },
  {
    id: "dialog-resume-incomplete-launch",
    group: GROUP,
    category: "exceptional",
    title: "Resume by selecting, the account's last sharing change did not finish",
    description: "Selecting Website's interrupted session, which ran on Client, whose last sharing change did not finish, would resume it, so the window asks first: a \"Start on this account\" confirmation opens over the window, naming Client and saying its configuration and conversations may be incomplete. Its buttons are View in Settings and the primary Start anyway, with no Cancel (the dialog's close button dismisses it); Start anyway holds the focus and keeps it while the session's terminal attaches. Nothing has been resumed yet.",
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
    id: "terminal-not-running-bar",
    group: "Terminal",
    category: "normal",
    title: "Interrupted session selected and left not running",
    description: "The question about starting on Client was closed without answering, so nothing was resumed: the selected session keeps its last output on screen, with a \"Not running\" bar and a primary Resume button floating at the bottom, and its row and the title bar show the interrupted pause sign. Pressing Resume starts the session.",
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
    steps: [
      (ui) => ui.press(ui.session("Port the importer to the new parser")),
      (ui) => ui.press(ui.t("common.close")),
      // The question's dialog holds the toggle inert, so the drawer a pick leaves up from 782px to 1168px is closed
      // after it.
      (ui) => ui.closeDrawer(),
    ],
  },
  {
    id: "settings-accounts-from-incomplete-launch",
    group: "Settings",
    category: "exceptional",
    title: "Settings opened from the question about an account's unfinished sharing change",
    description: "Search API runs on Team, a Codex account whose last sharing change did not finish. Open asked first, and View in Settings was pressed: the launch is cancelled, the question and the session dialog beneath it are gone, and Settings is open on Agent accounts with the Codex tab selected and Team's Account settings in front. Team's Share default configuration switch, shown on, its position before the change, has the focus ring on it, and a warning triangle named \"Last sharing change\", for the last attempt to turn sharing off that did not finish, sits right of it.",
    state: { ...launchState([onTeam], []), settings: teamIncomplete },
    steps: [openSessionDialog, (ui) => ui.confirm(), (ui) => ui.press(ui.t("dialog.incompleteLaunch.viewSettings"))],
  },
  {
    id: "dialog-new-session-quota-refused",
    group: GROUP,
    category: "exceptional",
    title: "Session launch refused, the account's usage limit is reached",
    description: "The daemon holds a newer reading than the window: Work's session limit is reached. An error dialog opens over the session dialog, which stays behind it: it names the account and the agent, says the session limit is reached and in how long it resets, and advises another account or waiting.",
    state: launchState([onWork], []),
    sessionOpenError: refusedWithReset,
    steps: [openSessionDialog, (ui) => ui.confirm()],
  },
  {
    id: "dialog-new-session-disabled-refused",
    group: GROUP,
    category: "exceptional",
    title: "Session launch refused, the account was turned off",
    description: "Work was turned off elsewhere after this window last heard, so it still looks usable and the dialog opened on it. Opening is refused by the daemon: an error dialog opens over the session dialog, which stays behind it, names the account and the agent and says it is disabled so a session cannot be opened on it, then advises choosing another account or enabling this account again; a Manage accounts… button beside OK opens Settings on Agent accounts.",
    state: launchState([onWork], []),
    sessionOpenError: refusedDisabled,
    steps: [openSessionDialog, (ui) => ui.confirm()],
  },
  {
    id: "dialog-new-session-signed-out-refused",
    group: GROUP,
    category: "exceptional",
    title: "Session launch refused, the account is not logged in",
    description: "Work was found not logged in elsewhere after this window last heard, so it still looks usable and the dialog opened on it. Opening is refused by the daemon: an error dialog opens over the session dialog, which stays behind it, names the account and the agent and says it is not logged in so a session cannot be opened on it, then advises choosing another account or logging in to this one; a Manage accounts… button beside OK opens Settings on Agent accounts.",
    state: launchState([onWork], []),
    sessionOpenError: refusedSignedOut,
    steps: [openSessionDialog, (ui) => ui.confirm()],
  },
  {
    id: "sidebar-session-switch-account-quota-menu",
    group: GROUP,
    category: "normal",
    title: "Switch account menu, an account at its limit",
    description: "The row's Switch account submenu lists Default (current), Work, Personal and Client; each account shows its compact usage bar in place of the key icon, as in the account picker. Work, whose session limit is reached, is greyed out and cannot be chosen, with its reason on two lines under its name, \"Limit reached\" and when it resets (arrow keys still reach it, and its accessible name carries the reason); Personal, nearly spent, and Client, not read yet (an empty track), can be chosen. Work's name is long enough to be clipped at the menu's edge, and with the pointer resting on it a tooltip (drawn over the Default row) carries the full name while the greyed-out row stays unlit.",
    state: { ...switchState(claudeQuotas), settings: longWorkNameSettings },
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Review the release checklist" })),
      (ui) => ui.press(ui.t("sidebar.session.switchAccount")),
      (ui) => ui.hoverMenuLabel(longWorkName, longWorkName),
    ],
  },
  {
    id: "sidebar-session-switch-account-signed-out-menu",
    group: GROUP,
    category: "normal",
    title: "Switch account menu, accounts not logged in",
    description: "The row's Switch account submenu lists Default (current), Work, Personal and Client. Work and Client, whose last reading found them not logged in, are greyed out and cannot be chosen, with the short reason \"Not logged in\" under each name, in the style of an account at its limit (arrow keys still reach them; the accessible name stays the account's name and its description reads \"Unavailable until it is logged in\"); Personal can be chosen.",
    state: switchState(signedOutQuotas),
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
    description: "The row's Switch account submenu lists Default (current), Work, Personal and Client. Client, whose sharing change is running, is greyed out and cannot be chosen, with no reason under its name since the change ends on its own (arrow keys still reach it); the others can be chosen.",
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
    description: "Verify the migration, bound to Release coordinator, reads interrupted while the daemon holds it for its account switch. In its menu, Hand off and Unbind are greyed out with no reason under their names, since the switch ends on its own; in the Switch account submenu, Default stays marked as current and Work, Personal and Client are greyed out the same way and cannot be chosen.",
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
    description: "The row's session is on Client, whose sharing change is running, so the daemon would refuse to move it away or hand it off. Every other entry in the Switch account submenu, Default, Work and Personal, is greyed out with no reason under its name and cannot be chosen; Client stays marked as current. In the menu behind it, Hand off is disabled the same way.",
    state: { ...switchState([], accounts[2]), accountSharing: clientChangingSharing },
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Review the release checklist" })),
      (ui) => ui.press(ui.t("sidebar.session.switchAccount")),
    ],
  },
  {
    id: "sidebar-session-switch-account-disabled-menu",
    group: GROUP,
    category: "normal",
    title: "Switch account menu, accounts turned off",
    description: "The session runs on Work, which is turned off, and keeps running: Work stays in the Switch account submenu, marked as current and neither greyed out nor labelled, since turning an account off does not touch the sessions already on it. Personal, which is on, is listed and can be chosen; Default and Client, turned off, are not listed at all.",
    state: { ...switchState([], accounts[0]), settings: claudeOnlyPersonalOn },
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Review the release checklist" })),
      (ui) => ui.press(ui.t("sidebar.session.switchAccount")),
    ],
  },
  {
    id: "dialog-switch-account-disabled",
    group: GROUP,
    category: "exceptional",
    title: "Switch account, the account turned out disabled",
    description: "The menu offered Client, and it was turned off after the menu was built (here by the steps, as another window would). The dialog shows no Alert and Switch is enabled; pressing it is refused here, and the refusal is an error alert dialog over the switch dialog, saying Account Client is disabled so a session cannot use it, then advising to choose another account or enable this account again; a Manage accounts… button beside OK opens Settings on Agent accounts. The switch dialog stays open behind it.",
    state: switchState([]),
    steps: [...openSwitchTo("Client"), (ui) => ui.daemonEvents((state) => ({ type: "settings_updated", settings: { ...state.settings, accounts: state.settings.accounts.map((account) => (account.id === "a-client" ? { ...account, disabled: true } : account)) } })), (ui) => ui.confirm()],
  },
  {
    id: "dialog-switch-account-signed-out",
    group: GROUP,
    category: "exceptional",
    title: "Switch account, the account turned out not to be logged in",
    description: "The menu offered Work, and a reading that found it not logged in landed after: the dialog shows no Alert and Switch is enabled. Pressing it is refused here, and the refusal is an error alert dialog over the switch dialog, saying Account Work is not logged in so a session cannot use it, then advising to choose another account or log in to this one; a Manage accounts… button beside OK opens Settings on Agent accounts. The switch dialog stays open behind it.",
    state: switchState([]),
    steps: [...openSwitchTo("Work"), (ui) => ui.setAccountQuotas(signedOutQuotas), (ui) => ui.confirm()],
  },
  {
    id: "dialog-switch-account-quota-used-up",
    group: GROUP,
    category: "exceptional",
    title: "Switch account, the account turned out to be at its limit",
    description: "The menu offered Work, and its reading landed after: the dialog shows no Alert and Switch is enabled. Pressing it is refused here, and the refusal is an error alert dialog over the switch dialog, saying Work cannot be used for now, with the limit reason and reset time. The switch dialog stays open behind it.",
    state: switchState([]),
    steps: [...openSwitchTo("Work"), (ui) => ui.setAccountQuotas(claudeQuotas), (ui) => ui.confirm()],
  },
  {
    id: "dialog-hand-off-quota-picker",
    group: GROUP,
    category: "normal",
    title: "Hand off, the inherited Codex account has reached its limit",
    description: "Hub hands off to Codex, whose default account has reached its limit: the field opens on Team, the account with usage left, and the open picker lists the default account unselectable, its name in the secondary text colour and its bar faded and its reason at full contrast under the name on two lines, \"Limit reached\" and when it resets.",
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
    title: "Hand off, the only Codex account has reached its limit",
    description: "Hub hands off to Codex, which has only its default account, and that has reached its limit: Hand off is disabled and the Hand off to field is marked invalid, with its own error under the pair naming the unavailable account and giving the limit reason and reset time. The account picker beside the agent select holds that one account, Default, with its two-layer usage bar, the 5h bar empty over the part-filled weekly one, and the picker stays closed.",
    state: { ...launchState([], [codexSpent[0]]), settings: { ...settings, accounts: accounts.filter((account) => account.agent === "claude") } },
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: hub.title })),
      (ui) => ui.press(ui.t("sidebar.session.handOff")),
    ],
  },
  {
    id: "sidebar-quick-create-auto-select-agent",
    group: GROUP,
    category: "normal",
    title: "Quick create falls back to another agent",
    description: "The console defaults to Codex, whose Default account has reached its limit and whose Team account is turned off. With automatic account selection on, quick create opens a Claude Code session on Default, the first available agent in the added list. The new session is selected.",
    state: {
      ...launchState([searchApi], [codexSpent[0]], codexConsole),
      settings: { ...codexTeamOff, auto_select_account: true },
    },
    steps: [(ui) => ui.pressWithModifier(ui.t("sidebar.consoleSessions.new"))],
  },
  {
    id: "sidebar-quick-create-quota-used-up",
    group: GROUP,
    category: "exceptional",
    held: true,
    title: "Quick create, the default agent's accounts at their limit or turned off",
    description: "With the modifier held the new-session buttons are bolts. The console's sessions run on Codex, the default agent: its default account has reached its 5h limit and Team is turned off, so no Codex account can open a session. The bolt in the sidebar header looks disabled but can be focused and hovered: the pointer rests on it and its tooltip is the short reason \"No available account\", while its accessible name stays \"New console session with defaults\". Search API's project row has the same disabled bolt, whose tooltip shows the same reason on hover.",
    state: { ...launchState([searchApi], [codexSpent[0]], codexConsole), settings: codexTeamOff },
    // The "+" is still plain while the steps run: the modifier is pressed after them, and the open tooltip then reads the reason.
    steps: [(ui) => ui.hoverTitledControl(ui.t("sidebar.consoleSessions.new"))],
  },
  {
    id: "sidebar-quick-create-unavailable",
    group: GROUP,
    category: "exceptional",
    title: "Quick create refused, no agent has an available account",
    description: "Automatic account selection is on, but none of the added agents can open a session. Pressing New console session with the modifier opens a short error dialog: \"No account is currently available to open a session.\" It offers OK, with no recovery advice or account-management action.",
    state: {
      ...launchState([searchApi], [...codexSpent, ...grokSignedOut, { ...signedOutQuotas[1], account: "a-personal" }], codexConsole),
      settings: {
        ...settings,
        auto_select_account: true,
        disabled_default_accounts: ["claude"],
        accounts: accounts.map((account) => account.agent === "claude" && account.id !== "a-client" && account.id !== "a-personal" ? { ...account, disabled: true } : account),
      },
      accountSharing: clientChangingSharing,
    },
    steps: [(ui) => ui.pressWithModifier(ui.t("sidebar.consoleSessions.new"))],
  },
  {
    id: "focus-project-quick-create-quota-switched",
    group: FOCUS_GROUP,
    category: "normal",
    title: "Quick create when the inherited account has reached its limit",
    description: "Website would run on Work, whose usage limit is reached, so a quick create (pressed here with the modifier) runs the new session on Default, the account with usage left. Its card is selected and its line beside the agent mark opens with Default.",
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
    title: "Quick create menu with an agent's accounts at their limit",
    description: "With the modifier held, the project chooser opened from the focus header is in its quick form: Search API, whose default agent (Codex) has all its accounts at their limit, is greyed out and cannot be chosen, with the short reason \"No available account\" under its name, while Website, which can fall back to Default, is offered.",
    state: launchState([onWork, searchApi], [...claudeQuotas, ...codexSpent]),
    preferences: { sidebarConsole: console_.id, sidebarFocus: `consoleSession:${hub.id}` },
    steps: [(ui) => ui.press(ui.t("sidebar.focus.newSession"))],
  },
];
