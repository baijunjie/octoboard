// The gallery's "Configuration" scenarios: exporting a console or everything, and importing a file
// through its three steps (choosing it, reviewing what it holds, the result). What the daemon would
// work out from the filesystem — the offers of a project, the outcome of a clone — is written out
// here by hand, and the test beside this file holds those answers to the rules the daemon follows.
import type { Imported } from "../../configuration/ImportResult";
import type { Inspection } from "../../configuration/ImportReview";
import { DEFAULT_GIT_WORKFLOW } from "../../gitWorkflow";
import type { GitWorkflowSettings, InspectedProject } from "../../protocol";
import type { Ui } from "../interact";
import type { ConfigurationFixture, Scenario, Step } from "../scenario";
import { ALL_AGENTS, SAMPLE, consoleOf, effectiveClaudeCompatibility, projectOf, snapshotState } from "./builders";
import type { FixtureError } from "./projectFiles";

const GROUP = "Configuration";

const { console: console_, web, api, sessions } = SAMPLE;

/** The machine's default clone directory, which every offer of a clone or of a derived directory
 * lies under. */
export const DEFAULT_CLONE_DIR = "/Users/dev/Projects";

/** The accounts the machine has. A document's account that is one of these is rebound to it, so
 * none of what a document names as falling back to the default account is among them. */
const accounts = [
  { id: "a-studio", agent: "claude" as const, name: "Studio", config_dir: "/Users/dev/.claude-studio" },
  { id: "a-home", agent: "codex" as const, name: "Home", config_dir: "/Users/dev/.codex-home" },
];

