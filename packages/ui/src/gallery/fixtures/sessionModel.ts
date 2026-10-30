import { AGENT_LABEL } from "../../agents";
import type { Session } from "../../protocol";
import { describing, type Ui } from "../interact";
import type { Scenario } from "../scenario";
import { accountOf, consoleOf, minutesAgo, projectOf, sessionOf, snapshotState, switchingAccount } from "./builders";
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
// A running session nothing has been typed into yet, and one that is showing a prompt and waiting for an answer.
const fresh = sessionOf("s-model-fresh", console_.id, web.id, "Plan the next release", "idle", { started_at: minutesAgo(2), has_conversation: false, model: "sonnet", effort: "medium" });
const waiting = sessionOf("s-model-waiting", console_.id, web.id, "Approve the index rebuild", "waiting_user", { started_at: minutesAgo(20), model: "sonnet", effort: "medium" });

const state = snapshotState({ consoles: [console_], projects: [web], sessions: [running, interrupted, starting, switching] });
const menuState = {
  ...state,
  settings: { ...state.settings!, accounts: [accountOf("a-model-work", "claude", "Work", "/Users/dev/.claude-work")] },
};
const switchingState = switchingAccount(menuState, [switching.id]);

// The session chose its model and effort and its records have not named any yet, as right after a change.
const chosen = { ...running, model: "opus", effort: "high", observed_model: undefined, observed_effort: undefined };
const chosenState = snapshotState({ consoles: [console_], projects: [web], sessions: [chosen] });
const freshState = snapshotState({ consoles: [console_], projects: [web], sessions: [fresh] });
// The running session beside the fresh one, so that deleting the fresh one leaves the window with a session.
const beside = snapshotState({ consoles: [console_], projects: [web], sessions: [running, fresh] });
const waitingState = snapshotState({ consoles: [console_], projects: [web], sessions: [waiting] });

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
const didNotComeUp: FixtureError = {
  code: "model_change_did_not_come_up",
  params: { session: running.id },
  message: "the session ended as soon as it was relaunched with the new model, so it keeps the model and effort it had",
};

