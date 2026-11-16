import { browserPlatform } from "./browser";
import { type HostFacts, inTauri } from "./host";
import { tauriPlatform } from "./tauri";
import type { Agent } from "../protocol";

export { ConfigurationFileError } from "./configurationFileError";
export { UpdaterError } from "./updaterError";
export { LinkForwardError } from "./linkForwardError";
export { RemoteFileError } from "./remoteFileError";
export type { HostFacts } from "./host";
export { daemonIdentity, remoteDaemonName, windowKind, type WindowKind } from "./windowKind";

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
  /** What the machine the window runs on is: declared by the shell, or for a browser derived from the platform it
   * names (`browserHost`). Where a behaviour or a word differs by system, it reads this and nothing else; what a
   * capability below is there to do is not repeated here. */
  readonly host: HostFacts;
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
  /** The taskbar's jump list on Windows, whose lines read the labels the UI hands it: Remote devices, the local
   * window and, under the Remote menu's name, the saved remote daemons. The shell gives every window's system menu the
   * same items from the same labels, with a line saying there is no saved daemon. Absent where there is none. */
  readonly jumpList?: JumpListCapability;
  /** The shell's menu bar icon, which keeps the app reachable while its window is closed: its menu
   * lists the sessions the UI hands it, and choosing one comes back here. */
  readonly statusItem?: StatusItemCapability;
  /** The notification-area flyout's own page (`?flyout=1`, see `windowKind`): the model it draws and the answers it
   * gives the shell. Present only there, in the one window that has no daemon and is not a window of any. */
  readonly trayFlyout?: TrayFlyoutCapability;
  /** The system clipboard, for text the UI puts there on the user's say-so. */
  readonly clipboard?: ClipboardCapability;
  /** Saving the configuration document as a file the user names and opening one the user picks,
   * each through the system's own dialog. Present wherever the environment offers a way to
   * do both. */
  readonly configurationFile?: ConfigurationFileCapability;
  /** Choosing a directory on the machine running the shell, through the system's own dialog. */
  readonly directoryPicker?: DirectoryPickerCapability;
  /** Opening an address in the user's own browser (or mail client), for a link the UI shows but
   * must not navigate its own window to. */
  readonly links?: LinkCapability;
  /** The macOS privacy permissions an agent's child process can need, which the UI lets the user
   * grant on purpose; present only under a desktop shell, which answers `undefined` from `status`
   * when it is not running on macOS. */
  readonly macosPermissions?: MacosPermissionsCapability;
  /** Whether this computer's SSH server (the service another computer reaches this one through: Remote Login on macOS,
   * OpenSSH Server on Windows) is on, and the way to the system settings that turn it on; present only in the local
   * daemon's window of the desktop shell, which answers `undefined` from `status` where it cannot tell. Also what tells
   * that window from a browser, whose adapter has none of the shell's capabilities. */
  readonly remoteLogin?: RemoteLoginCapability;
  /** The list of Octoboard daemons on other computers that the shell reaches over SSH, and connecting to one, which
   * opens a window of its own on it: present in the local daemon's window of the desktop application only, which has the
   * rail's Remote devices button and the page it shows (`remoteDaemonsOffered`), and in no browser or remote daemon's
   * window. No command of it hands a secret to the page. */
  readonly remoteDaemons?: RemoteDaemonsCapability;
  /** The ports of this computer that forward to loopback ports of the remote daemon's machine, for the addresses the
   * window opens there (`links`); present only in a remote window of the desktop shell, whose own connection carries
   * them. */
  readonly remoteForwards?: RemoteForwardsCapability;
  /** Storing a copy of a file of the remote daemon's machine on this computer and opening it, which the viewer's
   * Download and open does; present only in a remote window of the desktop shell, whose commands these are. */
  readonly remoteFiles?: RemoteFilesCapability;
  /** Whether closing the window keeps the app running in the background: present only where a
   * shell answers the window's close button and can do either, so the Settings switch for it
   * exists only there. */
  readonly backgroundRun?: BackgroundRunCapability;
  /** Showing on the menu bar icon that the daemon is keeping the computer awake, present only where a shell has such
   * an icon. */
  readonly keepAwake?: KeepAwakeCapability;
  /** Opening the application when the user logs in, present only where the system lets the shell register it. */
  readonly launchAtLogin?: LaunchAtLoginCapability;
  /** Looking for, downloading and installing a new version of the application: present only where a shell
   * can replace itself, so the update notices and Settings rows exist only there. */
  readonly updater?: UpdaterCapability;
}

