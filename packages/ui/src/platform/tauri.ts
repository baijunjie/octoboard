import { ConfigurationFileError } from "./configurationFileError";
import type {
  AppMenuCapability,
  JumpListCapability,
  BackgroundRunCapability,
  BadgeCapability,
  ClipboardCapability,
  ConfigurationFileCapability,
  DirectoryPickerCapability,
  ExitCapability,
  KeepAwakeCapability,
  LaunchAtLoginCapability,
  LaunchAtLoginStatus,
  LinkCapability,
  MacosPermissionsCapability,
  MacosPermissionsStatus,
  NativeWindowCapability,
  NotificationCapability,
  OpenedConfigurationFile,
  PlatformAdapter,
  RemoteDaemonEntry,
  RemoteDaemonsCapability,
  RemoteDaemonStatus,
  RemoteLoginCapability,
  StatusItemCapability,
  TrayFlyoutCapability,
  TrayFlyoutModel,
  UpdaterCapability,
  UpdateProgress,
  WindowChromeCapability,
} from "./index";
import { tauriHost, type HostFacts } from "./host";
import { UpdaterError } from "./updaterError";
import { windowKind } from "./windowKind";

// Every Tauri module is loaded with a dynamic `import()` and only from here, so a plain browser
// never fetches them.

export function tauriPlatform(): PlatformAdapter {
  const host = tauriHost();
  // The window of a daemon elsewhere neither quits the application nor stops its daemon, and offers nothing of this
  // machine's own to act on the daemon's files or grants; the shell gives it none of the commands (its host facts drop
  // the application's own features already).
  const kind = windowKind();
  const local = kind === "local";
  return {
    kind: "tauri",
    host,
    exit: local ? tauriExit() : undefined,
    notifications: tauriNotifications(host),
    badge: host.badge ? tauriBadge() : undefined,
    nativeWindow: tauriNativeWindow(),
    windowChrome: host.chrome === "overlay-left" ? tauriWindowChrome(host.trafficLightInset ?? 0) : undefined,
    translucentWindow: host.translucent,
    appMenu: host.appMenu ? tauriAppMenu() : undefined,
    jumpList: host.os === "windows" ? tauriJumpList() : undefined,
    statusItem: host.statusItem === "none" ? undefined : tauriStatusItem(local),
    trayFlyout: kind === "flyout" ? tauriTrayFlyout() : undefined,
    clipboard: tauriClipboard(),
    configurationFile: tauriConfigurationFile(),
    directoryPicker: local ? tauriDirectoryPicker() : undefined,
    links: tauriLinks(),
    macosPermissions: local ? tauriMacosPermissions() : undefined,
    // This computer's own SSH server, which only the local daemon's window is about.
    remoteLogin: local ? tauriRemoteLogin() : undefined,
    // Every window of the application reaches daemons elsewhere: the connect page is where one is chosen, and a remote
    // daemon's window lists and opens its siblings from its menu.
    remoteDaemons: tauriRemoteDaemons(),
    backgroundRun: host.runInBackground ? tauriBackgroundRun() : undefined,
    keepAwake: host.keepAwake ? tauriKeepAwake() : undefined,
    launchAtLogin: host.launchAtLogin ? tauriLaunchAtLogin() : undefined,
    updater: host.updater === "none" ? undefined : tauriUpdater(),
  };
}

/** Collects the unsubscribe functions of listeners registered after an `await`, so the whole
 * registration can be undone before any of them exists. `track` unsubscribes at once when the
 * registration is already undone, which is what keeps an undo during that window from leaking a
 * listener: a leaked one fires for the rest of the window's life. */
function unlistenTracker(): { track: (unlisten: () => void) => void; undo: () => void } {
  let cancelled = false;
  const unlisteners: Array<() => void> = [];
  return {
    track(unlisten) {
      if (cancelled) unlisten();
      else unlisteners.push(unlisten);
    },
    undo() {
      cancelled = true;
      for (const unlisten of unlisteners) unlisten();
      unlisteners.length = 0;
    },
  };
}

