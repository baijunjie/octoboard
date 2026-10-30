// The gallery's "Configuration" scenarios: exporting a console or everything, and importing a file
// through its three steps (choosing it, reviewing what it holds, the result). What the daemon would
// work out from the filesystem — the offers of a project, the outcome of a clone — is written out
// here by hand, and the test beside this file holds those answers to the rules the daemon follows.
import type { Imported } from "../../configuration/ImportResult";
import type { Inspection } from "../../configuration/importPlan";
import { DEFAULT_SETTINGS } from "../../defaultSettings";
import { DEFAULT_GIT_WORKFLOW } from "../../gitWorkflow";
import {
  DEFAULT_ACCOUNT_NAME,
  type AccountFallback,
  type Agent,
  type DocumentSettings,
  type GitWorkflowSettings,
  type ImportedAccount,
  type ImportedAccounts,
  type InspectedAccount,
  type InspectedAccounts,
  type InspectedProject,
  type InspectedSettings,
} from "../../protocol";
import type { Ui } from "../interact";
import type { ConfigurationFixture, Scenario, Step } from "../scenario";
import { accountOf, ALL_AGENTS, SAMPLE, consoleOf, effectiveClaudeCompatibility, projectOf, snapshotState } from "./builders";
import type { FixtureError } from "./projectFiles";

const GROUP = "Configuration";

const { console: console_, web, api, sessions } = SAMPLE;

/** The machine's default clone directory, which every offer of a clone or of a derived directory
 * lies under. */
export const DEFAULT_CLONE_DIR = "/Users/dev/Projects";

/** The accounts the machine has. A document's account that is one of these is rebound to it, so
 * none of what a document names as falling back to the default account is among them. */
const accounts = [
  accountOf("a-studio", "claude", "Studio", "/Users/dev/.claude-studio"),
  accountOf("a-home", "codex", "Home", "/Users/dev/.codex-home"),
];

// Two consoles, so the export has something to choose between, one of them naming an account. The
// inspections below say the application setting is off now; `autoPulling` is the machine where it is
// on.
const second = consoleOf("c-docs", "Documentation", { accounts: { claude: "a-studio" }, created_at: console_.created_at + 1 });
const layout = {
  consoles: [console_, second],
  projects: [
    { ...web, remote_url: "git@github.com:acme/website.git", tags: ["frontend"] },
    api,
    projectOf("p-guide", second.id, "Style Guide", { accounts: { claude: { kind: "account" as const, id: "a-studio" } } }),
  ],
  sessions,
};
const state = snapshotState({
  ...layout,
  settings: { ...DEFAULT_SETTINGS, added_agents: ALL_AGENTS, default_clone_dir: DEFAULT_CLONE_DIR, accounts },
});
const autoPulling = snapshotState({
  ...layout,
  settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: true, added_agents: ALL_AGENTS, default_clone_dir: DEFAULT_CLONE_DIR, accounts },
});
// The machine the accounts of a document are imported on: Grok is not among its added agents, and its default account
// already has a name of its own.
const accountsMachine = snapshotState({
  ...layout,
  settings: {
    ...DEFAULT_SETTINGS,
    added_agents: ["claude", "codex"],
    default_clone_dir: DEFAULT_CLONE_DIR,
    accounts,
    default_account_names: { grok: "Main" },
  },
});
const noConsoleState = snapshotState({
  consoles: [],
  projects: [],
  sessions: [],
  settings: { ...DEFAULT_SETTINGS, added_agents: ALL_AGENTS, default_clone_dir: DEFAULT_CLONE_DIR, accounts },
});

const openSettings = (ui: Ui) => ui.press(ui.t("rail.settings"));
const configurationSection = (ui: Ui) => ui.press(ui.t("settings.section.configuration"));
const exportFromSettings = [openSettings, configurationSection, (ui: Ui) => ui.press(ui.t("settings.configuration.export.button"))];
const importFromSettings = [
  openSettings,
  configurationSection,
  (ui: Ui) => ui.press(ui.t("settings.configuration.import.button")),
];
const chooseFile = (ui: Ui) => ui.press(ui.t("configuration.import.choose.submit"));
const importNow = (ui: Ui) => ui.press(ui.t("configuration.import.review.submit"));
// The "What to import" checkboxes stand above the tab bar and are named as the tabs are, so the tabs are pressed with
// `pressTab`, and the checkboxes with `press`, which finds them first.
const toggleConsoles = (ui: Ui) => ui.press(ui.t("configuration.import.include.consoles"));
const toggleAccounts = (ui: Ui) => ui.press(ui.t("configuration.import.include.accounts"));
const toggleSettings = (ui: Ui) => ui.press(ui.t("configuration.import.include.settings"));
const settingsTab = (ui: Ui) => ui.pressTab(ui.t("configuration.import.tab.settings"));
const accountsTab = (ui: Ui) => ui.pressTab(ui.t("configuration.import.tab.attention", { tab: ui.t("configuration.import.tab.accounts") }));
const consolesTab = (ui: Ui) => ui.pressTab(ui.t("configuration.import.tab.attention", { tab: ui.t("configuration.import.tab.consoles") }));
const leaveAccountOut = (account: string): Step[] => [
  (ui) => ui.press(ui.t("configuration.import.accounts.choice.label", { account })),
  (ui) => ui.press(ui.t("configuration.import.choice.skip")),
];
const hoverAccountMark = (key: "directoryInUseMark" | "invalidMark", account: string): Step => (ui) =>
  ui.hoverStatusMark(ui.t(`configuration.import.accounts.status.${key}`, { account }));
const importFromWelcome = [(ui: Ui) => ui.press(ui.t("welcome.importConfiguration")), chooseFile];
const exportScope = (scope: "all" | "none") => (ui: Ui) => ui.press(ui.t(`configuration.export.scope.${scope}`));
const toggleExportSettings = (ui: Ui) => ui.press(ui.t("configuration.export.settings.include"));
const toggleExportAccounts = (ui: Ui) => ui.press(ui.t("configuration.export.accounts.include"));
const toggleExportPaths = (ui: Ui) => ui.press(ui.t("configuration.export.paths.include"));

/** Puts an account of the review in a directory of the user's own, typed `path`. */
const placeAccountIn = (account: string, path: string): Step[] => [
  (ui) => ui.press(ui.t("configuration.import.accounts.choice.label", { account })),
  (ui) => ui.press(ui.t("configuration.import.choice.directory")),
  (ui) => ui.focus(ui.t("configuration.import.accounts.directory.label", { account })),
  (ui) => ui.type(path),
];

/** Puts a project in a directory of the user's own, typed `path`, in the review. */
const placeIn = (project: string, path: string): Step[] => [
  (ui) => ui.press(ui.t("configuration.import.project.choice", { project })),
  (ui) => ui.press(ui.t("configuration.import.choice.directory")),
  (ui) => ui.focus(ui.t("configuration.import.directory.label", { project })),
  (ui) => ui.type(path),
];

/** A reference to an account this machine has none by that name of. The inspection lists it whether or not the document's
 * own accounts would create one of that name: the review decides from the accounts and the user's choices whether it
 * resolves. */
const fallback = (agent: Agent, name: string): AccountFallback => ({ agent, name });

const project = (extra: Partial<InspectedProject> & Pick<InspectedProject, "name" | "path">): InspectedProject => ({
  remote_url: null,
  offers: { exported_path: null, default_clone_dir: null, clone: null },
  default: null,
  account_fallbacks: [],
  ...extra,
});

/** The git workflow settings the full export carries: not the ones this machine starts with. */
const FILE_GIT_WORKFLOW: GitWorkflowSettings = { development: "branch", squash_before_delivery: false, rebase_before_merge: false, delivery: "merge", revert_gate: true };

/** The Claude compatibility the full export carries: only what the user chose on the exporting machine, here Codex's off,
 * where this machine has every agent's on. */
const FILE_CLAUDE_COMPATIBILITY = { codex: false };

/** This machine's Claude compatibility as an import's review reports it: the values in effect for both agents. The
 * machines the document is set up on have every agent added and no explicit choice, so both are on. */
const THIS_CLAUDE_COMPATIBILITY = effectiveClaudeCompatibility({}, ALL_AGENTS);

/** An inspection of a document that carries no settings and no accounts unless `parts` gives them: the regions a document
 * does not carry are `null`, as the daemon sends them. */
function inspected(parts: Pick<Inspection, "consoles"> & Partial<Omit<Inspection, "type" | "consoles">>): Omit<Inspection, "type"> {
  return { id: null, settings: null, accounts: null, ...parts };
}

/** An import's result that applied no settings and no accounts unless `parts` says it did, likewise. */
function imported(parts: Pick<Imported, "consoles"> & Partial<Omit<Imported, "type" | "consoles">>): Omit<Imported, "type"> {
  return { id: null, settings_applied: null, settings_error: null, accounts: null, ...parts };
}

/** The settings of a document, which always carries every one: each switch off, the default git workflow and no Claude
 * compatibility choice, except what `carried` sets. */
