import { providerLabel } from "../../agents/providers";
import { FIXTURE_PROVIDERS, servedLabel } from "../../fixtures/fixtureProviders";
import type { Session } from "../../protocol";
import { describing, type Ui } from "../../fixtures/interact";
import type { Scenario } from "../scenario";
import { accountOf, consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "../../fixtures/builders";
import type { FixtureError } from "../../fixtures/sampleFiles";

const GROUP = "Sidebar sessions";

const console_ = consoleOf("c-1", "Main");
const web = projectOf("p-1", console_.id, "Website");

// A running session that chose its own model and effort, and one still launching.
const running = sessionOf("s-model-running", console_.id, web.id, "Rework the result ranking", "idle", {
  started_at: minutesAgo(50),
  model: "sonnet",
  effort: "medium",
  observed_model: "claude-sonnet-5-5",
  observed_effort: "medium",
});
const starting = sessionOf("s-model-starting", console_.id, web.id, "Set up the search index", "starting", { started_at: minutesAgo(1) });

const state = snapshotState({ consoles: [console_], projects: [web], sessions: [running, starting] });
const menuState = {
  ...state,
  settings: { ...state.settings!, accounts: [accountOf("a-model-work", "claude", "Work", "/Users/dev/.claude-work")] },
};

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
  // The option is named by the model (and its badge, if any); the version under it is its description.
  (ui: Ui) => ui.press(describing("the Opus option", (name) => name.startsWith("Opus"))),
  (ui: Ui) => ui.confirm(),
];

// The dialog of the running session with Reasoning effort moved to high, then the session changed from outside the
// dialog by `outside`, as another window or the daemon would.
const editedThenChangedOutside = (outside: Partial<Session>) => [
  ...openDialog(running.title),
  (ui: Ui) => ui.press(describing("the Reasoning effort picker holding medium", (name) => name.startsWith("medium"))),
  (ui: Ui) => ui.press("high"),
  (ui: Ui) => ui.daemonEvents({ type: "session_upserted", session: { ...running, ...outside } }),
];

const didNotStop: FixtureError = {
  code: "model_change_did_not_stop",
  params: { session: running.id },
  message: "the session's process did not end in time",
};

// The agent no longer lists the model the dialog sent, as can be so once its list changed behind the dialog.
const notOffered: FixtureError = {
  code: "invalid_launch_setting",
  params: { field: "model", value: "opus", agent: servedLabel("claude") },
  message: "opus is not a model that Claude Code offers",
};
// Change model confirmed on the running session, the alert about it up, and its Change model button pressed.
const openedFromAlert = [...changeToOpus, (ui: Ui) => ui.press(ui.t("sidebar.session.changeModel"))];

export const sessionModelScenarios: Scenario[] = [
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
    description: "The body names the session and says it restarts to take the new values, keeps its conversation, and that a turn in progress is cut off and continued where that is safe. Model and Reasoning effort start on what the session chose, Sonnet and medium, and Change model is greyed out until one of them differs; the note says a changed value is kept for this session from now on. A session that is not running, one with no conversation yet and one waiting for an answer and one showing its background agents (Dormant, whose restart closes that view) word the body differently in the same dialog.",
    state,
    steps: openDialog(running.title),
  },
  {
    id: "dialog-session-model-models-loading",
    group: GROUP,
    category: "normal",
    title: "Change model while the model list is out",
    description: "Both pickers are locked with the values the session chose, shown as the raw id sonnet and medium. A spinner replaces each trailing chevron, with no loading note below the Model field. Neither value carries a Not in the list badge, since nothing is known yet of what the agent lists; Change model stays greyed out.",
    state,
    agentModels: { claude: "pending" },
    steps: openDialog(running.title),
  },
  {
    id: "dialog-session-model-held-restarting",
    group: GROUP,
    category: "normal",
    title: "Change model held while the session restarts",
    description: "The amber warning says the session is starting and asks you to wait before changing its model settings. Change model is greyed out although Reasoning effort differs at high; the dialog stays open on the edit. A session switching its account holds the dialog the same way, with its own warning.",
    state,
    steps: editedThenChangedOutside({ status: "starting" }),
  },
  {
    id: "dialog-session-model-from-alert",
    group: GROUP,
    category: "normal",
    title: "Change model opened from an error alert's button",
    description: "The Change model button of the \"Could not change the model\" alert was pressed: the alert has gone and the Change model dialog stands over the same session, back on the model and effort it has, Sonnet and medium, with Change model greyed out until one differs, not on the Opus that was refused.",
    state,
    sessionModelChanges: { [running.id]: notOffered },
    steps: openedFromAlert,
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
    steps: [(ui) => ui.hoverSessionAgent(ui.session(chosen.title), providerLabel(ui.t, FIXTURE_PROVIDERS, "claude"), { account: ui.t("settings.accounts.defaultName"), model: "Opus", effort: "high" })],
  },
  {
    id: "dialog-session-model-models-unavailable",
    group: GROUP,
    category: "exceptional",
    title: "Change model of a session whose agent cannot list its models",
    description: "The daemon found no binary for Claude Code, so a warning mark named \"Could not load the models\" stands after the \"Model\" label, and the pickers hold the session's own sonnet and medium, shown as raw values, each plain with no Not in the list badge, since the list is unknown and the mark already says so; Change model stays greyed out.",
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
    id: "dialog-session-model-session-archived",
    group: GROUP,
    category: "exceptional",
    title: "Change model of a session archived while the dialog is open",
    description: "Another window archived the session after the dialog opened, so the dialog has turned into a failure modal titled \"Cannot change the model\": it names the session in an inline chip and says it was archived, so its model cannot be changed, and to reopen it to change its model and reasoning effort, with OK as its only button. A session deleted meanwhile turns the dialog into the same modal.",
    state,
    steps: [...openDialog(running.title), (ui) => ui.archiveSession(running.id)],
  },
  {
    id: "dialog-session-model-did-not-stop",
    group: GROUP,
    category: "exceptional",
    title: "Model change refused, the session did not stop",
    description: "The danger alert titled \"Could not change the model\" names the session and project on separate labelled lines, then says the session did not stop in time and may still be running, with the advice for a process that does not stop; the title alone says the model was not changed. The dialog has closed before the alert; the session behind it reads Done with the model and effort it had.",
    state,
    sessionModelChanges: { [running.id]: didNotStop },
    steps: changeToOpus,
  },
  {
    id: "dialog-session-model-not-offered",
    group: GROUP,
    category: "exceptional",
    title: "Model change refused, the agent does not offer the model",
    description: "The danger alert titled \"Could not change the model\" names the session and project on separate labelled lines, then says opus is not a model that Claude Code offers, with the advice to use Change model, shown as a chip, to pick one it does offer, in place of the launch settings of a new session. A secondary Change model button stands before OK. The dialog has closed before the alert; the session behind it reads Done with the model and effort it had. A relaunched session that ended at once is refused in the same alert, with the same button.",
    state,
    sessionModelChanges: { [running.id]: notOffered },
    steps: changeToOpus,
  },
];
