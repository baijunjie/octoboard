import { accountOf, queuedMessage, SAMPLE, snapshotState } from "../../fixtures/builders";
import { FIXTURE_AGENTS } from "../../fixtures/fixtureProviders";
import { SAMPLE_FILES, sourceUnavailable } from "../../fixtures/sampleFiles";
import { IMAGES } from "../../fixtures/viewerImages";
import type { Ui } from "../../fixtures/interact";
import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import type { QueuedMessage, Session } from "../../protocol";
import type { Scenario } from "../scenario";

const GROUP = "Terminal";

const { console: console_, web, api } = SAMPLE;
/** An account whose last sharing change did not finish and none runs now, so resuming a session on it asks first. */
const UNFINISHED_ACCOUNT = accountOf("a-client", "claude", "Client", "/Users/dev/.claude-client", { share_default_config_incomplete: true });
const IDLE_TITLE = "Update the dependencies";

/** The sample's sessions, the Website's idle one changed by `extra`. */
function sessionsWith(extra: Partial<Session> = {}): Session[] {
  return SAMPLE.sessions.map((session) => (session.id === "s-web-2" ? { ...session, ...extra } : session));
}

function stateWith(queue: QueuedMessage[], extra: Partial<Session> = {}) {
  return snapshotState({ consoles: [console_], projects: [web, api], sessions: sessionsWith(extra), messageQueues: { "s-web-2": queue } });
}

const UPGRADE_NOTES = "/Users/dev/.octoboard/composer/s-web-2/attachments/upgrade-notes.md";
const PASTED_IMAGE = "/Users/dev/.octoboard/composer/s-web-2/paste-1.png";

const QUEUE: QueuedMessage[] = [
  queuedMessage({ id: "q-1", text: "Once the lockfile is regenerated, run the whole test suite and list anything that fails with the error it gives." }),
  queuedMessage({ id: "q-2", text: "Then open a draft pull request with a summary of the version bumps.", mode: "manual", files: [UPGRADE_NOTES], images: [PASTED_IMAGE] }),
  queuedMessage({ id: "q-3", text: "/compact keep the upgrade plan", command: true }),
];

const preferences = { sessionComposer: true, panelStacks: { project: { open: [], shares: {} } }, sidebarConsole: console_.id, sidebarFocus: `project:${web.id}` } satisfies Scenario["preferences"];
const openIdle = (ui: Ui) => ui.press(ui.session(IDLE_TITLE));
const base = { group: GROUP, category: "normal", preferences } satisfies Partial<Scenario>;

