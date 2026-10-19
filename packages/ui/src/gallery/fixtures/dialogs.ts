import type { Agent, AgentAvailability, Settings } from "../../protocol";
import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { ALL_AGENTS, SAMPLE, branchItemOf, consoleOf, minutesAgo, projectOf, sessionOf, snapshotState, worktreeItemOf } from "./builders";
import { SAMPLE_CODEX_MODELS } from "./agentModels";
import { IMAGES } from "./viewerImages";
import { SHELL_TIMEOUT } from "./shellTimeout";


const { console: console_, web, api, sessions } = SAMPLE;

// A conversation ID as an agent writes it, typed into the Session ID field the dialog's Resume conversation tab shows.
const RESUME_ID = "3f2b8c1e-7a4d-4e9b-9c55-0d6a1b2e4f70";

// Two projects carrying tags, so the edited one has some of its own and the field suggests the rest.
// Each also carries a description: a short one of a few lines, and one longer than the field is tall.
const tagged = [
  projectOf("p-docs", console_.id, "Docs Site", {
    tags: ["x", "frontend", "a-long-tag-for-checking-project-interface-layout"],
    description: "The public documentation site, built with Astro.\nGuides live under src/content/guides.",
  }),
  projectOf("p-billing", console_.id, "Billing Service", {
    tags: ["backend", "payments"],
    description: [
      "Handles subscriptions, invoices and payment provider webhooks for every Octoboard product.",
      "",
      "Layout: the HTTP API is under api/, background jobs (invoice generation, dunning, usage metering) under jobs/, and the provider adapters under providers/. Each adapter implements the same trait, so a new provider only adds a module there.",
      "",
      "Conventions: amounts are integer minor units, never floats; every webhook handler is idempotent and keyed by the provider's event id; migrations are forward-only and reviewed by the payments owners.",
      "",
      "Testing: run cargo test for unit tests; the integration suite needs the local Postgres from docker compose up and the provider sandbox keys in .env.test.",
      "",
      "Deploys go through the staging environment first; production deploys are manual and announced in the payments channel.",
    ].join("\n"),
  }),
];
const taggedState = snapshotState({ consoles: [console_], projects: tagged, sessions: [] });

// A second live console session beside the sample's, and an archived one the dialog must not offer.
const twoHubsState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [
    ...sessions.map((s) => (s.id === SAMPLE.consoleSession.id ? { ...s, title: "Hub", ordinal: 1 } : s)),
    sessionOf("s-console-2", console_.id, undefined, "A release coordinator with a console session name too long for the owner picker", "working", { colour: "pink", ordinal: 2, started_at: minutesAgo(20) }),
    sessionOf("s-console-old", console_.id, undefined, "Hub 3", "archived", { colour: "cyan", ordinal: 3 }),
  ],
});

// Two accounts of one agent, so the console dialog has something for its account picker to offer.
const accountsState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions,
  settings: {
    auto_pull_repositories: false,
    auto_delete_unresumable_archives: true,
    added_agents: ALL_AGENTS,
    default_clone_dir: "/Users/dev/Projects",
    accounts: [
      { id: "a-work", agent: "claude", name: "Work account for release coordination across all production services", config_dir: "/Users/dev/.claude-work" },
      { id: "a-personal", agent: "claude", name: "Personal account for documentation and experimental development", config_dir: "/Users/dev/.claude-personal" },
    ],
  },
});

// Manually bound running sessions and a lead with its own child all join the archive confirmation.
const cascadeHub = sessionOf("s-hub-4", console_.id, undefined, "Hub 4", "idle", { colour: "cyan", ordinal: 4 });
const cascadeState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [
    cascadeHub,
    sessionOf("s-web-3", console_.id, web.id, "Tidy the changelog", "working", { bound_to: cascadeHub.id, origin: "user" }),
    sessionOf("s-api-3", console_.id, api.id, "Rename the index", "waiting_user", { bound_to: cascadeHub.id, account_id: "a-work" }),
    sessionOf("s-api-4", console_.id, api.id, "Review the migration", "interrupted", { bound_to: "s-api-3" }),
  ],
  settings: accountsState.settings,
});

// A project whose running sessions are on the default account, on a stored one, and on one that has
// since been removed (its sessions keep the directory they recorded).
const removeState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [
    ...sessions,
    sessionOf("s-web-4", console_.id, web.id, "Check the release notes", "waiting_user", {
      account_id: "a-removed",
      config_dir: "/Users/dev/.claude-removed",
    }),
  ].map((session) => (session.id === "s-web-2" ? { ...session, account_id: "a-work" } : session)),
  settings: accountsState.settings,
});

// A console that carries a stored model and effort for its default agent: the model is one the
// dialog's list no longer names, so it is offered on the end, marked.
const launchSettingsState = snapshotState({
  consoles: [{ ...console_, claude_console_session_model: "retired-model-for-production-release-coordination-and-compatibility-review", claude_console_session_effort: "high" }],
  projects: [web, api],
  sessions,
});

// A console with a custom avatar, so the dialog shows the image and its delete button.
const avatarState = snapshotState({
  consoles: [{ ...console_, icon: `data:${IMAGES.photo_jpg.mediaType};base64,${IMAGES.photo_jpg.data}` }],
  projects: [web, api],
  sessions,
});

// A project that carries a stored account and launch settings for its agent, beside a console that
// refers to another account, so the project's account picker names what "Inherit" resolves to.
const configured = projectOf("p-configured", console_.id, "Configured Site", {
  default_agent: "claude",
  claude_account_id: { kind: "account", id: "a-work" },
  claude_session_model: "retired-model-for-production-release-coordination-and-compatibility-review",
  claude_session_effort: "high",
});
const configuredState = snapshotState({
  consoles: [{ ...console_, claude_account_id: "a-personal" }],
  projects: [configured],
  sessions: [],
  settings: accountsState.settings,
});

// The console pins a named account while the project is on the agent's default account, the state
// the account picker's second entry exists for.
const onDefault = projectOf("p-default-account", console_.id, "Default Account Site", {
  default_agent: "claude",
  claude_account_id: { kind: "default" },
});
const onDefaultState = snapshotState({
  consoles: [{ ...console_, claude_account_id: "a-personal" }],
  projects: [onDefault],
  sessions: [],
  settings: accountsState.settings,
});

// Detection has settled: the listed agents are not on the machine, the rest are.
const detected = (missing: Agent[]) =>
  new Map<Agent, AgentAvailability>(
    (["claude", "codex", "grok"] as const).map((agent) => [
      agent,
      { agent, availability: missing.includes(agent) ? "unavailable" : "available", default_account_dir: `/Users/dev/.${agent}` },
    ]),
  );

