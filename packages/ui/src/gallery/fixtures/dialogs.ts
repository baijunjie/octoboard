import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { SAMPLE, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

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
    ...sessions.map((s) => (s.id === SAMPLE.consoleSession.id ? { ...s, title: "Hub 1" } : s)),
    sessionOf("s-console-2", console_.id, undefined, "Hub 2", "working", { colour: "rose", started_at: minutesAgo(20) }),
    sessionOf("s-console-old", console_.id, undefined, "Hub 3", "archived", { colour: "azure" }),
  ],
});

const consoleActions = (ui: Ui) =>
  ui.press(ui.t("sidebar.console.actions", { name: console_.name }));
const projectActions = (ui: Ui, name: string) =>
  ui.press(ui.t("sidebar.project.actions", { name }));
const editTaggedProject = [
  (ui: Ui) => projectActions(ui, tagged[0].name),
  (ui: Ui) => ui.press(ui.t("sidebar.project.edit")),
];

export const dialogScenarios: Scenario[] = [
  {
    id: "dialog-new-console",
    group: GROUP,
    title: "New console",
    state,
    steps: [(ui) => ui.press(ui.t("titleBar.newConsole"))],
  },
  {
    id: "dialog-edit-console",
    group: GROUP,
    title: "Edit console",
    state,
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
    id: "dialog-edit-project-tags",
    group: GROUP,
    title: "Edit project with tags",
    description: "The tags field of a project that carries tags, each removable.",
    state: taggedState,
    steps: editTaggedProject,
  },
  {
    id: "dialog-edit-project-tag-suggestions",
    group: GROUP,
    title: "Tag suggestions in the project dialog",
    description: "The field's list offers the tags the other projects carry, and none this project already has.",
    state: taggedState,
    steps: [...editTaggedProject, (ui) => ui.press(ui.t("dialog.project.tagsSuggestions"))],
  },
];