function documentSettings(carried: Partial<DocumentSettings> = {}): DocumentSettings {
  return {
    auto_pull_repositories: false,
    auto_delete_unresumable_archives: false,
    auto_select_account: false,
    auto_switch_on_usage_limit: false,
    sidebar_order_by_status: false,
    auto_start_quota_windows: false,
    auto_approve_session_requests: false,
    auto_approve_resume_requests: false,
    auto_approve_account_switches: false,
    auto_approve_handoffs: false,
    ask_questions_as_choices: false,
    git_workflow: DEFAULT_GIT_WORKFLOW,
    claude_compatibility: {},
    ...carried,
  };
}

/** The settings an inspection says its document carries, without this machine's values beside them. */
export function carriedSettings(settings: InspectedSettings): DocumentSettings {
  return documentSettings(Object.fromEntries(Object.entries(settings).filter(([key]) => !key.startsWith("current_"))));
}

/** The settings that have a value of the document and one of this machine's in an inspection. */
const SETTING_KEYS = [
  "auto_pull_repositories",
  "auto_delete_unresumable_archives",
  "auto_select_account",
  "auto_switch_on_usage_limit",
  "sidebar_order_by_status",
  "auto_start_quota_windows",
  "auto_approve_session_requests",
  "auto_approve_resume_requests",
  "auto_approve_account_switches",
  "auto_approve_handoffs",
  "ask_questions_as_choices",
  "git_workflow",
] as const;
type SettingKey = (typeof SETTING_KEYS)[number];

/** An inspection's settings of a document whose every setting is the one this machine already has (the settings a fresh
 * installation starts with, and this machine's Claude compatibility) except what `carried` sets; the document carries no
 * Claude compatibility choice, so that line is not shown. */
function inspectedSettings(carried: Partial<InspectedSettings> = {}): InspectedSettings {
  const entries = (name: (key: SettingKey) => string) => SETTING_KEYS.map((key) => [name(key), DEFAULT_SETTINGS[key]]);
  return {
    ...documentSettings(Object.fromEntries(entries((key) => key))),
    ...(Object.fromEntries(entries((key) => `current_${key}`)) as { [K in SettingKey as `current_${K}`]: DocumentSettings[K] }),
    current_claude_compatibility: THIS_CLAUDE_COMPATIBILITY,
    ...carried,
  };
}

/** A document of a laptop's layout set up on a machine where it sits elsewhere: one project at its
 * recorded path, one found under the default clone directory, one to be cloned and one that nothing
 * places (whose recorded path is long enough to break over lines, and whose account falls back too),
 * a second console whose only project can be cloned, and one with no project. */
export const REVIEW = inspected({
  settings: {
    auto_pull_repositories: true,
    current_auto_pull_repositories: false,
    auto_delete_unresumable_archives: false,
    current_auto_delete_unresumable_archives: true,
    auto_select_account: true,
    current_auto_select_account: false,
    auto_switch_on_usage_limit: true,
    current_auto_switch_on_usage_limit: false,
    sidebar_order_by_status: false,
    current_sidebar_order_by_status: true,
    auto_start_quota_windows: true,
    current_auto_start_quota_windows: false,
    auto_approve_session_requests: true,
    current_auto_approve_session_requests: false,
    auto_approve_resume_requests: true,
    current_auto_approve_resume_requests: false,
    auto_approve_account_switches: true,
    current_auto_approve_account_switches: false,
    auto_approve_handoffs: true,
    current_auto_approve_handoffs: false,
    ask_questions_as_choices: false,
    current_ask_questions_as_choices: true,
    git_workflow: FILE_GIT_WORKFLOW,
    current_git_workflow: DEFAULT_GIT_WORKFLOW,
    claude_compatibility: FILE_CLAUDE_COMPATIBILITY,
    current_claude_compatibility: THIS_CLAUDE_COMPATIBILITY,
  },
  consoles: [
    {
      name: "Work laptop",
      account_fallbacks: [fallback("claude", "Work")],
      projects: [
        project({
          name: "Website",
          path: "/Users/ana/code/website",
          remote_url: "git@github.com:acme/website.git",
          offers: { exported_path: "/Users/ana/code/website", default_clone_dir: "/Users/dev/Projects/website", clone: null },
          default: "exported_path",
        }),
        project({
          name: "Search API",
          path: "/Users/ana/work/search-api",
          offers: { exported_path: null, default_clone_dir: "/Users/dev/Projects/search-api", clone: null },
          default: "default_clone_dir",
          account_fallbacks: [fallback("codex", "Personal")],
        }),
        project({
          name: "Design System",
          path: "/Users/ana/work/design-system",
          remote_url: "https://github.com/acme/design-system.git",
          offers: { exported_path: null, default_clone_dir: null, clone: "/Users/dev/Projects/design-system" },
          default: "clone",
        }),
        project({
          name: "Notes",
          path: "/Users/ana/Documents/Work/Clients/Northwind/Archive/2019/Quarterly-Reports/final-notes",
          account_fallbacks: [fallback("codex", "Personal")],
        }),
      ],
    },
    {
      name: "Side projects",
      account_fallbacks: [],
      projects: [
        project({
          name: "Dotfiles",
          path: "/Users/ana/dotfiles",
          remote_url: "https://github.com/ana/dotfiles.git",
          offers: { exported_path: null, default_clone_dir: null, clone: "/Users/dev/Projects/dotfiles" },
          default: "clone",
        }),
      ],
    },
    { name: "Scratch", account_fallbacks: [], projects: [] },
  ],
});

/** One console's document: no application settings; a project at its recorded path, one that can
 * only be cloned, and one nothing places. */
const SINGLE = inspected({
  consoles: [
    {
      name: "Documentation",
      account_fallbacks: [],
      projects: [
        project({
          name: "Style Guide",
          path: "/Users/dev/code/style-guide",
          offers: { exported_path: "/Users/dev/code/style-guide", default_clone_dir: null, clone: null },
          default: "exported_path",
        }),
        project({
          name: "Handbook",
          path: "/Users/ana/work/handbook",
          remote_url: "https://github.com/acme/handbook.git",
          offers: { exported_path: null, default_clone_dir: null, clone: `${DEFAULT_CLONE_DIR}/handbook` },
          default: "clone",
        }),
        project({ name: "Releases", path: "/Users/ana/work/releases" }),
      ],
    },
  ],
});

/** A console and projects whose names are long enough to wrap in the dialog, each with a mark after the name: the
 * console's own account falls back, one project is found and its account falls back, the other is not found. */
const LONG_NAMES = inspected({
  consoles: [
    {
      name: "Northwest regional distribution centres, quarterly planning and reporting workspace for the whole logistics group",
      account_fallbacks: [fallback("claude", "Work")],
      projects: [
        project({
          name: "Warehouse slotting optimisation service with the seasonal demand forecasting and replenishment rules",
          path: "/Users/ana/code/slotting",
          offers: { exported_path: "/Users/ana/code/slotting", default_clone_dir: null, clone: null },
          default: "exported_path",
          account_fallbacks: [fallback("codex", "Personal")],
        }),
        project({
          name: "Carrier contract archive and rate comparison tooling for the freight procurement team",
          path: "/Users/ana/work/carrier-contracts",
        }),
      ],
    },
  ],
});

/** A document exported without full project paths, which records only each directory's name: one
 * project found under the default clone directory, one to be cloned, and one nothing places. */
const NO_PATHS = inspected({
  consoles: [
    {
      name: "Work laptop",
      account_fallbacks: [],
      projects: [
        project({
          name: "Website",
          path: "website",
          remote_url: "git@github.com:acme/website.git",
          offers: { exported_path: null, default_clone_dir: `${DEFAULT_CLONE_DIR}/website`, clone: null },
          default: "default_clone_dir",
        }),
        project({
          name: "Design System",
          path: "design-system",
          remote_url: "https://github.com/acme/design-system.git",
          offers: { exported_path: null, default_clone_dir: null, clone: `${DEFAULT_CLONE_DIR}/design-system` },
          default: "clone",
        }),
        project({ name: "Notes", path: "notes" }),
      ],
    },
  ],
});

/** The same document with every setting the one this machine has, except the automatic pull, which is off where this
 * machine has it on. */
const WITH_SETTING_OFF: Omit<Inspection, "type"> = {
  ...SINGLE,
  settings: inspectedSettings({ auto_pull_repositories: false, current_auto_pull_repositories: true }),
};

const failure = (code: string, params: Record<string, string>, message: string) => ({ code, params, message });

/** What git says of a clone that failed: long, and with an address in it that has no place to break. */
const LONG_CLONE_DETAIL = [
  "Cloning into '/Users/dev/Projects/مرحبا/design-system'...",
  "fatal: unable to access 'https://github.com/acme/design-system.git/': Could not resolve host: github.com",
  "    retry failed after reading the proxy configuration",
].join("\n");

/** What importing `REVIEW` with its defaults came to: a project whose directory was gone by the time
 * it was imported, a clone that failed with a long reason, a project nothing placed, a console that
 * could not be created with its project not tried, and the account references that fell back. */
