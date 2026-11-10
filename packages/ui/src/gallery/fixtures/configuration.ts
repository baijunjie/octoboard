// The gallery's "Configuration" scenarios: exporting a console or everything, and importing a file
// through its three steps (choosing it, reviewing what it holds, the result). What the daemon would
// work out from the filesystem — the offers of a project, the outcome of a clone — is written out
// here by hand, and the test beside this file holds those answers to the rules the daemon follows.
import type { Imported } from "../../configuration/ImportResult";
import type { Inspection } from "../../configuration/importPlan";
import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import { type DocumentAgentSettings, type DocumentSettings, type ImportedAccount, type ImportedAccounts, type InspectedAccount, type InspectedAccounts, type InspectedAgentSettings, type InspectedSettings, type SettingsSection } from "../../protocol";
import { SECTION_LABEL, SETTINGS_SECTIONS } from "../../configuration/settingsSections";
import { DEFAULT_ACCOUNT_NAME } from "../../accounts/defaultAccount";
import { FIXTURE_AGENTS, FIXTURE_PROVIDERS, UNPRESENTED_PROVIDER } from "../../fixtures/fixtureProviders";
import type { Ui, Step } from "../../fixtures/interact";
import type { ConfigurationFixture, ImportProgressFixture } from "../../fixtures/fixture";
import type { FixtureError } from "../../fixtures/sampleFiles";
import type { Scenario } from "../scenario";
import { accountOf, currentExportedProviderSettings, effectiveProviderSettings, SAMPLE, consoleOf, projectOf, snapshotState } from "../../fixtures/builders";
import { fallback, failure, imported, inspected, inspectedProject, MIXED_RESULT, REVIEW } from "../../fixtures/configurationReview";

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
  settings: { ...DEFAULT_SETTINGS, added_agents: [...FIXTURE_AGENTS], default_clone_dir: DEFAULT_CLONE_DIR, accounts },
});
const autoPulling = snapshotState({
  ...layout,
  settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: true, added_agents: [...FIXTURE_AGENTS], default_clone_dir: DEFAULT_CLONE_DIR, accounts },
});
/** The agent tags this machine has: Claude Code and Grok Build tagged. */
const MACHINE_AGENT_TAGS = { claude: ["personal"], grok: ["trial"] };

/** The providers the machine of the accounts scenarios serves: the registered ones with one this build has no presentation
 * for beside them, for a document that carries a setting of it. */
const ACCOUNTS_PROVIDERS = [...FIXTURE_PROVIDERS, UNPRESENTED_PROVIDER];

/** The machine the accounts of a document are imported on: Grok is not among its added agents, its default account
 * already has a name of its own, Codex's default account is turned off, and Claude Code and Grok Build carry tags. */