/** Listens to the shell's `event` until the returned function is called, even when that is before the
 * listener exists (see `unlistenTracker`). `handler` gets the event's payload. */
function listenTo<T = undefined>(event: string, handler: (payload: T) => void): () => void {
  const { track, undo } = unlistenTracker();
  void listenHere<T>(event, (payload) => handler(payload)).then(track);
  return undo;
}

/** Listens to the shell's `event` as sent to this window, or to every window. The shell sends what is one window's to
 * that window alone (`emit_to`), and a listener that names no window would hear what is sent to any other too. */
async function listenHere<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  return getCurrentWebviewWindow().listen<T>(event, (e) => handler(e.payload));
}

/** The `{ kind, detail? }` a shell command rejects with, when it did. */
function shellFailure(err: unknown): { kind?: string; detail?: string } {
  return (typeof err === "object" && err !== null ? err : {}) as { kind?: string; detail?: string };
}

/** Present only where the shell hides the native titlebar and floats the window controls over the page
 * (`chrome: "overlay-left"`, the traffic lights on macOS: `decorate` in
 * `apps/desktop/src-tauri/src/chrome/macos.rs`); `controlsInset` is what they cover at the bar's leading edge, as the shell
 * reports it. They leave the bar in fullscreen, and with them the inset. */
function tauriWindowChrome(controlsInset: number): WindowChromeCapability {
  let inset = controlsInset;
  const callbacks = new Set<() => void>();
  return {
    leftInset: () => inset,
    subscribe(callback) {
      callbacks.add(callback);
      const { track, undo } = unlistenTracker();
      void import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
        const win = getCurrentWindow();
        const sync = async () => {
          const next = (await win.isFullscreen()) ? 0 : controlsInset;
          if (next === inset) return;
          inset = next;
          for (const notify of callbacks) notify();
        };
        track(await win.onResized(() => void sync()));
        void sync();
      });
      return () => {
        callbacks.delete(callback);
        undo();
      };
    },
  };
}

function tauriAppMenu(): AppMenuCapability {
  return {
    onSettingsRequested(handler) {
      return listenTo("settings-requested", () => handler());
    },
    onAboutRequested(handler) {
      return listenTo("about-requested", () => handler());
    },
    async setLabels(labels, settingsAccelerator) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_menu_labels", { labels, settingsAccelerator });
    },
  };
}

function tauriJumpList(): JumpListCapability {
  return {
    async setLabels(labels) {
      const { invoke } = await import("@tauri-apps/api/core");
      // The menu's command: the Windows shell has no menu bar and keeps them for the jump list.
      await invoke("set_menu_labels", { labels, settingsAccelerator: null });
    },
  };
}

/** The menu bar icon of this window's daemon: the shell gives each daemon's window an icon of its own, whose menu this
 * window fills and whose choices reach this window alone. Whether the icons show is the application's setting, which
 * only the local daemon's window (`local`) offers. */
function tauriStatusItem(local: boolean): StatusItemCapability {
  return {
    ...(local && {
      async setVisible(visible: boolean) {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("set_app_settings", { settings: { show_menu_bar_icon: visible } });
      },
    }),
    async setMenu(menu) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_tray_menu", { menu });
    },
    onSessionChosen(handler) {
      return listenTo<string>("tray-session-chosen", handler);
    },
    onUpdatesRequested(handler) {
      return listenTo("tray-updates-requested", () => handler());
    },
    onOpened(handler) {
      return listenTo("tray-menu-opened", () => handler());
    },
  };
}

/** The notification-area flyout's page: the shell holds its model and acts on what is chosen, and sizes the window to
 * what the page reports. */
function tauriTrayFlyout(): TrayFlyoutCapability {
  return {
    async model() {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<TrayFlyoutModel | null>("tray_flyout_model");
    },
    onModel(handler) {
      return listenTo<TrayFlyoutModel>("tray-flyout-model", handler);
    },
    onOpened(handler) {
      return listenTo<{ chosen: number }>("tray-flyout-opened", handler);
    },
    async setSize(size) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("tray_flyout_size", size);
    },
    async choose(id) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("tray_flyout_choose", { id });
    },
    async close() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("tray_flyout_close");
    },
  };
}

