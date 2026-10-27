import { ConfigurationFileError } from "./configurationFileError";
import type {
  AppMenuCapability,
  BackgroundRunCapability,
  BadgeCapability,
  ClipboardCapability,
  ConfigurationFileCapability,
  ExitCapability,
  LinkCapability,
  MacosPermissionsCapability,
  MacosPermissionsStatus,
  NativeWindowCapability,
  NotificationCapability,
  OpenedConfigurationFile,
  PlatformAdapter,
  StatusItemCapability,
  UpdaterCapability,
  UpdateProgress,
  WindowChromeCapability,
} from "./index";
import { UpdaterError } from "./updaterError";

// Every Tauri module is loaded with a dynamic `import()` and only from here, so a plain browser
// never fetches them.

export function tauriPlatform(): PlatformAdapter {
  return {
    kind: "tauri",
    exit: tauriExit(),
    notifications: tauriNotifications(),
    badge: tauriBadge(),
    nativeWindow: tauriNativeWindow(),
    windowChrome: tauriWindowChrome(),
    translucentWindow: onMac(),
    appMenu: tauriAppMenu(),
    statusItem: tauriStatusItem(),
    clipboard: tauriClipboard(),
    configurationFile: tauriConfigurationFile(),
    links: tauriLinks(),
    macosPermissions: tauriMacosPermissions(),
    backgroundRun: tauriBackgroundRun(),
    updater: tauriUpdater(),
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

/** The shell hides the native titlebar and puts a translucent material behind the page on macOS
 * alone (`open_main_window` in `apps/desktop/src-tauri/src/lib.rs`). */
function onMac(): boolean {
  return navigator.userAgent.includes("Mac");
}

/** What the traffic lights cover at the bar's leading edge: `TRAFFIC_LIGHT_X` in
 * `apps/desktop/src-tauri/src/lib.rs` plus three buttons and a gap. */
const TRAFFIC_LIGHT_INSET = 80;

/** Present only on macOS, where `lib.rs` hides the native titlebar and floats the traffic lights
 * over the page; they leave the bar in fullscreen, and with them the inset. */
function tauriWindowChrome(): WindowChromeCapability | undefined {
  if (!onMac()) return undefined;
  let inset = TRAFFIC_LIGHT_INSET;
  const callbacks = new Set<() => void>();
  return {
    leftInset: () => inset,
    subscribe(callback) {
      callbacks.add(callback);
      const { track, undo } = unlistenTracker();
      void import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
        const win = getCurrentWindow();
        const sync = async () => {
          const next = (await win.isFullscreen()) ? 0 : TRAFFIC_LIGHT_INSET;
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
      const { track, undo } = unlistenTracker();
      void import("@tauri-apps/api/event").then(async ({ listen }) => {
        track(await listen("settings-requested", () => handler()));
      });
      return undo;
    },
    onAboutRequested(handler) {
      const { track, undo } = unlistenTracker();
      void import("@tauri-apps/api/event").then(async ({ listen }) => {
        track(await listen("about-requested", () => handler()));
      });
      return undo;
    },
    async setLabels(labels, settingsAccelerator) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_menu_labels", { labels, settingsAccelerator });
    },
  };
}

function tauriStatusItem(): StatusItemCapability {
  return {
    async setMenu(menu) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_tray_menu", { menu });
    },
    onSessionChosen(handler) {
      const { track, undo } = unlistenTracker();
      void import("@tauri-apps/api/event").then(async ({ listen }) => {
        track(await listen<string>("tray-session-chosen", (event) => handler(event.payload)));
      });
      return undo;
    },
    onOpened(handler) {
      const { track, undo } = unlistenTracker();
      void import("@tauri-apps/api/event").then(async ({ listen }) => {
        track(await listen("tray-menu-opened", () => handler()));
      });
      return undo;
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
        // letting an exit through instead of defaulting to letting every exit straight through —
        // called from every screen that registers, including the daemon-failed-to-start one, so a
        // window stuck there is always still quittable. `heartbeat` re-invokes the same command on
        // every quit gesture handled thereafter, which is what clears the Rust side's force-quit
        // debounce (see `frontend_exit_heartbeat` in `apps/desktop/src-tauri/src/exit.rs`).
        await invoke("frontend_exit_heartbeat");

        // No close listener: the shell answers the window's close button itself, by hiding the
        // window or taking it as a quit gesture (see `backgroundRun`), which a listener here would
        // stop it from doing.
        const { listen } = await import("@tauri-apps/api/event");
        track(await listen("exit-requested", () => onQuitRequested()));
        track(await listen<string>("daemon-exited", (event) => onDaemonExited(event.payload)));
      })();

      return undo;
    },
    async heartbeat() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("frontend_exit_heartbeat");
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

function tauriNotifications(): NotificationCapability {
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
    async notify(notification) {
      const { sendNotification } = await import("@tauri-apps/plugin-notification");
      sendNotification(notification);
    },
    ...(onMac() && {
      async openSystemSettings() {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("macos_open_notification_settings");
      },
    }),
  };
}

/** Through the shell's plugin, not `navigator.clipboard`: the web API is only offered to a secure
 * context, and the packaged window's custom scheme is not reliably counted as one. */
function tauriClipboard(): ClipboardCapability {
  return {
    async readText() {
      const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
      return readText();
    },
    async writeText(text) {
      const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
      await writeText(text);
    },
  };
}

/** Through the shell's own commands (`configuration_file.rs`), which raise the system's dialogs and
 * read or write the file the user picked: the page is never given a path, so it can touch nothing
 * else. The shell answers `null` for a dismissed dialog and rejects with `{ kind, detail? }`. */
function tauriConfigurationFile(): ConfigurationFileCapability {
  const failure = (err: unknown): ConfigurationFileError => {
    const { kind, detail } = (typeof err === "object" && err !== null ? err : {}) as { kind?: string; detail?: string };
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
      return (await invoke<MacosPermissionsStatus | null>("macos_permissions_status")) ?? undefined;
    },
    async request(kind) {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<boolean>("macos_request_permission", { kind });
    },
    async openSystemSettings(kind) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("macos_open_privacy_settings", { kind });
    },
  };
}

function tauriBackgroundRun(): BackgroundRunCapability {
  return {
    async set(enabled) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_run_in_background", { enabled });
    },
  };
}

/** Through the shell's own commands (`updater.rs`), which carry a channel's name in and a version, its notes and
 * the download's progress out. The shell rejects with `{ kind, detail? }`. */
function tauriUpdater(): UpdaterCapability {
  const failure = (err: unknown): UpdaterError => {
    const { kind, detail } = (typeof err === "object" && err !== null ? err : {}) as { kind?: string; detail?: string };
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
      const found = await invoke<{ version: string; notes: string | null } | null>("check_for_update", { channel }).catch((err) => {
        throw failure(err);
      });
      return found ? { version: found.version, notes: found.notes ?? undefined } : undefined;
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

function tauriBadge(): BadgeCapability {
  return {
    async set(count) {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().setBadgeCount(count > 0 ? count : undefined);
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
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().show();
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