const accountsMachine: Scenario["state"] = {
  ...snapshotState({
    ...layout,
    settings: {
      ...DEFAULT_SETTINGS,
      added_agents: ["claude", "codex"],
      default_clone_dir: DEFAULT_CLONE_DIR,
      accounts,
      default_account_names: { grok: "Main" },
      disabled_default_accounts: ["codex"],
      agent_tags: MACHINE_AGENT_TAGS,
    },
  }),
  providers: ACCOUNTS_PROVIDERS,
};
/** The values in effect on that machine of the provider settings an export carries. */
const MACHINE_PROVIDER_SETTINGS = currentExportedProviderSettings(effectiveProviderSettings({}, ACCOUNTS_PROVIDERS), ACCOUNTS_PROVIDERS);
const noConsoleState = snapshotState({
  consoles: [],
  projects: [],
  sessions: [],
  settings: { ...DEFAULT_SETTINGS, added_agents: [...FIXTURE_AGENTS], default_clone_dir: DEFAULT_CLONE_DIR, accounts },
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
// The "What to import" checkboxes stand above the tab bar, and two of them (Settings and Agent accounts) share a name with a
// tab or a control behind the dialog's modal layer, so the tabs are pressed with `pressTab`, and the checkboxes with
// `press`, which finds them first and leaves out what the layer holds inert.
const toggleConsoles = (ui: Ui) => ui.press(ui.t("configuration.import.tab.consoles"));
const toggleAccounts = (ui: Ui) => ui.press(ui.t("settings.section.accounts"));
const toggleSettings = (ui: Ui) => ui.press(ui.t("settings.title"));
const settingsTab = (ui: Ui) => ui.pressTab(ui.t("settings.title"));
/** Checks the Settings, which brings every section the file carries, then unchecks in the Settings tab each section but
 * `keep`: a card for each of them stays collapsed under its title. */
const settingsKeeping = (...keep: SettingsSection[]): Step[] => [
  toggleSettings,
  settingsTab,
  ...SETTINGS_SECTIONS.filter((section) => !keep.includes(section)).map((section): Step => (ui) => ui.press(ui.t(SECTION_LABEL[section]))),
];
const accountsTab = (ui: Ui) => ui.pressTab(ui.t("configuration.import.tab.attention", { tab: ui.t("settings.section.accounts") }));
const consolesTab = (ui: Ui) => ui.pressTab(ui.t("configuration.import.tab.attention", { tab: ui.t("configuration.import.tab.consoles") }));
const leaveAccountOut = (account: string): Step[] => [
  (ui) => ui.press(ui.t("configuration.import.accounts.choice.label", { account })),
  (ui) => ui.press(ui.t("configuration.import.choice.skip")),
];
const hoverAccountMark = (key: "directoryInUseMark" | "invalidMark", account: string): Step => (ui) =>
  ui.hoverStatusMark(ui.t(`configuration.import.accounts.status.${key}`, { account }));
const importFromWelcome = [(ui: Ui) => ui.press(ui.t("welcome.importConfiguration")), chooseFile];
const exportScope = (scope: "all" | "none") => (ui: Ui) => ui.press(ui.t(`configuration.export.scope.${scope}`));
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

/** The settings an inspection says its document carries, without this machine's values beside them: what the document
 * does not carry (a default clone directory that is not usable here) is left out, not set to `null`. */
export function carriedSettings(settings: InspectedSettings): DocumentSettings {
  return Object.fromEntries(
    Object.entries(settings).filter(([key, value]) => !key.startsWith("current_") && key !== "foreign_default_clone_dir" && value !== null),
  ) as DocumentSettings;
}

/** The settings that have a value of the document and one of this machine's in an inspection. */
const SETTING_KEYS = [
  "auto_close_idle_shell_terminals",
  "auto_pull_repositories",
  "auto_delete_unresumable_archives",
  "auto_approve_session_requests",
  "auto_approve_resume_requests",
  "auto_approve_account_switches",
  "auto_approve_handoffs",
  "ask_questions_as_choices",
  "resume_sessions_on_start",
  "keep_awake",
  "keep_awake_when",
  "git_workflow",
] as const;
type SettingKey = (typeof SETTING_KEYS)[number];

/** An inspection's settings of a document whose every setting is the one this machine already has (the settings a fresh
 * installation starts with) except what `carried` sets; the document carries no default clone directory, so that line is
 * not shown. */
function inspectedSettings(carried: Partial<InspectedSettings> = {}): InspectedSettings {
  const own = Object.fromEntries(SETTING_KEYS.flatMap((key) => [[key, DEFAULT_SETTINGS[key]], [`current_${key}`, DEFAULT_SETTINGS[key]]])) as Pick<InspectedSettings, SettingKey | `current_${SettingKey}`>;
  return { ...own, default_clone_dir: null, foreign_default_clone_dir: false, current_default_clone_dir: DEFAULT_CLONE_DIR, ...carried };
}

/** One console's document: no settings; a project at its recorded path, one that can
 * only be cloned, and one nothing places. */
const SINGLE = inspected({
  consoles: [
    {
      name: "Documentation",
      account_fallbacks: [],
      projects: [
        inspectedProject({
          name: "Style Guide",
          path: "/Users/dev/code/style-guide",
          offers: { exported_path: "/Users/dev/code/style-guide", default_clone_dir: null, clone: null },
          default: "exported_path",
        }),
        inspectedProject({
          name: "Handbook",
          path: "/Users/ana/work/handbook",
          remote_url: "https://github.com/acme/handbook.git",
          offers: { exported_path: null, default_clone_dir: null, clone: `${DEFAULT_CLONE_DIR}/handbook` },
          default: "clone",
        }),
        inspectedProject({ name: "Releases", path: "/Users/ana/work/releases" }),
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
        inspectedProject({
          name: "Warehouse slotting optimisation service with the seasonal demand forecasting and replenishment rules",
          path: "/Users/ana/code/slotting",
          offers: { exported_path: "/Users/ana/code/slotting", default_clone_dir: null, clone: null },
          default: "exported_path",
          account_fallbacks: [fallback("codex", "Personal")],
        }),
        inspectedProject({
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
        inspectedProject({
          name: "Website",
          path: "website",
          remote_url: "git@github.com:acme/website.git",
          offers: { exported_path: null, default_clone_dir: `${DEFAULT_CLONE_DIR}/website`, clone: null },
          default: "default_clone_dir",
        }),
        inspectedProject({
          name: "Design System",
          path: "design-system",
          remote_url: "https://github.com/acme/design-system.git",
          offers: { exported_path: null, default_clone_dir: null, clone: `${DEFAULT_CLONE_DIR}/design-system` },
          default: "clone",
        }),
        inspectedProject({ name: "Notes", path: "notes" }),
      ],
    },
  ],
});

/** A document written on Windows, read here: no project's path names anything on this machine, so none derives a directory.
 * The project with a remote is offered a clone, as the daemon does for a path it cannot use; the other has nothing to place it. */
const FOREIGN_PATHS = inspected({
  consoles: [
    {
      name: "Work laptop",
      account_fallbacks: [],
      projects: [
        inspectedProject({
          name: "Design System",
          path: "C:\\Users\\ana\\work\\design-system",
          foreign_path: true,
          remote_url: "https://github.com/acme/design-system.git",
          offers: { exported_path: null, default_clone_dir: null, clone: `${DEFAULT_CLONE_DIR}/design-system` },
          default: "clone",
        }),
        inspectedProject({
          name: "Notes",
          path: "C:\\Users\\ana\\Documents\\Work\\Clients\\Northwind\\Archive\\2019\\Quarterly-Reports\\final-notes",
          foreign_path: true,
        }),
      ],
    },
  ],
});

/** The same document with every setting the one this machine has, except the automatic pull, which is off where this
 * machine has it on. */
const WITH_SETTING_OFF: Omit<Inspection, "type"> = {
  ...SINGLE,
  settings: inspectedSettings({ auto_pull_repositories: false, current_auto_pull_repositories: true }),
  settings_sections: [...SETTINGS_SECTIONS],
};

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
        inspectedProject({
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
        inspectedProject({
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
        inspectedProject({
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

/** A full export's document, with `extra` as the client that saved it wrote it beside the settings `settings` reports for
 * it (`WITH_SETTING_OFF`'s by default): the keyboard shortcuts, and the settings only the client holds. The fixture daemon
 * echoes the shortcuts and the client-held values it passes through in the result's `settings_applied`, as the real one
 * does, so a result scenario needs none of its own. */
const documentWith = (extra: Record<string, unknown>, settings: InspectedSettings = WITH_SETTING_OFF.settings!) => ({
  name: FILE.name,
  text: JSON.stringify({ format: "octoboard-configuration", version: 1, settings: { ...carriedSettings(settings), ...extra }, consoles: [] }, null, 2),
});

/** A document that carries the keyboard shortcuts its client had changed: three that can stand here, one for a
 * command this version has no name for and one that clashes with a key the file gives an earlier entry, which keeps it. */
const SHORTCUTS = {
  keyboard_shortcuts: {
    focusMode: { code: "KeyG", primary: true, shift: true },
    quickCreate: { code: "KeyK", primary: true, shift: true },
    newSession: { code: "KeyO", primary: true },
    teleport: { code: "KeyT", primary: true, shift: true },
    nextConsoleSession: { code: "KeyK", primary: true, shift: true },
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
  foreign_directory: false,
  proposed_directory: resolvedAccountDirectory(extra.directory),
  status: "new",
  in_use_by: null,
  error: null,
  disabled: false,
  ...extra,
});

/** The agent tags the file of those scenarios carries: Codex with several, and Claude Code with none, which replaces the
 * tag it has here. Grok Build is not listed, so its tag stays as it is. */
const FILE_AGENT_TAGS = { codex: ["review", "backend", "nightly"], claude: [] };

/** The setting of a provider without a presentation module the file carries, which no provider has a heading for. */
const UNPRESENTED_FILE_SETTING = { review_mode: "careful" };

/** The settings of the Agent accounts section the file carries: the two switches the file turns on, the third that it
 * leaves as this machine has it, Claude Code's approval mode and Codex compatibility, Codex's approval reviewer and
 * Claude compatibility (both compatibility switches on, where this machine has every one off), the setting of a provider
 * this build has no words for, and the tags above. */
const FILE_AGENT_SETTINGS: DocumentAgentSettings = {
  auto_select_account: true,
  auto_switch_on_usage_limit: true,
  auto_start_quota_windows: false,
  agent_tags: FILE_AGENT_TAGS,
  provider_settings: {
    claude: { approval_mode: "manual", codex_compatibility: true },
    codex: { approval_reviewer: "user", claude_compatibility: true },
    [UNPRESENTED_PROVIDER.id]: UNPRESENTED_FILE_SETTING,
  },
};

/** What the review says of those settings on `accountsMachine`: its own values beside them. */
const ACCOUNTS_AGENT_SETTINGS: InspectedAgentSettings = {
  ...FILE_AGENT_SETTINGS,
  current_auto_select_account: DEFAULT_SETTINGS.auto_select_account,
  current_auto_switch_on_usage_limit: DEFAULT_SETTINGS.auto_switch_on_usage_limit,
  current_auto_start_quota_windows: DEFAULT_SETTINGS.auto_start_quota_windows,
  current_provider_settings: MACHINE_PROVIDER_SETTINGS,
  current_agent_tags: MACHINE_AGENT_TAGS,
};

/** A document's accounts met on `accountsMachine`. Claude Code's: three new ones (one is disabled), two whose directory
 * another account uses (an account of its own, and the default account), one whose name the Add account form refuses, its
 * default account to be named and turned off. Codex's: one that exists, a new one and one whose directory is a bare name, which is no directory
 * (an older or hand-edited file; an export always writes `~/...` or an absolute path), and its default account, which
 * this machine has turned off, turned on. Grok, to be added, with a default name that is not applied. Beside them the
 * settings of the Agent accounts section. */
const ACCOUNTS: InspectedAccounts = {
  agents_to_add: ["grok"],
  agents: [
    {
      agent: "claude",
      default_name: "Personal",
      default_name_applies: true,
      default_disabled: true,
      default_disabled_applies: true,
      accounts: [
        inspectedAccount({ name: "Work", directory: "~/.claude-work" }),
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
      default_disabled: false,
      default_disabled_applies: true,
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
    { agent: "grok", default_name: "Main", default_name_applies: false, default_disabled: null, default_disabled_applies: false, accounts: [] },
  ],
  agent_settings: ACCOUNTS_AGENT_SETTINGS,
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
 * and for Codex, one of the new ones disabled, and nothing that needs attention. A document of before the Agent accounts
 * settings travelled with the accounts, so it carries none. */
const ACCOUNTS_ONLY = inspected({
  consoles: [],
  accounts: {
    agents_to_add: [],
    agents: [
      {
        agent: "claude",
        default_name: null,
        default_name_applies: false,
        default_disabled: null,
        default_disabled_applies: false,
        accounts: [
          inspectedAccount({ name: "Studio", directory: "~/.claude-studio", status: "existing" }),
          inspectedAccount({ name: "Client", directory: "~/.claude-client" }),
        ],
      },
      {
        agent: "codex",
        default_name: null,
        default_name_applies: false,
        default_disabled: null,
        default_disabled_applies: false,
        accounts: [
          inspectedAccount({ name: "Home", directory: "~/.codex-home", status: "existing" }),
          inspectedAccount({ name: "Personal", directory: "~/.codex-personal", disabled: true }),
        ],
      },
    ],
    agent_settings: null,
  },
});

/** A document of accounts alone written on Windows, read here: Claude Code's one account has a Windows directory, which names
 * nothing on this machine, so it cannot be created as it is and the daemon, which can only say the directory is not
 * absolute, marks it as written on another operating system. */
const ACCOUNT_FOREIGN_DIRECTORY = inspected({
  consoles: [],
  accounts: {
    agents_to_add: [],
    agents: [
      {
        agent: "claude",
        default_name: null,
        default_name_applies: false,
        default_disabled: null,
        default_disabled_applies: false,
        accounts: [
          inspectedAccount({
            name: "Laptop",
            directory: "C:\\Users\\ana\\.claude-laptop",
            foreign_directory: true,
            proposed_directory: null,
            status: "invalid",
            error: failure("config_dir_not_absolute", { agent: "Claude Code" }, "the Claude Code config directory must be an absolute path or start with `~/`"),
          }),
        ],
      },
    ],
    agent_settings: null,
  },
});

/** A document of the Git settings alone, every one the machine's own, whose default clone directory was written on
 * Windows: the document carries none this machine can use, and says it had one. */
const SETTINGS_FOREIGN_CLONE_DIR = inspected({
  consoles: [],
  settings: inspectedSettings({ default_clone_dir: null, foreign_default_clone_dir: true }),
  settings_sections: ["git"],
});

/** The accounts of `ACCOUNTS` that cannot be created as the file has them, alone: a directory another account uses, one the
 * default account uses, and a name Claude Code already has. A document of these alone, so that all three rows stand in
 * view and their marks can be read. */
const ACCOUNTS_UNUSABLE = inspected({
  consoles: [],
  accounts: {
    agents_to_add: [],
    agents: [{ ...ACCOUNTS.agents[0], default_name: null, default_name_applies: false, default_disabled: null, default_disabled_applies: false, accounts: ACCOUNTS.agents[0].accounts.slice(3) }],
    agent_settings: null,
  },
});

/** `object` with only the `keys` it has. */
const pick = <T extends object, K extends keyof T>(object: T, keys: K[]): Pick<T, K> => Object.fromEntries(keys.filter((key) => key in object).map((key) => [key, object[key]])) as Pick<T, K>;

const importedAccount = (name: string, status: ImportedAccount["status"], extra: Partial<ImportedAccount> = {}): ImportedAccount => ({
  name,
  status,
  account: status === "created" || status === "existing" ? `a-${name.toLowerCase()}` : null,
  error: null,
  ...extra,
});

/** What importing `ACCOUNTS_REVIEW` with its defaults came to, the accounts of Claude Code first: Work and Archive created;
 * Sandbox's directory taken since the review; Clients, Original and Default left out as the review had them; Codex's Home
 * already there, Reports created and Backup left out as the review had it; Grok added; the default accounts of Claude Code
 * named as the file has it and, with Codex's, disabled or enabled as the file says (Claude Code's disabled, Codex's enabled);
 * and the agent account settings applied.
 * The Work laptop console's account Work is bound now, so only the console's other fallbacks stand. */
const ACCOUNTS_RESULT_ACCOUNTS: ImportedAccounts = {
  agents_added: ["grok"],
  agents: [
    {
      agent: "claude",
      default_name_applied: true,
      default_disabled_applied: true,
      accounts: [
        importedAccount("Work", "created"),
        importedAccount("Archive", "created"),
        importedAccount("Sandbox", "failed", {
          error: failure("config_dir_taken", { agent: "Claude Code", name: "Studio" }, "the directory is already used by `Studio` for Claude Code"),
        }),
        importedAccount("Clients", "left_out"),
        importedAccount("Original", "left_out"),
        importedAccount("Default", "left_out"),
      ],
    },
    {
      agent: "codex",
      default_name_applied: false,
      default_disabled_applied: false,
      accounts: [importedAccount("Home", "existing"), importedAccount("Reports", "created"), importedAccount("Backup", "left_out")],
    },
    { agent: "grok", default_name_applied: false, default_disabled_applied: null, accounts: [] },
  ],
  agent_settings_applied: FILE_AGENT_SETTINGS,
  agent_settings_error: null,
};

/** `MIXED_RESULT`'s consoles and projects, and the accounts above, the settings not asked for. */
const ACCOUNTS_RESULT = imported({
  accounts: ACCOUNTS_RESULT_ACCOUNTS,
  consoles: MIXED_RESULT.consoles.map((console) => (console.name === "Work laptop" ? { ...console, account_fallbacks: [] } : console)),
});

/** `ACCOUNTS_REVIEW` imported with the consoles and projects unchecked and the Git and Cleanup settings checked, where nothing
 * went wrong: every account the review would create was created (Sandbox's directory was not taken after all), only the
 * settings of those two sections were applied (the daemon reports no other), and no console exists, as the daemon answers
 * `apply_consoles` false. */
const ACCOUNTS_SETTINGS_RESULT = imported({
  settings_applied: pick(MIXED_RESULT.settings_applied!, ["default_clone_dir", "auto_pull_repositories", "git_workflow", "auto_delete_unresumable_archives"]),
  clone_dir_applied: MIXED_RESULT.clone_dir_applied,
  accounts: {
    ...ACCOUNTS_RESULT_ACCOUNTS,
    agents: ACCOUNTS_RESULT_ACCOUNTS.agents.map((agent) => ({
      ...agent,
      accounts: agent.accounts.map((account) =>
        account.status === "failed" ? importedAccount(account.name, "created") : account,
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
      { agent: "claude", default_name_applied: false, default_disabled_applied: null, accounts: [importedAccount("Studio", "existing", { account: "a-studio" }), importedAccount("Client", "created")] },
      { agent: "codex", default_name_applied: false, default_disabled_applied: null, accounts: [importedAccount("Home", "existing", { account: "a-home" }), importedAccount("Personal", "created")] },
    ],
    agent_settings_applied: null,
    agent_settings_error: null,
  },
});

/** `ACCOUNTS_ONLY` as a document that carries the Agent accounts settings, which could not be written when it was imported
 * on `accountsMachine`: the accounts all went through. */
const ACCOUNTS_ONLY_SETTINGS = inspected({ consoles: [], accounts: { ...ACCOUNTS_ONLY.accounts!, agent_settings: ACCOUNTS_AGENT_SETTINGS } });
const AGENT_SETTINGS_FAILED_RESULT = imported({
  consoles: [],
  accounts: {
    ...ACCOUNTS_ONLY_RESULT.accounts!,
    agent_settings_error: failure("internal_error", { detail: "the settings could not be written" }, "the settings could not be written"),
  },
});

/** `WITH_SETTING_OFF` with the accounts of `ACCOUNTS_ONLY` as well, imported with its defaults and the settings asked for:
 * the consoles and the accounts all went through, and only the settings could not be written. */
const WITH_ACCOUNTS_SETTING_OFF: Omit<Inspection, "type"> = { ...WITH_SETTING_OFF, accounts: ACCOUNTS_ONLY.accounts };
const SETTINGS_FAILED_RESULT: Omit<Imported, "type"> = { ...SETTINGS_FAILED_CONSOLES_RESULT, accounts: ACCOUNTS_ONLY_RESULT.accounts };

/** `REVIEW` with one setting the file leaves as this machine has it, so that the review has a line that reads No change
 * beside the ones that change: asking questions as choices, which the file also has on. */
const SETTINGS_REVIEW: Omit<Inspection, "type"> = {
  ...REVIEW,
  settings: { ...REVIEW.settings!, ask_questions_as_choices: true },
};

/** The settings only a client holds, as a document saved by a desktop window carries them, each unlike the window the
 * gallery shows them in: the system appearance, the GitHub Markdown style where the window has the Tailwind one, the
 * session and project order that is not the default, the app running in the background where it does not, and the menu
 * bar icon hidden where the window shows it. */
const CLIENT_VALUES = {
  appearance: "system",
  markdown_style: "github",
  // Each part unlike the order the window has, which a fresh one holds.
  session_order: { sessions: "started", active_projects: "used", inactive_projects: "used" },
  run_in_background: true,
  show_menu_bar_icon: false,
};

/** The document an export of the sections `inspection` lists would be: what the file holds is what the inspection
 * claims, since the review words the settings only the client holds (General's appearance, Markdown style and
 * session order; App behavior's), the language (the daemon's own, which the file carries in `settings` all the
 * same) and the keyboard shortcuts from the file's own text, so a section the inspection lists that the file gave
 * nothing for would be a card with no line. A document that carries no settings is the bare `FILE`. General carries
 * the Markdown style and the session order, but no language: a result scenario applies what its file carries, and a
 * language of the file's would turn the window into it whatever language the gallery was opened in. `held` is what
 * a scenario that is about those values has the file carry as well, or instead. */
const fileOf = (inspection: Omit<Inspection, "type">, held?: Record<string, unknown>) => {
  if (!inspection.settings) return FILE;
  const sections = inspection.settings_sections;
  return documentWith(
    {
      ...(sections.includes("general") && { markdown_style: CLIENT_VALUES.markdown_style, session_order: CLIENT_VALUES.session_order }),
      ...(sections.includes("keyboard_shortcuts") && { keyboard_shortcuts: { focusMode: SHORTCUTS.keyboard_shortcuts.focusMode } }),
      ...held,
    },
    inspection.settings,
  );
};

/** The document of those, beside the keyboard shortcuts of `SHORTCUTS`, with a language other than the one the daemon
 * holds: the fixture daemon seeds that with the language the window shows, as the app asks it to when it holds none. */
const CLIENT_FILE = documentWith({ ...SHORTCUTS, ...CLIENT_VALUES, interface_language: "ja" }, SETTINGS_REVIEW.settings!);

/** The document of the result: the same in English, the default viewing language, so that importing it leaves the
 * window as it is. Under another viewing language the import makes the fixture daemon hold English and the window
 * follows it, which is expected. */
const CLIENT_RESULT_FILE = documentWith({ ...SHORTCUTS, ...CLIENT_VALUES, interface_language: "en" }, SETTINGS_REVIEW.settings!);

/** What importing `SETTINGS_REVIEW` with the consoles unchecked came to: no console, as the daemon answers `apply_consoles` false. */
const SETTINGS_RESULT: Omit<Imported, "type"> = {
  ...MIXED_RESULT,
  consoles: [],
  settings_applied: { ...MIXED_RESULT.settings_applied!, ask_questions_as_choices: true, interface_language: "en" },
};

/** `REVIEW` of a file of an earlier version: its accounts region holds the Agent accounts settings alone, with no agent to add
 * and no account, which an upgrade of a file that had settings and no accounts leaves. */
const REVIEW_AGENT_SETTINGS_ONLY: Omit<Inspection, "type"> = {
  ...REVIEW,
  accounts: {
    agents_to_add: [],
    agents: [],
    agent_settings: {
      auto_select_account: true,
      current_auto_select_account: DEFAULT_SETTINGS.auto_select_account,
      auto_switch_on_usage_limit: DEFAULT_SETTINGS.auto_switch_on_usage_limit,
      current_auto_switch_on_usage_limit: DEFAULT_SETTINGS.auto_switch_on_usage_limit,
      auto_start_quota_windows: DEFAULT_SETTINGS.auto_start_quota_windows,
      current_auto_start_quota_windows: DEFAULT_SETTINGS.auto_start_quota_windows,
      provider_settings: {},
      current_provider_settings: currentExportedProviderSettings(effectiveProviderSettings({}, FIXTURE_PROVIDERS), FIXTURE_PROVIDERS),
      agent_tags: {},
      current_agent_tags: {},
    },
  },
};

/** The default clone directory of a document of the Git settings alone, which is not this machine's. */
const CLONE_DIR_FILE_DIR = "/Users/ana/Code/clones";
const CLONE_DIR_SETTINGS = inspected({
  consoles: [],
  settings: inspectedSettings({ default_clone_dir: CLONE_DIR_FILE_DIR, current_default_clone_dir: DEFAULT_CLONE_DIR }),
  settings_sections: ["git"],
});

/** What importing `CLONE_DIR_SETTINGS` came to: the default clone directory written, then the rest of the Git settings
 * could not be. */
const CLONE_DIR_KEPT_RESULT = imported({
  consoles: [],
  settings_error: failure("internal_error", { detail: "the settings could not be written" }, "the settings could not be written"),
  clone_dir_applied: CLONE_DIR_FILE_DIR,
});

const reviewing = (extra: Partial<ConfigurationFixture> = {}): ConfigurationFixture => {
  const inspection = extra.inspection ?? REVIEW;
  return { file: "code" in inspection ? FILE : fileOf(inspection), inspection, imported: MIXED_RESULT, ...extra };
};

/** `ACCOUNTS_REVIEW` with the console that cannot be created first, ahead of the one whose project is being cloned: the daemon
 * writes the default clone directory, creates the accounts, applies the Agent accounts settings, then creates each console
 * with its projects in the document's order, then applies the rest of the settings, so a console already failed and a clone
 * running can be on screen together only in this order. */
const PROGRESS_REVIEW: Omit<Inspection, "type"> = {
  ...ACCOUNTS_REVIEW,
  consoles: [ACCOUNTS_REVIEW.consoles[1], ACCOUNTS_REVIEW.consoles[0], ACCOUNTS_REVIEW.consoles[2]],
};

/** Where `PROGRESS_REVIEW` imported with its defaults and the settings ticked stands part-way (the review's accounts left out
 * and the project nothing places are not announced): the default clone directory and the accounts all settled, one failed,
 * and the Agent accounts settings done; the first console failed with its project not tried; the second console created,
 * which the daemon reports done at once, with one project failed, one done and the last being cloned; the third console and
 * the rest of the settings still waiting. */
const IMPORT_UNDER_WAY: ImportProgressFixture[] = [
  { item: { kind: "clone_dir" }, state: "done" },
  { item: { kind: "account", agent: "claude", account: 0 }, state: "done" },
  { item: { kind: "account", agent: "claude", account: 1 }, state: "done" },
  {
    item: { kind: "account", agent: "claude", account: 2 },
    state: "failed",
    error: failure("config_dir_taken", { agent: "Claude Code", name: "Studio" }, "the directory is already used by `Studio` for Claude Code"),
  },
  { item: { kind: "account", agent: "codex", account: 0 }, state: "done" },
  { item: { kind: "account", agent: "codex", account: 1 }, state: "done" },
  { item: { kind: "agent_settings" }, state: "done" },
  {
    item: { kind: "console", console: 0 },
    state: "failed",
    error: failure("internal_error", { detail: "the data directory cannot be written" }, "the data directory cannot be written"),
  },
  { item: { kind: "project", console: 0, project: 0 }, state: "not_attempted" },
  { item: { kind: "console", console: 1 }, state: "done" },
  {
    item: { kind: "project", console: 1, project: 0 },
    state: "failed",
    error: failure("path_not_found", { path: "/Users/ana/code/website" }, "/Users/ana/code/website does not exist"),
  },
  { item: { kind: "project", console: 1, project: 1 }, state: "done" },
  { item: { kind: "project", console: 1, project: 2 }, state: "cloning" },
];

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
    description: "The Consoles and projects radio has three options, each only a label with no description under it: \"Console Main and its projects\" (the sidebar's console, named), \"All consoles and their projects\" and \"Do not export consoles\", the first chosen. Under Include full project paths, a group headed Settings holds six checkboxes in the order the Settings page lists its sections: General, App behavior, Git, Agent behavior, Keyboard shortcuts and Cleanup. After it, a group headed Agent accounts holds one checkbox, Include agent accounts. All seven checkboxes are unchecked, each with a one-sentence note under it that begins \"Includes\". App behavior's note, on this platform with the desktop app's background run and menu bar icon (named as the host names it: notification area icon on Windows and Linux), lists them all: \"Includes resuming sessions on start, running in the background, the menu bar icon, preventing sleep and closing idle terminals.\" Check that the lists and their notes lay out evenly, with the notes wrapping under their labels, that the label of each group sits 16px above its first option as the Consoles and projects radio group does, with the same 16px between options, and that each group starts 24px after what stands above it. Include full project paths starts unchecked: its description says what checking it does (\"Once checked, the file records each project's full directory path, so an import can look for the project there first.\") and no alert stands beneath it. Export closes the dialog and raises a toast naming the saved file.",
    state,
    backgroundRun: true,
    menuBarIcon: true,
    steps: exportFromSettings,
  },
  {
    id: "configuration-export-with-paths",
    group: GROUP,
    category: "normal",
    title: "Export, with full project paths included",
    description: "With Include full project paths checked, its description changes to \"An import first looks for each project at its recorded full path.\" and a warning alert stands directly under the option, titled \"The file contains project paths.\": the description says project paths may reveal the user name and folder names; a separate line beneath says \"Share the file only with people you trust.\" No mark sits beside the checkbox.",
    state,
    steps: [...exportFromSettings, toggleExportPaths],
  },
  {
    id: "configuration-export-everything-only",
    group: GROUP,
    category: "normal",
    title: "Export, with no console to export on its own",
    description: "The Consoles and projects radio offers only \"All consoles and their projects\", the one chosen, and \"Do not export consoles\", each a label with no description; there is no option for a console of the sidebar's. Include full project paths is shown, unchecked. Without the desktop app's background run and menu bar icon, App behavior's note reads \"Includes resuming sessions on start, preventing sleep and closing idle terminals.\"",
    state: noConsoleState,
    steps: exportFromSettings,
  },
  {
    id: "configuration-export-accounts",
    group: GROUP,
    category: "normal",
    title: "Export, with the accounts included",
    description: "With Include agent accounts checked, under the Agent accounts group heading, its description (\"Includes account names, configuration directories, disabled settings, account order, added agents and the agent account settings.\") goes on to say that sharing the default configuration and login credentials are not included, so sharing has to be turned on and the accounts logged in again after an import. Include full project paths, still unchecked, keeps its own description about project paths and no alert stands beneath it: checking Include agent accounts does not change it.",
    state,
    steps: [...exportFromSettings, toggleExportAccounts],
  },
  {
    id: "configuration-export-accounts-only",
    group: GROUP,
    category: "normal",
    title: "Export, only the accounts",
    description: "With Do not export consoles chosen, only Include agent accounts, under the Agent accounts group heading, is checked and Export is available. Include full project paths is not shown at all, as no console is exported to have project paths: account directories are written whatever the choice.",
    state,
    steps: [...exportFromSettings, exportScope("none"), toggleExportAccounts],
  },
  {
    id: "configuration-export-nothing",
    group: GROUP,
    category: "exceptional",
    title: "Export, with nothing chosen to export",
    description: "Do not export consoles is chosen and none of the checkboxes is checked, so there is nothing to export: Export is unavailable. Include full project paths is not shown.",
    state,
    steps: [...exportFromSettings, exportScope("none")],
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
    id: "configuration-import-choose",
    group: GROUP,
    category: "normal",
    title: "Import, choosing the file",
    description: "Opened from the welcome screen, where a new computer starts. A single sentence asks for an exported configuration file; details about its contents and import choices appear after choosing it. The picker is never reported dismissed, as in a browser without the file input's cancel event: Choose file shows that it is waiting and ignores another press, while Cancel, the close button and Escape still work.",
    state: noConsoleState,
    configuration: { file: "pending" },
    steps: [
      (ui) => ui.press(ui.t("welcome.importConfiguration")),
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
    description: "A file that is not a configuration, one of a version this one cannot read, one too large or not text, and an avatar that is too large or not an image refuse in the same error alert dialog over the import dialog, titled \"Could not import the configuration\", whose remedy asks for a file exported by a compatible Octoboard version (or another avatar), with no Error details.",
    state,
    configuration: refusal("notes.json", JSON.stringify({ hello: "world" }), failure("configuration_not_recognised", {}, "not a configuration document")),
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review-reread-refused",
    group: GROUP,
    category: "exceptional",
    title: "Import review, the file must be chosen again",
    description: "The review is out of date and the daemon can no longer read the file's format version. An Alert above the review says why, followed by Choose file again and a Choose file button instead of Try again. Import stays unavailable until a current file is read; its description is the Alert's title.",
    state: { ...state, settings: { ...state.settings!, versions: { ...state.settings!.versions, auto_pull_repositories: 2 } } },
    configuration: {
      file: { name: FILE.name, text: JSON.stringify({ format: "octoboard-configuration", version: 3, settings: carriedSettings(CLONE_DIR_SETTINGS.settings!), consoles: [] }) },
      inspection: CLONE_DIR_SETTINGS,
      reinspection: failure("configuration_version_unsupported", { version: "3", oldest: "1", supported: "2" }, "configuration format version 3 is not supported"),
    },
    steps: [...importFromSettings, chooseFile],
  },
  {
    id: "configuration-import-refused-unreadable",
    group: GROUP,
    category: "exceptional",
    title: "Import, a document that cannot be read",
    description: "The refusal says only that the configuration file could not be read; the parser's text about the unexpected type sits only in the Error details disclosure, closed here. A file the system could not read, or an export that could not be saved, uses the same alert with its own sentence and remedy.",
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
    description: "The intro line reads \"These are the contents of octoboard-configuration.json.\". Above the review, a What to import group holds three checkboxes: Consoles and projects, Settings and Agent accounts. Consoles and projects is checked; Settings is unchecked, as the settings of a file change how this machine behaves, and Agent accounts is unchecked because the file's accounts hold only the Agent account settings, with no agent to add and no account. Only a checked part has a tab, but the tab bar stays for a single one: it holds the one tab, Consoles and projects, and the review stands under it, with the targets under this machine's own default clone directory. The panel starts with the note that each console is created as a new one and the existing consoles stay as they are, then the apply-to-all buttons; cards for three consoles follow, with the clone note last. Project rows show each state: found at its exported path (only \"Placed in\"), found in the default clone directory, can be cloned (\"Clone from\" and \"Clone into\") and not found, left out, with a long exported path that wraps and no state line under its name. A warning triangle follows the name of the Work laptop console in its heading and the name of Search API in its row, where an account the file names is not set up here (the popover is in a scenario below); no sentence or alert repeats it. The not-found project, Notes, carries one warning triangle after its name, \"Notes not found, and accounts not set up\": the project is missing and its account falls back too, which are one point and so one mark (its popover is in a scenario below). The last console says it has no projects. The unchecked boxes (here Settings and Agent accounts) show their edge against the dialog. The What to import label sits 16px above its checkboxes. Each console card's header row is a filled band set apart from its body by fill alone, with no line at its edges, and the dividers between project rows are inset by the card's padding.",
    state,
    configuration: reviewing({ inspection: REVIEW_AGENT_SETTINGS_ONLY }),
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review-settings",
    group: GROUP,
    category: "normal",
    title: "Import, review of the settings",
    description: "Settings ticked in the What to import group: the Settings tab appears beside Consoles and projects (with a warning glyph) and is the one open. It holds one card for each section the file carries, titled and ordered as the Settings dialog's sections (General, App behavior, Git, Agent behavior, Keyboard shortcuts, Cleanup), each title a checkbox named by the section, centred on the title line, and all six checked. Lines that turn a setting on carry a muted consequence line (the pull line, \"Automatically pull repositories\", in Git, says the repositories will then be pulled automatically), lines that turn one off do not (their \"Turns off\" badge is neutral, not accented), and \"Ask questions as choices\" (in Agent behavior), which the device already has as the file does, reads No change with nothing under it. General opens with the settings only the window itself holds, each \"Set to the file's\" with the file's value and the current one in brackets: Appearance (System, now Light), Language (日本語, now English, the language the daemon holds and every window follows) and Markdown style (GitHub, now Tailwind); Appearance and Language read No change under a viewing condition that already has the file's value. Sorting follows them: it reads \"Set to the file's\" and lists under the badge the three parts that differ, each as the file's value with what it is now in brackets: Sessions (Recently started, now Status), Active projects (Recently used, now Status) and Inactive projects (Recently used, now Name). App behavior opens with Resume sessions on start, turned on, with its muted consequence line, then holds, as this window offers each of them, the settings only the window itself holds, each \"Set to the file's\" with the file's value and the current one in brackets: Keep running in the background when the window is closed (On, now Off) and Show menu bar icon (Off, now On). Prevent sleep follows them, turned on, with no consequence line under it, and so does When to prevent sleep, \"Set to the file's\" Always (now While sessions work); these two are shown whatever the window offers. The terminal-cleanup switch ends the card. Git starts with Default clone directory, which gives the file's directory and this machine's, each in code style, above the checkbox \"Use the file's default clone directory\", checked, its label in normal text; unchecking it turns the badge to the neutral one and the line to \"<this machine's directory> is kept instead of the file's <file's directory>\". Then come the pull line and the Git workflow line, which reads \"Set to the file's\" and lists under the badge only what applying would change, each item as the file's value with what it is now in brackets: four of its five items (where sessions develop, rebase before delivery, delivery, revert gate; \"Squash commits before delivery\" is the same and is left out). Agent behavior holds the four auto-approve switches and Ask questions as choices. Keyboard shortcuts holds a \"Changed shortcuts\" line that says 3 of the file's shortcuts differ from the defaults and that shortcut changes apply only to the current browser, then lists each of the three as its key caps with the current ones in brackets. Cleanup holds the archive-cleanup switch alone.",
    state,
    backgroundRun: true,
    menuBarIcon: true,
    configuration: reviewing({ file: CLIENT_FILE, inspection: SETTINGS_REVIEW, imported: SETTINGS_RESULT }),
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
    title: "Import, review of a document with consoles, accounts and some of the settings",
    description: "A file with consoles, settings and accounts: the What to import group holds three checkboxes, all checked, so there are three tabs, Consoles and projects, Settings and Agent accounts, the first and the last each with a warning glyph (a project is not found; an account's directory is in use or cannot be used) and Settings without. The Settings tab is the one open: Git and Agent behavior are checked and show their lines, and the other four sections (General, App behavior, Keyboard shortcuts, Cleanup) are unchecked, each collapsed to its title row with the Left out outcome at its right; in Git, the checkbox \"Use the file's default clone directory\" stands under the file's directory. With Git ticked, the file's default clone directory is the one applied, so the targets on the Consoles and projects tab stand under it: Search API is placed in a folder of /Users/ana/Code/clones, Design System and Dotfiles are cloned into folders there, and Website, which this machine finds in its own default clone directory, can be cloned there instead (the apply-to-all buttons count Default clone directory (1) and Clone from remote (3)). With Agent accounts checked and every account left at its proposal, the references to accounts the import would create (the Work laptop console's Claude Code account Work, the Website project's Codex account Reports) carry no warning triangle, while Search API's and Notes's Codex account Personal, which the import does not create, still do. Switching the checkboxes on the page moves the tabs in and out with the list motion; unchecking Git collapses the Git card and puts the targets back under this machine's own default clone directory, and checking a collapsed section opens it again.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      ...settingsKeeping("git", "agent_behavior"),
    ],
  },
  {
    id: "configuration-import-review-tab-tooltip",
    group: GROUP,
    category: "normal",
    title: "Import, review, what a tab's warning glyph says",
    description: "The mouse rests on the Agent accounts tab, whose warning glyph is about an account that cannot be created as it is: a tooltip opens against the tab, saying this part has something that needs attention. It opens in the same way while the tab has keyboard focus.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      (ui) =>
        ui.hoverAnchored(
          ui.t("configuration.import.tab.attention", { tab: ui.t("settings.section.accounts") }),
          ui.t("configuration.import.tab.attentionTip"),
        ),
    ],
  },
  {
    id: "configuration-import-review-accounts",
    group: GROUP,
    category: "normal",
    title: "Import, review of the accounts",
    description: "The Agent accounts tab, opened from a tab bar of Consoles and projects and Agent accounts (the What to import group above it has Consoles and projects and Agent accounts checked, and Settings unchecked), is grouped by agent. The first card, \"Agent account settings\", holds the three switches and nothing per agent: \"Automatically select an account\" and \"Automatically switch accounts at the usage limit\" turn on, each with a muted consequence line, and \"Start usage windows automatically\", which the file leaves as this machine has it, reads No change with nothing under it. Then one card per agent, in this order: Claude Code, Codex, Grok Build and Acme Agent, each titled with its name beside a checkbox, checked. A card holds, in this order, the lines about the agent, its default account lines (which no longer name the agent, the card being its own) and its settings rows named by the setting alone, a Tags row, and under a filled \"Accounts\" band its account rows. Each card's header row and the Accounts band are set apart by their fill alone, with no line at their edges, and the dividers inside a card's body are inset by the card's padding, only the filled bands reaching the card's edges. Claude Code's card: \"The default account will be named Personal.\" and \"The default account will be disabled.\"; Permission approval, \"Set to the file's\" with Manual (now Follow account configuration); Codex compatibility, On (now Off); Tags, No tags (now personal). Codex's card: \"The default account will be enabled.\"; Permission approval, Ask for approval (now Custom (config.toml)); Claude compatibility, On (now Off); Tags, three tags (now No tags). Grok Build's card holds the one line \"Will be added to the agent list.\" (the card's title already names the agent), with no Accounts band (its default name, which is not applied, and its default account, which the file does not mention, have no line). The rows with several values list under the badge only what applying would change, each as the file's value with what it is now in brackets. Acme Agent, a provider this build has no words for, has a card of its own with one settings row, its setting's id, Careful (now Follow account configuration), and no accounts, so no Accounts band. A row's status is a badge after the account's name and before its \"Disabled\" badge: a success \"New\" badge with a check icon, or a neutral \"Already exists, used as it is\" badge with a dot icon; an account that cannot be created has no status badge but a warning mark after its name instead (its popovers are in the scenarios below). Claude Code's rows: Work, New; Archive, New, with the \"Disabled\" badge and its choice set to Leave out (changed from \"Create it\"); Sandbox, New; Clients, Original and Default, each with the warning mark and its choice at Leave out (a directory in use by another account, one in use by the default account, a name Claude Code already has). Clients and Original can be given another directory; Default, whose name is the problem, offers only Leave out. Codex's: Home, \"Already exists, used as it is\", with no choice and no directory line; Reports, New; Backup, with the warning mark, its directory written as the bare name codex-backup, at Leave out. No row says anything about sharing the default configuration, which an import never turns on. Every row but Home has a line led by a folder icon (its tooltip, \"Directory in the file\", opens on hover and on touch) giving the directory as the file writes it. A note says the accounts are created without being logged in.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
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
    description: "A document of accounts alone, with the three that cannot be created as the file has them. The mouse rests on the warning mark after the Clients name: its popover opens above it with no click and no focus taken, saying the directory is already in use by Studio and that the account is left out unless another directory is chosen for it. The directory being in use by the default account, or an account that cannot be created at all, puts its own sentence in the same popover.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_UNUSABLE), inspection: ACCOUNTS_UNUSABLE },
    steps: [...importFromSettings, chooseFile, hoverAccountMark("directoryInUseMark", "Clients")],
  },
  {
    id: "configuration-import-review-accounts-agent-left-out",
    group: GROUP,
    category: "normal",
    title: "Import, review of the accounts with agents left out",
    description: "Claude Code's and Codex's checkboxes on the Agent accounts tab are unchecked: each card collapses to its title row with the Left out outcome at the right, and its default account lines, settings rows and account rows are gone, while the cards of Grok Build and Acme Agent stay open below them. Every account the file cannot create as it is belongs to these two agents, so the Agent accounts tab no longer carries a warning glyph (the Consoles and projects tab keeps its own). The Agent account settings card keeps its three switches whichever agents are checked, and Import stays available. The note that the accounts are created without being logged in is gone too, as no checked agent has an account to create (the review of the accounts shows it with them checked).",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [...importFromSettings, chooseFile, accountsTab, (ui) => ui.press("Claude Code"), (ui) => ui.press("Codex")],
  },
  {
    id: "configuration-import-review-accounts-left-out",
    group: GROUP,
    category: "normal",
    title: "Import, review of the consoles when an account is left out",
    description: "Work, an account the file would create, is set to Leave out on the Agent accounts tab, and the Consoles and projects tab is open again: the warning triangle is back after the Work laptop heading, as that console's Claude Code account Work will not exist. The Website project's Codex account Reports is still created, so its row has no triangle, unlike Search API's and Notes's account Personal, which the file does not create.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [...importFromSettings, chooseFile, accountsTab, ...leaveAccountOut("Work"), consolesTab],
  },
  {
    id: "configuration-import-review-accounts-off-consoles",
    group: GROUP,
    category: "normal",
    title: "Import, review of the consoles when the accounts are not imported",
    description: "With Agent accounts unchecked in the What to import group, the Agent accounts tab is gone, and the references to accounts the import would have created fall back after all: the Work laptop console and the Website project, both marked as such, now carry the warning triangle beside Search API's and Notes's. With one region left, the tab bar stays with the single tab Consoles and projects.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
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
    description: "With Consoles and projects unchecked in the What to import group, its tab is gone and the tab bar holds the single tab Agent accounts, with its content: the file's consoles and projects are not imported, so no project row is asked to be placed.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
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
    description: "With Consoles and projects and Agent accounts both unchecked and Settings left unchecked, no tab is left and the body under the What to import group is empty; Import is unavailable.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleConsoles,
      toggleAccounts,
    ],
  },
  {
    id: "configuration-import-review-settings-left-out",
    group: GROUP,
    category: "normal",
    title: "Import, review with every Settings section left out",
    description: "Settings is the only part checked and every section in its tab is unchecked: each of the six cards is collapsed to its title row with the Left out outcome at its right, and Import is unavailable, as nothing is left to import.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleConsoles,
      toggleAccounts,
      ...settingsKeeping(),
    ],
  },
  {
    id: "configuration-import-review-accounts-only",
    group: GROUP,
    category: "normal",
    title: "Import, review of a document of accounts alone",
    description: "A document that carries only accounts has no What to import group, and its tab bar holds the single tab Agent accounts: the review opens straight on the accounts, as on a new computer with no console. Existing accounts carry the \"Already exists, used as it is\" badge and have no choice or directory line; Personal, a new one, carries the \"New\" and \"Disabled\" badges, in that order; nothing carries a warning, and there are no lines about agents to add or default accounts and no Agent account settings card, as the file carries none.",
    state: noConsoleState,
    configuration: { file: fileOf(ACCOUNTS_ONLY), inspection: ACCOUNTS_ONLY, imported: ACCOUNTS_ONLY_RESULT },
    steps: importFromWelcome,
  },
  {
    id: "configuration-import-review-single",
    group: GROUP,
    category: "normal",
    title: "Import, review of one console's document",
    description: "A single console's document has no What to import group, as it carries only that region, and its tab bar holds the single tab Consoles and projects, over one card. \"Default clone directory (0)\" is disabled because no project offers it, and the clone note follows the card. The project nothing places, Releases, has no state line, only a warning triangle after its name.",
    state,
    configuration: { file: fileOf(SINGLE), inspection: SINGLE, imported: CLEAN_RESULT },
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
    configuration: { file: fileOf(LONG_NAMES), inspection: LONG_NAMES },
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
    configuration: { file: fileOf(NO_PATHS), inspection: NO_PATHS },
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review-foreign-paths",
    group: GROUP,
    category: "normal",
    title: "Import, review of a document written on another operating system",
    description: "Each project's \"Exported path\" is a Windows path, which names nothing on this machine, and no loose sentence says so under the fields: a mark after the path does. Design System, offered a clone (\"Clone from\" and \"Clone into\"), has a note mark right after its \"Exported path\" value, named \"Exported path of Design System cannot be used\"; its popover says only that the exported path was written on another operating system, so it cannot be used here, with no remedy. Notes, found nowhere, whose long path wraps, has only the warning triangle after its name; its popover opens with the same sentence, then says it is skipped unless a directory is chosen for it. The accounts and the Git settings of such a document are in the two scenarios below.",
    state,
    configuration: { file: fileOf(FOREIGN_PATHS), inspection: FOREIGN_PATHS },
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review-foreign-account",
    group: GROUP,
    category: "normal",
    title: "Import, review of an account whose directory was written on another operating system",
    description: "A document of accounts alone, whose Laptop account has a Windows directory. Its folder-icon directory line shows the Windows path with no sentence under the account; its choice is Leave out. The mouse rests on the warning mark after the Laptop name: its popover opens above it with no click and no focus taken, saying first that the directory in the file was written on another operating system, so it cannot be used here, then that the account is left out unless another directory is chosen for it, and never that the path is not absolute.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNT_FOREIGN_DIRECTORY), inspection: ACCOUNT_FOREIGN_DIRECTORY },
    steps: [...importFromSettings, chooseFile, hoverAccountMark("invalidMark", "Laptop")],
  },
  {
    id: "configuration-import-review-foreign-clone-dir",
    group: GROUP,
    category: "normal",
    title: "Import, review of a default clone directory written on another operating system",
    description: "A document of the Git settings alone, so there is no What to import group, and its tab bar holds the single tab Settings. In the Git card, Default clone directory reads No change and the line under it says the file's default clone directory was written on another operating system, so it cannot be used here, and that the current default clone directory is kept. It has no directory to show and no checkbox to choose the file's directory, unlike a directory this machine can use.",
    state,
    configuration: { file: fileOf(SETTINGS_FOREIGN_CLONE_DIR), inspection: SETTINGS_FOREIGN_CLONE_DIR },
    steps: [...importFromSettings, chooseFile],
  },
  {
    id: "configuration-import-review-not-found-details",
    group: GROUP,
    category: "normal",
    title: "Import, review, why a project is skipped",
    description: "The mouse rests on the one warning triangle after the Notes name, \"Notes not found, and accounts not set up\": its popover opens above it with no click and no focus taken, listing each state as its own paragraph: the project directory was not found, it is skipped unless a directory is chosen for it, and then the account that falls back to the default. There is no second triangle beside it and the row has no state line under the name. A project's account that is not set up here, so the default account is used, is one such sentence in the same popover.",
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
    title: "Import, before the first progress report",
    description: "Import was pressed and the daemon has reported nothing yet: the progress bar labelled \"Importing configuration\" is indeterminate, with \"Starting the import…\" where its count will go and no list under it, and the footer holds the Import button, showing its pending spinner before the label, beside a Cancel that is present but disabled. The dialog cannot be left meanwhile: the close button is disabled, and Escape and a click outside do nothing.",
    state,
    configuration: reviewing({ imported: "pending" }),
    steps: [
      ...importFromSettings,
      chooseFile,
      importNow,
    ],
  },
  {
    id: "configuration-import-progress",
    group: GROUP,
    category: "normal",
    title: "Import, in progress",
    description: "Opened from Settings, the dialog stands over the Settings dialog, which stays open behind it. With Settings checked and only its Git section kept the import is part-way: the bar labelled \"Importing configuration\" is filled to the items that have settled, \"12 of 15 processed\". Below it are cards in the review's order: the consoles, with their projects, then Default clone directory, Settings, Agent account settings and last each agent with its accounts, each with its state: Done, Failed with the reason, Not attempted (the project of the console that could not be created), Waiting, and Creating… or Cloning… with a spinner. The Work laptop console is already Done, one of its projects failed, one is done and the last is being cloned; Default clone directory and Agent account settings are Done, and Settings, which stands for all the checked sections at once, is still Waiting. The footer holds the pending Import button, with its spinner before the label, beside a disabled Cancel, and the close button is disabled too.",
    state: accountsMachine,
    configuration: reviewing({ inspection: PROGRESS_REVIEW, imported: "pending", progress: IMPORT_UNDER_WAY }),
    steps: [
      ...importFromSettings,
      chooseFile,
      ...settingsKeeping("git"),
      importNow,
    ],
  },
  {
    id: "configuration-import-result-settings-failed",
    group: GROUP,
    category: "exceptional",
    title: "Import, result where the settings could not be applied",
    description: "With Settings checked and only its Git section kept, the import's result has a Settings tab for it: it carries a warning glyph and is the one open, as the only tab with a failure: a danger Alert holds the title \"Could not import settings\" and the short reason (\"The settings could not be applied: …\"), with Details at its end for the raw text, and no settings cards follow. The other two tabs have no glyph (the next scenario shows what they say).",
    state: autoPulling,
    configuration: { file: fileOf(WITH_ACCOUNTS_SETTING_OFF), inspection: WITH_ACCOUNTS_SETTING_OFF, imported: SETTINGS_FAILED_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      ...settingsKeeping("git"),
      importNow,
    ],
  },
  {
    id: "configuration-import-result-settings-failed-others",
    group: GROUP,
    category: "exceptional",
    title: "Import, result where only the settings failed, the other tabs",
    description: "The settings could not be applied, yet the Agent accounts tab, open here, starts with its own totals, \"2 accounts created · 2 accounts already existed · Nothing failed\", since none of its accounts failed; the Consoles and projects tab has its own too (\"1 console created · 2 projects created · 1 project left out · Nothing failed\"), and only the Settings tab carries the warning glyph.",
    state: autoPulling,
    configuration: { file: fileOf(WITH_ACCOUNTS_SETTING_OFF), inspection: WITH_ACCOUNTS_SETTING_OFF, imported: SETTINGS_FAILED_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      ...settingsKeeping("git"),
      importNow,
      (ui) => ui.pressTab(ui.t("settings.section.accounts")),
    ],
  },
  {
    id: "configuration-import-result-clone-dir-kept",
    group: GROUP,
    category: "exceptional",
    title: "Import result, the clone directory kept although the settings failed",
    description: "A document of the Git settings alone, so there is no What to import group and no tab bar: the settings could not be applied, but the default clone directory had been written before they failed. A danger Alert holds the settings failure's title and short reason, with Details at its end, and below it a Git card holds one line, Default clone directory, \"Matches the file\", with the note \"Set before the other settings failed to apply.\" and the file's directory under it as plain monospace text.",
    state,
    configuration: { file: fileOf(CLONE_DIR_SETTINGS), inspection: CLONE_DIR_SETTINGS, imported: CLONE_DIR_KEPT_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      importNow,
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
    description: "On the Agent accounts tab, \"Another directory\" under an account shows a text field labelled \"Directory\" with a placeholder that follows the agent (\"Ex. ~/.claude-work\" in Claude Code's Clients, \"Ex. ~/.codex-work\" in Codex's Backup, both left empty) and a Browse button, in place of the folder-icon directory line, which the other rows keep. Only a row whose directory is the problem offers it: Default, whose name is the problem, can only be left out. After Import is pressed, an empty directory (Clients, Backup) shows \"Enter a directory, or leave the account out.\" and a relative one (Original) shows that an absolute path, or one starting with ~/, is needed, together. The view is scrolled to Codex's Backup, whose empty field has focus and shows its own placeholder; Claude Code's Clients, empty too, is higher up the list.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: undefined },
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
    configuration: reviewing({ file: fileOf(REVIEW, SHORTCUTS) }),
    description: "The Consoles and projects tab, with a warning glyph (the Settings tab has none), is open. One line of totals counts only the consoles and projects: \"2 consoles created · 1 project created · 1 project left out · 3 failed · 1 project not attempted\", with no totals above the tabs. Below it a card per console, each with its own status in the header, always: Work laptop Created, Side projects Failed with its reason under the header (the daemon's generic sentence, with Details), Scratch Created with the note that it has no projects. Work laptop's rows: Website Failed with its reason written under the row (a path that does not exist, in code style); Search API Created with a muted line saying where it went; Design System Failed with its reason, Details behind a button; Notes Left out. Dotfiles, under the failed console, reads Not attempted with the muted line that its console could not be created. A warning triangle follows the Work laptop name and the Search API name, for the accounts that fell back to the default. Use the page's width control for how the rows wrap: the status goes under the name when the row is narrow.",
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
    title: "Import result, the settings applied",
    description: "The only tab, as the consoles were not imported, so there is no tab bar: one card for each of the six applied sections, titled and ordered as the Settings dialog's sections (General, App behavior, Git, Agent behavior, Keyboard shortcuts, Cleanup), their switches read On or Off. General lists the settings only the window itself holds, each with the value it was set to and no \"now\" value: Appearance (System), Language (English, the daemon's applied one) and Markdown style (GitHub), then Sorting, which lists the three parts it was set to (Sessions: Recently started, Active projects: Recently used, Inactive projects: Recently used), then its switches. App behavior lists Resume sessions on start, then Keep running in the background when the window is closed (On) and Show menu bar icon (Off), as this window offers each of them, then Prevent sleep (On) and When to prevent sleep (Always), then the terminal-cleanup switch. Git begins with Default clone directory (\"Matches the file\"), which gives the file's directory as plain monospace text, then the pull switch and Git workflow (\"Matches the file\"), which lists all five items. Agent behavior holds the four auto-approve switches and Ask questions as choices. Keyboard shortcuts holds a \"Changed shortcuts\" line (\"3 changes from the defaults\") that lists the three that were applied as key caps, under a muted line saying 2 shortcuts could not be applied. Cleanup holds the archive-cleanup switch alone.",
    state,
    backgroundRun: true,
    menuBarIcon: true,
    configuration: reviewing({ file: CLIENT_RESULT_FILE, inspection: SETTINGS_REVIEW, imported: SETTINGS_RESULT }),
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleConsoles,
      toggleSettings,
      importNow,
    ],
  },
  {
    id: "configuration-import-result-accounts",
    group: GROUP,
    category: "exceptional",
    title: "Import result of the accounts, with a failure",
    description: "The Agent accounts tab, with a warning glyph beside the Consoles and projects one, is open. It starts with one line of totals counting only the accounts, \"3 accounts created · 1 account already existed · 4 accounts left out · 1 failed\", directly above the cards. The first card, \"Agent account settings\", holds the three switches, the two that were turned on reading On and the one left Off. Then one card per agent, in the order Claude Code, Codex, Grok Build and Acme Agent, titled with its name and no checkbox: Claude Code's holds the lines \"The default account is now named as in the file.\" and \"The default account is now disabled.\", Permission approval, Codex compatibility and Tags (each \"Matches the file\" with the value that was set, Tags as No tags), then under a filled \"Accounts\" band a row per account with its status: Work Created, Archive Created, Sandbox Failed with the reason under the row, Clients, Original and Default Left out. Codex's holds the line \"The default account is now enabled.\", Permission approval, Claude compatibility and Tags (three tags), then Home Already existed, Reports Created and Backup Left out. Grok Build's card holds the one line \"Added to the agent list.\" and no Accounts band. Acme Agent, a provider this build has no words for, has a card of its own with one settings row and no accounts, so no Accounts band. A note with an info icon says the new accounts are not logged in yet; there is no Log in button.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      importNow,
      accountsTab,
    ],
  },
  {
    id: "configuration-import-result-agent-settings-failed",
    group: GROUP,
    category: "exceptional",
    title: "Import result, the agent account settings could not be applied",
    description: "A document of accounts alone, so there is no tab bar: the line of totals (\"2 accounts created · 2 accounts already existed\", which does not add \"Nothing failed\" beside an alert about the settings) is followed directly by a danger Alert holding the title \"Could not import agent account settings\" and the short reason, with Details at its end for the raw text. The cards of the accounts, none failed, come after it, with no Agent account settings card. The note that the new accounts are not logged in stays below the cards.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_ONLY_SETTINGS), inspection: ACCOUNTS_ONLY_SETTINGS, imported: AGENT_SETTINGS_FAILED_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      importNow,
    ],
  },
  {
    id: "configuration-import-result-no-consoles",
    group: GROUP,
    category: "normal",
    title: "Import result of a file with consoles, imported without them",
    description: "The file carries consoles, but Consoles and projects was unchecked, so no console or project was created and the result has no Consoles and projects tab: only Settings and Agent accounts (open), neither with a warning glyph. Settings holds a card for each of the two sections kept, Git and Cleanup, and none for the sections left unchecked. Above the cards, one line of totals counts the accounts alone, \"4 accounts created · 1 account already existed · 4 accounts left out · Nothing failed\"; the Settings tab has none.",
    state: accountsMachine,
    configuration: { file: fileOf(ACCOUNTS_REVIEW), inspection: ACCOUNTS_REVIEW, imported: ACCOUNTS_SETTINGS_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      toggleConsoles,
      ...settingsKeeping("git", "cleanup"),
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
    configuration: { file: fileOf(ACCOUNTS_ONLY), inspection: ACCOUNTS_ONLY, imported: ACCOUNTS_ONLY_RESULT },
    steps: [...importFromWelcome, importNow],
  },
  {
    id: "configuration-import-result-fallbacks",
    group: GROUP,
    category: "normal",
    title: "Import result, accounts that fell back to the default",
    description: "Everything was created, so the only thing to report is the fallbacks: a warning triangle follows the Documentation name in its header row and the Style Guide name in its row, and nothing else (no alert, no list of accounts) stands above or below the card but the one line of totals above it (\"1 console created · 1 project created · Nothing failed\"), with no tab bar as there is one region.",
    state,
    configuration: { file: fileOf(FALLBACKS_ONLY), inspection: FALLBACKS_ONLY, imported: FALLBACKS_ONLY_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      importNow,
    ],
  },
  {
    id: "configuration-import-result-failure-reason",
    group: GROUP,
    category: "exceptional",
    title: "Import result, the raw diagnostic behind a failure's Details",
    state: state,
    configuration: reviewing(),
    description: "Details under the Design System failure is open: a popover opening below the button holds the git output as the raw text, in a block whose original line breaks are preserved and whose long lines scroll horizontally without automatic wrapping. Its height is capped at 40vh, and the floating Copy button stays at the top right while scrolling and copies the complete original text. The sentence that git could not clone the repository stays on the row above it, and the other failed rows keep their own reasons and buttons.",
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
    configuration: { file: fileOf(LONG_RESULT_REVIEW), inspection: LONG_RESULT_REVIEW, imported: LONG_RESULT },
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
    configuration: { file: fileOf(SINGLE), inspection: SINGLE, imported: CLEAN_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      ...placeIn("Releases", "~/work/releases"),
      importNow,
    ],
  },
];