// Only the `added` agents are added, in that order, over a machine that has all of them installed.
const withAddedAgents = <S extends { settings?: Settings }>(state: S, added: Agent[]) => ({
  ...state,
  settings: { ...state.settings!, added_agents: added },
  agentAvailability: detected([]),
});

// A console that was set up while Codex was installed: its project sessions run on Codex, where its
// console sessions run on Claude Code, and it still pins a named account for each. Claude Code has
// a second account to choose between; Grok Build has only its default account.
const mixedSettings = {
  ...accountsState.settings!,
  accounts: [...accountsState.settings!.accounts, { id: "a-codex", agent: "codex" as const, name: "Codex personal", config_dir: "/Users/dev/.codex-personal" }],
};
const mixedState = (missing: Agent[]) => ({
  ...snapshotState({
    consoles: [{ ...console_, project_session_agent: "codex" as const, claude_account_id: "a-work", codex_account_id: "a-codex", codex_project_session_model: SAMPLE_CODEX_MODELS[0].id }],
    projects: [web, api],
    sessions,
    settings: mixedSettings,
  }),
  agentAvailability: detected(missing),
});

const consoleActions = (ui: Ui) =>
  ui.press(ui.t("sidebar.console.actions", { name: console_.name }));
const projectActions = (ui: Ui, name: string) =>
  ui.press(ui.t("sidebar.project.actions", { name }));
const editTaggedProject = [
  (ui: Ui) => projectActions(ui, tagged[0].name),
  (ui: Ui) => ui.press(ui.t("sidebar.project.edit")),
];

// Long fields share one view: the textarea scrolls while both counters approach their limits.
const nearLimitProject = projectOf("p-near-limit", console_.id,
  "Release coordination for documentation billing subscriptions deployment and support tools".padEnd(90, "."), {
    description: tagged[1].description!.repeat(3).slice(0, 1_800).trimEnd().padEnd(1_800, "."),
  });
const legacyProject = projectOf("p-legacy-text", console_.id, `${nearLimitProject.name} before limits`, {
  description: `${nearLimitProject.description}\n${"Earlier project notes. ".repeat(10)}`,
});
const textProjectState = (project: typeof nearLimitProject) => snapshotState({ consoles: [console_], projects: [project], sessions: [] });
const editTextProject = (project: typeof nearLimitProject) => [
  (ui: Ui) => ui.press((name) => name === ui.t("sidebar.project.actions", { name: project.name })),
  (ui: Ui) => ui.press(ui.t("sidebar.project.edit")),
];

const removeDocsSite = [
  (ui: Ui) => projectActions(ui, tagged[0].name),
  (ui: Ui) => ui.press(ui.t("sidebar.project.remove")),
  (ui: Ui) => ui.press(ui.t("dialog.removeProject.trash")),
];

/** A home directory with more subdirectories than the picker's list is tall, some of them repositories. */
const HOME_LISTING = [
  "Applications",
  "a-project-directory-with-a-name-too-long-for-the-directory-picker-row",
  "code",
  "Desktop",
  "Documents",
  "Downloads",
  "dotfiles",
  "Library",
  "Movies",
  "Music",
  "Pictures",
  "Projects",
  "repos",
  "scratch",
  "work",
].map((name) => ({ name, path: `/Users/dev/${name}`, is_git_repo: ["code", "dotfiles", "repos", "scratch"].includes(name) }));

/** Compatible console and project cases coexist; menus select the entity to inspect. */
const dialogState = {
  ...accountsState,
  consoles: new Map([
    [console_.id, {
      ...launchSettingsState.consoles!.get(console_.id)!,
      icon: avatarState.consoles!.get(console_.id)!.icon,
      claude_account_id: "a-personal",
      claude_project_session_model: "sonnet",
      claude_project_session_effort: "low",
    }],
    ["c-plain", consoleOf("c-plain", "Plain console")],
  ]),
  projects: new Map([...accountsState.projects!, ...taggedState.projects!, ...configuredState.projects!, ...onDefaultState.projects!]),
  sessions: new Map([...twoHubsState.sessions!, ...removeState.sessions!, ...cascadeState.sessions!]),
};

// A console whose agents were removed from the added list after it was set up: its console sessions run on Grok
// Build and its project sessions on Codex, both installed, with a named account pinned and model stored for each.
const removedAgentsState = (() => {
  const base = withAddedAgents(dialogState, ["claude"]);
  return {
    ...base,
    settings: {
      ...base.settings,
      accounts: [
        ...base.settings.accounts,
        { id: "a-codex", agent: "codex" as const, name: "Codex personal", config_dir: "/Users/dev/.codex-personal" },
        { id: "a-grok", agent: "grok" as const, name: "Grok team", config_dir: "/Users/dev/.grok-team" },
      ],
    },
    consoles: new Map([[console_.id, {
      ...console_,
      default_agent: "grok" as const,
      project_session_agent: "codex" as const,
      codex_account_id: "a-codex",
      grok_account_id: "a-grok",
      codex_project_session_model: SAMPLE_CODEX_MODELS[0].id,
      grok_console_session_model: "grok-code-fast-1",
    }]]),
  };
})();

// A project that still names an agent the user has since removed.
const removedAgentProject = projectOf("p-removed-agent", console_.id, "Grok Site", { default_agent: "grok" });

// An archived session of the Docs Site project: the one whose branches and worktree its removal with
// the folder takes along.
const docsSession = sessionOf("s-docs-1", console_.id, tagged[0].id, "Redesign the landing page", "archived");
const removalState = { ...dialogState, sessions: new Map([...dialogState.sessions, [docsSession.id, docsSession]]) };

// The sessions of the Hub 4 archive that the bound branches and worktrees belong to.
const hubSessions = {
  web: cascadeState.sessions!.get("s-web-3")!,
  api: cascadeState.sessions!.get("s-api-3")!,
  review: cascadeState.sessions!.get("s-api-4")!,
};

const dialogFixture = { state: dialogState, directories: { "/Users/dev": HOME_LISTING }, detections: { ...{ "~/code/slow": "pending" },
  ...{ "~/code/site": { agent: "codex" } },
  ...{
    "~/code/missing": { code: "path_not_found", params: { path: "/Users/me/code/missing" }, message: "`/Users/me/code/missing` does not exist" },
  },
  ...{
    "git@example.com:o/private.git": {
      code: "git_remote_unreachable",
      params: { detail: "Permission denied (publickey)." },
      message: "the repository could not be read: Permission denied (publickey).",
    },
  } } } satisfies Partial<Scenario>;

