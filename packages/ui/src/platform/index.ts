import { browserPlatform } from "./browser";
import { tauriPlatform } from "./tauri";
import type { Agent } from "../protocol";

export { ConfigurationFileError } from "./configurationFileError";

/**
 * The only route from the UI to anything native. Every capability is optional: a member that is
 * absent means the environment has no such thing, and the feature built on it is absent too — not
 * an error. Code reads a member, checks it is there, and otherwise does nothing.
 *
 * Every direct use of a Tauri API in the UI lives behind this interface, in `./tauri.ts`; nothing
 * else may import `@tauri-apps/*`.
 */
export interface PlatformAdapter {
  readonly kind: "tauri" | "browser";
  /** The application's own lifetime: present only where a shell owns the process and must be told
   * when it may end. */
  readonly exit?: ExitCapability;
  readonly notifications?: NotificationCapability;
  readonly badge?: BadgeCapability;
  /** The native window itself — its appearance (the controls and any native chrome), which CSS
   * cannot reach, and its own visibility and place in front of other apps; present only where a
   * shell owns a native window — a browser tab has neither an appearance to match nor a window to
   * reveal. */
  readonly nativeWindow?: NativeWindowCapability;
  /** Present only where the window draws no titlebar of its own, so the UI's top bar is the
   * window's titlebar: it then carries the drag region and keeps `leftInset` clear for the
   * window controls the shell floats over it. */
  readonly windowChrome?: WindowChromeCapability;
  /** True only where the shell puts a translucent native material behind a transparent webview, so
   * the UI leaves the window chrome's own background clear and paints the content panel opaquely
   * (`--window-background` in `style.css`). */
  readonly translucentWindow?: boolean;
  /** The shell's native menu bar: the items that ask the UI to do something, and the labels the
   * UI gives every item. */
  readonly appMenu?: AppMenuCapability;
  /** The shell's menu bar icon, which keeps the app reachable while its window is closed: its menu
   * lists the sessions the UI hands it, and choosing one comes back here. */
  readonly statusItem?: StatusItemCapability;
  /** The system clipboard, for text the UI puts there on the user's say-so. */
  readonly clipboard?: ClipboardCapability;
  /** Saving the configuration document as a file the user names and opening one the user picks,
   * each through the system's own dialog. Present wherever the environment offers a way to
   * do both. */
  readonly configurationFile?: ConfigurationFileCapability;
  /** Opening an address in the user's own browser (or mail client), for a link the UI shows but
   * must not navigate its own window to. */
  readonly links?: LinkCapability;
  /** The macOS privacy permissions an agent's child process can need, which the UI lets the user
   * grant on purpose; present only under a desktop shell, which answers `undefined` from `status`
   * when it is not running on macOS. */
  readonly macosPermissions?: MacosPermissionsCapability;
  /** Whether closing the window keeps the app running in the background: present only where a
   * shell answers the window's close button and can do either, so the Settings switch for it
   * exists only there. */
  readonly backgroundRun?: BackgroundRunCapability;
}

export interface ExitHandlers {
  /** The user asked to quit by any gesture the shell reports: Cmd+Q, the app menu, the menu bar
   * icon's Quit, the Dock icon's own Quit, a system-initiated logout/restart/shutdown. The window's
   * close button is one only while the user has not chosen to run in the background (see
   * `BackgroundRunCapability`): the shell then takes it as a quit gesture. */
  onQuitRequested: () => void;
  /** The shell's daemon sidecar exited on its own; `detail` says how. */
  onDaemonExited: (detail: string) => void;
}

export interface ExitCapability {
  /**
   * Registers this webview as the one handling the exit flow and subscribes to the shell's quit
   * gestures, returning the function that undoes the subscriptions. The shell only starts asking
   * before letting an exit through once this has run, so every screen that can be on show calls
   * it — including the daemon-failed-to-start one — or a window stuck there would not be
   * quittable.
   */
  register(handlers: ExitHandlers): () => void;
  /** Proof this webview is alive and answering; see `useAppExit` for when it is sent. */
  heartbeat(): Promise<void>;
  /** Ends the process. Only after the daemon has been shut down. */
  confirmQuit(): Promise<void>;
}

export type NotificationPermissionStatus = "granted" | "denied" | "undecided";

