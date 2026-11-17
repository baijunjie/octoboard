import type { HostFacts } from "../../platform";
import type { HostInfo } from "../../protocol";
import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import { daemonUpdateOf, minutesAgo, processTreeOf, SAMPLE, snapshotState } from "../../fixtures/builders";
import type { Step } from "../../fixtures/interact";
import { MAC_HOST, WINDOWS_HOST } from "../../test/hosts";
import type { Scenario } from "../scenario";
import { PATH_LINK_FIXTURE, PATH_LINKS, TEMP_IMAGE_PATH } from "./pathLinkFiles";
import { buildServer, relay, staging } from "./remoteDaemons";

const { console: console_, web, api, sessions } = SAMPLE;

/** What the shell reports to the window of a remote daemon (`for_remote_window` in `host_facts.rs`): the system's own
 * facts, `host`, less what is the application's rather than a daemon's (running in the background, launching at login,
 * the privacy grants and the updater). */
function remoteWindowHost(host: HostFacts): HostFacts {
  return { ...host, runInBackground: false, launchAtLogin: false, permissionsPane: false, updater: "none" };
}

/** A remote window's host on macOS. */
export const REMOTE_WINDOW_HOST = remoteWindowHost(MAC_HOST);

/** The title bar's own host: the Windows application's, less what a remote window is not given. */
const WINDOWS_REMOTE_HOST = remoteWindowHost(WINDOWS_HOST);

/** The host of a remote daemon: a Linux server, which cannot be held awake. */
const LINUX_MACHINE = {
  host: { os: "linux", path_style: "unix", file_links: true, trash: "trash", settings: "settings", keep_awake: false },
  homeDir: "/home/deploy",
} as const satisfies { host: HostInfo; homeDir: string };

/** The remote daemons the window's shell keeps, with the one the window is open on connected. */
const REMOTE_DAEMONS: NonNullable<Scenario["remoteDaemons"]> = { entries: [staging, buildServer], status: [{ id: staging.id, state: "connected", via: "window" }] };

/** Staging's daemon: a Linux server. It holds no language, so it is given the one the window shows, as the app asks
 * it to: a window follows its daemon's language, so one that held another would turn the window into it whatever
 * language the gallery was opened in. */
const remoteState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions,
  ...LINUX_MACHINE,
  settings: { ...DEFAULT_SETTINGS, default_clone_dir: "/home/deploy/projects" },
});

/** What the title bar shows in a remote window, which has no state of its own to set up. */
const titleBarState = snapshotState({ consoles: [console_], projects: [web, api], sessions, ...LINUX_MACHINE });

/** What every scenario of a remote daemon's window says of the window: the shell's facts of such a window, the desktop
 * platform kind, and the daemon it is open on. */
const REMOTE_WINDOW = {
  host: REMOTE_WINDOW_HOST,
  desktopApp: true,
  remoteWindow: { name: staging.name },
  remoteDaemons: REMOTE_DAEMONS,
} as const satisfies Partial<Scenario>;

/** The sample terminal output naming files, in a remote window: the same paths as the local path link scenarios, found
 * on the daemon's machine, and the folder `docs/plans/summary-layout` with no README or index file, which a remote window
 * has no file manager for and so leaves as text. */
const REMOTE_PATH_LINKS = {
  ...PATH_LINK_FIXTURE,
  pathLinks: { ...PATH_LINKS, "/Users/dev/code/website/docs/plans/summary-layout": "directory" },
  state: titleBarState,
  ...REMOTE_WINDOW,
} satisfies Partial<Scenario>;

/** Selects the session whose terminal the scenarios look at, then closes the drawer a pick leaves up when the sidebar
 * floats, where it would cover the terminal's left side. */
const pickSession: Step = async (ui) => {
  await ui.press(ui.session("Fix the summary layout"));
  await ui.closeDrawer();
};

/** A dev server on the daemon's machine, whose port the window forwards. */
const devServer = processTreeOf("700:1", SAMPLE.sessions[1], "node server.js --port 3000", 8, { ports: [3000], memory_bytes: 96_000_000 });

