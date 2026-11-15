import type { RemoteDaemonEntry } from "../../platform";
import { SAMPLE, snapshotState } from "../../fixtures/builders";
import type { Ui } from "../../fixtures/interact";
import type { Scenario } from "../scenario";

const GROUP = "Window layout";

const { console: console_, web, api, sessions } = SAMPLE;

/** The rail has the consoles above its remote daemons; one console is enough to see the group under them. */
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });

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

/** What the connect page lists when it is opened for them: every state a row can be in. */
const SAVED = [staging, production, buildServer, homeLab, relay];
const CONNECTED = [
  { id: staging.id, state: "connected", via: "window" },
  { id: production.id, state: "connected", via: "browser" },
  { id: buildServer.id, state: "reconnecting", via: "window" },
] as const;

/** A row's Connect button, as the page names it. */
const connectOf = (ui: Ui, entry: RemoteDaemonEntry) => ui.t("remoteDaemons.connectLabel", { name: entry.name });

export const remoteDaemonsScenarios: Scenario[] = [
  {
    id: "rail-remote-daemons",
    group: GROUP,
    category: "normal",
    title: "Rail with the connected remote daemons",
    description:
      "Under the consoles and a groove, one round Server tile for each remote daemon that is connected, each on a fill picked from its name: Staging (a window is open) with a green check badge, Production the same because its connection is held for the browser (its tooltip says Opened in browser), and Build server with an amber refresh badge (reconnecting). Home lab and the long-named relay are saved but not connected, so they have no tile. Hover a tile for its name, address and state; right-click one for Open and Disconnect. There is no Add button: daemons are added and connected from a new remote window.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: SAVED, status: [...CONNECTED] },
  },
  {
    id: "rail-remote-daemons-none",
    group: GROUP,
    category: "normal",
    title: "Rail with no remote daemon connected",
    description: "Remote daemons are saved but none is connected: the rail has no group and no groove under the consoles, as if there were no remote daemons at all.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: SAVED },
  },
  {
    id: "connect-page-list",
    group: GROUP,
    category: "normal",
    title: "Connect page with saved remote daemons",
    description:
      "The page a new remote window opens on, with an empty title bar (it only keeps the window's controls clear and drags the window) and no daemon behind it: the saved remote daemons as rows set apart by hairline dividers, each with its name in plain text, followed where it is connected by a status badge (Connected and Opened in browser in green, Reconnecting in amber), its address under it, then Edit and Remove and, in place of Connect, Open for a connected one; Add remote daemon follows the list.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: { entries: SAVED, status: [...CONNECTED] },
  },
  {
    id: "connect-page-empty",
    group: GROUP,
    category: "normal",
    title: "Connect page with no remote daemons",
    description: "The empty list: the glyph and a heading saying there are none yet, with Add remote daemon as the action of that empty panel; the standalone Add remote daemon button below a list is not drawn.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: { entries: [] },
  },
  {
    id: "connect-page-add",
    group: GROUP,
    category: "normal",
    title: "Add remote daemon with connection details pasted",
    description:
      "The form opened by Add remote daemon, with the line `octoboardd connection` prints pasted into Connection details: it fills the name, host, port and user below it, and the host key to pin is kept for the first connection.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: { entries: [] },
    steps: [
      (ui) => ui.press(ui.t("remoteDaemons.add")),
      (ui) => ui.focus(ui.t("remoteDaemons.form.details")),
      (ui) => ui.type("octoboard+ssh://deploy@staging.example.com:22?fingerprint=SHA256:Qm9vcHNjb25zdGFudGZpbmdlcnByaW50Zm9yZ2FsbGVyeQ&name=Staging"),
    ],
  },
  {
    id: "connect-page-connecting",
    group: GROUP,
    category: "normal",
    title: "Connect page while a connection is being made",
    description:
      "Connect was pressed on Staging and the connection has not ended: its Connect button shows its spinner, and the Connect buttons of the other rows are disabled until it does, so a second connection cannot be started. Edit and Remove stay available.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: { entries: [staging, homeLab, buildServer] },
    steps: [(ui) => ui.press(connectOf(ui, staging))],
  },
  {
    id: "connect-page-password",
    group: GROUP,
    category: "normal",
    title: "Connect page asking for the password",
    description: "Connect on an entry that signs in with a password asks for it first, in a dialog of this window: a sentence saying who is being signed in to (the entry's name and the user as chips), then the Password field with its placeholder; the password is used for this connection only and is not stored.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: { entries: [buildServer, homeLab] },
    steps: [(ui) => ui.press(connectOf(ui, buildServer))],
  },
  {
    id: "connect-page-fingerprint",
    group: GROUP,
    category: "normal",
    title: "Connect page confirming a host key",
    description: "The first connection to a host whose key is not pinned: the dialog's opening sentence names the entry and carries its fingerprint as a monospace chip, then says to compare it with the server's, with the command to run as a chip, before connecting.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: {
      entries: [staging],
      outcomes: { staging: { outcome: "host_key_unknown", fingerprint: "SHA256:Qm9vcHNjb25zdGFudGZpbmdlcnByaW50Zm9yZ2FsbGVyeQ" } },
    },
    steps: [(ui) => ui.press(connectOf(ui, staging))],
  },
  {
    id: "connect-page-failure",
    group: GROUP,
    category: "normal",
    title: "Connect page after a failed connection",
    description: "A connection that fails for a known reason ends in an alert of this window titled Could not reach the server, with the server's address as a chip in its sentence and the shell's own words behind Error details; the list stays as it was. There is no Report issue: this is not an unexpected failure.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: {
      entries: [staging],
      outcomes: { staging: { outcome: "unreachable", detail: "ssh: connect to host staging.example.com port 22: Connection timed out" } },
    },
    steps: [(ui) => ui.press(connectOf(ui, staging))],
  },
  {
    id: "connect-page-version-mismatch",
    group: GROUP,
    category: "normal",
    title: "Connect page with a daemon of another version",
    description:
      "The daemon on the server is older than this application: the dialog names both versions, says the daemon has to be updated on the server itself, and offers Open in browser (the primary action), which has the shell open the daemon's own web interface in the system browser, besides Cancel. Nothing here updates the daemon. A daemon that is newer than the application has a different footer, in its own scenario.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: {
      entries: [staging],
      outcomes: { staging: { outcome: "generation_mismatch", remoteGeneration: 1, localGeneration: 2, remoteVersion: "1.4.0", localVersion: "2.0.0" } },
    },
    steps: [(ui) => ui.press(connectOf(ui, staging))],
  },
  {
    id: "connect-page-edit",
    group: GROUP,
    category: "normal",
    title: "Edit remote daemon",
    description:
      "The form opened by Edit on Staging, which already signs in with this computer's own key: the name, host, port and user filled in, each field with its example as a placeholder when emptied; Authentication offers a third choice, This computer's key, besides Password and Key file, with its own description; the pinned host key is shown as plain monospace text, not a chip, with a Clear button and the line saying when clearing it helps. Scroll the dialog to see them.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: { entries: [{ ...staging, octoboarddPath: "/opt/octoboard/bin/octoboardd" }] },
    steps: [(ui) => ui.press(ui.t("remoteDaemons.editLabel", { name: staging.name }))],
  },
  {
    id: "connect-page-remove",
    group: GROUP,
    category: "normal",
    title: "Remove remote daemon",
    description: "Remove on Home lab asks first, in a destructive confirmation naming the entry; nothing else is touched: the daemon itself and its window are not affected.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: { entries: [staging, homeLab] },
    steps: [(ui) => ui.press(ui.t("remoteDaemons.removeLabel", { name: homeLab.name }))],
  },
  {
    id: "connect-page-start-daemon",
    group: GROUP,
    category: "normal",
    title: "Connect page offering to start the daemon",
    description: "The server answers but no daemon of Octoboard is running on it: a confirmation titled Start the daemon names the entry and offers Start daemon, which starts it on the server and connects.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: { entries: [staging], outcomes: { staging: { outcome: "no_daemon" } } },
    steps: [(ui) => ui.press(connectOf(ui, staging))],
  },
  {
    id: "connect-page-locate-daemon",
    group: GROUP,
    category: "normal",
    title: "Connect page asking where octoboardd is",
    description: "octoboardd was not found where the shell looks on the server: a dialog lists the places it looked in, as plain monospace text, not chips, and asks for the path in a field with an example as its placeholder, with Save and connect.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: { entries: [staging], outcomes: { staging: { outcome: "octoboardd_not_found", tried: ["/usr/local/bin/octoboardd", "/home/deploy/.octoboard/bin/octoboardd", "/opt/octoboard/bin/octoboardd"] } } },
    steps: [(ui) => ui.press(connectOf(ui, staging))],
  },
  {
    id: "connect-page-version-mismatch-newer",
    group: GROUP,
    category: "normal",
    title: "Connect page with a daemon newer than the application",
    description:
      "The daemon on the server is newer than this application: the dialog names both versions and says they cannot work together until this app is updated, then that the daemon's own web interface can be opened in the browser. Its footer holds Go to Updates and Open in browser (the primary one) and no Cancel; the close button in the header is the way out. Go to Updates shows the application's own Updates in its main window, which this gallery does not draw, so nothing visible follows.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: {
      entries: [staging],
      outcomes: { staging: { outcome: "generation_mismatch", remoteGeneration: 3, localGeneration: 2, remoteVersion: "2.1.0", localVersion: "2.0.0" } },
    },
    steps: [(ui) => ui.press(connectOf(ui, staging))],
  },
  {
    id: "connect-page-application-not-running",
    group: GROUP,
    category: "exceptional",
    title: "Connect page, the application is not running on the server",
    description:
      "The octoboardd on the server is the desktop application's, whose daemon runs only while the application does, and the application is closed there: a confirmation titled The app is not running on the server says so in a sentence naming the entry, tells to open the app on that computer, and offers Try again besides Cancel. The shell's own words are in a block behind an Error details disclosure, open here.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: { entries: [staging], outcomes: { staging: { outcome: "application_not_running", detail: "octoboardd: the owner endpoint /Users/dev/.octoboard/browser.sock does not exist" } } },
    steps: [(ui) => ui.press(connectOf(ui, staging)), (ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "connect-page-failure-unexpected",
    group: GROUP,
    category: "exceptional",
    title: "Connect page after a connection failed unexpectedly",
    description:
      "The shell ended the connection with a failure the application has no wording for: the alert is the unexpected-failure one, titled Could not connect to the remote daemon, saying only that something unexpected went wrong with the Report issue guidance, and the shell's own words behind Error details, closed here. Report issue sits before OK in the footer.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: { entries: [staging], outcomes: { staging: { outcome: "failed", detail: "ssh: unexpected packet type 97 in the key exchange" } } },
    steps: [(ui) => ui.press(connectOf(ui, staging))],
  },
  {
    id: "connect-page-host-key-changed",
    group: GROUP,
    category: "exceptional",
    title: "Connect page after a changed host key",
    description:
      "The server presented a different host key than the one pinned, so the connection was refused: an alert with a two-sentence body, the second telling to edit the entry and clear the pinned key, and its Error details open on the pinned and the presented fingerprint, one to a line. Unlike a plain failure it has a remedy in the entry's own form.",
    state,
    desktopApp: true,
    connectPage: true,
    remoteDaemons: {
      entries: [staging],
      outcomes: { staging: { outcome: "host_key_mismatch", expected: "SHA256:Qm9vcHNjb25zdGFudGZpbmdlcnByaW50Zm9yZ2FsbGVyeQ", actual: "SHA256:b3RoZXJob3N0a2V5ZmluZ2VycHJpbnRvZmF0aGlyZHNlcnZlcg" } },
    },
    steps: [(ui) => ui.press(connectOf(ui, staging)), (ui) => ui.press(ui.t("error.diagnostics"))],
  },
];
