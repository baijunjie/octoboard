import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { SAMPLE, snapshotState } from "./builders";

const GROUP = "Dialogs";

const { console: console_, web, api, sessions } = SAMPLE;
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });

const consoleActions = (ui: Ui) =>
  ui.press(ui.t("sidebar.console.actions", { name: console_.name }));
const projectActions = (ui: Ui, name: string) =>
  ui.press(ui.t("sidebar.project.actions", { name }));

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
    id: "dialog-directory-picker",
    group: GROUP,
    title: "Directory picker",
    description: "Browse from the add-project dialog; the fixture lists generic directories.",
    state,
    steps: [
      consoleActions,
      (ui) => ui.press(ui.t("sidebar.console.addProject")),
      (ui) => ui.press(ui.t("dialog.project.browse")),
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
];
