import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { SAMPLE, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";
import { IMAGES } from "./viewerImages";

const GROUP = "Dialogs";

const { console: console_, web, api, sessions } = SAMPLE;
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });

// Two projects carrying tags, so the edited one has some of its own and the field suggests the rest.
const tagged = [
  projectOf("p-docs", console_.id, "Docs Site", { tags: ["frontend", "customer-facing"] }),
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

// A console session whose bound sessions are all interrupted, so archiving it takes them along.
const cascadeHub = sessionOf("s-hub-4", console_.id, undefined, "Hub 4", "idle", { colour: "azure" });
const cascadeState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions: [
    cascadeHub,
    sessionOf("s-web-3", console_.id, web.id, "Tidy the changelog", "interrupted", { bound_to: cascadeHub.id }),
    sessionOf("s-api-3", console_.id, api.id, "Rename the index", "interrupted", { bound_to: cascadeHub.id }),
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

const addProject = [consoleActions, (ui: Ui) => ui.press(ui.t("sidebar.console.addProject"))];

export const dialogScenarios: Scenario[] = [
  {
    id: "dialog-new-console",
    group: GROUP,
    title: "New console",
    state,
    steps: [(ui) => ui.press(ui.t("rail.newConsole"))],
  },
  {
    id: "dialog-new-console-over-welcome",
    group: GROUP,
    title: "New console, over the welcome screen",
    description: "The first console is created over the welcome screen, with no sidebar behind the dialog.",
    state: snapshotState({}),
    // The rail's button, not the welcome screen's: both carry the same name, and `press` finds the
    // first of them in the DOM. What this scenario is for is the dialog over the welcome screen, and
    // either button puts it there; that the welcome screen's own button opens it is a unit test.
    steps: [(ui) => ui.press(ui.t("rail.newConsole"))],
  },
  {
    id: "dialog-edit-console",
    group: GROUP,
    title: "Edit console",
    state,
    steps: [consoleActions, (ui) => ui.press(ui.t("sidebar.console.edit"))],
  },
  {
    id: "dialog-new-console-accounts",
    group: GROUP,
    title: "New console where the agent has accounts",
    description: "The agent and its account are one input group; agents with no account of their own show the agent alone.",
    state: accountsState,
    steps: [(ui) => ui.press(ui.t("rail.newConsole"))],
  },
  {
    id: "dialog-new-console-advanced",
    group: GROUP,
    title: "New console with Advanced open",
    description: "The model and reasoning effort pickers empty, each showing that the agent's own setting is used, with the model list the daemon answered.",
    state,
    steps: [(ui) => ui.press(ui.t("rail.newConsole")), (ui) => ui.press(ui.t("dialog.agentSettings.advanced"))],
  },
  {
    id: "dialog-new-console-models-loading",
    group: GROUP,
    title: "New console while the model list is out",
    description: "The model and effort pickers are locked and the model field says the list is loading.",
    state,
    agentModels: { claude: "pending" },
    steps: [(ui) => ui.press(ui.t("rail.newConsole")), (ui) => ui.press(ui.t("dialog.agentSettings.advanced"))],
  },
  {
    id: "dialog-edit-console-models-unavailable",
    group: GROUP,
    title: "Edit a console whose agent cannot list its models",
    description: "The daemon found no binary for the agent, so the model picker offers the stored model alone and says why; the effort picker still works over what is stored.",
    state: launchSettingsState,
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
    steps: [consoleActions, (ui) => ui.press(ui.t("sidebar.console.edit")), (ui) => ui.press(ui.t("dialog.agentSettings.advanced"))],
  },
  {
    id: "dialog-new-console-models-no-effort",
    group: GROUP,
    title: "New console with a model that takes no reasoning effort",
    description: "Haiku is chosen: the effort picker stays operable over an empty list, and its note says the model takes none.",
    state,
    steps: [
      (ui) => ui.press(ui.t("rail.newConsole")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
      // The pickers are named by their placeholder, the Model one being the first.
      (ui) => ui.press(ui.t("dialog.agentSettings.placeholder")),
      (ui) => ui.press("Haiku"),
    ],
  },
  {
    id: "dialog-edit-console-launch-settings",
    group: GROUP,
    title: "Edit a console with a stored model and effort",
    description: "The stored model is not in the list the daemon answered, so it is kept and marked; the effort is a listed one.",
    state: launchSettingsState,
    steps: [consoleActions, (ui) => ui.press(ui.t("sidebar.console.edit")), (ui) => ui.press(ui.t("dialog.agentSettings.advanced"))],
  },
  {
    id: "dialog-edit-console-avatar",
    group: GROUP,
    title: "Edit a console with an avatar",
    description: "The image fills the round control, with its delete button over the corner.",
    state: avatarState,
    steps: [consoleActions, (ui) => ui.press(ui.t("sidebar.console.edit"))],
  },
  {
    id: "dialog-delete-console",
    group: GROUP,
    title: "Confirm deleting a console",
    description: "The destructive confirmation where the user types a word.",
    state,
    steps: [consoleActions, (ui) => ui.press(ui.t("sidebar.console.delete"))],
  },
  {
    id: "dialog-add-project",
    group: GROUP,
    title: "Add project",
    state,
    steps: [consoleActions, (ui) => ui.press(ui.t("sidebar.console.addProject"))],
  },
  {
    id: "dialog-add-project-invalid",
    group: GROUP,
    title: "Add project with nothing filled in",
    description: "Submitting an empty form: what is required is said under the field it belongs to.",
    state,
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press(ui.t("common.add")),
    ],
  },
  {
    id: "dialog-add-project-checking",
    group: GROUP,
    title: "Add project while the directory is checked",
    description: "The daemon has not answered for the path yet: the form says so and keeps what is below the path disabled.",
    state,
    detections: { "~/code/slow": "pending" },
    steps: [...addProject, (ui) => ui.focus(ui.t("dialog.project.directory")), (ui) => ui.type("~/code/slow"), (ui) => ui.wait(700)],
  },
  {
    id: "dialog-add-project-detected",
    group: GROUP,
    title: "Add project with a detected agent",
    description: "The directory's agent is preselected in the Default agent field, which stays changeable.",
    state,
    detections: { "~/code/site": { agent: "codex" } },
    steps: [...addProject, (ui) => ui.focus(ui.t("dialog.project.directory")), (ui) => ui.type("~/code/site"), (ui) => ui.wait(700)],
  },
  {
    id: "dialog-add-project-path-refused",
    group: GROUP,
    title: "Add project with a path that is no directory",
    description: "The refusal shows under the path field and keeps the fields below it disabled.",
    state,
    detections: {
      "~/code/missing": { code: "path_not_found", params: { path: "/Users/me/code/missing" }, message: "`/Users/me/code/missing` does not exist" },
    },
    steps: [...addProject, (ui) => ui.focus(ui.t("dialog.project.directory")), (ui) => ui.type("~/code/missing"), (ui) => ui.wait(700)],
  },
  {
    id: "dialog-add-project-git-unreachable",
    group: GROUP,
    title: "Add project from a repository that cannot be read",
    description: "The probe failed: git's reason shows under the URL, and everything below it stays disabled.",
    state,
    detections: {
      "git@example.com:o/private.git": {
        code: "git_remote_unreachable",
        params: { detail: "Permission denied (publickey)." },
        message: "the repository could not be read: Permission denied (publickey).",
      },
    },
    steps: [
      ...addProject,
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.local"))),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.git"))),
      (ui) => ui.focus(ui.t("dialog.project.repositoryUrl")),
      (ui) => ui.type("git@example.com:o/private.git"),
      (ui) => ui.wait(1100),
    ],
  },
  {
    id: "dialog-add-project-parent",
    group: GROUP,
    title: "Add the repositories under a parent directory",
    description: "\"Detect from files\" is preselected, with the helper text saying what it reads; the agent has accounts, yet neither the account picker nor Advanced is offered.",
    state: accountsState,
    steps: [
      ...addProject,
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.local"))),
      (ui) => ui.press((name) => name.includes(ui.t("dialog.project.source.parent"))),
    ],
  },
  {
    id: "dialog-directory-picker",
    group: GROUP,
    title: "Directory picker",
    description: "Browse from the add-project dialog; the fixture lists generic directories.",
    state,
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press(ui.t("common.browse")),
      (ui) => ui.wait(300),
    ],
  },
  {
    id: "dialog-new-session",
    group: GROUP,
    title: "New session",
    state,
    steps: [(ui) => projectActions(ui, web.name), (ui) => ui.press(ui.t("sidebar.project.openSession"))],
  },
  {
    id: "dialog-new-session-owner",
    group: GROUP,
    title: "New session, choosing its console session",
    description: "The owner choice open: none, then each live console session beside its colour; the archived one is absent.",
    state: twoHubsState,
    steps: [
      (ui) => projectActions(ui, web.name),
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.press(ui.t("dialog.session.ownerNone")),
    ],
  },
  {
    id: "dialog-remove-project-running",
    group: GROUP,
    title: "Remove a project with running sessions",
    state,
    steps: [(ui) => projectActions(ui, web.name), (ui) => ui.press(ui.t("sidebar.project.remove"))],
  },
  {
    id: "dialog-archive-session",
    group: GROUP,
    title: "Confirm archiving a session",
    state,
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Fix the summary layout" })),
      (ui) => ui.press(ui.t("sidebar.session.archive")),
    ],
  },
  {
    id: "dialog-archive-console-session",
    group: GROUP,
    title: "Confirm archiving a console session with bound sessions",
    description: "The confirmation lists the interrupted sessions bound to it, which are archived with it.",
    state: cascadeState,
    steps: [
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Hub 4" })),
      (ui) => ui.press(ui.t("sidebar.session.archive")),
    ],
  },
  {
    id: "dialog-edit-project-tag-suggestions",
    group: GROUP,
    title: "Tag suggestions in the project dialog",
    description: "The field's list offers the tags the other projects carry, and none this project already has.",
    state: taggedState,
    steps: [...editTaggedProject, (ui) => ui.press(ui.t("dialog.project.tagsSuggestions"))],
  },
  {
    id: "dialog-add-project-accounts",
    group: GROUP,
    title: "Add project where the agent has accounts",
    description: "The default agent and its account are one input group, the account starting on inheriting the console's.",
    state: accountsState,
    steps: addProject,
  },
  {
    id: "dialog-edit-project-launch-settings",
    group: GROUP,
    title: "Edit a project with a stored account, model and effort",
    description: "The stored model is not in the list the daemon answered, so it is kept and marked; the effort is a listed one.",
    state: configuredState,
    steps: [
      (ui) => projectActions(ui, configured.name),
      (ui) => ui.press(ui.t("sidebar.project.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
    ],
  },
  {
    id: "dialog-edit-project-default-account",
    group: GROUP,
    title: "Edit a project on the agent's default account",
    description: "The console pins a named account; the project is on the agent's default account, so its sessions do not take the console's.",
    state: onDefaultState,
    steps: [
      (ui) => projectActions(ui, onDefault.name),
      (ui) => ui.press(ui.t("sidebar.project.edit")),
    ],
  },
  {
    id: "dialog-edit-project-account-choices",
    group: GROUP,
    title: "Edit a project with its account choices open",
    description: "Inheriting the console's, the agent's default account, then its named accounts.",
    state: onDefaultState,
    steps: [
      (ui) => projectActions(ui, onDefault.name),
      (ui) => ui.press(ui.t("sidebar.project.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.account", { agent: "Claude Code" })),
    ],
  },
];