export const MIXED_RESULT = imported({
  settings_applied: { auto_pull_repositories: true, auto_delete_unresumable_archives: false, auto_select_account: true, auto_switch_on_usage_limit: true, sidebar_order_by_status: false, auto_start_quota_windows: true, auto_approve_session_requests: true, auto_approve_resume_requests: true, auto_approve_account_switches: true, auto_approve_handoffs: true, ask_questions_as_choices: false, git_workflow: FILE_GIT_WORKFLOW, claude_compatibility: FILE_CLAUDE_COMPATIBILITY },
  consoles: [
    {
      name: "Work laptop",
      status: "created",
      console: "c-new-1",
      error: null,
      account_fallbacks: [fallback("claude", "Work")],
      projects: [
        {
          name: "Website",
          status: "failed",
          via: null,
          project: null,
          error: failure(
            "path_not_found",
            { path: "/Users/ana/code/website" },
            "/Users/ana/code/website does not exist",
          ),
          account_fallbacks: [],
        },
        {
          name: "Search API",
          status: "created",
          via: "default_clone_dir",
          project: "p-new-2",
          error: null,
          account_fallbacks: [fallback("codex", "Personal")],
        },
        {
          name: "Design System",
          status: "failed",
          via: null,
          project: null,
          error: failure(
            "git_clone_failed",
            { detail: LONG_CLONE_DETAIL },
            `git clone failed: ${LONG_CLONE_DETAIL}`,
          ),
          account_fallbacks: [],
        },
        { name: "Notes", status: "left_out", via: null, project: null, error: null, account_fallbacks: [] },
      ],
    },
    {
      name: "Side projects",
      status: "failed",
      console: null,
      error: failure("internal_error", { detail: "the data directory cannot be written" }, "the data directory cannot be written"),
      account_fallbacks: [],
      projects: [{ name: "Dotfiles", status: "not_attempted", via: null, project: null, error: null, account_fallbacks: [] }],
    },
    { name: "Scratch", status: "created", console: "c-new-3", error: null, account_fallbacks: [], projects: [] },
  ],
});

const LONG_CONSOLE = LONG_NAMES.consoles[0].name;
const LONG_FOUND = LONG_NAMES.consoles[0].projects[0].name;
const LONG_PLACED = LONG_NAMES.consoles[0].projects[1].name;
const LONG_CLONED = "Customs brokerage document pipeline with the tariff classification and duty drawback reconciliation jobs";
const LONG_SIBLING = "Regional returns and reverse logistics consoles for the claims handling and refurbishment teams";
const LONG_ORPHAN = "Port congestion early warning dashboard and the berth scheduling assistant for the terminal operators";
const LONG_PLACED_DIRECTORY = "/Volumes/Archive/Logistics/Northwest/Carrier-Contracts/2019-2025/Rate-Comparison-Tooling/current";

/** `LONG_NAMES` with a project that can only be cloned and a second console, where each name is too long for one line: the
 * result of importing it has a failure whose reason carries a long path, one whose raw text is long, and a console that
 * could not be created. */
const LONG_RESULT_REVIEW = inspected({
  consoles: [
    {
      ...LONG_NAMES.consoles[0],
      projects: [
        ...LONG_NAMES.consoles[0].projects,
        project({
          name: LONG_CLONED,
          path: "/Users/ana/work/customs-pipeline",
          remote_url: "https://github.com/acme/customs-pipeline.git",
          offers: { exported_path: null, default_clone_dir: null, clone: `${DEFAULT_CLONE_DIR}/customs-pipeline` },
          default: "clone",
        }),
      ],
    },
    {
      name: LONG_SIBLING,
      account_fallbacks: [],
      projects: [
        project({
          name: LONG_ORPHAN,
          path: "/Users/ana/work/port-congestion",
          offers: { exported_path: "/Users/ana/work/port-congestion", default_clone_dir: null, clone: null },
          default: "exported_path",
        }),
      ],
    },
  ],
});

const LONG_CLONE_FAILURE = [
  "Cloning into '/Users/dev/Projects/customs-pipeline'...",
  "fatal: unable to access 'https://github.com/acme/customs-pipeline-with-a-very-long-repository-name-that-has-no-place-to-break.git/': Could not resolve host: github.com",
].join("\n");

/** What importing `LONG_RESULT_REVIEW` came to with the second project placed in a directory of the user's own: the first
 * project created with the console's accounts falling back, the second failing on a long path, the third on a long git
 * message, and the second console not created with its project not tried, each by a reason in the application's words. */
const LONG_RESULT = imported({
  consoles: [
    {
      name: LONG_CONSOLE,
      status: "created",
      console: "c-new-1",
      error: null,
      account_fallbacks: [fallback("claude", "Work")],
      projects: [
        { name: LONG_FOUND, status: "created", via: "exported_path", project: "p-new-1", error: null, account_fallbacks: [fallback("codex", "Personal")] },
        {
          name: LONG_PLACED,
          status: "failed",
          via: null,
          project: null,
          error: failure("path_not_found", { path: LONG_PLACED_DIRECTORY }, `${LONG_PLACED_DIRECTORY} does not exist`),
          account_fallbacks: [],
        },
        {
          name: LONG_CLONED,
          status: "failed",
          via: null,
          project: null,
          error: failure("git_clone_failed", { detail: LONG_CLONE_FAILURE }, `git clone failed: ${LONG_CLONE_FAILURE}`),
          account_fallbacks: [],
        },
      ],
    },
    {
      name: LONG_SIBLING,
      status: "failed",
      console: null,
      error: failure("internal_error", { detail: "the data directory cannot be written" }, "the data directory cannot be written"),
      account_fallbacks: [],
      projects: [{ name: LONG_ORPHAN, status: "not_attempted", via: null, project: null, error: null, account_fallbacks: [] }],
    },
  ],
});

/** Everything created, nothing to report: `SINGLE` with the user's own directory given to the
 * project nothing placed. */
const CLEAN_RESULT = imported({
  consoles: [
    {
      name: "Documentation",
      status: "created",
      console: "c-new-1",
      error: null,
      account_fallbacks: [],
      projects: [
        { name: "Style Guide", status: "created", via: "exported_path", project: "p-new-1", error: null, account_fallbacks: [] },
        { name: "Handbook", status: "created", via: "clone", project: "p-new-2", error: null, account_fallbacks: [] },
        { name: "Releases", status: "created", via: "directory", project: "p-new-3", error: null, account_fallbacks: [] },
      ],
    },
  ],
});

/** A console and its one project, both naming accounts this machine does not have, and where the project is
 * found at its recorded path: so the only thing the import has to report is the fallbacks. */
const FALLBACKS_ONLY = inspected({
  consoles: [
    {
      name: "Documentation",
      account_fallbacks: [fallback("claude", "Work")],
      projects: [
        project({
          name: "Style Guide",
          path: "/Users/dev/code/style-guide",
          offers: { exported_path: "/Users/dev/code/style-guide", default_clone_dir: null, clone: null },
          default: "exported_path",
          account_fallbacks: [fallback("codex", "Personal")],
        }),
      ],
    },
  ],
});

/** `FALLBACKS_ONLY` imported with its defaults. */
const FALLBACKS_ONLY_RESULT = imported({
  consoles: [
    {
      name: "Documentation",
      status: "created",
      console: "c-new-1",
      error: null,
      account_fallbacks: [fallback("claude", "Work")],
      projects: [{ name: "Style Guide", status: "created", via: "exported_path", project: "p-new-1", error: null, account_fallbacks: [fallback("codex", "Personal")] }],
    },
  ],
});

/** `WITH_SETTING_OFF` imported with its defaults and the settings asked for: the project nothing
 * placed is left out, and the settings could not be written. */
const SETTINGS_FAILED_CONSOLES_RESULT = imported({
  settings_error: failure("internal_error", { detail: "the settings could not be written" }, "the settings could not be written"),
  consoles: [
    {
      ...CLEAN_RESULT.consoles[0],
      projects: [
        CLEAN_RESULT.consoles[0].projects[0],
        CLEAN_RESULT.consoles[0].projects[1],
        { name: "Releases", status: "left_out", via: null, project: null, error: null, account_fallbacks: [] },
      ],
    },
  ],
});

const FILE = { name: "octoboard-configuration.json", text: JSON.stringify({ format: "octoboard-configuration", version: 1, consoles: [] }, null, 2) };

const refusal = (name: string, text: string, inspection: FixtureError): ConfigurationFixture => ({ file: { name, text }, inspection });

const DIRECTORY_PROJECT = "Notes for production release coordination مرحبا 发布";
const DIRECTORY_REVIEW = { ...REVIEW, consoles: REVIEW.consoles.map((console) => ({ ...console, projects: console.projects.map((project) => project.name === "Notes" ? { ...project, name: DIRECTORY_PROJECT } : project) })) };

/** A full export's document, with `keyboard_shortcuts` as the client that saved it wrote them beside the settings
 * `WITH_SETTING_OFF` reports for it. The fixture daemon echoes them in the result's `settings_applied`, as the real one
 * does, so a result scenario needs none of its own. */
const documentWith = (shortcuts: Record<string, unknown>) => ({
  name: FILE.name,
  text: JSON.stringify(
    { format: "octoboard-configuration", version: 1, settings: { ...carriedSettings(WITH_SETTING_OFF.settings!), keyboard_shortcuts: shortcuts }, consoles: [] },
    null,
    2,
  ),
});

/** A document that carries the keyboard shortcuts its client had changed: three that can stand here, one for a
 * command this version has no name for and one that clashes with a key the file gives an earlier entry, which keeps it. */
const SHORTCUTS_FILE = documentWith({
  focusMode: { code: "KeyG", primary: true, shift: true },
  lock: { code: "KeyK", primary: true, shift: true },
  newSession: { code: "KeyO", primary: true },
  teleport: { code: "KeyT", primary: true, shift: true },
  pin: { code: "KeyK", primary: true, shift: true },
});