// The agent no longer lists the model the dialog sent, as can be so once its list changed behind the dialog.
const notOffered: FixtureError = {
  code: "invalid_launch_setting",
  params: { field: "model", value: "opus", agent: AGENT_LABEL.claude },
  message: "opus is not a model that Claude Code offers",
};
// Change model confirmed on the running session, the alert about it up, and its Change model button pressed.
const openedFromAlert = [...changeToOpus, (ui: Ui) => ui.press(ui.t("sidebar.session.changeModel"))];

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
    description: "Both pickers are locked with the values the session chose, shown as the raw id sonnet and medium, and a note under the Model field says the models are loading. Neither value carries a Not in the list badge, since nothing is known yet of what the agent lists; Change model stays greyed out.",
    state,
    agentModels: { claude: "pending" },
    steps: openDialog(running.title),
  },
  {
    id: "dialog-session-model-fresh-conversation",
    group: GROUP,
    category: "normal",
    title: "Change the model of a running session with no conversation yet",
    description: "The body says the session restarts to take the values with a fresh conversation, as nothing has been typed into it yet, in place of the sentence about keeping its conversation. Nothing is said of a prompt.",
    state: freshState,
    steps: openDialog(fresh.title),
  },
  {
    id: "dialog-session-model-waiting-prompt",
    group: GROUP,
    category: "normal",
    title: "Change the model of a session waiting for the user",
    description: "After the sentence about keeping its conversation the body adds that the session is waiting for you and restarting ends the prompt it is showing.",
    state: waitingState,
    steps: openDialog(waiting.title),
  },
  {
    id: "dialog-session-model-follows-session",
    group: GROUP,
    category: "normal",
    title: "Change model keeps an edit while the session changes outside the dialog",
    description: "Reasoning effort was moved to high in the dialog, then another window changed the session to Opus: Model has followed to Opus while Reasoning effort stays on the edited high, and Change model is enabled.",
    state,
    steps: editedThenChangedOutside({ model: "opus", observed_model: undefined, observed_effort: undefined }),
  },
  {
    id: "dialog-session-model-held-restarting",
    group: GROUP,
    category: "normal",
    title: "Change model held while the session restarts",
    description: "The amber warning says the session is starting and asks you to wait before changing its model settings. Change model is greyed out although Reasoning effort differs at high; the dialog stays open on the edit.",
    state,
    steps: editedThenChangedOutside({ status: "starting" }),
  },
  {
    id: "dialog-session-model-held-switching-account",
    group: GROUP,
    category: "normal",
    title: "Change model held while the session switches accounts",
    description: "The amber warning says the session is switching accounts and asks you to wait before changing its model settings. Change model is greyed out although Reasoning effort differs at high; the dialog stays open on the edit.",
    state,
    steps: editedThenChangedOutside({ switching_account: true }),
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
    steps: [(ui) => ui.hoverSessionAgent(ui.session(chosen.title), AGENT_LABEL.claude, { account: ui.t("settings.accounts.defaultName"), model: "Opus", effort: "high" })],
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
    description: "Another window archived the session after the dialog opened, so the dialog has turned into a failure modal titled \"Cannot change the model\": it names the session in an inline chip and says it was archived, so its model cannot be changed, and to reopen it to change its model and reasoning effort, with OK as its only button.",
    state,
    steps: [...openDialog(running.title), (ui) => ui.archiveSession(running.id)],
  },
  {
    id: "dialog-session-model-session-deleted",
    group: GROUP,
    category: "exceptional",
    title: "Change model of a session deleted while the dialog is open",
    description: "The session was never typed into and another window deleted it after the dialog opened, so the dialog has turned into a failure modal titled \"Cannot change the model\": it names the session in an inline chip and says it no longer exists, so its model cannot be changed, with OK as its only button. The chip reads Plan the release checklist, the name the session had just before it was deleted, rather than the name it had when the dialog opened.",
    state: beside,
    steps: [
      ...openDialog(fresh.title),
      (ui) => ui.daemonEvents({ type: "session_upserted", session: { ...fresh, title: "Plan the release checklist" } }),
      (ui) => ui.daemonEvents({ type: "session_deleted", session: fresh.id }),
    ],
  },
  {
    id: "dialog-session-model-did-not-stop",
    group: GROUP,
    category: "exceptional",
    title: "Model change refused, the session did not stop",
    description: "The danger alert titled \"Could not change the model\" names the session and project on separate labelled lines, then says the session did not stop in time and may still be running, with the advice for a process that does not stop; the title alone says the model was not changed. The dialog has closed before the alert; the session behind it reads Idle with the model and effort it had.",
    state,
    sessionModelChanges: { [running.id]: didNotStop },
    steps: changeToOpus,
  },
  {
    id: "dialog-session-model-did-not-come-up",
    group: GROUP,
    category: "exceptional",
    title: "Model change refused, the relaunched session ended at once",
    description: "The danger alert titled \"Could not change the model\" names the session and project on separate labelled lines, then says the session ended as soon as it was relaunched with the new model, so it keeps the model and reasoning effort it had, with the advice that the session is no longer running and Change model, shown as a chip, picks a model and effort the agent offers, or resuming keeps the ones it had. A secondary Change model button stands before OK. The dialog has closed before the alert; the session behind it reads as not running, with Resume offered, still on the model and effort it had.",
    state,
    sessionModelChanges: { [running.id]: didNotComeUp },
    steps: changeToOpus,
  },
  {
    id: "dialog-session-model-not-offered",
    group: GROUP,
    category: "exceptional",
    title: "Model change refused, the agent does not offer the model",
    description: "The danger alert titled \"Could not change the model\" names the session and project on separate labelled lines, then says opus is not a model that Claude Code offers, with the advice to use Change model, shown as a chip, to pick one it does offer, in place of the launch settings of a new session. A secondary Change model button stands before OK. The dialog has closed before the alert; the session behind it reads Idle with the model and effort it had.",
    state,
    sessionModelChanges: { [running.id]: notOffered },
    steps: changeToOpus,
  },
];