/** The windows of the daemons of another computer, which the desktop shell opens on a loopback address. */
export const remoteWindowScenarios: Scenario[] = [
  {
    id: "settings-general-remote-window",
    group: "Settings",
    category: "normal",
    title: "General settings in a remote daemon's window",
    description:
      "Settings of the window opened on Staging, a Linux server. The section opens on the Language row, which is the remote daemon's own setting: a remote window shows the language its daemon holds and changes that daemon's. Under it only what is the daemon's: Resume sessions on start, Terminals and Sorting. The application's own groups (Launch at login, Window, and Power, which a Linux server cannot offer) are absent, and so are the Permissions, Remote access and Updates sections in the list on the left.",
    state: remoteState,
    host: REMOTE_WINDOW_HOST,
    desktopApp: true,
    remoteWindow: { name: staging.name },
    remoteDaemons: REMOTE_DAEMONS,
    steps: [(ui) => ui.press(ui.t("rail.settings"))],
  },
  {
    id: "remote-window-title-bar",
    group: "Window layout",
    category: "normal",
    title: "Remote daemon's window, title bar",
    description:
      "The window opened on Staging on Windows. The title bar names the daemon in a small chip with the Server glyph, filled like a control and set apart before the breadcrumb (console, project, selected session), so windows of several daemons can be told apart; it has no Remote menu button, as the Remote items are in the window's system menu (the title bar icon's, or Alt+Space), which the shell draws. The rail has no Remote devices button, and none of the application's own settings are offered. Choose a narrow width to see the breadcrumb fade while the chip keeps its width.",
    state: titleBarState,
    host: WINDOWS_REMOTE_HOST,
    desktopApp: true,
    steps: [(ui) => ui.press(ui.session("Update the dependencies"))],
    remoteWindow: { name: staging.name },
    remoteDaemons: REMOTE_DAEMONS,
  },
  {
    id: "remote-window-title-bar-long-name",
    group: "Window layout",
    category: "normal",
    title: "Remote daemon's window, title bar with a long name",
    description:
      "The window opened on a daemon with a long name: the chip keeps its fixed width and the name fades at its end instead of being cut off. Rest the mouse on it to see the name run as a marquee and, after a moment, in full in a tooltip.",
    state: titleBarState,
    host: WINDOWS_REMOTE_HOST,
    desktopApp: true,
    steps: [(ui) => ui.press(ui.session("Update the dependencies"))],
    remoteWindow: { name: relay.name },
    remoteDaemons: { entries: [relay], status: [{ id: relay.id, state: "connected", via: "window" }] },
  },
  {
    id: "remote-window-settings-updates",
    group: "Settings",
    category: "normal",
    title: "Updates settings in a remote daemon's window",
    description:
      "Settings of the window opened on Staging, whose daemon updates itself and has found a newer version. The Updates section holds only the daemon's two settings of its updates, Check for updates automatically and Update channel, which stay editable here; there is no version row, no Check for updates, download or restart button, and the window shows no update notice: updating is done on the server itself.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api],
      sessions,
      ...LINUX_MACHINE,
      settings: { ...DEFAULT_SETTINGS, daemon_update_channel: "beta" },
      daemonUpdate: daemonUpdateOf({ available: { version: "2.1.0", channel: "beta", holder_floor: "2.0.0" }, last_check: { outcome: "available", at: minutesAgo(5), error: null, detail: null } }),
    }),
    host: REMOTE_WINDOW_HOST,
    desktopApp: true,
    remoteWindow: { name: staging.name },
    remoteDaemons: REMOTE_DAEMONS,
    steps: [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.updates"))],
  },
  {
    id: "remote-window-terminal-path-links",
    group: "Terminal",
    category: "normal",
    title: "File paths in a remote daemon's terminal",
    description:
      "The terminal of the window opened on Staging names the same paths as on this computer, found on the daemon's machine: the project's files and the temp-folder image are in the accent text colour, and so is the address on the last line. docs/plans/summary-layout is plain text here, since it has no README or index file to show and the window has no file manager for the daemon's machine. " +
      "The right-click was made on src/summary/Summary.tsx:42:7: the menu is only Copy, Paste and Select all, with no Open in, since the daemon's file manager and applications are not this computer's. A click on a path opens it in the viewer: see the next scenario.",
    ...REMOTE_PATH_LINKS,
    clipboard: { text: "echo hello" },
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.terminalContextMenu("src/summary/Summary.tsx:42:7")],
  },
  {
    id: "remote-window-path-link-viewer",
    group: "Terminal",
    category: "normal",
    title: "A file of the daemon's machine opened from its terminal",
    description:
      "Clicking /private/tmp/octoboard/scratchpad/icon/sheet3.png in the terminal of the window opened on Staging opened the viewer on it, read through the daemon: the screenshot is drawn as an image, and the header names the path on the daemon's machine. " +
      "The header ends with Download and open alone, where this computer's window has File manager and Default application: they act on the daemon's machine, so they are not offered.",
    ...REMOTE_PATH_LINKS,
    remoteFiles: true,
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.clickTerminalLink(TEMP_IMAGE_PATH)],
  },
  {
    id: "remote-window-forwarded-ports",
    group: "Window layout",
    category: "normal",
    title: "Forwarded ports of a remote daemon's window",
    description:
      "The window opened on Staging holds two forwards, so its rail has a Forwarded ports button, a plug glyph with a green count of 2, directly below the Processes button with the same even gap to its neighbours. Its popover is open, with the Forwarded ports heading and a line saying what the ports are, then a row for each: localhost:3000 over Port 3000 on Staging, and localhost:52871 over Port 8080 on Staging, which another program of this computer held at 8080. Each row has a danger-toned Stop at its end, and the first row has focus, its tooltip naming the address as written on the daemon's machine. Pressing a row opens its address again; Stop lets the port go and takes the row out, and stopping the last one takes the button and the popover with it.",
    state: titleBarState,
    ...REMOTE_WINDOW,
    processes: { trees: [devServer] },
    remoteForwards: [
      { remotePort: 3000, localPort: 3000, address: "http://localhost:3000/", localAddress: "http://localhost:3000/" },
      { remotePort: 8080, localPort: 52871, address: "http://localhost:8080/docs", localAddress: "http://localhost:52871/docs" },
    ],
    steps: [(ui) => ui.press(ui.t("rail.forwards", { count: 2 }))],
  },
  {
    id: "remote-window-forward-nothing-listening",
    group: "Terminal",
    category: "exceptional",
    title: "An address in a remote daemon's terminal that nothing serves",
    description:
      "Clicking http://localhost:5176/gallery.html in the terminal of the window opened on Staging, where nothing listens on port 5176, opened the error alert titled \"Could not open the link\" and saying nothing is listening on that port on Staging. The browser stays shut, and there is no address to open by hand: as written it would lead to this computer. The sentence has no remedy line.",
    ...REMOTE_PATH_LINKS,
    forwardFailure: "nothing_listening",
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("http://localhost:5176/gallery.html")],
  },
  {
    id: "remote-window-forward-refused",
    group: "Terminal",
    category: "exceptional",
    title: "An address in a remote daemon's terminal whose server forbids forwarding",
    description:
      "Clicking http://localhost:5176/gallery.html in the terminal of the window opened on Staging, whose SSH server does not allow port forwarding, opened the error alert titled \"Could not open the link\" and saying so, with the remedy line under it: allow TCP forwarding in that server's settings or open the address on that computer. " +
      "The alert for a connection that is down is laid out the same, with its own sentence (the connection to Staging is down) and the connection remedy.",
    ...REMOTE_PATH_LINKS,
    forwardFailure: "forwarding_refused",
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("http://localhost:5176/gallery.html")],
  },
  {
    id: "remote-window-reconnecting",
    group: "Startup and connection",
    category: "exceptional",
    title: "Remote daemon's window reconnecting after a drop",
    description:
      "The window opened on Staging lost its connection to the daemon and keeps the last known sessions on screen. The banner along the bottom says it is disconnected and reconnecting, with Try again beside the line: a remote window retries without end, so Try again is there all the while, not only once attempts are spent as in a local window (`daemon-reconnecting`, `daemon-lost`).",
    ...REMOTE_WINDOW,
    state: { ...remoteState, connectionState: "reconnecting" },
    steps: [pickSession],
  },
  {
    id: "remote-window-reconnecting-before-snapshot",
    group: "Startup and connection",
    category: "exceptional",
    title: "Remote daemon's window that has not connected yet",
    description:
      "The window opened on Staging has never received a snapshot and its first connection keeps failing. Nothing to show yet, so the content area holds only the line saying the daemon is not answering, as a local window's does once its attempts are spent (`daemon-closed-before-snapshot`), with the title bar naming Staging, and the banner along the bottom offers Try again beside the reconnecting line. A local window has neither until its attempts are spent.",
    ...REMOTE_WINDOW,
    state: { connectionState: "reconnecting" },
  },
];
