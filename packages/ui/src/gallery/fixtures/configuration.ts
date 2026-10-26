// The gallery's "Configuration" scenarios: exporting a console or everything, and importing a file
// through its three steps (choosing it, reviewing what it holds, the result). What the daemon would
// work out from the filesystem — the offers of a project, the outcome of a clone — is written out
// here by hand, and the test beside this file holds those answers to the rules the daemon follows.
import type { Imported } from "../../configuration/ImportResult";
import type { Inspection } from "../../configuration/ImportReview";
import type { InspectedProject, WorktreeSettings } from "../../protocol";
import { DEFAULT_WORKTREE_SETTINGS } from "../../worktreeSettings";
import type { Ui } from "../interact";
import type { ConfigurationFixture, Scenario, Step } from "../scenario";
import { ALL_AGENTS, SAMPLE, consoleOf, projectOf, snapshotState } from "./builders";
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
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ALL_AGENTS, default_clone_dir: DEFAULT_CLONE_DIR, worktrees: DEFAULT_WORKTREE_SETTINGS, accounts },
});
const autoPulling = snapshotState({
  ...layout,
  settings: { auto_pull_repositories: true, auto_delete_unresumable_archives: true, added_agents: ALL_AGENTS, default_clone_dir: DEFAULT_CLONE_DIR, worktrees: DEFAULT_WORKTREE_SETTINGS, accounts },
});
const noConsoleState = snapshotState({
  consoles: [],
  projects: [],
  sessions: [],
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ALL_AGENTS, default_clone_dir: DEFAULT_CLONE_DIR, worktrees: DEFAULT_WORKTREE_SETTINGS, accounts },
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

/** The worktree settings the full export carries: not the ones this machine starts with. */
const FILE_WORKTREES: WorktreeSettings = { use_worktrees: true, rebase_before_merge: false, delivery: "merge", revert_gate: true };

/** A document of a laptop's layout set up on a machine where it sits elsewhere: one project at its
 * recorded path, one found under the default clone directory, one to be cloned and one that nothing
 * places (whose recorded path is long enough to break over lines), a second console whose only
 * project can be cloned, and one with no project. */
export const REVIEW: Omit<Inspection, "type"> = {
  settings: {
    auto_pull_repositories: true,
    current_auto_pull_repositories: false,
    auto_delete_unresumable_archives: false,
    current_auto_delete_unresumable_archives: true,
    auto_select_account: true,
    current_auto_select_account: false,
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
    worktrees: FILE_WORKTREES,
    current_worktrees: DEFAULT_WORKTREE_SETTINGS,
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
        project({ name: "Notes", path: "/Users/ana/Documents/Work/Clients/Northwind/Archive/2019/Quarterly-Reports/final-notes" }),
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
  settings_applied: { auto_pull_repositories: true, auto_delete_unresumable_archives: false, auto_select_account: true, sidebar_order_by_status: false, auto_start_quota_windows: true, auto_approve_session_requests: true, auto_approve_resume_requests: true, auto_approve_account_switches: true, auto_approve_handoffs: true, ask_questions_as_choices: false, worktrees: FILE_WORKTREES },
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

const FILE = { name: "octoboard-configuration.json", text: JSON.stringify({ format: "octoboard-configuration", version: 1, consoles: [] }, null, 2) };

const refusal = (name: string, text: string, inspection: FixtureError): ConfigurationFixture => ({ file: { name, text }, inspection });

const DIRECTORY_PROJECT = "Notes for production release coordination مرحبا 发布";
const DIRECTORY_REVIEW = { ...REVIEW, consoles: REVIEW.consoles.map((console) => ({ ...console, projects: console.projects.map((project) => project.name === "Notes" ? { ...project, name: DIRECTORY_PROJECT } : project) })) };

/** A full export's document, with `keyboard_shortcuts` as the client that saved it wrote them. The fixture daemon
 * echoes them in the result's `settings_applied`, as the real one does, so a result scenario needs none of its own. */
const documentWith = (shortcuts: Record<string, unknown>) => ({
  name: FILE.name,
  text: JSON.stringify(
    { format: "octoboard-configuration", version: 1, settings: { auto_pull_repositories: true, keyboard_shortcuts: shortcuts }, consoles: [] },
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

/** `WITH_SETTING_OFF`, its document carrying the worktree settings this machine already has. */
const WITH_WORKTREES_HERE: Omit<Inspection, "type"> = {
  ...WITH_SETTING_OFF,
  settings: { ...WITH_SETTING_OFF.settings!, worktrees: DEFAULT_WORKTREE_SETTINGS, current_worktrees: DEFAULT_WORKTREE_SETTINGS },
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
    description: "Choose the console the sidebar shows or everything; Include full project paths starts unchecked and no notice stands beneath it. Export closes the dialog and raises a toast naming the saved file.",
    state,
    steps: exportFromSettings,
  },
  {
    id: "configuration-export-with-paths",
    group: GROUP,
    category: "normal",
    title: "Export, with full project paths included",
    description: "Checking Include full project paths brings up the warning titled \"The file contains project paths\", saying under it that the file carries the paths on the machine running Octoboard.",
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
    description: "The error alert dialog, over the export dialog and titled \"Could not export the configuration\", says only \"Could not save the file.\" and what to do about it. The system's own text, Permission denied (os error 13), appears nowhere in the sentences and sits only in the Error details disclosure, closed here: open it to read it in its block.",
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
    description: "The error alert dialog, over the import dialog and titled \"Could not import the configuration\", says only \"Could not read the file.\" and what to do about it. The system's own text, Permission denied (os error 13), sits only in the Error details disclosure, closed here.",
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
      failure("configuration_version_unsupported", { version: "3", supported: "1" }, "version 3 is not supported"),
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
    description: "Cards for two consoles, then the application-settings card, with the clone note last. Project rows show each state: found at its exported path (only \"Placed in\"), found in the default clone directory, can be cloned (\"Clone from\" and \"Clone into\") and not found, left out, with a long exported path that wraps. Account fallbacks sit under a console's name and at the end of a project's row; the last console says it has no projects. The settings card starts unchecked: lines that turn a setting on carry a muted consequence line, lines that turn one off do not (their \"Turns off\" badge is neutral, not accented), worktrees reads \"Set to the file's\", and keyboard shortcuts read \"Set to the file's\" with a note that 3 of the file's shortcuts differ from the defaults and apply to this device or browser only.",
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
    description: "A single console's document has one card and no application-settings card. \"Default clone directory (0)\" is disabled because no project offers it, and the clone note follows the card.",
    state,
    configuration: { file: FILE, inspection: SINGLE, imported: CLEAN_RESULT },
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
    description: "A project recorded only by its directory's name has no \"Exported path\" line: found in the default clone directory (only \"Placed in\"), can be cloned, or not found, with nothing to place it.",
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
    title: "Import, keyboard shortcuts and worktree settings the device already has",
    description: "Worktrees and keyboard shortcuts both read \"No change\" because the file's are the ones this device already has, below automatic pull, which turns off.",
    state: autoPulling,
    preferences: { keyboardShortcuts: SHORTCUTS_HERE },
    configuration: { file: SHORTCUTS_FILE, inspection: WITH_WORKTREES_HERE },
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
    description: "The checkbox is ticked. The older file carries only automatic pull, which turns off, and its keyboard shortcuts are all defaults, so that line reads \"Back to defaults\" with the note that it applies to this device or browser only.",
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
    title: "Import, refused before anything is created",
    description: "The daemon's reason is the error alert dialog's, raised over the review, which stays as it was with nothing written into it.",
    state,
    configuration: reviewing({ imported: failure("icon_too_large", { limit_kib: "256" }, "the console avatar is larger than 256 KiB") }),
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
    description: "The table groups projects under their console, with a console's own result on its header row when it failed (\"Side projects\") or came out empty (\"Scratch\"); a failed project's result is a button opening the reason, and its Detail is empty. Below come the fallback accounts and the \"Application settings applied\" card, whose badges read On, Off, \"Matches the file\" and \"3 changes from the defaults\", the last with a muted line saying 2 shortcuts could not be applied. Phone width stacks each row into labelled blocks under the console's name; a short height exposes scrolling.",
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
