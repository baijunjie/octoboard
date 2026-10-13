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
// inspections below say the application setting is off now; `syncing` is the machine where it is
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
  settings: { auto_sync_repositories: false, default_clone_dir: DEFAULT_CLONE_DIR, accounts },
});
const syncing = snapshotState({
  ...layout,
  settings: { auto_sync_repositories: true, default_clone_dir: DEFAULT_CLONE_DIR, accounts },
});
const noConsoleState = snapshotState({
  consoles: [],
  projects: [],
  sessions: [],
  settings: { auto_sync_repositories: false, default_clone_dir: DEFAULT_CLONE_DIR, accounts },
});

const openSettings = (ui: Ui) => ui.press(ui.t("rail.settings"));
const configurationSection = (ui: Ui) => ui.press(ui.t("settings.section.configuration"));
const openConsoleMenu = (ui: Ui) => ui.press(ui.t("sidebar.console.actions", { name: console_.name }));
const exportFromMenu = [openConsoleMenu, (ui: Ui) => ui.press(ui.t("sidebar.console.exportConfiguration"))];
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
  settings: { auto_sync_repositories: true, current_auto_sync_repositories: false },
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

/** The same document of a full export, whose application setting is off where this machine has it
 * on. */
const WITH_SETTING_OFF: Omit<Inspection, "type"> = {
  ...SINGLE,
  settings: { auto_sync_repositories: false, current_auto_sync_repositories: true },
};

const failure = (code: string, params: Record<string, string>, message: string) => ({ code, params, message });

/** What git says of a clone that failed: long, and with an address in it that has no place to break. */
const LONG_CLONE_DETAIL = "unable to access 'https://github.com/acme/design-system.git/': Could not resolve host: github.com";

/** What importing `REVIEW` with its defaults came to: a clone that failed with a long reason, a project nothing
 * placed, a console that could not be created with its project not tried, and the account
 * references that fell back. */
