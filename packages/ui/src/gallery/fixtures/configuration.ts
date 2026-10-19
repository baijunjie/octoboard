// The gallery's "Configuration" scenarios: exporting a console or everything, and importing a file
// through its three steps (choosing it, reviewing what it holds, the result). What the daemon would
// work out from the filesystem — the offers of a project, the outcome of a clone — is written out
// here by hand, and the test beside this file holds those answers to the rules the daemon follows.
import type { Imported } from "../../configuration/ImportResult";
import type { Inspection } from "../../configuration/ImportReview";
import type { InspectedProject } from "../../protocol";
import type { Ui } from "../interact";
import type { ConfigurationFixture, Scenario, Step } from "../scenario";
import { SAMPLE, consoleOf, projectOf, snapshotState } from "./builders";
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
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: DEFAULT_CLONE_DIR, accounts },
});
const autoPulling = snapshotState({
  ...layout,
  settings: { auto_pull_repositories: true, auto_delete_unresumable_archives: true, default_clone_dir: DEFAULT_CLONE_DIR, accounts },
});
const noConsoleState = snapshotState({
  consoles: [],
  projects: [],
  sessions: [],
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: DEFAULT_CLONE_DIR, accounts },
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

/** A document of a laptop's layout set up on a machine where it sits elsewhere: one project at its
 * recorded path, one found under the default clone directory, one to be cloned and one that nothing
 * places (whose recorded path is long enough to break over lines), a second console whose only
 * project can be cloned, and one with no project. */
export const REVIEW: Omit<Inspection, "type"> = {
  settings: { auto_pull_repositories: true, current_auto_pull_repositories: false, auto_delete_unresumable_archives: false, current_auto_delete_unresumable_archives: true },
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

/** The same document of a full export made before the archive cleanup setting existed, so it holds
 * only the automatic pull, which is off where this machine has it on. */
const WITH_SETTING_OFF: Omit<Inspection, "type"> = {
  ...SINGLE,
  settings: { auto_pull_repositories: false, current_auto_pull_repositories: true, current_auto_delete_unresumable_archives: true },
};

const failure = (code: string, params: Record<string, string>, message: string) => ({ code, params, message });

/** What git says of a clone that failed: long, and with an address in it that has no place to break. */
const LONG_CLONE_DETAIL = "unable to access 'https://github.com/acme/design-system.git/': Could not resolve host: github.com";

/** What importing `REVIEW` with its defaults came to: a project whose directory was gone by the time it was
 * imported, a clone that failed with a long reason, a project nothing
 * placed, a console that could not be created with its project not tried, and the account
 * references that fell back. */
export const MIXED_RESULT: Omit<Imported, "type"> = {
  settings_applied: { auto_pull_repositories: true, auto_delete_unresumable_archives: false },
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
    description: "Choose the console the sidebar shows or everything, with the local-paths notice beneath. Export closes the dialog and raises a toast naming the saved file.",
    state,
    steps: exportFromSettings,
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
    description: "Opened from the welcome screen, where a new computer starts. The intro ends by saying which files hold application settings. The picker is never reported dismissed, as in a browser without the file input's cancel event: Choose file shows that it is waiting and ignores another press, while Cancel, the close button and Escape still work.",
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
    description: "The plain file-read error appears in a danger alert without a heading.",
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
    configuration: { file: { name: "photo.json", text: "not json at all" } },
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
    description:
      "Projects found at recorded or default paths, clone candidates, missing paths, account fallbacks and an empty console appear together; settings start unchecked, and their note has two lines: automatic pull turns on, archive cleanup turns off.",
    state,
    configuration: reviewing(),
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
    description:
      "A single console document has no application-settings checkbox; unavailable placement choices are disabled.",
    state,
    configuration: { file: FILE, inspection: SINGLE, imported: CLEAN_RESULT },
    steps: [
      ...importFromSettings,
      chooseFile,
    ],
  },
  {
    id: "configuration-import-review-setting-off",
    group: GROUP,
    category: "normal",
    title: "Import, an older document whose setting is off where it is on here",
    description: "The checkbox is ticked, and its note is a single line: applying turns automatic pull off. The file predates archive cleanup, so that setting has no line.",
    state: autoPulling,
    configuration: { file: FILE, inspection: WITH_SETTING_OFF },
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
    description: "The daemon's reason stands in the footer beside the buttons, over the review, which stays as it was.",
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
    description: "The settings failure is a line of its own above the table, beside a project that was left out.",
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
    configuration: reviewing(),
    description: "Empty and relative directories show errors together; the valid directory is accepted.",
    steps: [
      ...importFromSettings,
      chooseFile,
      (ui) => ui.press(ui.t("configuration.import.project.choice", { project: "Notes" })),
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
    configuration: reviewing(),
    description: "Placement outcomes, failed projects and consoles, fallback accounts and applied settings share the result; the settings show two lines, automatic pull on and archive cleanup off. The error alert the failures raise is dismissed to leave the result in view. Short height exposes scrolling; Phone folds rows.",
    steps: [
      ...importFromSettings,
      chooseFile,
      applySettings,
      importNow,
      (ui) => ui.press(ui.t("error.acknowledge")),
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