export const composerScenarios: Scenario[] = [
  {
    ...base,
    id: "composer-empty",
    title: "Message composer, nothing queued",
    description: "With the message composer switched on in Settings, an idle session's terminal has the composer below it: an empty three-line editor with its placeholder, the paperclip that attaches files, the When to send selector on Send now, and Send, disabled while there is nothing to send. Hover Send for its ⌘ Enter (Ctrl Enter elsewhere) key caps. The terminal above still takes typing.",
    state: stateWith([]),
    steps: [openIdle],
  },
  {
    ...base,
    id: "composer-queue",
    title: "Message composer with a queue and a draft",
    description: "The session sends new messages once its agent is idle, so the selector reads When idle and the button Add to queue. Above the editor, the queue in its order: a message waiting for the agent to be idle, a manual one carrying a file and an image (the paperclip and 2 files), and a slash command shown monospace with its Command badge. Each row has its drag handle, Send now, Withdraw and a menu (Edit, the two modes, Move earlier and Move later). The editor holds a half-written message, which stays when you switch to another session and back.",
    state: stateWith(QUEUE, { send_mode: "when_idle" }),
    steps: [openIdle, (ui) => ui.focus(ui.t("composer.label")), (ui) => ui.type("Also check whether the lint rules changed between the two versions")],
  },
  {
    ...base,
    id: "composer-editing",
    title: "Message composer editing a queued message",
    description: "Edit from a queued message's menu takes its text and its attachments into the editor: the row is tinted and marked Editing, each attachment shows as a chip with its remove button (the image marked Image), the field is named Queued message, the selector gives way to Cancel, and the button reads Save. Pressing a chip opens the attachment in the file viewer (upgrade-notes.md as a rendered Markdown document, the image as a picture), and closing the viewer returns focus to the chip. Cancel brings back what was being written before.",
    state: stateWith([QUEUE[1]!]),
    // What the daemon's host holds at the attachments' paths, which the viewer reads as it reads a terminal's path link.
    pathLinks: {
      [UPGRADE_NOTES]: { content: { text: "# Upgrade notes\n\n- Regenerate the lockfile first.\n- Run the whole test suite afterwards and list what fails.\n" } },
      [PASTED_IMAGE]: { content: { image: { mediaType: "image/png", data: IMAGES.before_png.data } } },
    },
    steps: [openIdle, (ui) => ui.press(ui.t("queue.actions")), (ui) => ui.pressMenuItem(ui.t("queue.edit"))],
  },
  {
    ...base,
    id: "composer-no-process",
    title: "Message composer for a session with no process",
    description: "An interrupted session keeps its composer: a note above the editor says the session is not running, that queued messages wait until it is resumed and that sending now resumes it. Its queued message waits; the terminal's own \"Not running\" bar offers Resume.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api],
      sessions: sessionsWith({ status: "interrupted", account_id: UNFINISHED_ACCOUNT.id, config_dir: UNFINISHED_ACCOUNT.config_dir }),
      messageQueues: { "s-web-2": [QUEUE[0]!] },
      settings: { ...DEFAULT_SETTINGS, added_agents: [...FIXTURE_AGENTS], accounts: [UNFINISHED_ACCOUNT] },
    }),
    // Selecting an interrupted session resumes it, so the session is on an account whose last sharing change did not
    // finish: that asks first, and closing the question leaves the session not running.
    steps: [openIdle, (ui) => ui.press(ui.t("common.close")), (ui) => ui.closeDrawer()],
  },
  {
    ...base,
    id: "composer-waiting-user",
    title: "Message composer while the agent waits on the user",
    description: "The agent is waiting for an answer (a permission prompt or a question): the note above the editor says it waits for the answer in the terminal, which is where to give it; a message can still be written and queued meanwhile.",
    state: stateWith([], { status: "waiting_user" }),
    steps: [openIdle],
  },
  {
    ...base,
    id: "composer-add-context",
    title: "Message composer with a file added as context",
    description: "Add as context, the first entry of a file row's menu in the Files panel (a folder's comes after Find in folder), appended main.ts's path, relative to the project's root, to the half-written message, on a line of its own at the end of the text. The menu is closed and the composer is where the path went; with the composer off the same entry writes the path into the terminal's input instead.",
    state: stateWith([]),
    steps: [
      openIdle,
      (ui) => ui.focus(ui.t("composer.label")),
      (ui) => ui.type("Check the server setup against"),
      (ui) => ui.showPanel("files"),
      (ui) => ui.press("src"),
      (ui) => ui.contextMenu("main.ts"),
      (ui) => ui.pressMenuItem(ui.t("browser.addAsContext")),
    ],
  },
  {
    ...base,
    id: "composer-quote",
    title: "Message composer with the terminal's output quoted",
    description: "Quote, chosen in the terminal's menu with output selected, wrote the selection into the composer as a block quote, each line behind a > mark, instead of into the terminal's input. The editor keeps its height and scrolls to hold it. Quote in the file viewer's text menu does the same.",
    state: stateWith([]),
    steps: [
      openIdle,
      (ui) => ui.terminalContextMenu(),
      (ui) => ui.press(ui.t("terminal.selectAll")),
      (ui) => ui.terminalContextMenu(),
      (ui) => ui.pressMenuItem(ui.t("common.quote")),
    ],
  },
  {
    ...base,
    id: "composer-failures",
    category: "exceptional",
    title: "Queued messages being sent, not delivered and refused",
    description: "A message being handed over reads Sending with a spinner, and its Send now is disabled. One that delivery reported not taken came back as manual with a Not delivered badge, and one whose send now was refused keeps its mode with a Refused badge; each badge has a mark whose popover says why in the user's language, the daemon's own text under it.",
    state: stateWith([
      queuedMessage({ id: "q-sending", text: "Run the migration on the staging database.", state: "sending" }),
      queuedMessage({ id: "q-back", text: "Summarise what changed in the API since yesterday.", mode: "manual", failure: { kind: "not_delivered", code: "message_not_taken", params: { agent: "claude" }, message: "Claude Code did not take a message from Octoboard that was written while its turn ran" } }),
      queuedMessage({ id: "q-refused", text: "Ship it.", failure: { kind: "refused", code: "session_locked", params: { session: "s-web-2" }, message: "the session is locked" } }),
    ]),
    steps: [openIdle],
  },
  {
    ...base,
    id: "composer-mention",
    title: "Message composer's @ picker",
    description: "Typing @ in the composer opens a list above the editor: under Files, the entries of the session's project directory, folders first; under Sessions, every other unarchived session, those of the same project first, then those of the same console, then those of other consoles, each with where it is. The first option is highlighted; the arrow keys move the highlight, Enter or Tab picks it and Escape closes the list, while focus stays in the editor. A picked file inserts its path relative to the project's root, a picked session @「title」(session id).",
    state: stateWith([]),
    steps: [openIdle, (ui) => ui.focus(ui.t("composer.label")), (ui) => ui.type("Compare the server setup with @"), (ui) => ui.wait(300)],
  },
  {
    ...base,
    id: "composer-mention-filtered",
    title: "Message composer's @ picker narrowed by what follows",
    description: "What follows the @ narrows the list: the files whose name contains it, found anywhere in the project, with the folders above them whose name contains it, and the sessions whose title contains it. A query with a / lists the entries of the folder it names instead (try @src/).",
    state: stateWith([]),
    steps: [openIdle, (ui) => ui.focus(ui.t("composer.label")), (ui) => ui.type("Ask about @se"), (ui) => ui.wait(300)],
  },
  {
    ...base,
    id: "composer-mention-files-loading",
    title: "Message composer's @ picker while the files are being looked for",
    description: "The project's files have not been listed yet: the Sessions list is there already, and in the Files' place a spinner stands before \"Searching project files\" on a line of its own.",
    state: stateWith([]),
    // The project's root listing is never answered, so the picker keeps waiting for it.
    filesByProject: { [web.id]: { ...SAMPLE_FILES, "": { dir: true, pending: true } } },
    steps: [openIdle, (ui) => ui.focus(ui.t("composer.label")), (ui) => ui.type("Compare the server setup with @"), (ui) => ui.wait(300)],
  },
  {
    ...base,
    id: "composer-mention-no-match",
    title: "Message composer's @ picker with nothing matching",
    description: "A query that names no file, folder or session: the picker is one plain line, \"No matching files\", with no glyph before it and no option to highlight.",
    state: stateWith([]),
    steps: [openIdle, (ui) => ui.focus(ui.t("composer.label")), (ui) => ui.type("Ask about @zzz"), (ui) => ui.wait(300)],
  },
  {
    ...base,
    id: "composer-mention-files-failed",
    category: "exceptional",
    title: "Message composer's @ picker when the file search failed",
    description: "The project-wide file search failed: a red error glyph stands before \"Could not search project files\" in the Files' place, above the Sessions that still match what follows the @.",
    state: stateWith([]),
    searchError: sourceUnavailable("", "missing", "No such file or directory (os error 2)"),
    steps: [openIdle, (ui) => ui.focus(ui.t("composer.label")), (ui) => ui.type("Ask about @hub"), (ui) => ui.wait(300)],
  },
  {
    ...base,
    id: "queue-line",
    title: "Queued messages with the composer off",
    description: "With the composer off in this window, a session that has messages queued (from another window, say) shows a compact line over the top of its terminal saying how many. View lists them, each with Send now and Withdraw only.",
    preferences: { ...preferences, sessionComposer: false },
    state: stateWith(QUEUE.slice(0, 2)),
    steps: [openIdle, (ui) => ui.press(ui.t("queue.view.label", { count: 2 }))],
  },
];