// The user's own console session instructions, several paragraphs and a list, longer than the
// field's six rows so that the field scrolls.
const SESSION_INSTRUCTIONS = [
  "Answer in Japanese unless asked otherwise.",
  "",
  "Before starting a project session, check the project's README for its test command and give it in the brief.",
  "",
  "When reporting back:",
  "- lead with the outcome in one sentence",
  "- list what changed, one line per file",
  "- say what was not verified",
  "",
  "Never start a session on the production branch.",
  "Ask me before deleting anything outside the project folder.",
  "Keep the summary under ten lines.",
].join("\n");

// Session settings initially reflect the selected agent's project defaults, each value falling back
// to the console's project session value where its project sets none. Both agents have usable
// stored values so changing the agent lets the same view show either model list.
const sessionLaunchState = {
  ...dialogState,
  // The open-session buttons share one label; keep the confirmation target's project alone
  // rather than depending on the first button of the alphabetically sorted sidebar.
  projects: new Map([[web.id, {
    ...web,
    claude_session_model: "opus",
    codex_session_model: SAMPLE_CODEX_MODELS[0].id,
    codex_session_effort: "xhigh",
  }]]),
  sessions: new Map([...dialogState.sessions].filter(([, session]) => !session.project_id || session.project_id === web.id)),
};

const consoleSessionLaunchState = {
  ...sessionLaunchState,
  consoles: new Map([[console_.id, {
    ...dialogState.consoles.get(console_.id)!,
    claude_console_session_model: "opus",
    claude_console_session_effort: "high",
    codex_console_session_model: SAMPLE_CODEX_MODELS[0].id,
    codex_console_session_effort: "xhigh",
  }]]),
};

// Three eligible Claude sessions share a console: its hub, an independent project session,
// and a bound child. Other-agent, archived, already-target and other-console sessions stay put.
const switchAccountState = snapshotState({
  consoles: [console_, consoleOf("c-other", "Personal")],
  projects: [web, api],
  sessions: [
    sessionOf("s-switch-hub", console_.id, undefined, "Release coordinator", "idle"),
    sessionOf("s-switch-work", console_.id, web.id, "Review the release checklist", "working"),
    sessionOf("s-switch-bound", console_.id, api.id, "Verify deployment and database migration compatibility", "waiting_user", { bound_to: "s-switch-hub" }),
    sessionOf("s-switch-already", console_.id, web.id, "Already on the work account", "idle", { account_id: "a-work", config_dir: "/Users/dev/.claude-work" }),
    sessionOf("s-switch-codex", console_.id, api.id, "Codex deployment review", "idle", { agent: "codex" }),
    sessionOf("s-switch-archived", console_.id, web.id, "Earlier release", "archived"),
    sessionOf("s-switch-other", "c-other", undefined, "Personal coordinator", "idle"),
  ],
  settings: accountsState.settings,
});
// Never-conversed sessions from all three agents share the same Delete shortcut, regardless
// of native record presence; the used session keeps Archive instead.
const unusedSession = sessionOf("s-web-unused", console_.id, web.id, "Draft the migration notes", "idle", { has_conversation: false, started_at: minutesAgo(4) });
const unusedCodex = sessionOf("s-codex-unused", console_.id, web.id, "Untouched Codex draft", "idle", { agent: "codex", has_conversation: false });
const unusedGrok = sessionOf("s-grok-unused", console_.id, web.id, "Untouched Grok draft", "idle", { agent: "grok", has_conversation: false });
const unusedState = snapshotState({
  consoles: [console_],
  projects: [web],
  sessions: [
    sessionOf("s-web-used", console_.id, web.id, "Fix the summary layout", "working"),
    unusedSession,
    unusedCodex,
    unusedGrok,
  ],
});
const openUnusedMenu = (ui: Ui) => ui.press(ui.t("sidebar.session.actions", { title: unusedSession.title }));
const openSwitchAccount = [
  (ui: Ui) => ui.press(ui.t("sidebar.session.actions", { title: "Review the release checklist" })),
  (ui: Ui) => ui.press(ui.t("sidebar.session.switchAccount")),
  (ui: Ui) => ui.press(accountsState.settings!.accounts[0].name),
];
const submitConsoleSwitch = [
  ...openSwitchAccount,
  (ui: Ui) => ui.press(ui.t("dialog.switchAccount.all", { agent: "Claude Code" })),
  (ui: Ui) => ui.confirm(),
];

const removeProjectTrash: Scenario = {
  ...dialogFixture,
  id: "dialog-remove-project-trash",
  group: "Projects",
  category: "normal",
  title: "Remove project and move its folder to the Trash",
  description: "The Trash option is checked: the message and the hint under the option say the folder goes too, while the confirming button still reads \"Remove\". Under the option, the disclosure \"What goes with it (4)\" is closed, and the locked-worktree option \"Also move the locked worktrees to the Trash (4)\" follows as a plain block, unchecked, with its own closed Details. Under the options, the dialog's one warning has appeared because the Trash option is checked, with a line each for the worktrees moved to the Trash and the branches deleted permanently.",
  state: removalState,
  projectRemovalBranches: {
    [tagged[0].id]: [
      worktreeItemOf(tagged[0], docsSession, "feat/redesign", "/Users/dev/worktrees/docs-site-redesign"),
      branchItemOf(tagged[0], docsSession, "fix/footer-links"),
      branchItemOf(tagged[0], docsSession, "feat/search-box"),
      worktreeItemOf(tagged[0], null, null, "/Users/dev/worktrees/docs-site-scratch", "removable"),
      worktreeItemOf(tagged[0], docsSession, "feat/old-nav", "/Users/dev/worktrees/docs-site-old-nav", "locked"),
      worktreeItemOf(tagged[0], docsSession, "feat/new-footer", "/Users/dev/worktrees/docs-site-new-footer", "locked", true),
      worktreeItemOf(tagged[0], null, null, "/Users/dev/worktrees/docs-site-hotfix", "locked"),
      worktreeItemOf(tagged[0], null, null, "/Users/dev/worktrees/docs-site-spike", "locked", true),
      branchItemOf(tagged[0], docsSession, "feat/old-nav"),
    ],
  },
  steps: removeDocsSite,
};