export interface ExitHandlers {
  /** The user asked to quit by any gesture the shell reports: Cmd+Q, the app menu, the menu bar
   * icon's Quit, the Dock icon's own Quit, a system-initiated logout/restart/shutdown. The window's
   * close button is one only while the user has not chosen to run in the background (see
   * `BackgroundRunCapability`): the shell then takes it as a quit gesture. */
  onQuitRequested: () => void;
  /** The shell's daemon sidecar exited on its own and is not started again; `detail` says how. Called at most once
   * per registration, also when the daemon was given up on before this registration. */
  onDaemonExited: (detail: string) => void;
}

export interface ExitCapability {
  /**
   * Registers this webview as the one handling the exit flow and subscribes to the shell's quit
   * gestures, returning the function that undoes the subscriptions. The shell only starts asking
   * before letting an exit through once this has run, and it never stops: it has no unregister, so
   * a page that replaces one that registered (the crash fallback) must register itself, or quits
   * and window closes would wait on a page that no longer answers. A page shown before anything
   * registered (the daemon-failed and no-address screens) must not, and is quit by the shell
   * directly.
   */
  register(handlers: ExitHandlers): () => void;
  /** Proof this webview is alive and answering; see `useAppExit` for when it is sent. */
  heartbeat(): Promise<void>;
  /** Ends the process, or with `restart` relaunches the application in its place (how an installed update
   * takes effect). Only after the daemon has been told to stop. */
  confirmQuit(options?: { restart?: boolean }): Promise<void>;
  /** Records that the user is quitting the application, before the daemon is told to shut down: the shell keeps the
   * intent until the quit completes, so that the stop is finished at the next start if this one is cut short. Only
   * for a plain quit, never a restart. */
  beginQuit(): Promise<void>;
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
  /** Opens the app's own pane in the system's notification settings. Present only where the system
   * manages the app's notifications and the app keeps no switch of its own (the macOS app), so the
   * UI links there instead. Rejects when the system did not open the pane. */
  openSystemSettings?(): Promise<void>;
}

