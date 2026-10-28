import { AGENT_LABEL } from "../../agents";
import { describing, type Ui } from "../interact";
import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState, switchingAccount } from "./builders";
import type { FixtureError } from "./projectFiles";

const GROUP = "Sidebar sessions";

const console_ = consoleOf("c-1", "Main");
const web = projectOf("p-1", console_.id, "Website");

// A running session that chose its own model and effort, one interrupted that chose only a model, one still
// launching and one in the middle of an account switch.
const running = sessionOf("s-model-running", console_.id, web.id, "Rework the result ranking", "idle", {
  started_at: minutesAgo(50),
  model: "sonnet",
  effort: "medium",
  observed_model: "claude-sonnet-5-5",
  observed_effort: "medium",
});
const interrupted = sessionOf("s-model-interrupted", console_.id, web.id, "Fix the summary layout", "interrupted", { started_at: minutesAgo(80), model: "sonnet" });
const starting = sessionOf("s-model-starting", console_.id, web.id, "Set up the search index", "starting", { started_at: minutesAgo(1) });
const switching = sessionOf("s-model-switching", console_.id, web.id, "Check the index sizes", "idle", { started_at: minutesAgo(30), model: "sonnet", effort: "medium" });

const state = snapshotState({ consoles: [console_], projects: [web], sessions: [running, interrupted, starting, switching] });
const menuState = {
  ...state,
  settings: { ...state.settings!, accounts: [{ id: "a-model-work", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" }] },
};
const switchingState = switchingAccount(menuState, [switching.id]);

// The session chose its model and effort and its records have not named any yet, as right after a change.
const chosen = { ...running, model: "opus", effort: "high", observed_model: undefined, observed_effort: undefined };
const chosenState = snapshotState({ consoles: [console_], projects: [web], sessions: [chosen] });

const openMenu = (title: string) => (ui: Ui) => ui.press(ui.t("sidebar.session.actions", { title }));
const openDialog = (title: string) => [openMenu(title), (ui: Ui) => ui.press(ui.t("sidebar.session.changeModel"))];
// The dialog of the running session, with the Model picker moved from Sonnet to Opus, which the Confirm button then submits;
// the dialog closes at once and the change goes on in the background.
const changeToOpus = [
  (ui: Ui) => ui.press(ui.session(running.title)),
  ...openDialog(running.title),
  (ui: Ui) => ui.press(describing("the Model picker holding Sonnet", (name) => name.startsWith("Sonnet"))),
  (ui: Ui) => ui.press("Opus"),
  (ui: Ui) => ui.confirm(),
];

const didNotStop: FixtureError = {
  code: "model_change_did_not_stop",
  params: { session: running.id },
  message: "the session's process did not end in time, so its model was not changed",
};
const didNotComeUp: FixtureError = {
  code: "model_change_did_not_come_up",
  params: { session: running.id },
  message: "the session ended as soon as it was relaunched with the new model, so it keeps the model and effort it had",
};

export const sessionModelScenarios: Scenario[] = [
  {
    id: "sidebar-session-menu-switching-account",
    group: GROUP,
    category: "normal",
    title: "Menu of a session switching its account, Change model disabled",
    description: "Change model, with its chip icon, is greyed out with no reason under its name, as is Hand off, since the daemon refuses both until the switch ends; it stands after Copy session ID, before Switch account and Hand off.",
    state: switchingState,
    steps: [openMenu(switching.title)],
  },
  {
    id: "focus-session-menu-change-model",
    group: "Focus mode",
    category: "normal",
    title: "Focus mode card menu with Change model",
    description: "The actions menu of the Rework the result ranking card offers Change model, enabled, with the chip icon, after Copy session ID and above Switch account, followed by Hand off, as in the sidebar row's menu. The Set up the search index card, still starting, reads Starting.",
    state: menuState,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${web.id}` },
    steps: [openMenu(running.title)],
  },
  {
    id: "dialog-session-model-running",
    group: GROUP,
    category: "normal",
    title: "Change the model of a running session",
    description: "The body names the session and says it restarts to take the new values, keeps its conversation, and that a turn in progress is cut off and continued where that is safe. Model and Reasoning effort start on what the session chose, Sonnet and medium, and Change model is greyed out until one of them differs; the note says a changed value is kept for this session from now on.",
    state,
    steps: openDialog(running.title),
  },
  {
    id: "dialog-session-model-interrupted",
    group: GROUP,
    category: "normal",
    title: "Change the model of an interrupted session",
    description: "The body says the session is not running and takes the values the next time it is resumed, with no word of a restart. Model starts on Sonnet and Reasoning effort is empty, since the session chose none.",
    state,
    steps: openDialog(interrupted.title),
  },
  {
    id: "dialog-session-model-models-loading",
    group: GROUP,
    category: "normal",
    title: "Change model while the model list is out",
    description: "Both pickers are locked with the values the session chose, Sonnet and medium, and a note under the Model field says the models are loading. Neither value carries a Not in the list badge, since nothing is known yet of what the agent lists; Change model stays greyed out.",
    state,
    agentModels: { claude: "pending" },
    steps: openDialog(running.title),
  },
  {
    id: "sidebar-session-model-restarting",
    group: GROUP,
    category: "normal",
    title: "Session restarting to take a new model",
    description: "Opus was picked and Change model pressed, so the dialog has closed and nothing else is open. The session reads Starting, with the turning arc, in its sidebar row and in the top bar, with the Starting session note in the main area, and stays so until the daemon answers.",
    state,
    sessionModelChanges: { [running.id]: "pending" },
    steps: changeToOpus,
  },
  {
    id: "sidebar-session-model-own-choice",
    group: GROUP,
    category: "normal",
    title: "Session row agent mark tooltip right after a model change",
    description: "The pointer rests on the Claude Code mark of Rework the result ranking, which was restarted with Opus and high: its records have named no model yet, so the tooltip shows the session's own choice, Opus and high, where it showed the model it ran before, under the default account.",
    state: chosenState,
    preferences: { sidebarConsole: console_.id },
    steps: [(ui) => ui.hoverSessionAgent(ui.session(chosen.title), AGENT_LABEL.claude, { account: ui.t("settings.accounts.defaultName"), model: "Opus", effort: "high" })],
  },
  {
    id: "dialog-session-model-models-unavailable",
    group: GROUP,
    category: "exceptional",
    title: "Change model of a session whose agent cannot list its models",
    description: "The daemon found no binary for Claude Code, so a warning mark named \"Could not load the models\" stands after the \"Model\" label, and the pickers hold the session's own Sonnet and medium, each plain with no Not in the list badge, since the list is unknown and the mark already says so; Change model stays greyed out.",
    state,
    agentModels: {
      claude: {
        models: [],
        unavailable: { code: "models_binary_not_found", params: { binary: "claude" }, message: "claude was not found on the login shell's PATH" },
      },
    },
    steps: openDialog(running.title),
  },
  {
    id: "dialog-session-model-did-not-stop",
    group: GROUP,
    category: "exceptional",
    title: "Model change refused, the session did not stop",
    description: "The danger alert titled \"Could not change the model\" names the session and project on separate labelled lines, then says the session did not stop in time so its model was not changed and it may still be running, with the advice for a process that does not stop. The dialog has closed before the alert; the session behind it reads Idle with the model and effort it had.",
    state,
    sessionModelChanges: { [running.id]: didNotStop },
    steps: changeToOpus,
  },
  {
    id: "dialog-session-model-did-not-come-up",
    group: GROUP,
    category: "exceptional",
    title: "Model change refused, the relaunched session ended at once",
    description: "The danger alert titled \"Could not change the model\" names the session and project on separate labelled lines, then says the session ended as soon as it was relaunched with the new model, so it keeps the model and reasoning effort it had, with the advice to open the session launch settings and pick a model and effort the agent offers. The dialog has closed before the alert; the session behind it reads as not running, with Resume offered, still on the model and effort it had.",
    state,
    sessionModelChanges: { [running.id]: didNotComeUp },
    steps: changeToOpus,
  },
];