export interface NotificationCapability {
  /** Whether notifications may be shown. Where `permissionPrompt` is present this only reads the
   * answer and never asks; otherwise it asks the user if that has not happened yet. */
  ensurePermission(): Promise<boolean>;
  /** Present only where asking for permission needs a user gesture, as in a browser, which ignores
   * or denies a request made from anywhere else. The UI offers a control that calls `request`. */
  readonly permissionPrompt?: {
    status(): NotificationPermissionStatus;
    request(): Promise<void>;
  };
  notify(notification: { title: string; body: string }): Promise<void>;
}

export interface BadgeCapability {
  /** Shows `count` on the application's icon; 0 clears it. */
  set(count: number): Promise<void>;
}

export interface NativeWindowCapability {
  /** Pins the window's appearance to `theme`; `undefined` hands control back to the OS, so the
   * window keeps following a live system appearance change instead of being pinned to whatever
   * it happened to resolve to at the moment of the call. */
  setTheme(theme: "light" | "dark" | undefined): Promise<void>;
  /** Shows a window the shell created hidden (`open_main_window` in
   * `apps/desktop/src-tauri/src/lib.rs`), once this process has pushed the native theme and
   * painted the page — showing it any earlier is exactly the white/dark flash of the OS's own
   * appearance that creating it hidden exists to avoid. Idempotent: showing an already-visible
   * window, or the shell's own timeout-driven fallback racing this call, is a no-op either way. */
  reveal(): Promise<void>;
  /** Activates the app and brings the window to the front, restoring it first if minimized or
   * hidden in the background. */
  bringToFront(): Promise<void>;
  /** Whether the window is on screen; false while it is hidden in the background. */
  isVisible(): Promise<boolean>;
}

export interface WindowChromeCapability {
  /** The width, in CSS pixels, at the window's left edge, whatever the direction, that the window
   * controls cover right now: none while they are hidden, as in fullscreen. */
  leftInset(): number;
  /** Subscribes to changes of `leftInset`, returning the function that undoes it. */
  subscribe(callback: () => void): () => void;
}

export interface AppMenuCapability {
  /** Subscribes to the menu's Settings item (Cmd+,), returning the function that undoes it. */
  onSettingsRequested(handler: () => void): () => void;
  /** Hands the shell the menu's labels, keyed by `menu.*` message key and already in the UI's
   * language; the shell rebuilds the menu with them, and keeps its English text for any label
   * missing. */
  setLabels(labels: Record<string, string>): Promise<void>;
}

/** The menu bar icon's menu, already in the UI's language; it also decides the icon's picture,
 * through `attention`. */
export interface StatusItemMenu {
  sections: StatusItemSection[];
  /** Submenus after the sessions and before Open, such as an agent's quota; none until the UI has
   * what they list. */
  submenus: StatusItemSubmenu[];
  /** Each picture the lines name, once, by the key a line's `icon` gives. */
  icons: Record<string, StatusItemIcon>;
  open: string;
  quit: string;
  /** Whether the icon raises its hand: a live session is waiting for the user. */
  attention: boolean;
}

export interface StatusItemIcon {
  /** A base64 PNG, at twice the size it shows at. */
  png: string;
  /** Drawn in black for its alpha alone, so the system tints it to suit a light or dark menu. */
  template: boolean;
}

export interface StatusItemSection {
  heading: string;
  /** A key of `StatusItemMenu.icons`. */
  icon?: string;
  items: StatusItemEntry[];
}

export interface StatusItemEntry {
  label: string;
  /** A key of `StatusItemMenu.icons`. */
  icon?: string;
  /** Absent for a line that cannot be chosen. */
  session?: string;
}

/** A submenu, opened from a line of the menu itself, whose lines cannot be chosen. */
export interface StatusItemSubmenu {
  title: string;
  /** The key of the picture beside the title, such as the agent's mark; none for no picture. It is
   * decoration: the title is what is read. */
  icon?: string;
  items: StatusItemSubmenuItem[];
}

/** What a submenu holds. The shell draws a window's bar, so the menu carries what it needs to,
 * and a line cannot be chosen. */
export type StatusItemSubmenuItem =
  | { kind: "header"; title: string }
  /** A window's name and, right-aligned, its detail, with a bar of `fraction` (0 to 1) beneath in
   * `agent`'s colour. `label` is the whole line as one phrase, for assistive technology. */
  | { kind: "window"; name: string; detail: string; label: string; fraction: number; agent: Agent }
  | { kind: "note"; text: string }
  | { kind: "separator" };