const archiveConsoleSession: Scenario = {
  ...dialogFixture,
  id: "dialog-archive-console-session",
  group: "Archive view",
  category: "normal",
  title: "Archive console session and its bound sessions",
  description: "The confirmation offers two plain options, unchecked, each a checkbox with its count in the label, a short note and a closed Details disclosure: \"Also force-remove the worktrees and branches Git keeps (5)\" and \"Also delete the locked worktree\". Under them, the one warning lists the running manually bound session, the waiting lead and its interrupted child; nothing but the sessions is in it. Confirming stops and archives all of them together. The confirming button reads \"Archive\" in the normal colour until a deletion option is ticked.",
  sessionBranches: [
    worktreeItemOf(web, hubSessions.web, "feat/changelog-tidy", "/Users/dev/worktrees/website-changelog-tidy"),
    worktreeItemOf(web, hubSessions.web, null, "/Users/dev/worktrees/website-changelog-scratch"),
    worktreeItemOf(api, hubSessions.api, "feat/vendored-parser", "/Users/dev/worktrees/search-api-vendored-parser", "submodules"),
    worktreeItemOf(api, hubSessions.api, null, "/Users/dev/worktrees/search-api-fixtures", "submodules"),
    branchItemOf(api, hubSessions.api, "fix/rename-index"),
    worktreeItemOf(api, hubSessions.review, "feat/migration-review", "/Users/dev/worktrees/search-api-migration-review", "locked", true),
  ],
  steps: [
    (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Hub 4" })),
    (ui) => ui.press(ui.t("sidebar.session.archive")),
  ],
};

// An idle session whose turn ended with a dev server still running, beside one sitting at its prompt
// with nothing left running, which archives without asking.
const watcher = sessionOf("s-web-watch", console_.id, web.id, "Serve the preview", "idle", { background_tasks: [{ id: "task-dev", type: "shell", description: "pnpm dev" }] });
const backgroundArchiveState = snapshotState({
  consoles: [console_],
  projects: [web],
  sessions: [watcher, sessionOf("s-web-quiet", console_.id, web.id, "Fix the summary layout", "idle")],
});

export const dialogScenarios: Scenario[] = [
  {
    id: "sidebar-session-menu-unused",
    group: "Sidebar sessions",
    category: "normal",
    title: "Menu of a session that was never used",
    description: "Copy session ID sits directly after Rename, and the last entry is the destructive Delete in place of Archive. Close the menu and hover or focus the unused row: its Delete shortcut sits immediately before the actions menu, and the untouched Claude Code, Codex and Grok sessions all delete immediately, moving any native records to the Trash without confirmation.",
    state: unusedState,
    sessionRecords: { [unusedSession.id]: "absent", [unusedCodex.id]: "unknown", [unusedGrok.id]: "present" },
    steps: [openUnusedMenu],
  },
  {
    id: "dialog-switch-account",
    group: "Sidebar sessions",
    category: "normal",
    title: "Switch account confirmation",
    description: "The console-wide checkbox starts unchecked. Selecting it includes the console hub, independent project sessions and bound sessions of the same agent; archived sessions and sessions already on the target account are omitted.",
    state: switchAccountState,
    steps: openSwitchAccount,
  },
  {
    id: "dialog-switch-account-progress",
    group: "Sidebar sessions",
    category: "normal",
    title: "Console account switch in progress",
    description: "Two of three sessions have finished; the determinate bar and current session remain visible while the third switch is pending. Controls stay locked until switching completes.",
    state: switchAccountState,
    sessionAccountSwitches: { "s-switch-bound": "pending" },
    steps: submitConsoleSwitch,
  },
  {
    id: "dialog-switch-account-partial-failure",
    group: "Sidebar sessions",
    category: "exceptional",
    title: "Console account switch partial failure",
    description: "The completed batch reports a full disk and an unexpected internal failure together. Each row keeps its own cause and guidance; the unexpected item also enables diagnostics and Report issue. After acknowledgement, Retry targets only failed sessions and preserves the successful switch.",
    state: switchAccountState,
    externalLinks: "available",
    sessionAccountSwitches: {
      "s-switch-bound": {
        code: "internal_error",
        params: { detail: "Unexpected session state while applying the account update" },
        message: "Unexpected session state while applying the account update",
      },
      "s-switch-work": {
        code: "relocation_failed",
        params: { path: "/Users/dev/.claude-work/projects/-Users-dev-code-website", detail: "No space left on device (os error 28)" },
        message: "The conversation could not be copied into /Users/dev/.claude-work/projects/-Users-dev-code-website: No space left on device (os error 28)",
      },
    },
    steps: submitConsoleSwitch,
  },
  {
    id: "dialog-switch-account-shared-recovery",
    group: "Sidebar sessions",
    category: "exceptional",
    title: "Console account switch, every failure shares one remedy",
    description: "Both failed sessions say the target account no longer exists, so the alert's footer carries a Manage accounts… button, left of OK, and the failure rows carry no button of their own. Compare the partial failure above, whose different remedies leave the footer without one.",
    state: switchAccountState,
    sessionAccountSwitches: {
      "s-switch-bound": { code: "unknown_account", params: { account: "a-removed" }, message: "unknown account a-removed" },
      "s-switch-work": { code: "unknown_account", params: { account: "a-removed" }, message: "unknown account a-removed" },
    },
    steps: submitConsoleSwitch,
  },
  {
    ...dialogFixture,
    state: consoleSessionLaunchState,
    id: "dialog-new-console-session",
    group: "Sidebar sessions",
    category: "normal",
    title: "New console session",
    description: "The empty Name field previews the console name followed by the localized session label, without a numeric suffix. Joined agent and inherited account pickers follow; Advanced starts collapsed and there is no report-to field.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.consoleSessions.new")),
    ],
  },
  {
    ...dialogFixture,
    state: consoleSessionLaunchState,
    id: "dialog-new-console-session-advanced",
    group: "Sidebar sessions",
    category: "normal",
    title: "New console session advanced settings",
    description: "Advanced shows the console's Opus model and high reasoning effort for Claude Code; changes apply to this new session.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.consoleSessions.new")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-new-console-accounts",
    group: "Consoles",
    category: "normal",
    title: "New console with accounts",
    description: "Project sessions start on Same as console sessions, naming Claude Code beside its faded icon. The \"Agent default accounts\" section has the same picker on every agent's row: Claude Code, which has named accounts, offers them with its default account under the name the user gave it (Studio laptop); Codex and Grok Build have only their default account, so their pickers hold that one option, Codex under its renamed name (Team seat) and Grok Build as \"Default account\". One icon-only key button named \"Manage accounts\" (with a tooltip) sits beside the section title.",
    state: { ...dialogState, settings: { ...dialogState.settings!, default_account_names: { claude: "Studio laptop", codex: "Team seat" } } },
    steps: [
      (ui) => ui.press(ui.t("rail.newConsole")),
    ],
  },
  {
    ...dialogFixture,
    state: { ...dialogState, settings: { ...accountsState.settings!, accounts: [] } },
    id: "dialog-new-console-advanced",
    group: "Consoles",
    category: "normal",
    title: "New console advanced settings",
    description: "Without named accounts every agent in the \"Agent default accounts\" section has a picker holding only its default account, as \"Default account\", with the icon-only key button named \"Manage accounts\" beside the section title. Advanced opens on the console sessions' agent among tabs for every agent, each with a Console sessions and a Project sessions group of model and reasoning effort settings, and at its foot the empty Console session instructions field with its placeholder and note. No line about each agent having its own models sits above the tabs.",
    steps: [
      (ui) => ui.press(ui.t("rail.newConsole")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    state: mixedState(["codex"]),
    id: "dialog-edit-console-agents",
    group: "Consoles",
    category: "normal",
    title: "Console settings with differing default agents",
    description: "Console sessions run on Claude Code and project sessions on Codex, which is no longer installed: that picker keeps it, with a grey Not installed badge after the name. The \"Agent default accounts\" rows sit on the same two columns as the Default agent pickers above, each agent's name on the left and its account picker on the right: Claude Code (with its named accounts), Grok Build (only \"Default account\") and Codex with the badge, which stays because the console still pins its account. In Advanced the agent tabs include Codex with the badge, which stays while the console stores a project session model for it.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    state: mixedState(["codex", "grok"]),
    id: "dialog-edit-console-project-agent-picker",
    group: "Consoles",
    category: "normal",
    title: "Project sessions agent choices",
    description: "The list offers Same as console sessions, Claude Code, and Codex as the held value, with a grey Not installed badge after its name in the option as well as in the field; Grok Build, also missing, is left out.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.edit")),
      (ui) => ui.press(ui.t("dialog.console.defaultAgentProject")),
    ],
  },
  {
    ...dialogFixture,
    state: withAddedAgents(dialogState, ["codex", "claude"]),
    id: "dialog-new-console-added-agents",
    group: "Consoles",
    category: "normal",
    title: "New console with only some agents added",
    description: "Codex was added before Claude Code and Grok Build is not added. The open Project sessions list offers Same as console sessions, then Codex and Claude Code; the \"Agent default accounts\" rows and the agent tabs under Advanced follow the same order, and Grok Build appears nowhere.",
    steps: [
      (ui) => ui.press(ui.t("rail.newConsole")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
      (ui) => ui.press(ui.t("dialog.console.defaultAgentProject")),
    ],
  },
  {
    ...dialogFixture,
    state: removedAgentsState,
    id: "dialog-edit-console-agents-not-added",
    group: "Consoles",
    category: "normal",
    title: "Console settings with agents that are no longer added",
    description: "Only Claude Code is added. Console sessions still run on Grok Build and project sessions on Codex, so both pickers hold their agent with a grey Not added badge after the name. The \"Agent default accounts\" rows list Claude Code first, then Codex and Grok Build badged the same way and dimmed (the name in the secondary colour, the icon faded, against Claude Code's foreground-coloured name), because the console still pins an account for each; Advanced does the same for the agent tabs, which keep Codex and Grok Build while the console stores a model for each.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    state: { ...dialogState, agentAvailability: detected(["codex"]) },
    id: "dialog-new-console-manage-accounts",
    group: "Consoles",
    category: "normal",
    title: "Manage accounts from the console dialog",
    description: "Settings opens on Agent accounts, reached from the icon-only key button named \"Manage accounts\" beside the title of the new console dialog's default-accounts section.",
    steps: [
      (ui) => ui.press(ui.t("rail.newConsole")),
      (ui) => ui.press(ui.t("dialog.console.manageAccounts")),
    ],
  },
  {
    ...dialogFixture,
    state: { ...dialogState, consoles: new Map([...dialogState.consoles, [console_.id, { ...dialogState.consoles.get(console_.id)!, console_session_prompt: SESSION_INSTRUCTIONS }]]) },
    id: "dialog-edit-console-launch-settings",
    group: "Consoles",
    category: "normal",
    title: "Console settings with avatar, stored launch settings and session instructions",
    description: "The Claude Code tab holds different values for its two groups: the retired console session model is retained and marked, while project sessions have Sonnet at Low. The avatar and its remove control are visible. At the foot of Advanced, the Console session instructions field holds the console's own multi-line text, longer than the field is tall, so it scrolls inside the field.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-delete-console",
    group: "Consoles",
    category: "normal",
    title: "Delete console confirmation",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.delete")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project",
    group: "Projects",
    category: "normal",
    title: "Add project",
    description: "Description follows Name, empty, with the placeholder No description and a note under it saying what it is for.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-invalid",
    group: "Projects",
    category: "exceptional",
    title: "Required project fields",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press(ui.t("common.add")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-new-session-shell-timeout",
    group: "Sidebar sessions",
    category: "exceptional",
    title: "Session launch, shell environment timed out",
    description: "The danger alert presents the failed shell command, cause, launch consequence and recovery as separate readable parts. Short shows the recovery at the bottom of the scrolling body.",
    sessionOpenError: SHELL_TIMEOUT,
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.focus(ui.t("common.name")),
      (ui) => ui.confirm(),
      async () => { document.querySelector<HTMLElement>('[role="dialog"] [role="alert"]')?.scrollIntoView({ block: "end" }); },
    ],
  },
  {
    ...dialogFixture,
    state: sessionLaunchState,
    id: "dialog-new-session-advanced",
    group: "Sidebar sessions",
    category: "normal",
    title: "New session advanced settings",
    description: "New conversation is selected with Name below the tabs; its placeholder combines the project name and localized session label. Claude Code and its long inherited account name share the joined field; expanded Advanced shows the project's Opus model with the console's Low project session reasoning effort, with Report to still offered above.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    state: sessionLaunchState,
    id: "dialog-new-session-resume-invalid",
    group: "Sidebar sessions",
    category: "exceptional",
    title: "Session resume, conversation ID not valid",
    description: "On the Resume conversation tab, a session title typed instead of an ID is marked invalid under the Session ID field once Open is pressed, with an example ID as code in the danger tone, the field focused and Report to gone; the dialog stays as typed and nothing is sent.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.press(ui.t("dialog.session.conversationResume")),
      (ui) => ui.focus(ui.t("dialog.session.resumeId")),
      (ui) => ui.type("my-title"),
      (ui) => ui.confirm(),
    ],
  },
  {
    ...dialogFixture,
    state: sessionLaunchState,
    sessionOpenError: {
      code: "conversation_to_resume_not_found",
      params: { agent: "Claude Code", path: "/Users/dev/.claude-personal", id: RESUME_ID },
      message: "no conversation `" + RESUME_ID + "` of Claude Code was found in `/Users/dev/.claude-personal`",
    },
    id: "dialog-new-session-resume-not-found",
    group: "Sidebar sessions",
    category: "exceptional",
    title: "Session resume, conversation not found",
    description: "On the Resume conversation tab with a session ID filled in and Report to gone, the refusal stands under the Session ID field, which is focused: no conversation with this ID is in the account's directory, the directory path set as code in the danger tone and the typed ID not repeated. The dialog has no alert at its foot.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.press(ui.t("dialog.session.conversationResume")),
      (ui) => ui.focus(ui.t("dialog.session.resumeId")),
      (ui) => ui.type(RESUME_ID),
      (ui) => ui.confirm(),
    ],
  },
  {
    ...dialogFixture,
    state: {
      ...sessionLaunchState,
      projects: new Map([...sessionLaunchState.projects].map(([id, project]) => [id, { ...project, default_agent: "codex" as const }])),
    },
    id: "dialog-new-session-agent-only",
    group: "Sidebar sessions",
    category: "normal",
    title: "New session without named agent accounts",
    description: "Codex has no named accounts, so the agent picker fills the whole field. Advanced shows its project model and xhigh reasoning effort; choosing Claude Code reveals its joined account picker.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    state: withAddedAgents(sessionLaunchState, ["codex", "claude"]),
    id: "dialog-new-session-added-agents",
    group: "Sidebar sessions",
    category: "normal",
    title: "New session with only some agents added",
    description: "The open Agent list offers Codex then Claude Code, the order the agents were added in; Grok Build is not added and is absent. The session opens on the project's own agent, Claude Code.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.press(ui.t("dialog.session.agent")),
    ],
  },
  {
    ...dialogFixture,
    state: withAddedAgents(sessionLaunchState, ["claude", "codex", "grok"]),
    id: "dialog-new-session-no-agent-added",
    group: "Sidebar sessions",
    category: "exceptional",
    title: "New session dialog open as the last agent is removed",
    description: "Reached only when the agents change under an open dialog (the buttons that open it are disabled without an added agent): every agent is removed meanwhile. The dialog closes itself and a plain error alert says no agent is available to open a session; OK is its only button.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.setAgents({ added: [] }),
    ],
  },
  {
    ...dialogFixture,
    state: sessionLaunchState,
    id: "dialog-new-session-agent-not-added",
    group: "Sidebar sessions",
    category: "exceptional",
    title: "Session launch refused, the agent is not added",
    description: "The daemon finds Claude Code off the added list, which the window has not heard yet. The danger alert says the session cannot be opened with it, with the remedy in words and an Add agent… button in the footer, left of OK; the dialog stays behind it.",
    sessionOpenError: { code: "agent_not_added", params: { agent: "Claude Code" }, message: "Claude Code is not added. Add it in Settings → Agent accounts to open a session with it." },
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.confirm(),
    ],
  },
  {
    ...dialogFixture,
    state: sessionLaunchState,
    id: "dialog-new-session-account-refused",
    group: "Sidebar sessions",
    category: "exceptional",
    title: "Session launch refused, the account is gone",
    description: "The daemon finds the account the session was to run under removed. The danger alert says so, with the remedy in words and a Manage accounts… button in the footer, left of OK; the dialog stays behind it.",
    sessionOpenError: { code: "unknown_account", params: { account: "a-removed" }, message: "unknown account a-removed" },
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.confirm(),
    ],
  },
  {
    ...dialogFixture,
    state: sessionLaunchState,
    id: "dialog-new-session-owner",
    group: "Sidebar sessions",
    category: "normal",
    title: "New session owner choices",
    description: "Short and long owner names appear together below the joined agent and account field. Hover the long option to read it as a marquee; the selected long account label uses the same fade.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.press(ui.t("dialog.session.ownerNone")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-remove-project-running",
    group: "Projects",
    category: "normal",
    title: "Remove project with running sessions",
    description: "The confirmation as first opened: the Trash option is unchecked, and the confirming button reads \"End and remove\" because sessions are running. Default, named and removed-directory account cases coexist in the running-sessions list; hover each agent mark.",
    steps: [
      (ui) => projectActions(ui, web.name),
      (ui) => ui.press(ui.t("sidebar.project.remove")),
    ],
  },
  removeProjectTrash,
  {
    ...removeProjectTrash,
    id: "dialog-remove-project-trash-expanded",
    title: "Remove project to the Trash, lists opened",
    description: "Both disclosures are open (What goes with it, Details), and the locked-worktree option is ticked: under the Trash option, the worktrees moved to the Trash with the folder (one on a detached HEAD) and the branches deleted permanently; under the locked option, the four locked worktrees in each wording, those with uncommitted changes ending \"uncommitted changes\". The one warning, under the options, gains a line for the locked worktrees moved to the Trash, and the locked worktree's branch, now going with it, joins the branches.",
    steps: [...removeDocsSite, (ui) => ui.press(ui.t("branchCleanup.along", { count: 4 })), (ui) => ui.pressEach(ui.t("branchCleanup.details")), (ui) => ui.press(ui.t("dialog.removeProject.alongLockedRemove", { count: 4 }))],
  },
  {
    ...dialogFixture,
    projectRemovalBranches: {
      [tagged[0].id]: { code: "internal_error", params: { detail: "git could not be run" }, message: "git could not be run" },
    },
    id: "dialog-remove-project-trash-inspection-failed",
    group: "Projects",
    category: "exceptional",
    title: "Remove project to the Trash, what goes along unknown",
    description: "The Trash option is checked but the daemon could not say what else goes: the dialog's one warning, under the option, appears with the single line that outside worktrees may be moved to the Trash with the project and its branches deleted permanently, with no disclosure or locked-worktree option. Confirm stays available.",
    steps: removeDocsSite,
  },
  {
    ...dialogFixture,
    projectDeleteError: {
      code: "project_directory_shared",
      params: { project: tagged[1].name },
      message: "The folder is shared with another project",
    },
    // The inspection is refused for the same reason, so the dialog lists nothing and shows no warning.
    projectRemovalBranches: {
      [tagged[0].id]: { code: "project_directory_shared", params: { project: tagged[1].name }, message: "The folder is shared with another project" },
    },
    id: "dialog-remove-project-folder-shared",
    group: "Projects",
    category: "exceptional",
    title: "Project folder shared with another project",
    description: "With Trash checked the dialog lists nothing and shows no warning about what goes along. The daemon refuses to move the folder to the Trash; the error shows in the dialog and nothing is changed.",
    steps: [...removeDocsSite, (ui) => ui.confirm()],
  },
  {
    ...dialogFixture,
    projectDeleteError: {
      code: "project_directory_protected",
      params: { path: tagged[0].path },
      message: "This folder is too important to move to the Trash from here",
    },
    projectRemovalBranches: {
      [tagged[0].id]: { code: "project_directory_protected", params: { path: tagged[0].path }, message: "This folder is too important to move to the Trash from here" },
    },
    id: "dialog-remove-project-folder-protected",
    group: "Projects",
    category: "exceptional",
    title: "Project folder protected from the Trash",
    description: "With Trash checked the dialog lists nothing and shows no warning about what goes along. The daemon refuses to move a protected folder (the home directory, the root of a disk, or one holding Octoboard's data); a separate error alert names why, and the project is kept.",
    steps: [...removeDocsSite, (ui) => ui.confirm()],
  },
  {
    ...dialogFixture,
    projectDeleteError: {
      code: "project_directory_trash_failed",
      params: { path: tagged[0].path, detail: "Operation not permitted (os error 1)" },
      message: "The folder could not be moved to the Trash",
    },
    id: "dialog-remove-project-trash-failed",
    group: "Projects",
    category: "exceptional",
    title: "Project folder could not be moved to the Trash",
    description: "The move to the Trash failed; a separate error alert requires acknowledgement and the project is kept.",
    steps: [...removeDocsSite, (ui) => ui.confirm()],
  },
  archiveConsoleSession,
  {
    ...archiveConsoleSession,
    id: "dialog-archive-console-session-expanded",
    title: "Archive console session, lists opened",
    description: "Both Details disclosures are open and both options ticked: under the force-remove option, the worktrees with uncommitted changes and with initialized submodules (each with its branch chip, or plain detached HEAD text) and the unmerged branch, each group under its lead line; under the locked option, the locked worktree with its row ending \"uncommitted changes\". The one warning, under the options and still holding the sessions, gains a line for each ticked option saying it also permanently deletes them. The confirming button reads \"Archive and delete\" in the danger colour because deletion options are ticked.",
    steps: [
      ...archiveConsoleSession.steps!,
      (ui) => ui.pressEach(ui.t("branchCleanup.details")),
      (ui) => ui.press(ui.t("dialog.sessionBranches.force", { count: 5 })),
      (ui) => ui.press(ui.t("dialog.lockedWorktrees.remove", { count: 1 })),
    ],
  },
  {
    ...dialogFixture,
    sessionBranches: "pending",
    id: "dialog-archive-console-session-inspecting",
    group: "Archive view",
    category: "normal",
    title: "Archive confirmation while the bound branches are inspected",
    description: "The sessions warning is shown at once, but Confirm is pending and cannot be pressed until the inspection of bound branches and worktrees answers, so nothing is archived before the user has seen what would be left. The same hold applies to every archive, delete and remove-project confirmation.",
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Hub 4" })),
      (ui) => ui.press(ui.t("sidebar.session.archive")),
    ],
  },
  {
    id: "dialog-archive-session-background",
    group: "Archive view",
    category: "normal",
    title: "Archive a session running in the background",
    description: "The session is idle, yet archiving it asks first, as it does a working one, and the message adds that the work it is running in the background is stopped: Its process ends, and the work it is running in the background is stopped. You can reopen it later. The row behind the dialog shows the pinging hollow green ring, listed below the idle Fix the summary layout as a working session is; that one, with nothing running, has an Archive shortcut that archives at once, and this one has none.",
    state: backgroundArchiveState,
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: watcher.title })),
      (ui) => ui.press(ui.t("sidebar.session.archive")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-archive-session-failed",
    group: "Archive view",
    category: "exceptional",
    title: "Archive failed after confirmation",
    description: "The danger error alert identifies the full disk and tells the user to free space before retrying. After OK, the archive confirmation remains open and usable, with the original sessions intact.",
    sessionArchiveError: { code: "store_io_failed", params: { detail: "No space left on device (os error 28)" }, message: "No space left on device (os error 28)" },
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Hub 4" })),
      (ui) => ui.press(ui.t("sidebar.session.archive")),
      (ui) => ui.confirm(),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-edit-project-description",
    group: "Projects",
    category: "normal",
    title: "Project settings with a description",
    description: "Description sits right after Name, holding two lines within the field's three rows; the note under it says agent sessions read it.",
    steps: editTaggedProject,
  },
  {
    ...dialogFixture,
    id: "dialog-edit-project-long-description",
    group: "Projects",
    category: "normal",
    title: "Project fields approaching their character limits",
    description: "Name shows 90 / 100 and Description 1800 / 2000. The long description keeps its three-row height and scrolls inside, with its note, counter and Tags still below it.",
    state: textProjectState(nearLimitProject),
    steps: editTextProject(nearLimitProject),
  },
  {
    ...dialogFixture,
    id: "dialog-edit-project-legacy-text",
    group: "Projects",
    category: "normal",
    title: "Existing project fields beyond the character limits",
    description: "Both counters exceed their limits, but unchanged stored text has no validation error. Other project settings remain editable without shortening the original name or description.",
    state: textProjectState(legacyProject),
    steps: editTextProject(legacyProject),
  },
  {
    ...dialogFixture,
    id: "dialog-edit-project-text-too-long",
    group: "Projects",
    category: "exceptional",
    title: "Project name and description beyond their limits",
    description: "After Save, both fields show their own length errors and counters together. The full input remains editable.",
    state: textProjectState(nearLimitProject),
    steps: [
      ...editTextProject(nearLimitProject),
      (ui) => ui.focus(ui.t("common.name")),
      (ui) => ui.type(`${nearLimitProject.name} extended name`),
      (ui) => ui.focus(ui.t("dialog.project.description")),
      (ui) => ui.type(legacyProject.description!),
      (ui) => ui.confirm(),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-rename-project-text-too-long",
    group: "Projects",
    category: "exceptional",
    title: "Rename project beyond the name limit",
    description: "The compact rename dialog shows the name error and character counter after Save, keeping the complete name editable.",
    steps: [
      (ui) => projectActions(ui, tagged[0].name),
      (ui) => ui.press(ui.t("sidebar.project.rename")),
      (ui) => ui.focus(ui.t("common.name")),
      (ui) => ui.type(legacyProject.name),
      (ui) => ui.confirm(),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-edit-project-tag-suggestions",
    group: "Projects",
    category: "normal",
    title: "Project tag suggestions",
    description: "Short and long tags share the field with the input. The short tag fits its content until hover or keyboard focus makes room for its remove button; unused project tags appear below. Pick a suggestion or type a new tag and press Enter, then remove a chip to see the chips fade and slide between wrapped lines.",
    steps: [
      ...editTaggedProject,
      (ui) => ui.press(ui.t("dialog.project.tagsSuggestions")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-edit-project-launch-settings",
    group: "Projects",
    category: "normal",
    title: "Stored project account and launch settings",
    description: "Long selected account and retained model labels fade at the field edge and scroll on hover. Open either picker to inspect the matching option labels.",
    steps: [
      (ui) => projectActions(ui, configured.name),
      (ui) => ui.press(ui.t("sidebar.project.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    state: {
      ...dialogState,
      consoles: new Map([...dialogState.consoles, [console_.id, { ...dialogState.consoles.get(console_.id)!, claude_project_session_effort: null }]]),
    },
    id: "dialog-edit-project-inherited-launch-settings",
    group: "Projects",
    category: "normal",
    title: "Project settings inheriting console launch settings",
    description: "Model inherits the console's project session value, named in its label; Reasoning effort, which the console leaves to the agent, says it inherits the agent's own setting.",
    steps: [
      (ui) => projectActions(ui, onDefault.name),
      (ui) => ui.press(ui.t("sidebar.project.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-edit-project-account-choices",
    group: "Projects",
    category: "normal",
    title: "Project account choices",
    description: "Inherited, default and long named accounts coexist. Hover a clipped account option to reveal its text as a marquee.",
    steps: [
      (ui) => projectActions(ui, onDefault.name),
      (ui) => ui.press(ui.t("sidebar.project.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.account", { agent: "Claude Code" })),
    ],
  },
  {
    ...dialogFixture,
    state: {
      ...snapshotState({
        consoles: [{ ...console_, project_session_agent: "codex" as const }],
        projects: [web, api],
        sessions,
        settings: accountsState.settings,
      }),
      agentAvailability: detected(["grok"]),
    },
    id: "dialog-add-project-inherit-project-agent",
    group: "Projects",
    category: "normal",
    title: "Add project in a console with its own project session agent",
    description: "Inherit from console names Codex, the console's project session agent rather than its Claude Code console sessions, beside a faded Codex icon. Grok Build, missing, is not offered.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press(ui.t("dialog.project.defaultAgent")),
    ],
  },
  {
    ...dialogFixture,
    state: (() => {
      const base = withAddedAgents(dialogState, ["codex", "claude"]);
      return { ...base, projects: new Map([...base.projects, [removedAgentProject.id, removedAgentProject]]) };
    })(),
    id: "dialog-edit-project-agent-not-added",
    group: "Projects",
    category: "normal",
    title: "Project default agent with only some agents added",
    description: "The open Default agent list offers Codex then Claude Code, the order the agents were added in, after Inherit from console and Detect from files. The project still names Grok Build, which is not added, so it stays selected at the foot, with a grey Not added badge after its name in the option as well as in the field, and unselectable.",
    steps: [
      (ui) => projectActions(ui, removedAgentProject.name),
      (ui) => ui.press(ui.t("sidebar.project.edit")),
      (ui) => ui.press(ui.t("dialog.project.defaultAgent")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-directory-picker",
    group: "Projects",
    category: "normal",
    title: "Directory picker",
    description: "Short directory names and a long directory row share the list; the long name fades and runs as a marquee on hover while its folder icon stays visible.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press(ui.t("common.browse")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-checking",
    group: "Projects",
    category: "normal",
    title: "Directory detection pending",
    description: "While the directory is checked, Name and Description are disabled along with the fields below them.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.focus(ui.t("dialog.project.directory")),
      (ui) => ui.type("~/code/slow"),
      (ui) => ui.wait(700),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-detected",
    group: "Projects",
    category: "normal",
    title: "Detected project agent",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.focus(ui.t("dialog.project.directory")),
      (ui) => ui.type("~/code/site"),
      (ui) => ui.wait(700),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-path-refused",
    group: "Projects",
    category: "exceptional",
    title: "Project path refused",
    description: "The refusal stands under the Directory field with the missing path as code in the danger tone.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.focus(ui.t("dialog.project.directory")),
      (ui) => ui.type("~/code/missing"),
      (ui) => ui.wait(700),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-parent",
    group: "Projects",
    category: "normal",
    title: "Parent directory source",
    description: "Default agent is a single field with a \"?\" at the end of its label and the visible line that an agent chosen here applies to every repository; the way Detect from files decides is in the tip. There are no Name and Description fields: both apply to a single project only.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.local"))),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.parent"))),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-parent-detect-help",
    group: "Projects",
    category: "normal",
    title: "Parent directory source, Detect help popover open",
    description: "The \"?\" at the end of Default agent has been pressed, so its popover stays open: Detect from files is a chip within the sentence, and CLAUDE.md, AGENTS.md, .claude and .codex are set as code in chips; the sentence ends with what is inherited from the console.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.local"))),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.parent"))),
      (ui) => ui.press(ui.t("dialog.project.agentDetectHelp")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-parent-account",
    group: "Projects",
    category: "normal",
    title: "Parent directory with a chosen agent and account",
    description: "The line that an agent chosen here applies to every repository remains below the joined agent and account fields when Detect from files is replaced by a chosen agent, with the \"?\" still at the end of the label.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.local"))),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.parent"))),
      (ui) => ui.press(ui.t("dialog.project.defaultAgent")),
      (ui) => ui.press("Claude Code"),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-git-unreachable",
    group: "Projects",
    category: "exceptional",
    title: "Unreadable Git source",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.local"))),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.git"))),
      (ui) => ui.focus(ui.t("dialog.project.repositoryUrl")),
      (ui) => ui.type("git@example.com:o/private.git"),
      (ui) => ui.wait(1100),
    ],
  },
  {
    id: "dialog-new-console-models-loading",
    group: "Consoles",
    category: "normal",
    title: "New console while the model list is out",
    description: "The model and effort pickers are locked and the model field says the list is loading.",
    state: dialogState,
    agentModels: { claude: "pending" },
    steps: [
      (ui) => ui.press(ui.t("rail.newConsole")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    id: "dialog-edit-console-models-unavailable",
    group: "Consoles",
    category: "exceptional",
    title: "Edit a console whose agent cannot list its models",
    description: "The daemon found no binary for the agent, so the model picker offers the stored model alone and says why; the effort picker still works over what is stored.",
    state: dialogState,
    agentModels: {
      claude: {
        models: [],
        unavailable: {
          code: "models_binary_not_found",
          params: { binary: "claude" },
          message: "claude was not found on the login shell's PATH",
        },
      },
    },
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
];
