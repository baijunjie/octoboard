import type { RemoteDaemonEntry } from "../../platform";
import { SAMPLE, consoleOf, snapshotState } from "../../fixtures/builders";
import type { Ui } from "../../fixtures/interact";
import { scheduledTaskOf } from "../../fixtures/scheduledTaskBuilders";
import type { Scenario } from "../scenario";

const RAIL_GROUP = "Window layout";
const GROUP = "Remote devices";

const { console: console_, web, api, sessions } = SAMPLE;

/** The rail has the consoles above its Remote devices button; one console is enough to see the button under them. */
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });

/** Three scheduled tasks, Morning report off, so the rail's badge counts the other two. */
const enabledTasks = {
  tasks: [
    scheduledTaskOf("t-deps", console_.id, { project_id: web.id, launch: { title: "Weekly dependency review", compatibility: {} }, schedule: { kind: "weekdays", time: { hour: 9, minute: 30 } } }),
    scheduledTaskOf("t-report", console_.id, { launch: { title: "Morning report", compatibility: {} }, enabled: false }),
    scheduledTaskOf("t-index", console_.id, { project_id: api.id, launch: { title: "Check the search index health", compatibility: {} }, schedule: { kind: "hourly", minute: 15 } }),
  ],
};

/** No console at all, which the Remote devices page is still reachable from. */
const noConsoleState = snapshotState({ consoles: [], projects: [], sessions: [] });

/** Enough consoles (about 40px each) that the rail's strip of them scrolls up to a window about 1300px tall, so the
 * Remote devices button meets the bottom group, which is where the groove under the button is drawn. */
const CROWD_NAMES = ["Main", "Website", "Search API", "Billing", "Docs", "Mobile app", "Data pipeline", "Infrastructure", "Design system", "Analytics", "Support tools", "Release train", "Internal wiki", "Prototypes", ...Array.from({ length: 10 }, (_, i) => `Archive ${i + 1}`)];
const crowdedState = snapshotState({
  consoles: CROWD_NAMES.map((name, i) => (i === 0 ? console_ : consoleOf(`c-crowd-${i}`, name))),
  projects: [web, api],
  sessions,
});

const entryOf = (id: string, name: string, host: string, auth: RemoteDaemonEntry["auth"], overrides: Partial<RemoteDaemonEntry> = {}): RemoteDaemonEntry => ({
  id,
  name,
  host,
  port: 22,
  user: "deploy",
  auth,
  keyPath: auth === "key" ? "~/.ssh/id_ed25519" : null,
  fingerprint: null,
  octoboarddPath: null,
  ...overrides,
});

export const staging = entryOf("staging", "Staging", "staging.example.com", "device", { fingerprint: "SHA256:Qm9vcHNjb25zdGFudGZpbmdlcnByaW50Zm9yZ2FsbGVyeQ" });
export const production = entryOf("production", "Production", "prod.example.com", "device", { fingerprint: "SHA256:aGFsbGVyeWZpbmdlcnByaW50Zm9ycHJvZHVjdGlvbg" });
export const buildServer = entryOf("build", "Build server", "10.0.4.17", "password", { port: 2222, user: "ci" });
export const homeLab = entryOf("home-lab", "Home lab", "lab.local", "key");
export const relay = entryOf("relay", "Edge relay in the Frankfurt data centre", "relay-fra-02.edge.example.net", "key", { user: "octoboard" });

/** What the Remote devices page lists: every state a saved daemon can be in. */
const SAVED = [staging, production, buildServer, homeLab, relay];
const CONNECTED = [
  { id: staging.id, state: "connected", via: "window" },
  { id: production.id, state: "connected", via: "browser" },
  { id: buildServer.id, state: "reconnecting", via: "window" },
] as const;

/** The line `octoboardd connection` prints on a server, as it is copied from there. */
const CONNECTION_LINE = "octoboard+ssh://deploy@staging.example.com:22?fingerprint=SHA256:Qm9vcHNjb25zdGFudGZpbmdlcnByaW50Zm9yZ2FsbGVyeQ&name=Staging";

/** Presses the rail's Remote devices button, which shows the page; its name counts the connections held, when there are any. */
const showPage = (ui: Ui) => ui.press((name) => name.startsWith(ui.t("remoteDevices.label")));

/** The Connect button in the header of the selected device's page, as it names itself. */
const connectOf = (ui: Ui, entry: RemoteDaemonEntry) => ui.t("remoteDaemons.connectLabel", { name: entry.name });

