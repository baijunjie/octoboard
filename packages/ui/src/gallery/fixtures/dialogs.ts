import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { SAMPLE, consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";
import { IMAGES } from "./viewerImages";

const GROUP = "Dialogs";

const { console: console_, web, api, sessions } = SAMPLE;

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

export const dialogScenarios: Scenario[] = [
  {
    id: "dialog-new-console-accounts",
    group: GROUP,
    title: "Console, project and session dialogs",
    description: "New console offers accounts; Advanced starts on the agent defaults and Haiku takes no effort. Main has an avatar and retired stored model; Plain console has neither. Edit Configured Site for stored account/model/effort, Default Account Site for inherit/default/named choices, and Docs Site for tag suggestions. New session offers live owners, leaving archived Hub 3 out. Website removal lists default, Work and removed-directory accounts; Hub 4 archive lists its interrupted children. Add project: ~/code/slow stays checking, ~/code/site detects Codex, ~/code/missing fails, and git@example.com:o/private.git is unreadable. The parent-directory source hides account/Advanced; Browse opens a long directory listing. Empty Add shows field errors.",
    state: dialogState,
    directories: { "/Users/dev": HOME_LISTING },
    detections: { ...{ "~/code/slow": "pending" },
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
    } },
    steps: [
      (ui) => projectActions(ui, web.name),
      (ui) => ui.press(ui.t("sidebar.project.remove")),
      (ui) => ui.hoverCalloutAgent("Fix the summary layout", ui.t("settings.accounts.defaultName")),
      (ui) => ui.hoverCalloutAgent("Update the dependencies", "Work"),
      (ui) => ui.hoverCalloutAgent("Check the release notes", "/Users/dev/.claude-removed"),
      (ui) => ui.press(ui.t("common.cancel")),
      (ui) => ui.press(ui.t("sidebar.session.actions", { title: "Hub 4" })),
      (ui) => ui.press(ui.t("sidebar.session.archive")),
      (ui) => ui.hoverCalloutStatus("Tidy the changelog"),
      (ui) => ui.hoverCalloutAgent("Rename the index", "Work"),
      (ui) => ui.press(ui.t("common.cancel")),
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
      (ui) => ui.press(ui.t("common.cancel")),
      (ui) => projectActions(ui, configured.name),
      (ui) => ui.press(ui.t("sidebar.project.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.advanced")),
      (ui) => ui.press(ui.t("common.cancel")),
      (ui) => projectActions(ui, web.name),
      (ui) => ui.press(ui.t("sidebar.project.openSession")),
      (ui) => ui.press(ui.t("dialog.session.ownerNone")),
      (ui) => ui.key("Escape"),
      (ui) => ui.key("Escape"),
      (ui) => ui.wait(300),
      ...editTaggedProject,
      (ui) => ui.press(ui.t("dialog.project.tagsSuggestions")),
      (ui) => ui.key("Escape"),
      (ui) => ui.key("Escape"),
      (ui) => ui.wait(300),
      (ui) => projectActions(ui, onDefault.name),
      (ui) => ui.press(ui.t("sidebar.project.edit")),
      (ui) => ui.press(ui.t("dialog.agentSettings.account", { agent: "Claude Code" })),
      (ui) => ui.key("Escape"),
      (ui) => ui.key("Escape"),
      (ui) => ui.wait(300),
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press(ui.t("common.browse")),
      (ui) => ui.wait(300),
      (ui) => ui.key("Escape"),
      (ui) => ui.key("Escape"),
      (ui) => ui.wait(300),
      (ui) => ui.press(ui.t("rail.newConsole")),
    ],
  },
  {
    id: "dialog-new-console-models-loading",
    group: GROUP,
    title: "New console while the model list is out",
    description: "The model and effort pickers are locked and the model field says the list is loading.",
    state: dialogState,
    agentModels: { claude: "pending" },
    steps: [(ui) => ui.press(ui.t("rail.newConsole")), (ui) => ui.press(ui.t("dialog.agentSettings.advanced"))],
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
    steps: [consoleActions, (ui) => ui.press(ui.t("sidebar.console.edit")), (ui) => ui.press(ui.t("dialog.agentSettings.advanced"))],
  },
];