function tauriExit(): ExitCapability {
  return {
    register({ onQuitRequested, onDaemonExited }) {
      const { track, undo } = unlistenTracker();

      (async () => {
        const { invoke } = await import("@tauri-apps/api/core");
        // Tells the Rust side a confirmation flow actually exists now, so it starts asking before
        // letting an exit through instead of defaulting to letting every exit straight through.
        // `heartbeat` re-invokes the same command on every quit gesture handled thereafter, which is what
        // clears the Rust side's force-quit debounce (see `frontend_exit_heartbeat` in
        // `apps/desktop/src-tauri/src/exit.rs`).
        await invoke("frontend_exit_heartbeat");

        // No close listener: the shell answers the window's close button itself, by hiding the
        // window or taking it as a quit gesture (see `backgroundRun`), which a listener here would
        // stop it from doing.
        track(await listenHere("exit-requested", () => onQuitRequested()));
        // The shell gives up on the daemon once, for good, so this page reports that once: from the
        // event, or from asking the shell below. The event reaches only a page already listening,
        // and a page that loads while the daemon dies again — the window pointed at a restarted
        // daemon that ended at once — would otherwise never hear of it. Undoing the registration
        // counts as reported, so a stale answer raises nothing.
        let reported = false;
        const daemonExited = (detail: string) => {
          if (reported) return;
          reported = true;
          onDaemonExited(detail);
        };
        track(() => {
          reported = true;
        });
        track(await listenHere<string>("daemon-exited", daemonExited));
        const missed = await invoke<string | null>("daemon_exit_detail").catch(() => null);
        if (missed !== null) daemonExited(missed);
      })();

      return undo;
    },
    async heartbeat() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("frontend_exit_heartbeat");
    },
    async beginQuit() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("begin_quit");
    },
    async confirmQuit({ restart } = {}) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("confirm_quit", { restart });
    },
  };
}

/** Cached across every call so the plugin is asked once per process rather than once per waiting
 * session. On macOS the plugin answers "granted" without showing any prompt. Only a granted answer
 * is cached: caching `false` would leave notifications off for the rest of the process even after
 * the user grants the permission later. */
let permissionRequest: Promise<boolean> | undefined;

function tauriNotifications(host: HostFacts): NotificationCapability {
  return {
    ensurePermission() {
      if (!permissionRequest) {
        permissionRequest = (async () => {
          const { isPermissionGranted, requestPermission } = await import("@tauri-apps/plugin-notification");
          if (await isPermissionGranted()) return true;
          return (await requestPermission()) === "granted";
        })().catch(() => false);
        void permissionRequest.then((granted) => {
          if (!granted) permissionRequest = undefined;
        });
      }
      return permissionRequest;
    },
    async notify({ title, body }) {
      // Through the shell, which marks the notification with this window, so a click on it brings this window forward.
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("show_notification", { title, body });
    },
    ...(host.notificationSettings === "system-pane" && {
      async openSystemSettings() {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("open_notification_settings");
      },
    }),
  };
}

/** How the clipboard plugin rejects a read when the clipboard holds nothing in the format asked for (no text, no
 * image): it passes on only the message of `arboard::Error::ContentNotAvailable`, which names no kind to tell it by.
 * The text follows the arboard release `Cargo.lock` resolves: check it whenever that changes. */
const NOTHING_ON_CLIPBOARD = "The clipboard contents were not available in the requested format or the clipboard is empty.";

/** Through the shell's plugin, not `navigator.clipboard`: the web API is only offered to a secure
 * context, and the packaged window's custom scheme is not reliably counted as one. */
function tauriClipboard(): ClipboardCapability {
  return {
    async readText() {
      const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
      try {
        return await readText();
      } catch (err) {
        if (String(err) === NOTHING_ON_CLIPBOARD) return "";
        throw err;
      }
    },
    async readImage() {
      const { readImage } = await import("@tauri-apps/plugin-clipboard-manager");
      let image;
      try {
        image = await readImage();
      } catch (err) {
        if (String(err) === NOTHING_ON_CLIPBOARD) return undefined;
        throw err;
      }
      try {
        const [{ width, height }, rgba] = await Promise.all([image.size(), image.rgba()]);
        return await pngOfRgba(rgba, width, height);
      } finally {
        // The image is a resource on the shell's side, which holds its pixels until it is closed.
        void image.close().catch(() => {});
      }
    },
    async writeText(text) {
      const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
      await writeText(text);
    },
  };
}