// Two consoles, so the export has something to choose between, one of them naming an account. The
// inspections below say the application setting is off now; `autoPulling` is the machine where it is
// on.
const second = consoleOf("c-docs", "Documentation", { claude_account_id: "a-studio", created_at: console_.created_at + 1 });
const layout = {
  consoles: [console_, second],
  projects: [
    { ...web, remote_url: "git@github.com:acme/website.git", tags: ["frontend"] },
    api,
    projectOf("p-guide", second.id, "Style Guide", { claude_account_id: { kind: "account" as const, id: "a-studio" } }),
  ],
  sessions,
};
const state = snapshotState({
  ...layout,
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ALL_AGENTS, default_clone_dir: DEFAULT_CLONE_DIR, git_workflow: DEFAULT_GIT_WORKFLOW, accounts },
});
const autoPulling = snapshotState({
  ...layout,
  settings: { auto_pull_repositories: true, auto_delete_unresumable_archives: true, added_agents: ALL_AGENTS, default_clone_dir: DEFAULT_CLONE_DIR, git_workflow: DEFAULT_GIT_WORKFLOW, accounts },
});
const noConsoleState = snapshotState({
  consoles: [],
  projects: [],
  sessions: [],
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ALL_AGENTS, default_clone_dir: DEFAULT_CLONE_DIR, git_workflow: DEFAULT_GIT_WORKFLOW, accounts },
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
const applySettings = (ui: Ui) => ui.press(ui.t("configuration.import.settings.label"));

/** Puts a project in a directory of the user's own, typed `path`, in the review. */
const placeIn = (project: string, path: string): Step[] => [
  (ui) => ui.press(ui.t("configuration.import.project.choice", { project })),
  (ui) => ui.press(ui.t("configuration.import.choice.directory")),
  (ui) => ui.focus(ui.t("configuration.import.directory.label", { project })),
  (ui) => ui.type(path),
];

const project = (extra: Partial<InspectedProject> & Pick<InspectedProject, "name" | "path">): InspectedProject => ({
  remote_url: null,
  offers: { exported_path: null, default_clone_dir: null, clone: null },
  default: null,
  account_fallbacks: [],
  ...extra,
});

/** The git workflow settings the full export carries: not the ones this machine starts with. */
const FILE_GIT_WORKFLOW: GitWorkflowSettings = { development: "branch", rebase_before_merge: false, delivery: "merge", revert_gate: true };

/** The Claude compatibility the full export carries: only what the user chose on the exporting machine, here Codex's off,
 * where this machine has every agent's on. */
const FILE_CLAUDE_COMPATIBILITY = { codex: false };

/** This machine's Claude compatibility as an import's review reports it: the values in effect for both agents. The
 * machines the document is set up on have every agent added and no explicit choice, so both are on. */
const THIS_CLAUDE_COMPATIBILITY = effectiveClaudeCompatibility({}, ALL_AGENTS);

/** A document of a laptop's layout set up on a machine where it sits elsewhere: one project at its
 * recorded path, one found under the default clone directory, one to be cloned and one that nothing
 * places (whose recorded path is long enough to break over lines, and whose account falls back too),
 * a second console whose only project can be cloned, and one with no project. */
export const REVIEW: Omit<Inspection, "type"> = {
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
      account_fallbacks: [{ agent: "claude", name: "Work" }],
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
          account_fallbacks: [{ agent: "codex", name: "Personal" }],
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
          account_fallbacks: [{ agent: "codex", name: "Personal" }],
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
};

/** One console's document: no application settings; a project at its recorded path, one that can
 * only be cloned, and one nothing places. */
const SINGLE: Omit<Inspection, "type"> = {
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
};

/** A console and projects whose names are long enough to wrap in the dialog, each with a mark after the name: the
 * console's own account falls back, one project is found and its account falls back, the other is not found. */
const LONG_NAMES: Omit<Inspection, "type"> = {
  consoles: [
    {
      name: "Northwest regional distribution centres, quarterly planning and reporting workspace for the whole logistics group",
      account_fallbacks: [{ agent: "claude", name: "Work" }],
      projects: [
        project({
          name: "Warehouse slotting optimisation service with the seasonal demand forecasting and replenishment rules",
          path: "/Users/ana/code/slotting",
          offers: { exported_path: "/Users/ana/code/slotting", default_clone_dir: null, clone: null },
          default: "exported_path",
          account_fallbacks: [{ agent: "codex", name: "Personal" }],
        }),
        project({
          name: "Carrier contract archive and rate comparison tooling for the freight procurement team",
          path: "/Users/ana/work/carrier-contracts",
        }),
      ],
    },
  ],
};

/** A document exported without full project paths, which records only each directory's name: one
 * project found under the default clone directory, one to be cloned, and one nothing places. */
const NO_PATHS: Omit<Inspection, "type"> = {
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
};

/** The same document of a full export made before archive cleanup, automatic account selection, the
 * sidebar's status ordering, starting usage windows, the automatic approval of agents' requests and asking
 * questions as choices were carried, so it holds only the automatic pull, which is off where this machine has it on. */
const WITH_SETTING_OFF: Omit<Inspection, "type"> = {
  ...SINGLE,
  settings: { auto_pull_repositories: false, current_auto_pull_repositories: true, current_auto_delete_unresumable_archives: true },
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
export const MIXED_RESULT: Omit<Imported, "type"> = {
  settings_applied: { auto_pull_repositories: true, auto_delete_unresumable_archives: false, auto_select_account: true, auto_switch_on_usage_limit: true, sidebar_order_by_status: false, auto_start_quota_windows: true, auto_approve_session_requests: true, auto_approve_resume_requests: true, auto_approve_account_switches: true, auto_approve_handoffs: true, ask_questions_as_choices: false, git_workflow: FILE_GIT_WORKFLOW, claude_compatibility: FILE_CLAUDE_COMPATIBILITY },
  consoles: [
    {
      name: "Work laptop",
      status: "created",
      console: "c-new-1",
      error: null,
      account_fallbacks: [{ agent: "claude", name: "Work" }],
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
          account_fallbacks: [{ agent: "codex", name: "Personal" }],
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
};

/** Everything created, nothing to report: `SINGLE` with the user's own directory given to the
 * project nothing placed. */
const CLEAN_RESULT: Omit<Imported, "type"> = {
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
};

/** A console and its one project, both naming accounts this machine does not have, and where the project is
 * found at its recorded path: so the only thing the import has to report is the fallbacks. */
const FALLBACKS_ONLY: Omit<Inspection, "type"> = {
  consoles: [
    {
      name: "Documentation",
      account_fallbacks: [{ agent: "claude", name: "Work" }],
      projects: [
        project({
          name: "Style Guide",
          path: "/Users/dev/code/style-guide",
          offers: { exported_path: "/Users/dev/code/style-guide", default_clone_dir: null, clone: null },
          default: "exported_path",
          account_fallbacks: [{ agent: "codex", name: "Personal" }],
        }),
      ],
    },
  ],
};

/** `FALLBACKS_ONLY` imported with its defaults. */
const FALLBACKS_ONLY_RESULT: Omit<Imported, "type"> = {
  consoles: [
    {
      name: "Documentation",
      status: "created",
      console: "c-new-1",
      error: null,
      account_fallbacks: [{ agent: "claude", name: "Work" }],
      projects: [{ name: "Style Guide", status: "created", via: "exported_path", project: "p-new-1", error: null, account_fallbacks: [{ agent: "codex", name: "Personal" }] }],
    },
  ],
};

/** `WITH_SETTING_OFF` imported with its defaults and the settings asked for: the project nothing
 * placed is left out, and the settings could not be written. */
const SETTINGS_FAILED_RESULT: Omit<Imported, "type"> = {
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
};

const FILE = { name: "octoboard-configuration.json", text: JSON.stringify({ format: "octoboard-configuration", version: 2, consoles: [] }, null, 2) };

const refusal = (name: string, text: string, inspection: FixtureError): ConfigurationFixture => ({ file: { name, text }, inspection });

const DIRECTORY_PROJECT = "Notes for production release coordination مرحبا 发布";
const DIRECTORY_REVIEW = { ...REVIEW, consoles: REVIEW.consoles.map((console) => ({ ...console, projects: console.projects.map((project) => project.name === "Notes" ? { ...project, name: DIRECTORY_PROJECT } : project) })) };

/** A full export's document, with `keyboard_shortcuts` as the client that saved it wrote them. The fixture daemon
 * echoes them in the result's `settings_applied`, as the real one does, so a result scenario needs none of its own. */
const documentWith = (shortcuts: Record<string, unknown>) => ({
  name: FILE.name,
  text: JSON.stringify(
    { format: "octoboard-configuration", version: 2, settings: { auto_pull_repositories: true, keyboard_shortcuts: shortcuts }, consoles: [] },
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

/** `SETTINGS_FAILED_RESULT`'s consoles with the settings applied this time. */
const SETTINGS_APPLIED_RESULT: Omit<Imported, "type"> = {
  settings_applied: { auto_pull_repositories: false },
  consoles: SETTINGS_FAILED_RESULT.consoles,
};

/** `WITH_SETTING_OFF`, its document carrying the git workflow and Claude compatibility settings this machine already has. */
const WITH_SETTINGS_HERE: Omit<Inspection, "type"> = {
  ...WITH_SETTING_OFF,
  settings: {
    ...WITH_SETTING_OFF.settings!,
    git_workflow: DEFAULT_GIT_WORKFLOW,
    current_git_workflow: DEFAULT_GIT_WORKFLOW,
    claude_compatibility: { grok: true },
    current_claude_compatibility: THIS_CLAUDE_COMPATIBILITY,
  },
};

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
    description: "Choose the console the sidebar shows or everything. Include full project paths starts unchecked: its description says what checking it does (\"Once checked, the file records each project's full directory path, so an import can look for the project there first.\") and no alert stands beneath it. Export closes the dialog and raises a toast naming the saved file.",
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
    description: "Only everything is offered.",
    state: noConsoleState,
    steps: exportFromSettings,
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
    description: "Opened from the welcome screen, where a new computer starts. The intro ends by saying that an export of everything also includes the application settings. The picker is never reported dismissed, as in a browser without the file input's cancel event: Choose file shows that it is waiting and ignores another press, while Cancel, the close button and Escape still work.",
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
      JSON.stringify({ format: "octoboard-configuration", version: 3, consoles: [] }),
      failure("configuration_version_unsupported", { version: "3", supported: "2" }, "version 3 is not supported"),
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
      JSON.stringify({ format: "octoboard-configuration", version: 2, consoles: "none" }),
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
    description: "Cards for three consoles, then the application-settings card, with the clone note last. Project rows show each state: found at its exported path (only \"Placed in\"), found in the default clone directory, can be cloned (\"Clone from\" and \"Clone into\") and not found, left out, with a long exported path that wraps and no state line under its name. A warning triangle follows the name of the Work laptop console in its heading and the name of Search API in its row, where an account the file names is not set up here (the popover is in the next scenario); no sentence or alert repeats it. The not-found project, Notes, carries one warning triangle after its name, \"Notes not found, and accounts not set up\": the project is missing and its account falls back too, which are one point and so one mark (its popover is in a scenario below). The last console says it has no projects. The settings card starts unchecked: lines that turn a setting on carry a muted consequence line (the automatic-switch line, \"Automatically switch accounts at the usage limit\", says a session that stops at its account's usage limit is then switched to another account with usage left and carries on), lines that turn one off do not (their \"Turns off\" badge is neutral, not accented), Git workflow and Claude compatibility read \"Set to the file's\" (the file turns Codex's off), and keyboard shortcuts read \"Set to the file's\" with a note that 3 of the file's shortcuts differ from the defaults and apply to this device or browser only.",
    state,
    configuration: reviewing({ file: SHORTCUTS_FILE }),
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review-single",
    group: GROUP,
    category: "normal",
    title: "Import, review of one console's document",
    description: "A single console's document has one card and no application-settings card. \"Default clone directory (0)\" is disabled because no project offers it, and the clone note follows the card. The project nothing places, Releases, has no state line, only a warning triangle after its name.",
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
    description: "Git workflow, Claude compatibility and keyboard shortcuts all read \"No change\" because the file's are the ones this device already has, below automatic pull, which turns off.",
    state: autoPulling,
    preferences: { keyboardShortcuts: SHORTCUTS_HERE },
    configuration: { file: SHORTCUTS_FILE, inspection: WITH_SETTINGS_HERE },
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review-shortcuts-reset",
    group: GROUP,
    category: "normal",
    title: "Import, an older file whose keyboard shortcuts are all defaults",
    description: "The checkbox is ticked. The older file carries only automatic pull, which turns off, so there is no Git workflow or Claude compatibility line, and its keyboard shortcuts are all defaults, so that line reads \"Back to defaults\" with the note that it applies to this device or browser only.",
    state: autoPulling,
    preferences: { keyboardShortcuts: SHORTCUTS_HERE },
    configuration: { file: SHORTCUTS_DEFAULT_FILE, inspection: WITH_SETTING_OFF },
    steps: [
      ...importFromSettings,
      chooseFile,
      applySettings,
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
    description: "A danger Alert above the table holds the settings failure's title and short reason, with Details on its right for the raw text; the table below shows the project that was left out, and no settings card follows.",
    state: autoPulling,
    configuration: { file: FILE, inspection: WITH_SETTING_OFF, imported: SETTINGS_FAILED_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      applySettings,
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
    id: "configuration-import-result",
    group: GROUP,
    category: "exceptional",
    title: "Import result with failures and fallbacks",
    state: state,
    configuration: reviewing({ file: SHORTCUTS_FILE }),
    description: "The table groups projects under their console, with a console's own result on its header row when it failed (\"Side projects\") or came out empty (\"Scratch\"); a failed project's result is a button opening the reason, and its Detail is empty. A warning triangle follows the Work laptop name in its header row and the Search API name in its row, for the accounts that fell back to the default; no alert or section lists them. Below the table comes the \"Application settings applied\" card, whose badges read On, Off, \"Matches the file\" (git workflow and Claude compatibility, in that order) and \"3 changes from the defaults\", the last with a muted line saying 2 shortcuts could not be applied. Phone width stacks each row into labelled blocks under the console's name; a short height exposes scrolling.",
    steps: [
      ...importFromSettings,
      chooseFile,
      applySettings,
      importNow,
    ],
  },
  {
    id: "configuration-import-result-shortcuts-reset",
    group: GROUP,
    category: "normal",
    title: "Import result, keyboard shortcuts back to their defaults",
    description: "The settings card shows automatic pull as Off and keyboard shortcuts as \"Defaults\", with no muted line under it.",
    state: autoPulling,
    preferences: { keyboardShortcuts: SHORTCUTS_HERE },
    configuration: { file: SHORTCUTS_DEFAULT_FILE, inspection: WITH_SETTING_OFF, imported: SETTINGS_APPLIED_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
      applySettings,
      importNow,
    ],
  },
  {
    id: "configuration-import-result-fallbacks",
    group: GROUP,
    category: "normal",
    title: "Import result, accounts that fell back to the default",
    description: "Everything was created, so the only thing to report is the fallbacks: a warning triangle follows the Documentation name in its header row and the Style Guide name in its row, and nothing else (no alert, no list of accounts) stands above or below the table. Their popovers are in the next scenario.",
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
    description: "The mouse rests on the warning triangle after the Documentation name in the result's header row: its popover opens above it with no click and no focus taken, saying that the Claude Code account Work is not set up here, so the default account is used. The table stays as it is under the popover.",
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
    title: "Import result, the reason a failed row carries",
    state: state,
    configuration: reviewing(),
    description: "A failed row is tapped with a finger: its popover holds why that project could not be created, while the row keeps only the failed mark and word.",
    steps: [
      ...importFromSettings,
      chooseFile,
      applySettings,
      importNow,
      (ui) => ui.touchPressStatusMark(ui.t("configuration.import.result.failedFor", { subject: "Website" })),
    ],
  },
  {
    id: "configuration-import-result-clean",
    group: GROUP,
    category: "normal",
    title: "Successful import result",
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
