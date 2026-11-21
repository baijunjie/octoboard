/**
 * What the UI takes from the machine it runs on, declared by the shell instead of inferred from the user agent. The
 * desktop shell reports them in the window's `host` query parameter (`host_facts.rs`); a browser derives the operating system
 * and input facts, and supplies no native capabilities (`browserHost`).
 */
export type HostFacts = {
  os: "macos" | "windows" | "linux";
  /** Android's TalkBack represents a virtual press as a 1px mouse press. */
  isAndroid?: boolean;
  /** The modifier xterm uses to force selection under mouse reporting, Option on macOS by default. */
  xtermSelectionModifier?: "alt" | "shift";
  /** The primary shortcut modifier. */
  modifier: "meta" | "control";
  /** What the top bar must keep clear: the window controls floated over it at its leading or trailing edge, or nothing. */
  chrome: "overlay-left" | "overlay-right" | "native" | "none";
  /** The width, in CSS pixels, the window controls cover at the bar's leading edge; with an overlay chrome. */
  trafficLightInset?: number;
  /** A translucent native material sits behind a transparent webview. */
  translucent: boolean;
  /** A native menu bar exists. */
  appMenu: boolean;
  statusItem: "rich" | "plain" | "none";
  /** The application's icon can carry a count. */
  badge: boolean;
  /** Closing the window can keep the application running. */
  runInBackground: boolean;
  /** The shell can show the daemon's hold on the computer awake on its menu bar icon. */
  keepAwake: boolean;
  /** The system can open the application when the user logs in. */
  launchAtLogin: boolean;
  /** The application can link its `octoboardd` into the PATH, which the Settings row for the command-line tool offers. */
  commandLine: boolean;
  /** The privacy grants section, which the system has only on macOS. */
  permissionsPane: boolean;
  notificationSettings: "system-pane" | "none";
  updater: "in-place" | "installer" | "none";
  /** The system's own words for what the copy names. */
  terms: {
    trash: "trash" | "recycleBin";
    settings: "systemSettings" | "settings";
    menuBarIcon: "menuBar" | "notificationArea";
  };
};

const TERMS: Record<HostFacts["os"], HostFacts["terms"]> = {
  macos: { trash: "trash", settings: "systemSettings", menuBarIcon: "menuBar" },
  windows: { trash: "recycleBin", settings: "settings", menuBarIcon: "notificationArea" },
  linux: { trash: "trash", settings: "settings", menuBarIcon: "notificationArea" },
};

/**
 * A browser's system and input facts from the platform it names (an iPhone or iPad uses ⌘ for shortcuts),
 * with Android identified by the user agent for TalkBack's pointer events.
 */
export function browserHost(
  // eslint-disable-next-line no-restricted-syntax -- This is where the client's host is built from the browser.
  platform: string = globalThis.navigator?.platform ?? "",
  // eslint-disable-next-line no-restricted-syntax -- This is where the client's host is built from the browser.
  userAgent: string = globalThis.navigator?.userAgent ?? "",
): HostFacts {
  const os = /Mac|iPhone|iPad/.test(platform) ? "macos" : /Win/.test(platform) ? "windows" : "linux";
  return {
    os,
    isAndroid: /Android/i.test(userAgent),
    // xterm treats an iPhone or iPad in mobile mode like other non-Mac platforms for selection.
    xtermSelectionModifier: /^(Macintosh|MacIntel|MacPPC|Mac68K)$/.test(platform) ? "alt" : "shift",
    modifier: os === "macos" ? "meta" : "control",
    chrome: "none",
    translucent: false,
    appMenu: false,
    statusItem: "none",
    badge: false,
    runInBackground: false,
    keepAwake: false,
    launchAtLogin: false,
    commandLine: false,
    permissionsPane: false,
    notificationSettings: "none",
    updater: "none",
    terms: TERMS[os],
  };
}

const OPTIONS = {
  os: ["macos", "windows", "linux"],
  modifier: ["meta", "control"],
  chrome: ["overlay-left", "overlay-right", "native", "none"],
  statusItem: ["rich", "plain", "none"],
  notificationSettings: ["system-pane", "none"],
  updater: ["in-place", "installer", "none"],
} as const;

const TERM_OPTIONS = {
  trash: ["trash", "recycleBin"],
  settings: ["systemSettings", "settings"],
  menuBarIcon: ["menuBar", "notificationArea"],
} as const;

/** The facts the shell put in the window's query (`?host=`, JSON), or `undefined` when there are none or they are not a whole, valid set. */
export function hostFromQuery(search: string): HostFacts | undefined {
  const raw = new URLSearchParams(search).get("host");
  if (!raw) return undefined;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return undefined;
  }
  if (typeof value !== "object" || value === null) return undefined;
  const facts = value as Record<string, unknown>;
  const terms = facts.terms as Record<string, unknown> | null | undefined;
  const oneOf = (options: readonly unknown[], field: unknown) => options.includes(field);
  const valid =
    (Object.keys(OPTIONS) as (keyof typeof OPTIONS)[]).every((key) => oneOf(OPTIONS[key], facts[key])) &&
    (Object.keys(TERM_OPTIONS) as (keyof typeof TERM_OPTIONS)[]).every((key) => oneOf(TERM_OPTIONS[key], terms?.[key])) &&
    ["translucent", "appMenu", "badge", "runInBackground", "keepAwake", "launchAtLogin", "commandLine", "permissionsPane"].every((key) => typeof facts[key] === "boolean") &&
    (facts.trafficLightInset === undefined || typeof facts.trafficLightInset === "number") &&
    (facts.isAndroid === undefined || typeof facts.isAndroid === "boolean") &&
    (facts.xtermSelectionModifier === undefined || oneOf(["alt", "shift"], facts.xtermSelectionModifier));
  return valid ? (facts as unknown as HostFacts) : undefined;
}

/** A shell that reported none (a dev server in a Tauri window, an older shell) is taken for what the browser says it is. */
export function tauriHost(search: string = window.location.search): HostFacts {
  return hostFromQuery(search) ?? browserHost();
}

/** Whether this page runs in the desktop shell's webview, which injects its IPC bridge before any script runs. */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** The facts of the environment this page runs in, the same as the platform adapter's `host`, for what must know them
 * where no adapter is at hand: a value read when a module loads, or a scripted page. */
export function readHost(): HostFacts {
  return inTauri() ? tauriHost() : browserHost();
}