/** The PNG of an image's raw RGBA pixels, which is what the plugin reads an image as. */
async function pngOfRgba(rgba: Uint8Array, width: number, height: number): Promise<Blob> {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("no canvas to draw the clipboard image on");
  context.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
  return new Promise((resolve, reject) =>
    canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error("the clipboard image could not be encoded"))), "image/png"),
  );
}

/** Through the shell's own commands (`configuration_file.rs`), which raise the system's dialogs and
 * read or write the file the user picked: the page is never given a path, so it can touch nothing
 * else. The shell answers `null` for a dismissed dialog and rejects with `{ kind, detail? }`. */
function tauriConfigurationFile(): ConfigurationFileCapability {
  const failure = (err: unknown): ConfigurationFileError => {
    const { kind, detail } = shellFailure(err);
    if (kind === "too_large" || kind === "not_text") return new ConfigurationFileError(kind);
    return new ConfigurationFileError("io", detail ?? String(err));
  };
  return {
    async save(suggestedName, text) {
      const { invoke } = await import("@tauri-apps/api/core");
      const name = await invoke<string | null>("save_configuration_file", { suggestedName, text }).catch((err) => {
        throw failure(err);
      });
      return name === null ? undefined : { kind: "saved", name };
    },
    async open() {
      const { invoke } = await import("@tauri-apps/api/core");
      const opened = await invoke<OpenedConfigurationFile | null>("open_configuration_file").catch((err) => {
        throw failure(err);
      });
      return opened ?? undefined;
    },
  };
}

function tauriDirectoryPicker(): DirectoryPickerCapability {
  return {
    async pick({ title, initialPath }) {
      const { invoke } = await import("@tauri-apps/api/core");
      const picked = await invoke<string | null>("pick_directory", { title, initialPath }).catch((err: unknown) => {
        throw new Error(shellFailure(err).detail ?? (err instanceof Error ? err.message : String(err)));
      });
      return picked ?? undefined;
    },
  };
}

/** Through the shell plugin's `open`, which hands the address to the system's default handler and
 * only accepts `http:`, `https:`, `mailto:` and `tel:` ones (`plugins.shell.open` in
 * `tauri.conf.json`). */
function tauriLinks(): LinkCapability {
  return {
    async open(url) {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open(url);
    },
  };
}

/** Present whatever the OS: the shell's `status` answers `undefined` off macOS, which is how that
 * is reported. */
function tauriMacosPermissions(): MacosPermissionsCapability {
  return {
    async status() {
      const { invoke } = await import("@tauri-apps/api/core");
      return (await invoke<MacosPermissionsStatus | null>("permissions_status")) ?? undefined;
    },
    async request(kind) {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<boolean>("request_permission", { kind });
    },
    async openSystemSettings(kind) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("open_privacy_settings", { kind });
    },
  };
}

/** Present whatever the OS: the shell reads Remote Login on macOS and the OpenSSH Server service on Windows, and its
 * `status` answers `undefined` where it cannot tell. */
function tauriRemoteLogin(): RemoteLoginCapability {
  return {
    async status() {
      const { invoke } = await import("@tauri-apps/api/core");
      return (await invoke<boolean | null>("remote_login_status")) ?? undefined;
    },
    async openSystemSettings() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("open_remote_login_settings");
    },
  };
}

