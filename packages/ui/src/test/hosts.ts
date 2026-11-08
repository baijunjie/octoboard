import { browserHost, type HostFacts } from "../platform/host";

/** What the desktop shell reports on macOS (`host_facts.rs`). */
export const MAC_HOST: HostFacts = {
  os: "macos",
  modifier: "meta",
  chrome: "overlay-left",
  trafficLightInset: 80,
  translucent: true,
  appMenu: true,
  statusItem: "rich",
  badge: true,
  runInBackground: true,
  keepAwake: true,
  launchAtLogin: true,
  permissionsPane: true,
  notificationSettings: "system-pane",
  updater: "in-place",
  terms: { trash: "trash", settings: "systemSettings", menuBarIcon: "menuBar" },
};

/** What it reports on Windows. */
export const WINDOWS_HOST: HostFacts = {
  os: "windows",
  modifier: "control",
  chrome: "native",
  translucent: false,
  appMenu: false,
  statusItem: "plain",
  badge: false,
  runInBackground: true,
  keepAwake: true,
  launchAtLogin: true,
  permissionsPane: false,
  notificationSettings: "system-pane",
  updater: "installer",
  terms: { trash: "recycleBin", settings: "settings", menuBarIcon: "notificationArea" },
};

/** What a browser in the test environment (jsdom, which names no platform) derives: the facts every test not about a host runs on. */
export const BROWSER_HOST: HostFacts = browserHost();

/** Whether the test environment's browser is on an Apple keyboard, for a test that presses the modifier its browser host has. */
export const IS_MAC = BROWSER_HOST.modifier === "meta";
