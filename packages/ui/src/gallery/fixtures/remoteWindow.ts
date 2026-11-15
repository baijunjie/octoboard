import type { HostFacts } from "../../platform";
import type { HostInfo } from "../../protocol";
import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import { daemonUpdateOf, minutesAgo, SAMPLE, snapshotState } from "../../fixtures/builders";
import { MAC_HOST, WINDOWS_HOST } from "../../test/hosts";
import type { Scenario } from "../scenario";
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

/** Staging's daemon: a Linux server whose agent language is German, while the window shows the interface language. */
const remoteState = snapshotState({
  consoles: [console_],
  projects: [web, api],
  sessions,
  ...LINUX_MACHINE,
  settings: { ...DEFAULT_SETTINGS, agent_language: "de", default_clone_dir: "/home/deploy/projects" },
});

/** What the title bar shows in a remote window, which has no state of its own to set up. */
const titleBarState = snapshotState({ consoles: [console_], projects: [web, api], sessions, ...LINUX_MACHINE });

/** The windows of the daemons of another computer, which the desktop shell opens on a loopback address. */
export const remoteWindowScenarios: Scenario[] = [
  {
    id: "settings-general-remote-window",
    group: "Settings",
    category: "normal",
    title: "General settings in a remote daemon's window",
    description:
      "Settings of the window opened on Staging, a Linux server. The section opens on two language rows: Interface language, the language this window and every other desktop window share, and Agent language, the remote daemon's own setting, which reads Deutsch here while the window is in the viewing language. Under them only what is the daemon's: Resume sessions on start, Terminals and Sorting. The application's own groups (Launch at login, Window, and Power, which a Linux server cannot offer) are absent, and so are the Permissions, Remote access and Updates sections in the list on the left.",
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
      "The window opened on Staging on Windows. The title bar names the daemon in a small chip with the Server glyph, filled like a control and set apart before the breadcrumb (console, project, selected session), so windows of several daemons can be told apart; it has no Remote menu button, as the Remote items are in the window's system menu (the title bar icon's, or Alt+Space), which the shell draws. The rail has no group of remote daemons and none of the application's own settings are offered. Choose a narrow width to see the breadcrumb fade while the chip keeps its width.",
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
];