function tauriRemoteDaemons(): RemoteDaemonsCapability {
  return {
    async list() {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<RemoteDaemonEntry[]>("remote_daemons_list");
    },
    async status() {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<RemoteDaemonStatus[]>("remote_daemons_status");
    },
    onChanged: (listener) => listenTo("remote-daemons-changed", listener),
    async save(entry) {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<RemoteDaemonEntry>("remote_daemon_save", { entry });
    },
    async remove(id) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("remote_daemon_remove", { id });
    },
    async connect(request) {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke("remote_daemon_connect", { request });
    },
    async disconnect(id) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("remote_daemon_disconnect", { id });
    },
    async newWindow() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("remote_window_new");
    },
    async openWindow(id) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("remote_window_open", { id });
    },
    async showMainWindow() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("main_window_show");
    },
    // The main window answers the event the menu bar icon's Check for updates sends it (`onUpdatesRequested`).
    async showUpdates() {
      const [{ invoke }, { emitTo }] = await Promise.all([import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]);
      await invoke("main_window_show");
      await emitTo("main", "tray-updates-requested");
    },
  };
}

function tauriBackgroundRun(): BackgroundRunCapability {
  return {
    async set(enabled) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_app_settings", { settings: { run_in_background: enabled } });
    },
  };
}

function tauriLaunchAtLogin(): LaunchAtLoginCapability {
  return {
    async status() {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<LaunchAtLoginStatus>("launch_at_login_status");
    },
    async set(enabled) {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<LaunchAtLoginStatus>("set_launch_at_login", { enabled });
    },
    async openSystemSettings() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("open_login_items_settings");
    },
  };
}

function tauriKeepAwake(): KeepAwakeCapability {
  return {
    async set(held) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_keep_awake_shown", { held });
    },
  };
}

/** Through the shell's own commands (`updater/`), which carry a channel's name in and a version and the
 * download's progress out. The shell rejects with `{ kind, detail? }`. */
function tauriUpdater(): UpdaterCapability {
  const failure = (err: unknown): UpdaterError => {
    const { kind, detail } = shellFailure(err);
    if (kind === "busy" || kind === "installed" || kind === "nothing_to_install") return new UpdaterError(kind);
    return new UpdaterError("failed", detail ?? (err instanceof Error ? err.message : typeof err === "string" ? err : ""));
  };
  return {
    async currentVersion() {
      const { getVersion } = await import("@tauri-apps/api/app");
      return getVersion();
    },
    async check(channel) {
      const { invoke } = await import("@tauri-apps/api/core");
      const found = await invoke<{ version: string; holder_floor?: string | null } | null>("check_for_update", { channel }).catch((err) => {
        throw failure(err);
      });
      return found ? { version: found.version, holderFloor: found.holder_floor ?? null } : undefined;
    },
    async install(onProgress) {
      const { Channel, invoke } = await import("@tauri-apps/api/core");
      const progress = new Channel<{ downloaded: number; total: number | null }>();
      // Ticks the shell sends after the call has settled are not progress of anything.
      let settled = false;
      progress.onmessage = ({ downloaded, total }) => {
        if (!settled) onProgress({ downloaded, total: total ?? undefined } satisfies UpdateProgress);
      };
      await invoke("install_update", { onProgress: progress })
        .catch((err) => {
          throw failure(err);
        })
        .finally(() => {
          settled = true;
        });
    },
  };
}

/** Through the shell, which shows the sum of every daemon window's count on the Dock tile. */
function tauriBadge(): BadgeCapability {
  return {
    async set(count) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_badge_count", { count: Math.max(0, Math.round(count)) });
    },
  };
}

function tauriNativeWindow(): NativeWindowCapability {
  return {
    async setTheme(theme) {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      // Passing no theme is what tells the window to resume following the OS appearance; on
      // macOS this sets (or, with none, clears) the app-level `NSAppearance` (tao calls
      // `NSApp setAppearance:`), so the web view's `prefers-color-scheme` and the native menu both
      // follow it, rather than freezing at whatever the OS happened to be at the moment of the call.
      await getCurrentWindow().setTheme(theme ?? null);
    },
    async reveal() {
      // The shell's own command rather than the window API: it keeps the window hidden after a launch at login
      // that runs in the background.
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("reveal_main_window");
    },
    async bringToFront() {
      // The shell's own command rather than the window API, since a window hidden in the
      // background also needs the app put back in the Dock.
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("bring_to_front");
    },
    async isVisible() {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      return getCurrentWindow().isVisible();
    },
  };
}