export const MIXED_RESULT: Omit<Imported, "type"> = {
  settings_applied: { auto_sync_repositories: true },
  consoles: [
    {
      name: "Work laptop",
      status: "created",
      console: "c-new-1",
      error: null,
      account_fallbacks: [{ agent: "claude", name: "Work" }],
      projects: [
        { name: "Website", status: "created", via: "exported_path", project: "p-new-1", error: null, account_fallbacks: [] },
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
    id: "configuration-settings",
    group: GROUP,
    title: "Settings, Configuration section",
    description: "The two rows that start an export and an import.",
    state,
    steps: [openSettings, configurationSection],
  },
  {
    id: "configuration-settings-unavailable",
    group: GROUP,
    title: "Settings, Configuration section where files cannot be saved or opened",
    description: "No row to press: the section only says it is not available here.",
    state,
    configuration: { unavailable: true },
    steps: [openSettings, configurationSection],
  },
  {
    id: "configuration-console-menu-unavailable",
    group: GROUP,
    title: "Console menu where files cannot be saved or opened",
    description: "The menu has no Export configuration item.",
    state,
    configuration: { unavailable: true },
    steps: [openConsoleMenu],
  },
  {
    id: "configuration-welcome-unavailable",
    group: GROUP,
    title: "Welcome screen where files cannot be saved or opened",
    description: "New console is the only action: there is no Import configuration under it.",
    state: noConsoleState,
    configuration: { unavailable: true },
  },
  {
    id: "configuration-export-console",
    group: GROUP,
    title: "Export, from a console's menu",
    description: "The scope is chosen between the console and everything, and the local-paths notice stands under it.",
    state,
    steps: exportFromMenu,
  },
  {
    id: "configuration-export-everything-only",
    group: GROUP,
    title: "Export, with no console to export on its own",
    description: "Reached from Settings, since there is no console menu: only everything is offered.",
    state: noConsoleState,
    steps: [openSettings, configurationSection, (ui) => ui.press(ui.t("settings.configuration.export.button"))],
  },
  {
    id: "configuration-export-saved",
    group: GROUP,
    title: "Export, saved",
    description: "The desktop app's save dialog is the user's; the toast names the file it was saved as.",
    state,
    steps: [...exportFromMenu, (ui) => ui.press(ui.t("configuration.export.submit"))],
  },
  {
    id: "configuration-export-downloaded",
    group: GROUP,
    title: "Export, handed to the browser",
    description: "A browser files the download itself and does not say as what, so the toast names no file.",
    state,
    configuration: { downloads: true },
    steps: [...exportFromMenu, (ui) => ui.press(ui.t("configuration.export.submit"))],
  },
  {
    id: "configuration-export-save-failed",
    group: GROUP,
    title: "Export, the file could not be written",
    state,
    configuration: { saveError: "Permission denied (os error 13)" },
    steps: [...exportFromMenu, (ui) => ui.press(ui.t("configuration.export.submit"))],
  },
  {
    id: "configuration-import-choose",
    group: GROUP,
    title: "Import, choosing the file",
    description:
      "Opened from the welcome screen, where a new computer starts. The picker has been opened and is never reported dismissed, as in a browser without the file input's cancel event: Choose file shows that it is waiting and ignores another press, while Cancel, the close button and Escape still work.",
    state: noConsoleState,
    configuration: { file: "pending" },
    steps: [(ui) => ui.press(ui.t("welcome.importConfiguration")), chooseFile],
  },
  {
    id: "configuration-import-read-failed",
    group: GROUP,
    title: "Import, the file could not be read",
    state,
    configuration: { readError: "Permission denied (os error 13)" },
    steps: [...importFromSettings, chooseFile],
  },
  {
    id: "configuration-import-file-too-large",
    group: GROUP,
    title: "Import, a file too large to be a configuration",
    state,
    configuration: { readError: { kind: "too_large" } },
    steps: [...importFromSettings, chooseFile],
  },
  {
    id: "configuration-import-file-not-text",
    group: GROUP,
    title: "Import, a file that is not text",
    state,
    configuration: { readError: { kind: "not_text" } },
    steps: [...importFromSettings, chooseFile],
  },
  {
    id: "configuration-import-not-json",
    group: GROUP,
    title: "Import, a file that is not JSON",
    state,
    configuration: { file: { name: "photo.json", text: "not json at all" } },
    steps: [...importFromSettings, chooseFile],
  },
  {
    id: "configuration-import-refused-format",
    group: GROUP,
    title: "Import, not a configuration file",
    state,
    configuration: refusal("notes.json", JSON.stringify({ hello: "world" }), failure("configuration_not_recognised", {}, "not a configuration document")),
    steps: [...importFromSettings, chooseFile],
  },
  {
    id: "configuration-import-refused-version",
    group: GROUP,
    title: "Import, a version this one cannot read",
    state,
    configuration: refusal(
      "octoboard-configuration.json",
      JSON.stringify({ format: "octoboard-configuration", version: 3, consoles: [] }),
      failure("configuration_version_unsupported", { version: "3", supported: "1" }, "version 3 is not supported"),
    ),
    steps: [...importFromSettings, chooseFile],
  },
  {
    id: "configuration-import-refused-unreadable",
    group: GROUP,
    title: "Import, a document that cannot be read",
    state,
    configuration: refusal(
      "octoboard-configuration.json",
      JSON.stringify({ format: "octoboard-configuration", version: 1, consoles: "none" }),
      failure("configuration_unreadable", { detail: "invalid type: string \"none\", expected a sequence" }, "unreadable"),
    ),
    steps: [...importFromSettings, chooseFile],
  },
  {
    id: "configuration-import-review",
    group: GROUP,
    title: "Import, review",
    description:
      "A project is a name with its choice beside it, the path it was exported from with a sentence on whether it was found, and where it would land. Every state a project can be in: found at its recorded path (placed there, so one line says both and the sentence is left out), found under the default clone directory, to be cloned, and found nowhere (Notes, whose long path takes two lines). Account references that fall back stand on the console and on a project, a console with nothing in it says so, and the settings are offered and left off.",
    state,
    configuration: reviewing(),
    steps: [...importFromSettings, chooseFile],
  },
  {
    id: "configuration-import-review-all-default",
    group: GROUP,
    title: "Import, one choice applied to every project",
    description:
      "“Use default clone directory” pressed: Website leaves its exported path for it and Search API already had it, while the projects that do not offer it keep the choice they had. Website, no longer placed at its exported path, now says it was found there.",
    state,
    configuration: reviewing(),
    steps: [...importFromSettings, chooseFile, (ui) => ui.press(ui.t("configuration.import.all.defaultCloneDir", { count: 2 }))],
  },
  {
    id: "configuration-import-review-directory",
    group: GROUP,
    title: "Import, projects placed in directories of the user's own",
    description:
      "Another directory chosen for three projects and Import pressed: Notes has nothing typed and Search API a relative path, each marked on its row, while Website's typed path is accepted. Dotfiles is left out, which takes its landing line away and says nothing more. In a window too short for the dialog the body is scrolled to the first mistake and fades at both edges.",
    state,
    configuration: reviewing(),
    steps: [
      ...importFromSettings,
      chooseFile,
      (ui) => ui.press(ui.t("configuration.import.project.choice", { project: "Notes" })),
      (ui) => ui.press(ui.t("configuration.import.choice.directory")),
      ...placeIn("Search API", "projects/search-api"),
      ...placeIn("Website", "~/work/website"),
      (ui) => ui.press(ui.t("configuration.import.project.choice", { project: "Dotfiles" })),
      (ui) => ui.press(ui.t("configuration.import.choice.skip")),
      importNow,
    ],
  },
  {
    id: "configuration-import-review-single",
    group: GROUP,
    title: "Import, review of one console's document",
    description:
      "No application settings, so there is no checkbox for them. The button for a choice no project offers is disabled, and the project found nowhere is left out.",
    state,
    configuration: { file: FILE, inspection: SINGLE, imported: CLEAN_RESULT },
    steps: [...importFromSettings, chooseFile],
  },
  {
    id: "configuration-import-review-setting-off",
    group: GROUP,
    title: "Import, a document whose setting is off where it is on here",
    description: "The checkbox is ticked, and its note says applying turns the setting off.",
    state: syncing,
    configuration: { file: FILE, inspection: WITH_SETTING_OFF },
    steps: [...importFromSettings, chooseFile, applySettings],
  },
  {
    id: "configuration-import-importing",
    group: GROUP,
    title: "Import, applying",
    description: "The reply comes after the last clone; the dialog cannot be dismissed meanwhile.",
    state,
    configuration: reviewing({ imported: "pending" }),
    steps: [...importFromSettings, chooseFile, importNow],
  },
  {
    id: "configuration-import-refused-apply",
    group: GROUP,
    title: "Import, refused before anything is created",
    description: "The daemon's reason stands in the footer beside the buttons, over the review, which stays as it was.",
    state,
    configuration: reviewing({ imported: failure("icon_too_large", { limit_kib: "256" }, "the console avatar is larger than 256 KiB") }),
    steps: [...importFromSettings, chooseFile, importNow],
  },
  {
    id: "configuration-import-result",
    group: GROUP,
    title: "Import, result with failures and fallbacks",
    description:
      "One row per project: how each created one was placed, one left out, one that failed with its reason (a long one, wrapping in its column), and a console that could not be created as a row of its own, with its project shown as not tried. A console created with nothing in it is a row too. A sentence above the table counts the outcomes, each count naming what it counts. The accounts that fell back are listed under the table, and the settings that were asked for are reported applied. At the Phone width the table folds into one labelled block per row.",
    state,
    configuration: reviewing(),
    steps: [...importFromSettings, chooseFile, applySettings, importNow],
  },
  {
    id: "configuration-import-result-short-window",
    group: GROUP,
    title: "Import, result in a window too short for it",
    description:
      "A page of text with no control in it but the Done button. Its body scrolls, fading at the edge it can still be scrolled towards, and is a region of its own that the keyboard reaches: it is focused here, its ring drawn round the whole body, as it is for a user who tabbed to it.",
    height: 480,
    state,
    configuration: reviewing(),
    steps: [...importFromSettings, chooseFile, applySettings, importNow, (ui) => ui.tabToRegion(ui.t("configuration.import.title"))],
  },
  {
    id: "configuration-import-result-clean",
    group: GROUP,
    title: "Import, result with nothing to report",
    description: "The summary says nothing failed, and each project's row says how it was placed: at its recorded path, cloned, in the directory the user gave.",
    state,
    configuration: { file: FILE, inspection: SINGLE, imported: CLEAN_RESULT },
    steps: [...importFromSettings, chooseFile, ...placeIn("Releases", "~/work/releases"), importNow],
  },
  {
    id: "configuration-import-result-settings-failed",
    group: GROUP,
    title: "Import, result where the settings could not be applied",
    description: "The settings failure is a line of its own above the table, beside a project that was left out.",
    state: syncing,
    configuration: { file: FILE, inspection: WITH_SETTING_OFF, imported: SETTINGS_FAILED_RESULT },
    steps: [...importFromSettings, chooseFile, applySettings, importNow],
  },
];