export interface BadgeCapability {
  /** Reports `count` as this window's share of the badge on the application's icon, which shows the sum of every
   * window's share; 0 gives up the share. */
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
   * window, or the shell's own timeout-driven fallback racing this call, is a no-op either way.
   * After a launch the system made at login, while the user keeps the app running in the background (which the
   * shell holds itself), the shell leaves the window hidden, as if it had been closed into the background. */
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

export interface JumpListCapability {
  /** Hands the shell the jump list's labels, keyed as the menu's (`menu.*`) and already in the UI's language. */
  setLabels(labels: Record<string, string>): Promise<void>;
}

export interface AppMenuCapability {
  /** Subscribes to the menu's Settings item (Cmd+, unless rebound), returning the function that undoes it. */
  onSettingsRequested(handler: () => void): () => void;
  /** Subscribes to the menu's About item, which the UI answers by opening Settings on its About section (the
   * shell shows no panel of its own), returning the function that undoes it. Absent in a shell whose menu has no
   * such item. */
  onAboutRequested?(handler: () => void): () => void;
  /** Hands the shell the menu's labels, keyed by `menu.*` message key and already in the UI's
   * language, and the accelerator of the Settings item as a Tauri accelerator string (the user can
   * rebind it); the shell rebuilds the menu with them, keeps its English text for any label
   * missing, and its own accelerator for one that is missing or that it cannot parse. */
  setLabels(labels: Record<string, string>, settingsAccelerator?: string): Promise<void>;
}

/** What this window's daemon shows in the menu bar icon's menu, its tab (a submenu on Windows), already in the UI's
 * language; it also decides the icon's picture, through `attention` (the dot is up while any daemon raises a hand) and
 * the selected session’s quota (shown while this is the window focused last). */
export interface StatusItemMenu {
  sections: StatusItemSection[];
  /** Submenus after the sessions and before Open, such as an agent's quota; none until the UI has
   * what they list. */
  submenus: StatusItemSubmenu[];
  /** Each picture the lines name, once, by the key a line's `icon` gives. */
  icons: Record<string, StatusItemIcon>;
  open: string;
  /** Opens the update controls in Settings; names a known update even when its notice is hidden. */
  updates?: string;
  /** Quits the application, below every tab. */
  quit: string;
  /** Ends the connection to a remote daemon (closing its window), in its tab; the application and the local daemon's
   * window run on. */
  disconnect: string;
  /** The labels of what the shell's one menu bar icon shows for the application rather than for this window's daemon:
   * the local daemon's tab and the line that opens a daemon held for the browser. The shell takes them from the local
   * daemon's window. */
  app: StatusItemAppLabels;
  /** Whether the icon shows its attention dot: a live session is waiting for the user. */
  attention: boolean;
  /** Remaining quota of the selected session's account, 0–100: its 5-hour window's when it has a weekly
   * one too, else its one window's. Absent hides the icon's bar. */
  quota_remaining?: number;
  /** The weekly window's remaining quota, 0–100, drawn as a thin bar beneath the main one; present only
   * with a 5-hour window in `quota_remaining`. */
  quota_weekly_remaining?: number;
  /** Localized account, window and remaining quota for the icon's accessible name. */
  quota_label?: string;
  /** The icon's whole accessible name while the shell holds the computer awake: `quota_label`, when there is one,
   * and that the app is keeping the computer awake. The shell, which knows when it holds, uses it in place of the
   * name. */
  keep_awake_label: string;
}

/** The labels the menu bar icon's menu takes from the application rather than from one daemon. Connecting to a remote
 * daemon is not offered there: that is the Remote menu's, the Dock menu's and the jump list's. */
export interface StatusItemAppLabels {
  /** The local daemon's tab: "This Mac", "This PC". */
  this_computer: string;
  /** Opens a remote daemon held for the browser there, in its tab. */
  open_in_browser: string;
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
  /** Remaining quota of this agent's configured account in the current console, 0–100. */
  quota_remaining?: number;
  /** Weekly remainder, drawn beneath the 5-hour bar when both windows are available. */
  quota_weekly_remaining?: number;
  /** Account, quota window and remainder announced with the submenu's opening row. */
  quota_label?: string;
  items: StatusItemSubmenuItem[];
}

/** What a submenu holds. The shell draws a window's bar, so the menu carries what it needs to,
 * and a line cannot be chosen. */
export type StatusItemSubmenuItem =
  /** An account's line: `title` at the left and, right-aligned, `detail` (when its reading was taken) when
   * there is one. `status` is a separate coloured tag after the title. `label`, when given, is the whole line as one
   * phrase, for assistive technology. */
  | { kind: "header"; title: string; detail?: string; label?: string; suffix?: string; status?: { label: string; foreground_colour?: [number, number, number]; background_colour?: [number, number, number, number] } }
  /** A window's name and, right-aligned, its detail, with a bar of `fraction` (0 to 1) beneath, drawn in
   * `fill_colour`, the agent's bar colour from the UI stylesheet as sRGB bytes (the text colour when absent).
   * `used_up` marks the window of an account whose quota is used up, whose bar is drawn in `used_up_colour`
   * instead, the stylesheet's used-up grey in the same form (the secondary text colour when absent). `label` is the
   * whole line as one phrase, for assistive technology. */
  | (StatusItemWindow & StatusItemBarColours)
  /** The 5-hour window and the account-wide weekly one as one item: the 5-hour name and detail with
   * its bar of `session.fraction`, a thinner bar of `weekly.fraction` directly beneath, then the weekly name
   * and detail in the secondary colour. `used_up`, `fill_colour` and `used_up_colour` are a window's, for both
   * bars. `label` is both phrases as one, for assistive technology. */
  | (StatusItemMeter & StatusItemBarColours)
  | { kind: "note"; text: string }
  | { kind: "separator" };

export type StatusItemWindow = { kind: "window"; name: string; detail: string; label: string; fraction: number; used_up?: boolean };
export type StatusItemMeter = { kind: "meter"; session: { name: string; detail: string; fraction: number }; weekly: { name: string; detail: string; fraction: number }; label: string; used_up?: boolean };
export interface StatusItemBarColours {
  fill_colour?: [number, number, number];
  used_up_colour?: [number, number, number];
}

/** A line of a submenu before its bars are coloured: a window or a meter names its `agent`, whose colour
 * `withIcons` (`statusItem/statusItemIcons.ts`) takes from the stylesheet; the agent itself is not sent. */
export type StatusItemSubmenuDraftItem =
  | Exclude<StatusItemSubmenuItem, { kind: "window" | "meter" }>
  | ((StatusItemWindow | StatusItemMeter) & { agent: Agent });

/** A submenu with its lines as drafted. */
export type StatusItemSubmenuDraft = Omit<StatusItemSubmenu, "items"> & { items: StatusItemSubmenuDraftItem[] };

export interface StatusItemCapability {
  /** Shows or hides every icon, which is the user's Settings switch. The shell shows them from launch, so the UI
   * sends its choice at startup; hiding keeps the menus, which are there again when the icons are shown. The switch is
   * the application's, so only the local daemon's window has it; another daemon's window has an icon of its own but
   * not this. */
  setVisible?(visible: boolean): Promise<void>;
  /** Replaces the menu of this window's own menu bar icon (each daemon's window has one), and with it whether the icon
   * shows its attention dot. */
  setMenu(menu: StatusItemMenu): Promise<void>;
  /** Subscribes to a session being chosen in that menu, returning the function that undoes it. The
   * shell has already brought the window to the front. */
  onSessionChosen(handler: (sessionId: string) => void): () => void;
  /** Opens update settings, checking manually when no newer version is already known. */
  onUpdatesRequested?(handler: () => void): () => void;
  /** Subscribes to the menu beginning to open, returning the function that undoes it. A menu sent
   * while it is open is held until it closes, so what the handler asks for lands on the next one. */
  onOpened(handler: () => void): () => void;
}

/** What the Windows notification-area flyout shows, built by the shell from the menu bar icon's model, already in the
 * UI's language. */
export interface TrayFlyoutModel {
  /** At least one; the first is the local daemon's. */
  tabs: TrayFlyoutTab[];
  /** The tab chosen when the model was built. */
  chosen: number;
  /** What each tab lists (its sessions and quota rows), one list per tab in the tabs' order. */
  panels: TrayFlyoutLine[][];
  /** The lines every tab shows below its panel: Check for updates. */
  shared: TrayFlyoutLine[];
  /** What can be done with each tab's daemon (Open board, Disconnect), one list per tab in the tabs' order, below
   * the shared lines. */
  actions: TrayFlyoutLine[][];
  /** The lines every tab shows last: Quit completely. */
  footer: TrayFlyoutLine[];
}

export interface TrayFlyoutTab {
  key: string;
  label: string;
  /** A remote daemon's tab. */
  remote: boolean;
  /** The daemon has a session waiting for the user. */
  raised: boolean;
}

export type TrayFlyoutLine =
  /** A section heading, which cannot be chosen. */
  | { kind: "heading"; label: string; icon?: StatusItemIcon }
  /** A line that is chosen: its `id` is what the shell is told. */
  | { kind: "item"; id: string; label: string; icon?: StatusItemIcon }
  /** A line that cannot be chosen. */
  | { kind: "note"; label: string; icon?: StatusItemIcon }
  /** An agent's quota, opened in place to its rows. */
  | { kind: "quota"; submenu: TrayFlyoutQuota }
  | { kind: "separator" };

/** A quota submenu as the macOS menu is given it, with its picture inline. */
export interface TrayFlyoutQuota {
  title: string;
  icon?: StatusItemIcon;
  items: StatusItemSubmenuItem[];
  quota_remaining?: number;
  quota_weekly_remaining?: number;
  quota_label?: string;
}

export interface TrayFlyoutCapability {
  /** The model as it is now, or `null` while the shell has none. */
  model(): Promise<TrayFlyoutModel | null>;
  /** Subscribes to a newer model, which the shell sends only while the flyout is hidden. */
  onModel(handler: (model: TrayFlyoutModel) => void): () => void;
  /** Subscribes to the flyout being shown, with the tab to start on. */
  onOpened(handler: (opened: { chosen: number }) => void): () => void;
  /** Tells the shell the content's size in CSS pixels, to size and place the window. */
  setSize(size: { width: number; height: number }): Promise<void>;
  /** The line with `id` was chosen; the shell hides the flyout and acts. */
  choose(id: string): Promise<void>;
  /** The user dismissed the flyout from the keyboard; the shell hides it. */
  close(): Promise<void>;
}

export interface ClipboardCapability {
  /** Puts `text` on the clipboard; rejects when the environment refuses (no permission, no
   * focused document). */
  writeText(text: string): Promise<void>;
  /** Reads text only when the user chooses Paste; may ask for permission or reject. A clipboard
   * holding no text (empty, or only an image) answers `""` rather than rejecting, as the clipboard
   * a native paste hands the page does. */
  readText?(): Promise<string>;
  /** Reads an image only when the user chooses Paste, as `readText` does: a clipboard holding none answers
   * `undefined` rather than rejecting. The bytes are an image file of a format a browser decodes (PNG from a
   * desktop shell). May ask for permission or reject. */
  readImage?(): Promise<Blob | undefined>;
}

/** The release stream the user follows: `beta` is offered every release, `stable` only those without a
 * prerelease suffix (and, until the first stable release, prereleases too: see "Update channels" in
 * `docs/product/in-app-updates.md`). */
export type UpdateChannel = "stable" | "beta";

/** A newer version a check found. */
export interface AvailableUpdate {
  version: string;
  /** The oldest process holder build that still carries its sessions and shell terminals over to this version's
   * daemon: a restart into the update leaves the ones held by an older build interrupted. `null` when the shell does
   * not say, which counts every running one as interrupted. */
  holderFloor?: string | null;
}

/** How much of the update has been downloaded so far. */
export interface UpdateProgress {
  downloaded: number;
  /** The download's size, when the server said. */
  total?: number;
}

export interface UpdaterCapability {
  /** The version of the running application. */
  currentVersion(): Promise<string>;
  /** Looks for a newer version on `channel`: resolves with it, or `undefined` when the running version is the
   * newest or the channel has no release yet; rejects with an `UpdaterError`. Never replaces the running
   * application. */
  check(channel: UpdateChannel): Promise<AvailableUpdate | undefined>;
  /** Downloads and installs what the last `check` found, calling `onProgress` as the download advances.
   * Resolves once the new version is in place, which takes effect when the application is next launched; rejects
   * with an `UpdaterError`. */
  install(onProgress: (progress: UpdateProgress) => void): Promise<void>;
}

export interface BackgroundRunCapability {
  /** Tells the shell what the window's close button does: with `true` it hides the window and the
   * app keeps running behind the menu bar icon, with `false` it quits the app (through the same
   * confirmation as any quit gesture). The shell keeps the choice itself, in its own file, and starts from it, so the
   * UI sends only a change (`lifecycle/shellSettings.ts`). */
  set(enabled: boolean): Promise<void>;
}

/** Where the system's login entry for the application stands. */
export type LaunchAtLoginStatus = "enabled" | "disabled" | "requiresApproval";

export interface LaunchAtLoginCapability {
  /** The system's own state now, read from it each time: the user can change it in the system's own list. */
  status(): Promise<LaunchAtLoginStatus>;
  /** Registers or removes the login entry and resolves with where it stands afterwards (a registration the system
   * holds for the user's approval is `requiresApproval`); rejects when the system refused. */
  set(enabled: boolean): Promise<LaunchAtLoginStatus>;
  /** Opens the system's list of login items, where the user approves a registration that is `requiresApproval`.
   * Rejects when the system did not open it. */
  openSystemSettings(): Promise<void>;
}

export interface KeepAwakeCapability {
  /** Tells the shell whether the daemon is keeping the computer awake, for its menu bar icon to show. The shell holds
   * nothing itself: the daemon does (`Settings.keep_awake`), and the UI forwards the `keep_awake_held` it reports. */
  set(held: boolean): Promise<void>;
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

export interface DirectoryPickerCapability {
  /** Opens at `initialPath` (absolute, `~` or beneath it), or its nearest existing ancestor. Resolves with the
   * chosen absolute path, or `undefined` when dismissed; rejects when the shell cannot open the picker. */
  pick(options: { title: string; initialPath: string }): Promise<string | undefined>;
}

export interface LinkCapability {
  /** Opens `url`, an `http:`, `https:` or `mailto:` address, outside the app. Only the desktop
   * platform reports a refusal, by rejecting: a browser cannot tell a blocked popup from an opened
   * one, and resolves either way. In a remote window an address on the daemon machine's own loopback is opened at
   * a port of this computer forwarded to it, and one that could not be forwarded rejects with a `LinkForwardError`. */
  open(url: string): Promise<void>;
}

export type MacosPermissionKind = "screenRecording" | "accessibility";

export type MacosPermissionsStatus = Record<MacosPermissionKind, boolean>;

export interface MacosPermissionsCapability {
  /** The current grants, or `undefined` where the shell is not running on macOS. Only reads, and
   * never shows a system prompt, so it is safe to poll. */
  status(): Promise<MacosPermissionsStatus | undefined>;
  /** The one call that shows the system prompt, with whether the permission is granted as of the
   * call. Neither waits for the user's answer: both prompts are raised asynchronously and neither
   * permission is granted in place, so a first request resolves `false` at once, while the prompt is
   * still up. */
  request(kind: MacosPermissionKind): Promise<boolean>;
  /** Opens the System Settings pane for `kind`. Rejects when the system did not open it. */
  openSystemSettings(kind: MacosPermissionKind): Promise<void>;
}

/** A port of this computer that forwards to a loopback port of the remote daemon's machine. */
export interface RemoteForward {
  remotePort: number;
  localPort: number;
  /** The address last opened through the forward, as it was written (on the remote machine). */
  address: string;
  /** What the browser was given for it (on this computer). */
  localAddress: string;
}

export interface RemoteForwardsCapability {
  /** The forwards the window holds, in the order they were made. */
  list(): Promise<RemoteForward[]>;
  /** Lets the port of the forward to `remotePort` go and cuts the connections passing through it. */
  stop(remotePort: number): Promise<void>;
  /** Calls `listener` when a forward was made, stopped or opened at another address, for the caller to list again.
   * Returns the unsubscribe. */
  onChange(listener: () => void): () => void;
}

/** A copy of a remote file, kept by the shell in a folder of its own and removed at the next launch, which the page
 * fills in parts it read from the daemon. Each call rejects with a `RemoteFileError` where the shell says why. */
export interface RemoteFilesCapability {
  /** Begins a copy named `name` of `size` bytes and gives its id. `launchable` is the daemon's word for the file; the
   * shell judges the name as well, and refuses what this computer would run before it stores anything. */
  begin(file: { name: string; size: number; launchable: boolean }): Promise<string>;
  /** Appends `part`, handed over as the raw body of the call behind the copy's id, to the copy. */
  write(id: string, part: Uint8Array): Promise<void>;
  /** Opens the copy, which has to hold all `size` bytes, with this computer's default application, marked as
   * downloaded. The copy stays; the next launch removes it. */
  open(id: string): Promise<void>;
  /** Removes a copy that was cancelled or failed. Never rejects: what could not be removed goes at the next launch. */
  discard(id: string): Promise<void>;
}

/** This computer's SSH server: Remote Login on macOS, OpenSSH Server on Windows. */
export interface RemoteLoginCapability {
  /** Whether the SSH server is on (Remote Login on, or the OpenSSH Server service running), or `undefined` where the
   * shell cannot tell. Only reads. */
  status(): Promise<boolean | undefined>;
  /** Opens the system settings that turn it on: macOS System Settings at General, Sharing; Windows Settings at System,
   * Optional features. Rejects when the system did not open them. */
  openSystemSettings(): Promise<void>;
}

/** How the shell signs in to the server: the user's password, a key file on this computer, or this device's own key,
 * which the shell installs on the server after the first sign-in with one of the others and keeps in the system's
 * credential store. An entry is switched to "device" by the shell alone. */
export type RemoteDaemonAuth = "password" | "key" | "device";

/** One remote daemon the shell can connect to, as the shell keeps it. The password or passphrase is never part of it.
 */
export interface RemoteDaemonEntry {
  /** Assigned by the shell. */
  id: string;
  /** What the window of the connection is called. */
  name: string;
  host: string;
  /** The SSH port. */
  port: number;
  user: string;
  auth: RemoteDaemonAuth;
  /** The private key file on this computer, for `auth` "key". */
  keyPath: string | null;
  /** The pinned host key fingerprint (`SHA256:…`, as OpenSSH writes it); null until a first connection confirms one. */
  fingerprint: string | null;
  /** Where `octoboardd` is on the remote computer, when it is not found by itself. */
  octoboarddPath: string | null;
}

/** An entry to save: `id` is null for a new one. */
export type RemoteDaemonEntryInput = Omit<RemoteDaemonEntry, "id"> & { id: string | null };

export interface RemoteDaemonConnectRequest {
  id: string;
  /** The password (`auth` "password", or "device" to sign in again and reinstall the device key) or the key's
   * passphrase (`auth` "key"); the shell never stores it. */
  secret: string | null;
  /** The fingerprint the owner has just confirmed for a host with none pinned. */
  confirmFingerprint: string | null;
  /** What to do on the server before connecting: start its daemon. Nothing here updates it, which is done on the
   * server. */
  action: "start" | null;
  /** Hold the connection without opening a window, and have the shell open the daemon's own web interface in the system
   * browser (`opened_in_browser`), which skips the check that its version matches this application's. */
  browser: boolean;
}

/** How a connection attempt ended. */
export type RemoteDaemonConnectOutcome =
  /** The window is open, or the one already open for the entry was focused. */
  | { outcome: "opened" }
  /** The connection is held for the daemon's own web interface, which the shell has opened in the system browser (the
   * address carries the owner's key, so it never reaches the page); a shell that cannot open it answers `failed`.
   * Pressing the entry again opens it again. */
  | { outcome: "opened_in_browser" }
  | { outcome: "secret_needed"; kind: "password" | "passphrase" }
  | { outcome: "host_key_unknown"; fingerprint: string }
  | { outcome: "host_key_mismatch"; expected: string; actual: string }
  | { outcome: "unreachable"; detail: string }
  | { outcome: "auth_failed"; detail: string }
  /** The server no longer accepts this device's key; signing in with the password installs it again. */
  | { outcome: "device_key_refused" }
  | { outcome: "key_unreadable"; detail: string }
  | { outcome: "octoboardd_not_found"; tried: string[] }
  | { outcome: "no_daemon" }
  /** The host's `octoboardd` is the desktop application's, whose daemon runs only while the application does, and the
   * application is not running there. `detail` may be empty. */
  | { outcome: "application_not_running"; detail: string }
  | { outcome: "no_owner_endpoint"; detail: string }
  | { outcome: "endpoint_unreachable"; detail: string }
  | { outcome: "generation_mismatch"; remoteGeneration: number; localGeneration: number; remoteVersion: string; localVersion: string }
  | { outcome: "action_failed"; action: "start"; detail: string }
  | { outcome: "failed"; detail: string };

/** Whether an entry's window is open and its connection up, or being brought back after it dropped. An entry that
 * has no window is not connected, and is not reported. */
export type RemoteDaemonConnectionState = "connected" | "reconnecting";

export interface RemoteDaemonStatus {
  id: string;
  state: RemoteDaemonConnectionState;
  /** What holds the connection: the entry's window, or the system browser (`opened_in_browser`). */
  via: "window" | "browser";
}

/** An outcome of a connection to the entry `id` that the Remote devices page has to show: a question or a failure. */
export interface RemoteDaemonHandedOutcome {
  id: string;
  outcome: RemoteDaemonConnectOutcome;
}

export interface RemoteDaemonsCapability {
  list(): Promise<RemoteDaemonEntry[]>;
  /** The entries whose window is open; any other entry is not connected. */
  status(): Promise<RemoteDaemonStatus[]>;
  /** Calls `listener` when the list or a connection changed (an entry saved or removed, a remote window opened or
   * closed, a connection dropped or back), for the caller to read both again. Returns the unsubscribe. */
  onChanged(listener: () => void): () => void;
  /** Saves a new entry or changes one; rejects with the shell's reason when the entry is not valid. */
  save(entry: RemoteDaemonEntryInput): Promise<RemoteDaemonEntry>;
  remove(id: string): Promise<void>;
  /** Connects and opens the window; resolves with how it ended, and may take seconds. Rejects only for a failure of the
   * command itself. The shell saves the pinned fingerprint itself once a confirmed first connection has authenticated.
   */
  connect(request: RemoteDaemonConnectRequest): Promise<RemoteDaemonConnectOutcome>;
  /** Ends a connection held for the browser, releasing its port and SSH connection, or closes the daemon's window,
   * which ends its connection. */
  disconnect(id: string): Promise<void>;
  /** Calls `listener` with what a connection asked for away from the Remote devices page (a menu's saved daemon) left
   * for it to show, those already waiting included; only the main window is given any. Returns the unsubscribe. */
  onHandedOver(listener: (items: RemoteDaemonHandedOutcome[]) => void): () => void;
  /** Calls `listener` when the Remote devices page is asked for (the Remote Devices menu item), also for a request made
   * before this page listened; only the main window is given any. Returns the unsubscribe. */
  onPageRequested(listener: () => void): () => void;
  /** Shows and focuses this computer's own window. */
  showMainWindow(): Promise<void>;
  /** Shows this computer's own window with Settings open on Updates, as the menu bar icon's Check for updates does. */
  showUpdates(): Promise<void>;
}

/** Picks the implementation once, at startup, from the environment the page is running in. */
export function selectPlatform(): PlatformAdapter {
  return inTauri() ? tauriPlatform() : browserPlatform();
}