/** A document whose client had changed none: an empty `keyboard_shortcuts`, which puts every shortcut back to its default. */
const SHORTCUTS_DEFAULT_FILE = documentWith({});

/** The same shortcuts, as the device the document is imported on has them already. */
const SHORTCUTS_HERE = {
  focusMode: { code: "KeyG", primary: true, shift: true },
  lock: { code: "KeyK", primary: true, shift: true },
  newSession: { code: "KeyO", primary: true },
} as const;

/** `SETTINGS_FAILED_CONSOLES_RESULT`'s consoles with the settings applied this time. */
const SETTINGS_APPLIED_RESULT = imported({
  settings_applied: carriedSettings(WITH_SETTING_OFF.settings!),
  consoles: SETTINGS_FAILED_CONSOLES_RESULT.consoles,
});

/** `WITH_SETTING_OFF`, its document also carrying a Claude compatibility choice this machine already has. */
const WITH_SETTINGS_HERE: Omit<Inspection, "type"> = {
  ...WITH_SETTING_OFF,
  settings: {
    ...WITH_SETTING_OFF.settings!,
    claude_compatibility: { grok: true },
    current_claude_compatibility: THIS_CLAUDE_COMPATIBILITY,
  },
};

/** The machine's home directory, which a document's `~/` stands for. */
const HOME = "/Users/dev";

/** The directory a document's account directory comes to on this machine, as the daemon resolves it: `~` and `~/...`
 * against the home directory and an absolute path as it is; anything else (a bare name, a relative path) is no
 * directory at all. */
export const resolvedAccountDirectory = (directory: string): string | null => {
  if (directory === "~") return HOME;
  if (directory.startsWith("~/")) return `${HOME}${directory.slice(1)}`;
  return directory.startsWith("/") ? directory : null;
};

const inspectedAccount = (extra: Partial<InspectedAccount> & Pick<InspectedAccount, "name" | "directory">): InspectedAccount => ({
  proposed_directory: resolvedAccountDirectory(extra.directory),
  status: "new",
  in_use_by: null,
  error: null,
  share_default_config: false,
  disabled: false,
  ...extra,
});

/** A document's accounts met on `accountsMachine`. Claude Code's: three new ones (one shares the default configuration, one
 * is disabled), two whose directory another account uses (an account of its own, and the default account), one whose name
 * the Add account form refuses, and its default account to be named. Codex's: one that exists, a new one and one whose directory is a bare name, which is no directory
 * (an older or hand-edited file; an export always writes `~/...` or an absolute path). Grok, to be added, with a default
 * name that is not applied. */
const ACCOUNTS: InspectedAccounts = {
  agents_to_add: ["grok"],
  agents: [
    {
      agent: "claude",
      default_name: "Personal",
      default_name_applies: true,
      accounts: [
        inspectedAccount({ name: "Work", directory: "~/.claude-work", share_default_config: true }),
        inspectedAccount({ name: "Archive", directory: "~/.claude-archive", disabled: true }),
        inspectedAccount({ name: "Sandbox", directory: "~/.claude-sandbox" }),
        inspectedAccount({ name: "Clients", directory: "~/.claude-studio", status: "directory_in_use", in_use_by: "Studio" }),
        inspectedAccount({ name: "Original", directory: "~/.claude", status: "directory_in_use", in_use_by: DEFAULT_ACCOUNT_NAME }),
        inspectedAccount({
          name: "Default",
          directory: "~/.claude-default",
          status: "invalid",
          error: failure("account_name_taken", { agent: "Claude Code", name: "Default" }, "the name `Default` is already used by `Default` for Claude Code"),
        }),
      ],
    },
    {
      agent: "codex",
      default_name: null,
      default_name_applies: false,
      accounts: [
        inspectedAccount({ name: "Home", directory: "~/.codex-home", status: "existing" }),
        inspectedAccount({ name: "Reports", directory: "~/.codex-reports" }),
        inspectedAccount({
          name: "Backup",
          directory: "codex-backup",
          status: "invalid",
          error: failure("config_dir_not_absolute", { agent: "Codex" }, "the Codex config directory must be an absolute path or start with `~/`"),
        }),
      ],
    },
    { agent: "grok", default_name: "Main", default_name_applies: false, accounts: [] },
  ],
};

/** `REVIEW` with the accounts above: the references to accounts the import would create (the Work laptop console's Claude
 * Code account Work, the Website project's Codex account Reports) are listed as falling back, as the daemon lists them,
 * beside the ones that fall back whatever is imported; the review drops the first kind while those accounts are created. */
export const ACCOUNTS_REVIEW: Omit<Inspection, "type"> = {
  ...REVIEW,
  accounts: ACCOUNTS,
  consoles: REVIEW.consoles.map((console) =>
    console.name === "Work laptop"
      ? {
        ...console,
        account_fallbacks: [fallback("claude", "Work")],
        projects: console.projects.map((project) => (project.name === "Website" ? { ...project, account_fallbacks: [fallback("codex", "Reports")] } : project)),
      }
      : console,
  ),
};

/** A document of accounts alone, imported on a machine with no console: an existing and a new account for Claude Code
 * and for Codex, the new ones sharing the default configuration or disabled, and nothing that needs attention. */
const ACCOUNTS_ONLY = inspected({
  consoles: [],
  accounts: {
    agents_to_add: [],
    agents: [
      {
        agent: "claude",
        default_name: null,
        default_name_applies: false,
        accounts: [
          inspectedAccount({ name: "Studio", directory: "~/.claude-studio", status: "existing" }),
          inspectedAccount({ name: "Client", directory: "~/.claude-client", share_default_config: true }),
        ],
      },
      {
        agent: "codex",
        default_name: null,
        default_name_applies: false,
        accounts: [
          inspectedAccount({ name: "Home", directory: "~/.codex-home", status: "existing" }),
          inspectedAccount({ name: "Personal", directory: "~/.codex-personal", disabled: true }),
        ],
      },
    ],
  },
});

/** The accounts of `ACCOUNTS` that cannot be created as the file has them, alone: a directory another account uses, one the
 * default account uses, and a name Claude Code already has. A document of these alone, so that all three rows stand in
 * view and their marks can be read. */
const ACCOUNTS_UNUSABLE = inspected({
  consoles: [],
  accounts: { agents_to_add: [], agents: [{ ...ACCOUNTS.agents[0], default_name: null, default_name_applies: false, accounts: ACCOUNTS.agents[0].accounts.slice(3) }] },
});

const importedAccount = (name: string, status: ImportedAccount["status"], extra: Partial<ImportedAccount> = {}): ImportedAccount => ({
  name,
  status,
  account: status === "created" || status === "existing" ? `a-${name.toLowerCase()}` : null,
  error: null,
  sharing_warning: null,
  ...extra,
});

/** What importing `ACCOUNTS_REVIEW` with its defaults came to, the accounts of Claude Code first: Work created, but sharing
 * the default configuration could not be turned on; Archive created; Sandbox's directory taken since the review; Clients,
 * Original and Default left out as the review had them; Codex's Home already there, Reports created and Backup left out as the review had it; Grok added and its default account
 * named as the file has it for Claude Code. The Work laptop console's account Work is bound now, so only the console's
 * other fallbacks stand. */
const ACCOUNTS_RESULT_ACCOUNTS: ImportedAccounts = {
  agents_added: ["grok"],
  agents: [
    {
      agent: "claude",
      default_name_applied: true,
      accounts: [
        importedAccount("Work", "created", {
          sharing_warning: failure(
            "account_sharing_failed",
            {
              account: "a-work",
              name: "Work",
              cause: "permission_denied",
              items: JSON.stringify([{ item: "skills/", left: "unchanged", cause: "permission_denied", detail: "permission denied" }]),
              detail: "`skills/` left unchanged (permission denied)",
            },
            "sharing the default configuration with account `Work` did not complete: `skills/` left unchanged (permission denied)",
          ),
        }),
        importedAccount("Archive", "created"),
        importedAccount("Sandbox", "failed", {
          error: failure("config_dir_taken", { agent: "Claude Code", name: "Studio" }, "the directory is already used by `Studio` for Claude Code"),
        }),
        importedAccount("Clients", "left_out"),
        importedAccount("Original", "left_out"),
        importedAccount("Default", "left_out"),
      ],
    },
    { agent: "codex", default_name_applied: false, accounts: [importedAccount("Home", "existing"), importedAccount("Reports", "created"), importedAccount("Backup", "left_out")] },
    { agent: "grok", default_name_applied: false, accounts: [] },
  ],
};

/** `MIXED_RESULT`'s consoles and projects, and the accounts above, the application settings not asked for. */
const ACCOUNTS_RESULT = imported({
  accounts: ACCOUNTS_RESULT_ACCOUNTS,
  consoles: MIXED_RESULT.consoles.map((console) => (console.name === "Work laptop" ? { ...console, account_fallbacks: [] } : console)),
});

/** `ACCOUNTS_REVIEW` imported with the consoles and projects unchecked and the application settings checked, where nothing
 * went wrong: every account the review would create was created (Sandbox's directory was not taken after all, and sharing
 * the default configuration for Work went through), the settings were applied, and no console exists, as the daemon answers
 * `apply_consoles` false. */
