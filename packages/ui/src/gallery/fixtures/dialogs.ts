import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { SAMPLE, consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";
import { IMAGES } from "./viewerImages";
import { SHELL_TIMEOUT } from "./shellTimeout";

const GROUP = "Dialogs";

const { console: console_, web, api, sessions } = SAMPLE;

// Two projects carrying tags, so the edited one has some of its own and the field suggests the rest.
const tagged = [
  projectOf("p-docs", console_.id, "Docs Site", { tags: ["x", "frontend", "项目界面布局检查用的长标签名称"] }),
  projectOf("p-billing", console_.id, "Billing Service", { tags: ["backend", "payments"] }),
];
const taggedState = snapshotState({ consoles: [console_], projects: tagged, sessions: [] });

// A second live console session beside the sample's, and an archived one the dialog must not offer.
const twoHubsState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [
    ...sessions.map((s) => (s.id === SAMPLE.consoleSession.id ? { ...s, title: "Hub" } : s)),
    sessionOf("s-console-2", console_.id, undefined, "Hub 2", "working", { colour: "rose", started_at: minutesAgo(20) }),
    sessionOf("s-console-old", console_.id, undefined, "Hub 3", "archived", { colour: "azure" }),
  ],
});

// Two accounts of one agent, so the console dialog has something for its account picker to offer.
const accountsState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions,
  settings: {
    auto_sync_repositories: false,
    default_clone_dir: "/Users/dev/Projects",
    accounts: [
      { id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work" },
      { id: "a-personal", agent: "claude", name: "Personal", config_dir: "/Users/dev/.claude-personal" },
    ],
  },
});

// A console session whose bound sessions are all interrupted, so archiving it takes them along.
const cascadeHub = sessionOf("s-hub-4", console_.id, undefined, "Hub 4", "idle", { colour: "azure" });
const cascadeState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [
    cascadeHub,
    sessionOf("s-web-3", console_.id, web.id, "Tidy the changelog", "interrupted", { bound_to: cascadeHub.id }),
    sessionOf("s-api-3", console_.id, api.id, "Rename the index", "interrupted", { bound_to: cascadeHub.id, account_id: "a-work" }),
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
  consoles: [{ ...console_, claude_console_session_model: "retired-model", claude_console_session_effort: "high" }],
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
  claude_session_model: "retired-model",
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

const consoleActions = (ui: Ui) =>
  ui.press(ui.t("sidebar.console.actions", { name: console_.name }));
const projectActions = (ui: Ui, name: string) =>
  ui.press(ui.t("sidebar.project.actions", { name }));
const editTaggedProject = [
  (ui: Ui) => projectActions(ui, tagged[0].name),
  (ui: Ui) => ui.press(ui.t("sidebar.project.edit")),
];

/** A home directory with more subdirectories than the picker's list is tall, some of them repositories. */
const HOME_LISTING = [
  "Applications",
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
    [console_.id, { ...launchSettingsState.consoles!.get(console_.id)!, icon: avatarState.consoles!.get(console_.id)!.icon, claude_account_id: "a-personal" }],
    ["c-plain", consoleOf("c-plain", "Plain console")],
  ]),
  projects: new Map([...accountsState.projects!, ...taggedState.projects!, ...configuredState.projects!, ...onDefaultState.projects!]),
  sessions: new Map([...twoHubsState.sessions!, ...removeState.sessions!, ...cascadeState.sessions!]),
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

export const dialogScenarios: Scenario[] = [
  {
    ...dialogFixture,
    id: "dialog-new-console-accounts",
    group: GROUP,
    title: "New console with accounts",
    steps: [
      (ui) => ui.press(ui.t("rail.newConsole")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-new-console-advanced",
    group: GROUP,
    title: "New console advanced settings",
    steps: [
      (ui) => ui.press(ui.t("rail.newConsole")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-edit-console-launch-settings",
    group: GROUP,
    title: "Edit console with avatar and stored launch settings",
    description: "The retired model is retained and marked; the avatar and its remove control are visible.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-delete-console",
    group: GROUP,
    title: "Delete console confirmation",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.delete")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project",
    group: GROUP,
    title: "Add project",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-invalid",
    group: GROUP,
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
    group: GROUP,
    title: "Session launch, shell environment timed out",
    description: "The danger alert presents the failed shell command, cause, launch consequence and recovery as separate readable parts. Short shows the recovery at the bottom of the scrolling body.",
    sessionOpenError: SHELL_TIMEOUT,
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.focus(ui.t("dialog.session.titleField")),
      (ui) => ui.confirm(),
      async () => { document.querySelector<HTMLElement>('[role="dialog"] [role="alert"]')?.scrollIntoView({ block: "end" }); },
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-new-session-owner",
    group: GROUP,
    title: "New session owner choices",
    steps: [
      (ui) => projectActions(ui, web.name),
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.press(ui.t("dialog.session.ownerNone")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-remove-project-running",
    group: GROUP,
    title: "Remove project with running sessions",
    description: "Default, named and removed-directory account cases coexist in the confirmation; hover each agent mark.",
    steps: [
      (ui) => projectActions(ui, web.name),
      (ui) => ui.press(ui.t("sidebar.project.remove")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-archive-console-session",
    group: GROUP,
    title: "Archive console session and its bound sessions",
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Hub 4" })),
      (ui) => ui.press(ui.t("sidebar.session.archive")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-edit-project-tag-suggestions",
    group: GROUP,
    title: "Project tag suggestions",
    description: "Short and long Chinese tags share the field with the input. The short tag fits its content until hover or keyboard focus makes room for its remove button; unused project tags appear below.",
    steps: [
      ...editTaggedProject,
      (ui) => ui.press(ui.t("dialog.project.tagsSuggestions")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-edit-project-launch-settings",
    group: GROUP,
    title: "Stored project account and launch settings",
    steps: [
      (ui) => projectActions(ui, configured.name),
      (ui) => ui.press(ui.t("sidebar.project.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-edit-project-account-choices",
    group: GROUP,
    title: "Project account choices",
    steps: [
      (ui) => projectActions(ui, onDefault.name),
      (ui) => ui.press(ui.t("sidebar.project.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.account", { agent: "Claude Code" })),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-directory-picker",
    group: GROUP,
    title: "Directory picker",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press(ui.t("common.browse")),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-checking",
    group: GROUP,
    title: "Directory detection pending",
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
    group: GROUP,
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
    group: GROUP,
    title: "Project path refused",
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
    group: GROUP,
    title: "Parent directory source",
    description: "Detect from files explains the recognized instruction files and directories as code; the agent picker is a single field.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.local"))),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.parent"))),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-parent-account",
    group: GROUP,
    title: "Parent directory with a chosen agent and account",
    description: "The instruction-file hint remains below the joined agent and account fields when Detect from files is replaced by a chosen agent.",
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.local"))),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.parent"))),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.agentDetect"))),
      (ui) => ui.press("Claude Code"),
    ],
  },
  {
    ...dialogFixture,
    id: "dialog-add-project-git-unreachable",
    group: GROUP,
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
    group: GROUP,
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
    group: GROUP,
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