export interface StatusItemCapability {
  /** Replaces the menu bar icon's menu, and with it whether the icon raises its hand. */
  setMenu(menu: StatusItemMenu): Promise<void>;
  /** Subscribes to a session being chosen in that menu, returning the function that undoes it. The
   * shell has already brought the window to the front. */
  onSessionChosen(handler: (sessionId: string) => void): () => void;
  /** Subscribes to the menu beginning to open, returning the function that undoes it. A menu sent
   * while it is open is held until it closes, so what the handler asks for lands on the next one. */
  onOpened(handler: () => void): () => void;
}

export interface ClipboardCapability {
  /** Puts `text` on the clipboard; rejects when the environment refuses (no permission, no
   * focused document). */
  writeText(text: string): Promise<void>;
  /** Reads text only when the user chooses Paste; may ask for permission or reject. */
  readText?(): Promise<string>;
}

export interface BackgroundRunCapability {
  /** Tells the shell what the window's close button does: with `true` it hides the window and the
   * app keeps running behind the menu bar icon, with `false` it quits the app (through the same
   * confirmation as any quit gesture). The shell starts from `false`, so the UI sends its choice at
   * startup. */
  set(enabled: boolean): Promise<void>;
}

/** The configuration file the user picked. */
export interface OpenedConfigurationFile {
  /** The file's name, without its directory. */
  name: string;
  text: string;
}

/** What became of a file given to `save`: written where the user chose, under the name they
 * gave it, or handed to an environment that files it by itself (a browser's download), which
 * neither says where it went nor whether it was kept. */
export type SavedConfigurationFile = { kind: "saved"; name: string } | { kind: "handedOver" };

/** The configuration document is JSON, and both dialogs offer only JSON files: this is not a way to
 * save or open anything else. */
export interface ConfigurationFileCapability {
  /** Asks the user where to save `text` in the system's save dialog, which offers `suggestedName`,
   * and writes it there. Resolves with `{ kind: "saved" }` and the name the file was saved under,
   * or `undefined` when the user dismissed the dialog; rejects with a `ConfigurationFileError` when
   * the file could not be written. Where the environment has no dialog of its own (a browser that
   * downloads instead) it resolves with `{ kind: "handedOver" }`, since the name and place are the
   * browser's to choose and it does not report a download that was blocked or dismissed. */
  save(suggestedName: string, text: string): Promise<SavedConfigurationFile | undefined>;
  /** Asks the user for a file in the system's open dialog and reads it as text. Resolves with
   * `undefined` when the user dismissed the dialog; rejects with a `ConfigurationFileError` when
   * the file could not be read. */
  open(): Promise<OpenedConfigurationFile | undefined>;
}

export interface LinkCapability {
  /** Opens `url`, an `http:`, `https:` or `mailto:` address, outside the app. Only the desktop
   * platform reports a refusal, by rejecting: a browser cannot tell a blocked popup from an opened
   * one, and resolves either way. */
  open(url: string): Promise<void>;
}

export type MacosPermissionKind = "screenRecording" | "accessibility";

export type MacosPermissionsStatus = Record<MacosPermissionKind, boolean> & {
  /** Whether Screen Recording was already granted when this process started, which is the only
   * grant it can act on: macOS hands the capability out at launch, so `screenRecording` without
   * this one means the user has granted it and has yet to restart the app. */
  screenRecordingAtLaunch: boolean;
};

export interface MacosPermissionsCapability {
  /** The current grants, or `undefined` where the shell is not running on macOS. Only reads, and
   * never shows a system prompt, so it is safe to poll. */
  status(): Promise<MacosPermissionsStatus | undefined>;
  /** The one call that shows the system prompt, with whether the permission is granted once it is
   * out of the way. Screen Recording resolves only after the user has answered; Accessibility's
   * dialog is raised asynchronously and is not granted in place, so it resolves `false` at once,
   * while that dialog is still coming up. */
  request(kind: MacosPermissionKind): Promise<boolean>;
  /** Opens the System Settings pane for `kind`. */
  openSystemSettings(kind: MacosPermissionKind): Promise<void>;
}

/** Picks the implementation once, at startup, from the environment the page is running in. */
export function selectPlatform(): PlatformAdapter {
  return "__TAURI_INTERNALS__" in window ? tauriPlatform() : browserPlatform();
}