const ACCOUNTS_SETTINGS_RESULT = imported({
  settings_applied: MIXED_RESULT.settings_applied,
  accounts: {
    ...ACCOUNTS_RESULT_ACCOUNTS,
    agents: ACCOUNTS_RESULT_ACCOUNTS.agents.map((agent) => ({
      ...agent,
      accounts: agent.accounts.map((account) =>
        account.status === "failed" ? importedAccount(account.name, "created") : { ...account, sharing_warning: null },
      ),
    })),
  },
  consoles: [],
});

/** `ACCOUNTS_ONLY` imported with its defaults: the two new accounts created, the two existing ones used as they are. */
const ACCOUNTS_ONLY_RESULT = imported({
  consoles: [],
  accounts: {
    agents_added: [],
    agents: [
      { agent: "claude", default_name_applied: false, accounts: [importedAccount("Studio", "existing", { account: "a-studio" }), importedAccount("Client", "created")] },
      { agent: "codex", default_name_applied: false, accounts: [importedAccount("Home", "existing", { account: "a-home" }), importedAccount("Personal", "created")] },
    ],
  },
});

/** `WITH_SETTING_OFF` with the accounts of `ACCOUNTS_ONLY` as well, imported with its defaults and the settings asked for:
 * the consoles and the accounts all went through, and only the settings could not be written. */
const WITH_ACCOUNTS_SETTING_OFF: Omit<Inspection, "type"> = { ...WITH_SETTING_OFF, accounts: ACCOUNTS_ONLY.accounts };
const SETTINGS_FAILED_RESULT: Omit<Imported, "type"> = { ...SETTINGS_FAILED_CONSOLES_RESULT, accounts: ACCOUNTS_ONLY_RESULT.accounts };

const reviewing = (extra: Partial<ConfigurationFixture> = {}): ConfigurationFixture => ({ file: FILE, inspection: REVIEW, imported: MIXED_RESULT, ...extra });