/** Selects a device in the sidebar by pressing its row, which the page shows the form of; the first one saved is
 * selected already. */
const selectDevice = (ui: Ui, entry: RemoteDaemonEntry) => ui.press(entry.name);

export const remoteDaemonsScenarios: Scenario[] = [
  {
    id: "rail-remote-devices",
    group: RAIL_GROUP,
    category: "normal",
    title: "Rail with the Scheduled tasks and Remote devices buttons, both counting",
    description:
      "Under the consoles and a groove, the rail's two page buttons: Scheduled tasks above and the Remote devices button (the Server glyph) of the main window, each with a green badge at its top right corner. Scheduled tasks counts the tasks that are enabled, 2 here (Weekly dependency review and Check the search index health; Morning report is off and does not count), and its tooltip and name say so. Remote devices counts the connections held: 3 here (Staging in a window, Production for the browser, Build server reconnecting). Pressing Remote devices only opens the Remote devices page (in the main area, in place of the console's sessions), which the scenarios below draw.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: SAVED, status: [...CONNECTED] },
    scheduledTasks: enabledTasks,
  },

  {
    id: "rail-remote-devices-none-held",
    group: RAIL_GROUP,
    category: "normal",
    title: "Rail with the Remote devices button, nothing connected",
    description: "Remote daemons are saved but none is connected: the button is there all the same, under the Scheduled tasks button, the consoles and a groove, with no badge, and the Scheduled tasks button has none either, since no task is enabled.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: SAVED },
  },

  {
    id: "rail-remote-devices-crowded",
    group: RAIL_GROUP,
    category: "normal",
    title: "Rail with the Remote devices button, consoles filling the rail",
    description:
      "So many consoles that the rail's list of them scrolls, fading at its edge, and the two page buttons (Scheduled tasks and Remote devices) meet the group at the bottom (the waiting count, notifications, usage and settings). A groove is drawn above the buttons, and another between them and the bottom group, with equal spacing around both; nothing overlaps. Making the frame taller until the consoles fit leaves free space under the button, and the lower groove disappears.",
    state: crowdedState,
    desktopApp: true,
    remoteDaemons: { entries: SAVED, status: [...CONNECTED] },
  },

  {
    id: "remote-devices-list",
    group: GROUP,
    category: "normal",
    title: "Remote devices page with saved remote daemons",
    description:
      "The Remote devices page, a page of the main window like a console's sessions: the rail's button is drawn as the current item, a filled circle (a console button shows the current one the same way, behind its avatar), and the top bar's breadcrumb reads Remote devices, then the selected device. The sidebar lists the saved remote daemons as session-style rows: the Server glyph, the name and, where one is connected, a status glyph with a tooltip (a green chain link for Connected, a green browser window for Opened in browser, and a grey spinner for Reconnecting); the long-named relay is cut off at the row's end. The first, Staging, is selected, with its settings form in the content area: the header with its name, badge, address in monospace and Open (it is connected), the form in three cards, Connection (details, name, then user, host and SSH port on one row), Sign-in (this computer's key as its authentication) and Advanced (the path of octoboardd, the pinned host key with Clear), then Save and Revert, which stay disabled until something is changed, and a Remove card at the bottom. Choose a narrow width to see the sidebar become a drawer that the top bar's toggle opens.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: SAVED, status: [...CONNECTED] },
    steps: [showPage],
  },

  {
    id: "remote-devices-status-tooltip",
    group: GROUP,
    category: "normal",
    title: "Remote device connection status tooltip",
    description: "The pointer rests on Production's green browser glyph, showing its Opened in browser tooltip. The selected device's page header still shows its connection as a text badge.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: SAVED, status: [...CONNECTED] },
    steps: [showPage, (ui) => ui.hoverRemoteDeviceStatus((name) => name.startsWith(production.name), ui.t("remoteDaemons.state.browser"))],
  },

  {
    id: "remote-devices-unsaved",
    group: GROUP,
    category: "normal",
    title: "Remote device with unsaved changes",
    description:
      "The long-named relay, which is not connected, selected, with its SSH port changed in the form: the header offers Connect (choose a narrow width to see its long name and host wrap), the selected row stays in the sidebar, and Save and Revert, under the cards and above the Remove card, are enabled. Revert drops the change without asking; selecting another device, New device or any other way of leaving the page asks first, in the same Discard changes confirmation that Scheduled tasks shows.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: SAVED, status: [...CONNECTED] },
    steps: [showPage, (ui) => selectDevice(ui, relay), (ui) => ui.focus(ui.t("remoteDaemons.form.port")), (ui) => ui.type("2200")],
  },

  {
    id: "remote-devices-no-console",
    group: GROUP,
    category: "normal",
    title: "Remote devices page with no console",
    description:
      "Nothing but the Remote devices page to show, as before a first console exists: it still has its device sidebar listing the saved remote daemons, with the first one selected and its form in the content area, as it does beside consoles.",
    state: noConsoleState,
    desktopApp: true,
    remoteDaemons: { entries: SAVED, status: [...CONNECTED] },
    steps: [showPage],
  },

  {
    id: "remote-devices-row-menu",
    group: GROUP,
    category: "normal",
    title: "Remote device row's menu",
    description: "The menu a device row opens (by its button or a right-click), here on Home lab, which is not connected: Connect, then Remove in the destructive style under a separator. A connected device offers Open in place of Connect.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: SAVED, status: [...CONNECTED] },
    steps: [showPage, (ui) => ui.contextMenu(homeLab.name)],
  },

  {
    id: "remote-devices-empty",
    group: GROUP,
    category: "normal",
    title: "Remote devices page with no remote daemons",
    description:
      "With nothing saved the content area shows the empty state alone, centred in it: the glyph, No remote daemons yet, the line saying what a remote daemon is, and Add remote daemon as the panel's action. The sidebar keeps its header with the add button and holds one compact empty row in the list's place, whose press starts Add remote daemon too. The breadcrumb has no device after Remote devices.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: [] },
    steps: [showPage],
  },

  {
    id: "remote-devices-add-blank",
    group: GROUP,
    category: "normal",
    title: "Add remote daemon form, blank",
    description:
      "The blank form Add remote daemon opens in the content area, titled Add remote daemon, with none of the saved devices selected in the sidebar: the Connection details field has focus, with the icon-only Paste button as its trailing control (its tooltip reads Paste), then the cards for the name, address and sign-in, Save and Cancel and no Remove card. Where the platform cannot read the clipboard the button is not drawn. A clipboard that cannot be read opens an error alert titled Could not paste the connection details, laid out as any other error alert.",
    state,
    desktopApp: true,
    clipboard: { text: CONNECTION_LINE },
    remoteDaemons: { entries: [buildServer, homeLab] },
    steps: [showPage, (ui) => ui.press(ui.t("remoteDaemons.add"))],
  },

  {
    id: "remote-devices-add",
    group: GROUP,
    category: "normal",
    title: "Add remote daemon form after Paste",
    description:
      "Paste was pressed on the blank form with the line `octoboardd connection` prints on the clipboard: it fills Connection details and, from it, the name, user, host and port, and the host key to pin is kept for the first connection (shown as pinned, with Clear). The button's brief Pasted state (a check, and the tooltip reading Pasted) is over by the time the view settles.",
    state,
    desktopApp: true,
    clipboard: { text: CONNECTION_LINE },
    remoteDaemons: { entries: [buildServer, homeLab] },
    steps: [showPage, (ui) => ui.press(ui.t("remoteDaemons.add")), (ui) => ui.press(ui.t("common.paste"))],
  },

  {
    id: "remote-devices-connecting",
    group: GROUP,
    category: "normal",
    title: "Remote devices page while a connection is being made",
    description:
      "Connect was pressed on Staging and the connection has not ended: the Connect button in the page's header shows its spinner. Selecting another device shows its Connect disabled until the connection ends, so a second one cannot be started; Remove stays available.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: [staging, homeLab, buildServer] },
    steps: [showPage, (ui) => ui.press(connectOf(ui, staging))],
  },

  {
    id: "remote-devices-password",
    group: GROUP,
    category: "normal",
    title: "Remote devices page asking for the password",
    description: "Connect on an entry that signs in with a password asks for it first, in a dialog over the page: a sentence saying who is being signed in to (the entry's name and the user as chips), then the Password field with its placeholder; the password is used for this connection only and is not stored.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: [buildServer, homeLab] },
    steps: [showPage, (ui) => ui.press(connectOf(ui, buildServer))],
  },

  {
    id: "remote-devices-fingerprint",
    group: GROUP,
    category: "normal",
    title: "Remote devices page confirming a host key",
    description: "The first connection to a host whose key is not pinned: the dialog's opening sentence names the entry and carries its fingerprint as a monospace chip, then says to compare it with the server's, with the command to run as a chip, before connecting.",
    state,
    desktopApp: true,
    remoteDaemons: {
      entries: [staging],
      outcomes: { staging: { outcome: "host_key_unknown", fingerprint: "SHA256:Qm9vcHNjb25zdGFudGZpbmdlcnByaW50Zm9yZ2FsbGVyeQ" } },
    },
    steps: [showPage, (ui) => ui.press(connectOf(ui, staging))],
  },

  {
    id: "remote-devices-remove",
    group: GROUP,
    category: "normal",
    title: "Remove remote daemon",
    description: "Remove at the bottom of Home lab's page asks first, in a destructive confirmation naming the entry over the page; nothing else is touched: the daemon itself and any window or browser tab open on it are not affected.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: [staging, homeLab] },
    steps: [showPage, (ui) => ui.press(homeLab.name), (ui) => ui.press(ui.t("remoteDaemons.removeLabel", { name: homeLab.name }))],
  },

  {
    id: "remote-devices-start-daemon",
    group: GROUP,
    category: "normal",
    title: "Remote devices page offering to start the daemon",
    description: "The server answers but no daemon of Octoboard is running on it: a confirmation titled Start the daemon names the entry and offers Start daemon, which starts it on the server and connects.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: [staging], outcomes: { staging: { outcome: "no_daemon" } } },
    steps: [showPage, (ui) => ui.press(connectOf(ui, staging))],
  },

  {
    id: "remote-devices-locate-daemon",
    group: GROUP,
    category: "normal",
    title: "Remote devices page asking where octoboardd is",
    description: "octoboardd was not found where the shell looks on the server: a dialog lists the places it looked in, as plain monospace text, not chips, and asks for the path in a field with an example as its placeholder, with Save and connect.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: [staging], outcomes: { staging: { outcome: "octoboardd_not_found", tried: ["/usr/local/bin/octoboardd", "/home/deploy/.octoboard/bin/octoboardd", "/opt/octoboard/bin/octoboardd"] } } },
    steps: [showPage, (ui) => ui.press(connectOf(ui, staging))],
  },

  {
    id: "remote-devices-offline",
    group: GROUP,
    category: "exceptional",
    title: "Remote devices page while the local daemon is unavailable",
    description:
      "The rail's Remote devices button, the only one in the rail while the local daemon is not answering, opens the full page: the device sidebar and the selected device's form, with the connection banner along the bottom. Back returns to the status screen.",
    desktopApp: true,
    remoteDaemons: { entries: SAVED, status: [...CONNECTED] },
    state: { connectionState: "closed" },
    steps: [showPage],
  },

  {
    id: "remote-devices-invalid",
    group: GROUP,
    category: "exceptional",
    title: "Remote device form with invalid fields",
    description:
      "Home lab's form after its SSH port was set out of range and its key file path emptied, and Save was pressed: every invalid field shows its own error under it and focus is on the first of them, the SSH port. Save stays enabled, since it is the press that reports what is wrong; Revert drops the changes. A field that is only edited shows its error when it is left.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: [staging, homeLab] },
    steps: [
      showPage,
      (ui) => selectDevice(ui, homeLab),
      (ui) => ui.focus(ui.t("remoteDaemons.form.port")),
      (ui) => ui.type("99999"),
      (ui) => ui.focus(ui.t("remoteDaemons.form.keyPath")),
      (ui) => ui.type(""),
      (ui) => ui.press(ui.t("common.save")),
    ],
  },

  {
    id: "remote-devices-failure",
    group: GROUP,
    category: "exceptional",
    title: "Remote devices page after a failed connection",
    description: "A connection that fails for a known reason ends in an alert titled Could not reach the server, with the server's address as a chip in its sentence and the shell's own words behind Error details; the list stays as it was. There is no Report issue: this is not an unexpected failure.",
    state,
    desktopApp: true,
    remoteDaemons: {
      entries: [staging],
      outcomes: { staging: { outcome: "unreachable", detail: "ssh: connect to host staging.example.com port 22: Connection timed out" } },
    },
    steps: [showPage, (ui) => ui.press(connectOf(ui, staging))],
  },

  {
    id: "remote-devices-version-mismatch",
    group: GROUP,
    category: "exceptional",
    title: "Remote devices page with a daemon of another version",
    description:
      "The daemon on the server is older than this application: the dialog names both versions, says the daemon has to be updated on the server itself, and offers Open in browser (the primary action), which has the shell open the daemon's own web interface in the system browser, besides Cancel. Nothing here updates the daemon. A daemon that is newer than the application has a different footer, in its own scenario.",
    state,
    desktopApp: true,
    remoteDaemons: {
      entries: [staging],
      outcomes: { staging: { outcome: "generation_mismatch", remoteGeneration: 1, localGeneration: 2, remoteVersion: "1.4.0", localVersion: "2.0.0" } },
    },
    steps: [showPage, (ui) => ui.press(connectOf(ui, staging))],
  },

  {
    id: "remote-devices-version-mismatch-newer",
    group: GROUP,
    category: "exceptional",
    title: "Remote devices page with a daemon newer than the application",
    description:
      "The daemon on the server is newer than this application: the dialog names both versions and says they cannot work together until this app is updated, then that the daemon's own web interface can be opened in the browser. Its footer holds Go to Updates and Open in browser (the primary one) and no Cancel; the close button in the header is the way out. Go to Updates is the shell's to answer, which this gallery does not draw, so nothing visible follows.",
    state,
    desktopApp: true,
    remoteDaemons: {
      entries: [staging],
      outcomes: { staging: { outcome: "generation_mismatch", remoteGeneration: 3, localGeneration: 2, remoteVersion: "2.1.0", localVersion: "2.0.0" } },
    },
    steps: [showPage, (ui) => ui.press(connectOf(ui, staging))],
  },

  {
    id: "remote-devices-application-not-running",
    group: GROUP,
    category: "exceptional",
    title: "Remote devices page, the application is not running on the server",
    description:
      "The octoboardd on the server is the desktop application's, whose daemon runs only while the application does, and the application is closed there: a confirmation titled The app is not running on the server says so in a sentence naming the entry, tells to open the app on that computer, and offers Try again besides Cancel. The shell's own words are in a block behind an Error details disclosure, open here.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: [staging], outcomes: { staging: { outcome: "application_not_running", detail: "octoboardd: the owner endpoint /Users/dev/.octoboard/browser.sock does not exist" } } },
    steps: [showPage, (ui) => ui.press(connectOf(ui, staging)), (ui) => ui.press(ui.t("error.diagnostics"))],
  },

  {
    id: "remote-devices-failure-unexpected",
    group: GROUP,
    category: "exceptional",
    title: "Remote devices page after a connection failed unexpectedly",
    description:
      "The shell ended the connection with a failure the application has no wording for: the alert is the unexpected-failure one, titled Could not connect to the remote daemon, saying only that something unexpected went wrong with the Report issue guidance, and the shell's own words behind Error details, closed here. Report issue sits before OK in the footer.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: [staging], outcomes: { staging: { outcome: "failed", detail: "ssh: unexpected packet type 97 in the key exchange" } } },
    steps: [showPage, (ui) => ui.press(connectOf(ui, staging))],
  },

  {
    id: "remote-devices-host-key-changed",
    group: GROUP,
    category: "exceptional",
    title: "Remote devices page after a changed host key",
    description:
      "The server presented a different host key than the one pinned, so the connection was refused: an alert with a two-sentence body, the second telling to edit the entry and clear the pinned key, and its Error details open on the pinned and the presented fingerprint, one to a line. Unlike a plain failure it has a remedy in the entry's own form.",
    state,
    desktopApp: true,
    remoteDaemons: {
      entries: [staging],
      outcomes: { staging: { outcome: "host_key_mismatch", expected: "SHA256:Qm9vcHNjb25zdGFudGZpbmdlcnByaW50Zm9yZ2FsbGVyeQ", actual: "SHA256:b3RoZXJob3N0a2V5ZmluZ2VycHJpbnRvZmF0aGlyZHNlcnZlcg" } },
    },
    steps: [showPage, (ui) => ui.press(connectOf(ui, staging)), (ui) => ui.press(ui.t("error.diagnostics"))],
  },

  {
    id: "remote-device-reconnecting",
    group: GROUP,
    category: "exceptional",
    title: "Reconnecting remote device",
    description: "Build server is selected. Its page header pairs a spinner in the badge's text colour with Reconnecting in the soft status badge. The sidebar row shows a grey spinner with a status tooltip. Both spinners hold still with reduced motion.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: SAVED, status: [...CONNECTED] },
    steps: [showPage, (ui) => ui.press((name) => name.startsWith(buildServer.name))],
  },
];