export const configurationScenarios: Scenario[] = [
  {
    id: "configuration-settings-unavailable",
    group: GROUP,
    category: "normal",
    title: "Settings, Configuration section where files cannot be saved or opened",
    description: "The section only says it is not available here.",
    state,
    configuration: { unavailable: true },
    steps: [
      openSettings,
      configurationSection,
    ],
  },
  {
    id: "configuration-welcome-unavailable",
    group: GROUP,
    category: "normal",
    title: "Welcome screen where files cannot be saved or opened",
    description: "The primary New console button is the only action: Import configuration is unavailable.",
    state: noConsoleState,
    configuration: { unavailable: true },
  },
  {
    id: "configuration-export-console",
    group: GROUP,
    category: "normal",
    title: "Export, from Settings with a console in the sidebar",
    description: "The Consoles and projects radio has three options, each only a label with no description under it: \"Console Main and its projects\" (the sidebar's console, named), \"All consoles and their projects\" and \"Do not export consoles\", the first chosen. Under Include full project paths, a Global settings group holds Application settings, checked, and Accounts, unchecked, each with a description that begins \"Includes\" (\"Includes the settings that apply to every console…\", \"Includes account names…\"). Include full project paths starts unchecked: its description says what checking it does (\"Once checked, the file records each project's full directory path, so an import can look for the project there first.\") and no alert stands beneath it. Export closes the dialog and raises a toast naming the saved file.",
    state,
    steps: exportFromSettings,
  },
  {
    id: "configuration-export-with-paths",
    group: GROUP,
    category: "normal",
    title: "Export, with full project paths included",
    description: "With Include full project paths checked, its description changes to \"An import first looks for each project at its recorded full path.\" and a warning alert stands directly under the option, titled \"The file contains project paths.\": the description says the paths are on the machine running Octoboard, which may not be the device in use, and may reveal details such as a user name; a separate line beneath says \"Share the file only with people you trust.\" No mark sits beside the checkbox.",
    state,
    steps: [
      ...exportFromSettings,
      (ui) => ui.press(ui.t("configuration.export.paths.include")),
    ],
  },
  {
    id: "configuration-export-everything-only",
    group: GROUP,
    category: "normal",
    title: "Export, with no console to export on its own",
    description: "The Consoles and projects radio offers only \"All consoles and their projects\", the one chosen, and \"Do not export consoles\", each a label with no description; there is no option for a console of the sidebar's. Include full project paths is shown, unchecked.",
    state: noConsoleState,
    steps: exportFromSettings,
  },
  {
    id: "configuration-export-accounts",
    group: GROUP,
    category: "normal",
    title: "Export, with the accounts included",
    description: "With Accounts checked, its description (\"Includes account names, their configuration directories, sharing and disabled choices, their order and the added agents.\") goes on to say that no login credentials are included, so the accounts have to be logged in again after an import. Include full project paths, still unchecked, keeps its own description about project paths and no alert stands beneath it: checking Accounts does not change it.",
    state,
    steps: [...exportFromSettings, toggleExportAccounts],
  },
  {
    id: "configuration-export-accounts-paths",
    group: GROUP,
    category: "normal",
    title: "Export, with the accounts and full project paths included",
    description: "With both Accounts and Include full project paths checked, the paths option reads \"An import first looks for each project at its recorded full path.\" and the warning alert under it is titled \"The file contains project paths.\", exactly as with Accounts unchecked: the choice and its warning are about project paths alone, because account directories are always exported.",
    state,
    steps: [...exportFromSettings, toggleExportAccounts, toggleExportPaths],
  },
  {
    id: "configuration-export-accounts-only",
    group: GROUP,
    category: "normal",
    title: "Export, only the accounts",
    description: "With Do not export consoles chosen and Application settings unchecked, only Accounts is checked and Export stays available. Include full project paths is not shown at all, as no console is exported to have project paths: account directories are written whatever the choice.",
    state,
    steps: [...exportFromSettings, exportScope("none"), toggleExportSettings, toggleExportAccounts],
  },
  {
    id: "configuration-export-nothing",
    group: GROUP,
    category: "normal",
    title: "Export, with nothing selected",
    description: "With Do not export consoles chosen and both Application settings and Accounts unchecked, Export is unavailable, and Include full project paths is not shown.",
    state,
    steps: [...exportFromSettings, exportScope("none"), toggleExportSettings],
  },
  {
    id: "configuration-export-saved",
    group: GROUP,
    category: "normal",
    title: "Export, saved file confirmation",
    description: "The success toast names the saved configuration file as a chip.",
    state,
    steps: [
      ...exportFromSettings,
      (ui) => ui.press(ui.t("configuration.export.submit")),
    ],
  },
  {
    id: "configuration-export-downloaded",
    group: GROUP,
    category: "normal",
    title: "Export, handed to the browser",
    description: "A browser files the download itself and does not say as what, so the toast names no file.",
    state,
    configuration: { downloads: true },
    steps: [
      ...exportFromSettings,
      (ui) => ui.press(ui.t("configuration.export.submit")),
    ],
  },
  {
    id: "configuration-export-save-failed",
    group: GROUP,
    category: "exceptional",
    title: "Export, the file could not be written",
    description: "The error alert dialog, over the export dialog and titled \"Could not export the configuration\", says only \"Could not save the file.\" and a remedy about writing (file permissions, a writable disk or another folder), nothing about reading the file. The system's own text, Permission denied (os error 13), appears nowhere in the sentences and sits only in the Error details disclosure, closed here: open it to read it in its block.",
    state,
    configuration: { saveError: "Permission denied (os error 13)" },
    steps: [
      ...exportFromSettings,
      (ui) => ui.press(ui.t("configuration.export.submit")),
    ],
  },
  {
    id: "configuration-import-choose",
    group: GROUP,
    category: "normal",
    title: "Import, choosing the file",
    description: "Opened from the welcome screen, where a new computer starts. The intro ends by saying that the file may also carry accounts, which are created without being logged in, and the application settings. The picker is never reported dismissed, as in a browser without the file input's cancel event: Choose file shows that it is waiting and ignores another press, while Cancel, the close button and Escape still work.",
    state: noConsoleState,
    configuration: { file: "pending" },
    steps: [
      (ui) => ui.press(ui.t("welcome.importConfiguration")),
      chooseFile,
    ],
  },
  {
    id: "configuration-import-read-failed",
    group: GROUP,
    category: "exceptional",
    title: "Import, the file could not be read",
    description: "The error alert dialog, over the import dialog and titled \"Could not import the configuration\", says only \"Could not read the file.\" and a remedy about reading (that the file still exists and Octoboard may read it, where to grant access, or choosing another file), nothing about writing or disk space. Compare the next two, whose remedy is to choose a different file. The system's own text, Permission denied (os error 13), sits only in the Error details disclosure, closed here.",
    state,
    configuration: { readError: "Permission denied (os error 13)" },
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-file-too-large",
    group: GROUP,
    category: "exceptional",
    title: "Import, a file too large to be a configuration",
    description: "The remedy asks for a configuration file exported by a compatible Octoboard version, not about access to the file, which was read.",
    state,
    configuration: { readError: { kind: "too_large" } },
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-file-not-text",
    group: GROUP,
    category: "exceptional",
    title: "Import, a file that is not text",
    description: "The remedy asks for a configuration file exported by a compatible Octoboard version, not about access to the file, which was read.",
    state,
    configuration: { readError: { kind: "not_text" } },
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-not-json",
    group: GROUP,
    category: "exceptional",
    title: "Import, a file that is not JSON",
    state,
    configuration: { file: { name: "release-configuration-for-production-services-مرحبا-发布.json", text: "not json at all" } },
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-refused-format",
    group: GROUP,
    category: "exceptional",
    title: "Import, not a configuration file",
    state,
    configuration: refusal("notes.json", JSON.stringify({ hello: "world" }), failure("configuration_not_recognised", {}, "not a configuration document")),
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-refused-version",
    group: GROUP,
    category: "exceptional",
    title: "Import, a version this one cannot read",
    state,
    configuration: refusal(
      "octoboard-configuration.json",
      JSON.stringify({ format: "octoboard-configuration", version: 2, consoles: [] }),
      failure("configuration_version_unsupported", { version: "2", supported: "1" }, "version 2 is not supported"),
    ),
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-refused-unreadable",
    group: GROUP,
    category: "exceptional",
    title: "Import, a document that cannot be read",
    description: "The refusal says only that the configuration file could not be read; the parser's text about the unexpected type sits only in the Error details disclosure, closed here.",
    state,
    configuration: refusal(
      "octoboard-configuration.json",
      JSON.stringify({ format: "octoboard-configuration", version: 1, consoles: "none" }),
      failure("configuration_unreadable", { detail: "invalid type: string \"none\", expected a sequence" }, "unreadable"),
    ),
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review",
    group: GROUP,
    category: "normal",
    title: "Import, review",
    description: "The intro line reads \"These are the contents of octoboard-configuration.json.\". Above the review, a What to import group holds a checkbox for each of the two regions the file carries: Consoles and projects, checked, and Application settings, unchecked. Only the checked region has a tab, and a single one has no tab bar, so the review stands as it is. The panel starts with the note that each console is created as a new one and the existing consoles stay as they are, then the apply-to-all buttons; cards for three consoles follow, with the clone note last. Project rows show each state: found at its exported path (only \"Placed in\"), found in the default clone directory, can be cloned (\"Clone from\" and \"Clone into\") and not found, left out, with a long exported path that wraps and no state line under its name. A warning triangle follows the name of the Work laptop console in its heading and the name of Search API in its row, where an account the file names is not set up here (the popover is in a scenario below); no sentence or alert repeats it. The not-found project, Notes, carries one warning triangle after its name, \"Notes not found, and accounts not set up\": the project is missing and its account falls back too, which are one point and so one mark (its popover is in a scenario below). The last console says it has no projects. The unchecked boxes (here Application settings) show their edge against the dialog.",
    state,
    configuration: reviewing(),
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review-settings",
    group: GROUP,
    category: "normal",
    title: "Import, review of the application settings",
    description: "Application settings ticked in the What to import group: the Application settings tab appears beside Consoles and projects (with a warning glyph) and is the one open. It holds one card, with no checkbox of its own. Lines that turn a setting on carry a muted consequence line (the automatic-switch line, \"Automatically switch accounts at the usage limit\", says a session that stops at its account's usage limit is then switched to another account with usage left and carries on), lines that turn one off do not (their \"Turns off\" badge is neutral, not accented), Git workflow and Claude compatibility read \"Set to the file's\" (the file turns Codex's off), and keyboard shortcuts read \"Set to the file's\" with a note that 3 of the file's shortcuts differ from the defaults and apply to this device or browser only.",
    state,
    configuration: reviewing({ file: SHORTCUTS_FILE }),
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleSettings,
      settingsTab,
    ],
  },
  {
    id: "configuration-import-review-tabs",
    group: GROUP,
    category: "normal",
    title: "Import, review of a document with consoles, accounts and settings",
    description: "A file with consoles, accounts and settings: the What to import group holds three checkboxes, Consoles and projects and Accounts checked and Application settings ticked here, so there are three tabs, Consoles and projects and Accounts each with a warning glyph (a project is not found; an account's directory is in use or cannot be used) and Application settings without; the first is open. With Accounts checked and every account left at its proposal, the references to accounts the import would create (the Work laptop console's Claude Code account Work, the Website project's Codex account Reports) carry no warning triangle, while Search API's and Notes's Codex account Personal, which the import does not create, still do. Switching the checkboxes on the page moves the tabs in and out with the list motion.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleSettings,
    ],
  },
  {
    id: "configuration-import-review-tab-tooltip",
    group: GROUP,
    category: "normal",
    title: "Import, review, what a tab's warning glyph says",
    description: "The mouse rests on the Accounts tab, whose warning glyph is about an account that cannot be created as it is: a tooltip opens against the tab, saying this part has something that needs attention. It opens in the same way while the tab has keyboard focus.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      (ui) =>
        ui.hoverAnchored(
          ui.t("configuration.import.tab.attention", { tab: ui.t("configuration.import.tab.accounts") }),
          ui.t("configuration.import.tab.attentionTip"),
        ),
    ],
  },
  {
    id: "configuration-import-review-accounts",
    group: GROUP,
    category: "normal",
    title: "Import, review of the accounts",
    description: "The Accounts tab, opened from a tab bar of Consoles and projects and Accounts (the What to import group above it has both checked, Application settings unchecked): one line saying Grok Build would be added to the added agents and one saying the default account of Claude Code would be named Personal (Grok Build's default name, which is not applied, has no line). A card for each agent. A row's status line says only New or \"Already exists, used as it is\"; an account that cannot be created has no status line but a warning mark after its name instead (its popovers are in the scenarios below). Claude Code's rows: Work, New, with the \"Shares default configuration\" badge; Archive, New, with the \"Disabled\" badge and its choice set to Leave out (changed from \"Create it\"); Sandbox, New; Clients, Original and Default, each with the warning mark and its choice at Leave out (a directory in use by another account, one in use by the default account, a name Claude Code already has). Clients and Original can be given another directory; Default, whose name is the problem, offers only Leave out. Codex's: Home, \"Already exists, used as it is\", with no choice and no directory line; Reports, New; Backup, with the warning mark, its directory written as the bare name codex-backup, at Leave out. Every row but Home has a \"Directory in the file\" line giving the directory as the file writes it. Grok Build says the file has no accounts of its own for this agent. A note says the accounts are created without being logged in.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      accountsTab,
      (ui) => ui.press(ui.t("configuration.import.accounts.choice.label", { account: "Archive" })),
      (ui) => ui.press(ui.t("configuration.import.choice.skip")),
    ],
  },
  {
    id: "configuration-import-review-account-in-use-details",
    group: GROUP,
    category: "normal",
    title: "Import, review, why an account's directory cannot be used",
    description: "A document of accounts alone, with the three that cannot be created as the file has them. The mouse rests on the warning mark after the Clients name: its popover opens above it with no click and no focus taken, saying the directory is already in use by Studio and that the account is left out unless another directory is chosen for it.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_UNUSABLE },
    steps: [...importFromSettings, chooseFile, hoverAccountMark("directoryInUseMark", "Clients")],
  },
  {
    id: "configuration-import-review-account-in-use-default-details",
    group: GROUP,
    category: "normal",
    title: "Import, review, a directory in use by the default account",
    description: "Same document, the mouse on the warning mark after the Original name: the directory is in use by the default account, named as the application names it in the current language. Switch the language to 简体中文 to read it as 默认; any other account is named as it is, as with Studio in the scenario above.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_UNUSABLE },
    steps: [...importFromSettings, chooseFile, hoverAccountMark("directoryInUseMark", "Original")],
  },
  {
    id: "configuration-import-review-account-invalid-details",
    group: GROUP,
    category: "normal",
    title: "Import, review, why an account cannot be created",
    description: "Same document, the mouse on the warning mark after the Default name: its popover opens above it, saying \"This account cannot be created: …\" and why (Claude Code already has an account named Default), then only \"The account is left out.\": no other directory could help, so its choice offers Leave out alone.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_UNUSABLE },
    steps: [...importFromSettings, chooseFile, hoverAccountMark("invalidMark", "Default")],
  },
  {
    id: "configuration-import-review-accounts-left-out",
    group: GROUP,
    category: "normal",
    title: "Import, review of the consoles when an account is left out",
    description: "Work, an account the file would create, is set to Leave out on the Accounts tab, and the Consoles and projects tab is open again: the warning triangle is back after the Work laptop heading, as that console's Claude Code account Work will not exist. The Website project's Codex account Reports is still created, so its row has no triangle, unlike Search API's and Notes's account Personal, which the file does not create.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [...importFromSettings, chooseFile, accountsTab, ...leaveAccountOut("Work"), consolesTab],
  },
  {
    id: "configuration-import-review-accounts-off-consoles",
    group: GROUP,
    category: "normal",
    title: "Import, review of the consoles when the accounts are not imported",
    description: "With Accounts unchecked in the What to import group, the Accounts tab is gone, and the references to accounts the import would have created fall back after all: the Work laptop console and the Website project, both marked as such, now carry the warning triangle beside Search API's and Notes's. With one region left the tab bar is gone too.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleAccounts,
    ],
  },
  {
    id: "configuration-import-review-consoles-off",
    group: GROUP,
    category: "normal",
    title: "Import, review with the consoles not imported",
    description: "With Consoles and projects unchecked in the What to import group, its tab is gone and the review is the Accounts content alone, without a tab bar: the file's consoles and projects are not imported, so no project row is asked to be placed.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleConsoles,
    ],
  },
  {
    id: "configuration-import-review-nothing",
    group: GROUP,
    category: "normal",
    title: "Import, review with nothing chosen to import",
    description: "With Consoles and projects and Accounts both unchecked and Application settings left unchecked, no tab is left and the body under the What to import group is empty; Import is unavailable.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleConsoles,
      toggleAccounts,
    ],
  },
  {
    id: "configuration-import-review-accounts-only",
    group: GROUP,
    category: "normal",
    title: "Import, review of a document of accounts alone",
    description: "A document that carries only accounts has no What to import group and no tab bar: the review opens straight on the accounts, as on a new computer with no console. Existing accounts read \"Already exists, used as it is\" and have no choice; the new ones carry the \"Shares default configuration\" and \"Disabled\" badges; nothing carries a warning, and there are no lines about agents to add or default names.",
    state: noConsoleState,
    configuration: { file: FILE, inspection: ACCOUNTS_ONLY, imported: ACCOUNTS_ONLY_RESULT },
    steps: importFromWelcome,
  },
  {
    id: "configuration-import-review-single",
    group: GROUP,
    category: "normal",
    title: "Import, review of one console's document",
    description: "A single console's document has no What to import group and no tab bar, as it carries only that region, and one card. \"Default clone directory (0)\" is disabled because no project offers it, and the clone note follows the card. The project nothing places, Releases, has no state line, only a warning triangle after its name.",
    state,
    configuration: { file: FILE, inspection: SINGLE, imported: CLEAN_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review-long-names",
    group: GROUP,
    category: "normal",
    title: "Import, review with names that wrap",
    description: "The console name in its heading and both project names are too long for one line and wrap. Each warning triangle (the console's and the found project's account fallbacks, and the not-found project's \"not found\") stays one line high and centres on the first line of its name, not on the whole wrapped name.",
    state,
    configuration: { file: FILE, inspection: LONG_NAMES },
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review-no-paths",
    group: GROUP,
    category: "normal",
    title: "Import, review of a document exported without full paths",
    description: "A project recorded only by its directory's name has no \"Exported path\" line: found in the default clone directory (only \"Placed in\"), can be cloned, or not found, with nothing to place it: that one has no state line under its name, only a warning triangle after it.",
    state,
    configuration: { file: FILE, inspection: NO_PATHS },
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review-shortcuts-same",
    group: GROUP,
    category: "normal",
    title: "Import, keyboard shortcuts, git workflow and Claude compatibility the device already has",
    description: "With Application settings ticked in the What to import group, its tab holds Git workflow, Claude compatibility and keyboard shortcuts, all reading \"No change\" because the file's are the ones this device already has, below the switches, which all read \"No change\" too except automatic pull, which turns off.",
    state: autoPulling,
    preferences: { keyboardShortcuts: SHORTCUTS_HERE },
    configuration: { file: SHORTCUTS_FILE, inspection: WITH_SETTINGS_HERE },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleSettings,
      settingsTab,
    ],
  },
  {
    id: "configuration-import-review-shortcuts-reset",
    group: GROUP,
    category: "normal",
    title: "Import, a file whose keyboard shortcuts are all defaults",
    description: "With Application settings ticked, its tab is open. The file's settings are this device's except automatic pull, which turns off, so Git workflow reads \"No change\" and there is no Claude compatibility line (the file makes no choice), and its keyboard shortcuts are all defaults, so that line reads \"Back to defaults\" with the note that it applies to this device or browser only.",
    state: autoPulling,
    preferences: { keyboardShortcuts: SHORTCUTS_HERE },
    configuration: { file: SHORTCUTS_DEFAULT_FILE, inspection: WITH_SETTING_OFF },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleSettings,
      settingsTab,
    ],
  },
  {
    id: "configuration-import-review-fallback-details",
    group: GROUP,
    category: "normal",
    title: "Import, review, why a project's account is not used",
    description: "The mouse rests on the warning triangle after the Search API name: its popover opens above it with no click and no focus taken, saying that the Codex account Personal is not set up here, so the default account is used. The triangle after the Work laptop heading opens the same kind of popover for the console's own account. A touch or a key press opens it with focus inside.",
    state,
    configuration: reviewing(),
    steps: [
      ...importFromSettings,
      chooseFile,
      (ui) => ui.hoverStatusMark(ui.t("configuration.import.fallbacks.label", { subject: "Search API" })),
    ],
  },
  {
    id: "configuration-import-review-not-found-details",
    group: GROUP,
    category: "normal",
    title: "Import, review, why a project is skipped",
    description: "The mouse rests on the one warning triangle after the Notes name, \"Notes not found, and accounts not set up\": its popover opens above it with no click and no focus taken, listing each state as its own paragraph: the project is not found on the machine running Octoboard, it is skipped unless a directory is chosen for it, and then the account that falls back to the default. There is no second triangle beside it and the row has no state line under the name.",
    state,
    configuration: reviewing(),
    steps: [
      ...importFromSettings,
      chooseFile,
      (ui) => ui.hoverStatusMark(ui.t("configuration.import.project.state.noneAndFallbacksMark", { project: "Notes" })),
    ],
  },
  {
    id: "configuration-import-importing",
    group: GROUP,
    category: "normal",
    title: "Import, applying",
    description: "The reply comes after the last clone; the dialog cannot be dismissed meanwhile.",
    state,
    configuration: reviewing({ imported: "pending" }),
    steps: [
      ...importFromSettings,
      chooseFile,
      importNow,
    ],
  },
  {
    id: "configuration-import-refused-apply",
    group: GROUP,
    category: "exceptional",
    title: "Import, refused before anything is created, an avatar too large",
    description: "The daemon's reason is the error alert dialog's, raised over the review, which stays as it was with nothing written into it. The reason names the console whose avatar is larger than 256 KiB (Work laptop, as a name). The remedy under it is about the avatar, not the file: give the console a smaller avatar in the Octoboard the configuration came from, export it again, then import the new file; nothing suggests the file is damaged.",
    state,
    configuration: reviewing({ imported: failure("icon_too_large", { limit_kib: "256", console_name: "Work laptop" }, "the avatar of the console Work laptop is larger than 256 KiB") }),
    steps: [
      ...importFromSettings,
      chooseFile,
      importNow,
    ],
  },
  {
    id: "configuration-import-refused-avatar-image",
    group: GROUP,
    category: "exceptional",
    title: "Import, refused before anything is created, an avatar that is not an image",
    description: "The error alert dialog, raised over the unchanged review, says the avatar of the console Work laptop is not an image. Its remedy asks for an image as the console's avatar in the Octoboard the configuration came from, then to export again and import the new file; it does not ask for a smaller one.",
    state,
    configuration: reviewing({ imported: failure("icon_not_an_image", { console_name: "Work laptop" }, "the avatar of the console Work laptop is not an image") }),
    steps: [
      ...importFromSettings,
      chooseFile,
      importNow,
    ],
  },
  {
    id: "configuration-import-result-settings-failed",
    group: GROUP,
    category: "exceptional",
    title: "Import, result where the settings could not be applied",
    description: "With Application settings ticked, the import's result has a tab for it: it carries a warning glyph and is the one open, as the only tab with a failure: a danger Alert holds the settings failure's title and short reason, with Details on its right for the raw text, and no settings card follows. The other two tabs have no glyph (the next scenario shows what they say).",
    state: autoPulling,
    configuration: { file: FILE, inspection: WITH_ACCOUNTS_SETTING_OFF, imported: SETTINGS_FAILED_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleSettings,
      importNow,
    ],
  },
  {
    id: "configuration-import-result-settings-failed-others",
    group: GROUP,
    category: "exceptional",
    title: "Import, result where only the settings failed, the other tabs",
    description: "The settings could not be applied, yet the Accounts tab, open here, starts with its own totals, \"2 accounts created · 2 accounts already existed · Nothing failed\", since none of its accounts failed; the Consoles and projects tab has its own too (\"1 console created · 2 projects created · 1 project left out · Nothing failed\"), and only the Application settings tab carries the warning glyph.",
    state: autoPulling,
    configuration: { file: FILE, inspection: WITH_ACCOUNTS_SETTING_OFF, imported: SETTINGS_FAILED_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleSettings,
      importNow,
      (ui) => ui.pressTab(ui.t("configuration.import.tab.accounts")),
    ],
  },
  {
    id: "configuration-import-review-directory",
    group: GROUP,
    category: "exceptional",
    title: "Import placement validation",
    state: state,
    configuration: reviewing({ inspection: DIRECTORY_REVIEW, imported: undefined }),
    description: "\"Another directory\" under a project shows a text field labelled \"Directory\" and a Browse button. After Import is pressed, an empty directory and a relative one show their errors together while the absolute one is accepted; the field of a long mixed-script project name keeps its row readable.",
    steps: [
      ...importFromSettings,
      chooseFile,
      (ui) => ui.press(ui.t("configuration.import.project.choice", { project: DIRECTORY_PROJECT })),
      (ui) => ui.press(ui.t("configuration.import.choice.directory")),
      ...placeIn("Search API", "projects/search-api"),
      ...placeIn("Website", "~/work/website"),
      importNow,
    ],
  },
  {
    id: "configuration-import-review-accounts-directory",
    group: GROUP,
    category: "exceptional",
    title: "Import placement validation of an account",
    description: "On the Accounts tab, \"Another directory\" under an account shows a text field labelled \"Directory\" with a placeholder that follows the agent (\"Ex. ~/.claude-work\" in Claude Code's Clients, \"Ex. ~/.codex-work\" in Codex's Backup, both left empty) and a Browse button, in place of the \"Directory in the file\" line, which the other rows keep. Only a row whose directory is the problem offers it: Default, whose name is the problem, can only be left out. After Import is pressed, an empty directory (Clients, Backup) shows \"Enter a directory, or leave the account out.\" and a relative one (Original) shows that an absolute path, or one starting with ~/, is needed, together. The view is scrolled to Codex's Backup, whose empty field has focus and shows its own placeholder; Claude Code's Clients, empty too, is higher up the list.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_REVIEW, imported: undefined },
    steps: [
      ...importFromSettings,
      chooseFile,
      accountsTab,
      (ui) => ui.press(ui.t("configuration.import.accounts.choice.label", { account: "Clients" })),
      (ui) => ui.press(ui.t("configuration.import.choice.directory")),
      ...placeAccountIn("Original", "accounts/original"),
      (ui) => ui.press(ui.t("configuration.import.accounts.choice.label", { account: "Backup" })),
      (ui) => ui.press(ui.t("configuration.import.choice.directory")),
      importNow,
      (ui) => ui.focus(ui.t("configuration.import.accounts.directory.label", { account: "Backup" })),
    ],
  },
  {
    id: "configuration-import-result",
    group: GROUP,
    category: "exceptional",
    title: "Import result with failures and fallbacks",
    state: state,
    configuration: reviewing({ file: SHORTCUTS_FILE }),
    description: "The Consoles and projects tab, with a warning glyph (the Application settings tab has none), is open. One line of totals counts only the consoles and projects: \"2 consoles created · 1 project created · 1 project left out · 3 failed · 1 project not attempted\", with no totals above the tabs. Below it a card per console, each with its own status in the header, always: Work laptop Created, Side projects Failed with its reason under the header (the daemon's generic sentence, with Details), Scratch Created with the note that it has no projects. Work laptop's rows: Website Failed with its reason written under the row (a path that does not exist, in code style); Search API Created with a muted line saying where it went; Design System Failed with its reason, Details behind a button; Notes Left out. Dotfiles, under the failed console, reads Not attempted with the muted line that its console could not be created. A warning triangle follows the Work laptop name and the Search API name, for the accounts that fell back to the default. Use the page's width control for how the rows wrap: the status goes under the name when the row is narrow.",
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleSettings,
      importNow,
    ],
  },
  {
    id: "configuration-import-result-settings",
    group: GROUP,
    category: "normal",
    title: "Import result, the application settings applied",
    description: "The Application settings tab holds the \"Application settings applied\" card, whose badges read On, Off, \"Matches the file\" (git workflow and Claude compatibility, in that order) and \"3 changes from the defaults\", the last with a muted line saying 2 shortcuts could not be applied. The Consoles and projects tab keeps its warning glyph.",
    state,
    configuration: reviewing({ file: SHORTCUTS_FILE }),
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleSettings,
      importNow,
      settingsTab,
    ],
  },
  {
    id: "configuration-import-result-accounts",
    group: GROUP,
    category: "exceptional",
    title: "Import result of the accounts, with a failure and a warning",
    description: "The Accounts tab, with a warning glyph beside the Consoles and projects one, is open. It starts with one line of totals counting only the accounts, \"3 accounts created · 1 account already existed · 4 accounts left out · 1 failed\", directly above the cards, and holds a card per agent with a row per account, each with its status: for Claude Code, Work Created with the sharing warning written under the row (the sentence that the account was created but sharing the default configuration could not be turned on, then only the remedy and a Details button: the failure's own summary is not repeated), Archive Created, Sandbox Failed with the reason under the row, Clients, Original and Default Left out; for Codex, Home Already existed, Reports Created and Backup Left out; Grok Build has no card. Under the cards, one line says Grok Build was added to the agents, one that the default account of Claude Code is now named as in the file, and a note with an info icon says the new accounts are not logged in yet; there is no Log in button.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      importNow,
      accountsTab,
    ],
  },
  {
    id: "configuration-import-result-no-consoles",
    group: GROUP,
    category: "normal",
    title: "Import result of a file with consoles, imported without them",
    description: "The file carries consoles, but Consoles and projects was unchecked, so no console or project was created and the result has no Consoles and projects tab: only Accounts (open) and Application settings, which was ticked, neither with a warning glyph. Above the cards, one line of totals counts the accounts alone, \"4 accounts created · 1 account already existed · 4 accounts left out · Nothing failed\"; the Application settings tab has none.",
    state: accountsMachine,
    configuration: { file: FILE, inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_SETTINGS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleConsoles,
      toggleSettings,
      importNow,
    ],
  },
  {
    id: "configuration-import-result-accounts-only",
    group: GROUP,
    category: "normal",
    title: "Import result of a document of accounts alone",
    description: "No tab bar: the one line of totals above the cards says \"2 accounts created · 2 accounts already existed · Nothing failed\". Studio and Home read Already existed, Client and Personal Created; with no agent added and no default account named, the only line under the cards is the note, with an info icon, that the new accounts are not logged in; there is no Log in button.",
    state: noConsoleState,
    configuration: { file: FILE, inspection: ACCOUNTS_ONLY, imported: ACCOUNTS_ONLY_RESULT },
    steps: [...importFromWelcome, importNow],
  },
  {
    id: "configuration-import-result-shortcuts-reset",
    group: GROUP,
    category: "normal",
    title: "Import result, keyboard shortcuts back to their defaults",
    description: "On the Application settings tab the card lists all eleven switches with the values applied, automatic pull as Off, Git workflow as \"Matches the file\" and no Claude compatibility line, and keyboard shortcuts as \"Defaults\", with no muted line under it.",
    state: autoPulling,
    preferences: { keyboardShortcuts: SHORTCUTS_HERE },
    configuration: { file: SHORTCUTS_DEFAULT_FILE, inspection: WITH_SETTING_OFF, imported: SETTINGS_APPLIED_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleSettings,
      importNow,
      settingsTab,
    ],
  },
  {
    id: "configuration-import-result-fallbacks",
    group: GROUP,
    category: "normal",
    title: "Import result, accounts that fell back to the default",
    description: "Everything was created, so the only thing to report is the fallbacks: a warning triangle follows the Documentation name in its header row and the Style Guide name in its row, and nothing else (no alert, no list of accounts) stands above or below the card but the one line of totals above it (\"1 console created · 1 project created · Nothing failed\"), with no tab bar as there is one region. Their popovers are in the next scenario.",
    state,
    configuration: { file: FILE, inspection: FALLBACKS_ONLY, imported: FALLBACKS_ONLY_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      importNow,
    ],
  },
  {
    id: "configuration-import-result-fallbacks-details",
    group: GROUP,
    category: "normal",
    title: "Import result, why a console's accounts fell back",
    description: "The mouse rests on the warning triangle after the Documentation name in the result's header row: its popover opens above it with no click and no focus taken, saying that the Claude Code account Work is not set up here, so the default account is used. The card stays as it is under the popover.",
    state,
    configuration: { file: FILE, inspection: FALLBACKS_ONLY, imported: FALLBACKS_ONLY_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      importNow,
      (ui) => ui.hoverStatusMark(ui.t("configuration.import.fallbacks.label", { subject: "Documentation" })),
    ],
  },
  {
    id: "configuration-import-result-failure-reason",
    group: GROUP,
    category: "exceptional",
    title: "Import result, the raw diagnostic behind a failure's Details",
    state: state,
    configuration: reviewing(),
    description: "Details under the Design System failure is open: a popover opening below the button holds the git output as the raw text, in a block whose long lines wrap. The sentence that git could not clone the repository stays on the row above it, and the other failed rows keep their own reasons and buttons.",
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleSettings,
      importNow,
      (ui) => ui.press(ui.t("error.details")),
    ],
  },
  {
    id: "configuration-import-result-long-text",
    group: GROUP,
    category: "exceptional",
    title: "Import result with long names and reasons",
    state,
    configuration: { file: FILE, inspection: LONG_RESULT_REVIEW, imported: LONG_RESULT },
    description: "Names and reasons too long for a line: each console's and project's name wraps beside its status, the warning triangle staying on the name's first line, and a reason, such as the long path in code style, wraps under its row instead of widening the card. Use the page's width control (420) to see the phone layout, where the names wrap onto more lines beside their statuses. Two projects failed (a path that does not exist, a clone), each with Details, and the second console Failed with its project Not attempted.",
    steps: [
      ...importFromSettings,
      chooseFile,
      ...placeIn(LONG_PLACED, LONG_PLACED_DIRECTORY),
      importNow,
    ],
  },
  {
    id: "configuration-import-result-clean",
    group: GROUP,
    category: "normal",
    title: "Successful import result",
    description: "A single region, so no tab bar: one line of totals, \"1 console created · 3 projects created · Nothing failed\", sits directly above the card, whose three projects read Created, each with a muted line for where it went (at its exported path, cloned from its remote, in the directory you chose).",
    state: state,
    configuration: { file: FILE, inspection: SINGLE, imported: CLEAN_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      ...placeIn("Releases", "~/work/releases"),
      importNow,
    ],
  },
];
