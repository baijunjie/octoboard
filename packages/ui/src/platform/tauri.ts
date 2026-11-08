import { ConfigurationFileError } from "./configurationFileError";
import type {
  AppMenuCapability,
  BackgroundRunCapability,
  BadgeCapability,
  ClipboardCapability,
  ConfigurationFileCapability,
  DirectoryPickerCapability,
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
  WindowFocusCapability,
} from "./index";
import { tauriHost, type HostFacts } from "./host";
import { UpdaterError } from "./updaterError";

// Every Tauri module is loaded with a dynamic `import()` and only from here, so a plain browser
// never fetches them.

export function tauriPlatform(): PlatformAdapter {
  const host = tauriHost();
  return {
    kind: "tauri",
    host,
    exit: tauriExit(),
    notifications: tauriNotifications(host),
    badge: host.badge ? tauriBadge() : undefined,
    nativeWindow: tauriNativeWindow(),
    windowChrome: host.chrome === "overlay-left" ? tauriWindowChrome(host.trafficLightInset ?? 0) : undefined,
    windowFocus: tauriWindowFocus(),
    translucentWindow: host.translucent,
    appMenu: host.appMenu ? tauriAppMenu() : undefined,
    statusItem: host.statusItem === "none" ? undefined : tauriStatusItem(),
    clipboard: tauriClipboard(),
    configurationFile: tauriConfigurationFile(),
    directoryPicker: tauriDirectoryPicker(),
    links: tauriLinks(),
    macosPermissions: tauriMacosPermissions(),
    backgroundRun: host.runInBackground ? tauriBackgroundRun() : undefined,
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
  void import("@tauri-apps/api/event").then(async ({ listen }) => {
    track(await listen<T>(event, (e) => handler(e.payload)));
  });
  return undo;
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

/** From the window's own focus events (`WindowEvent::Focused`, which tao raises as the window system gives the
 * window focus or takes it away; on macOS, as AppKit makes it the key window or stops), listened to while anything is
 * subscribed. Once listening, the shell is asked once; an event that comes in while that answer is on its way is
 * newer, so the answer is then dropped. */
function tauriWindowFocus(): WindowFocusCapability {
  let focused: boolean | undefined;
  let events = 0;
  let undo: (() => void) | undefined;
  const callbacks = new Set<() => void>();
  const set = (next: boolean) => {
    if (next === focused) return;
    focused = next;
    for (const notify of callbacks) notify();
  };
  const start = () => {
    const tracker = unlistenTracker();
    undo = tracker.undo;
    void import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
      const win = getCurrentWindow();
      tracker.track(await win.onFocusChanged(({ payload }) => {
        events++;
        set(payload);
      }));
      const asked = events;
      const answer = await win.isFocused().catch(() => undefined);
      // Dropped too when the subscription it was asked for has ended since.
      if (answer !== undefined && asked === events && undo === tracker.undo) set(answer);
    });
  };
  return {
    focused: () => focused,
    subscribe(callback) {
      callbacks.add(callback);
      if (callbacks.size === 1) start();
      return () => {
        if (!callbacks.delete(callback) || callbacks.size > 0) return;
        undo?.();
        undo = undefined;
        // Nothing hears the window while nothing is subscribed, so what was last heard is not kept.
        focused = undefined;
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

function tauriStatusItem(): StatusItemCapability {
  return {
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
        const { listen } = await import("@tauri-apps/api/event");
        track(await listen("exit-requested", () => onQuitRequested()));
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
        track(await listen<string>("daemon-exited", (event) => daemonExited(event.payload)));
        const missed = await invoke<string | null>("daemon_exit_detail").catch(() => null);
        if (missed !== null) daemonExited(missed);
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
    async notify(notification) {
      const { sendNotification } = await import("@tauri-apps/plugin-notification");
      sendNotification(notification);
    },
    ...(host.notificationSettings === "system-pane" && {
      async openSystemSettings() {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("open_notification_settings");
      },
    }),
  };
}

/** How the clipboard plugin rejects a read when the clipboard holds no text: it passes on only the
 * message of `arboard::Error::ContentNotAvailable`, which names no kind to tell it by.
 * The text follows the arboard release `Cargo.lock` resolves: check it whenever that changes. */
const NO_TEXT_ON_CLIPBOARD = "The clipboard contents were not available in the requested format or the clipboard is empty.";

/** Through the shell's plugin, not `navigator.clipboard`: the web API is only offered to a secure
 * context, and the packaged window's custom scheme is not reliably counted as one. */
function tauriClipboard(): ClipboardCapability {
  return {
    async readText() {
      const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
      try {
        return await readText();
      } catch (err) {
        if (String(err) === NO_TEXT_ON_CLIPBOARD) return "";
        throw err;
      }
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

function tauriBackgroundRun(): BackgroundRunCapability {
  return {
    async set(enabled) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_run_in_background", { enabled });
    },
  };
}

/** Through the shell's own commands (`updater/`), which carry a channel's name in and a version, its notes and
 * the download's progress out. The shell rejects with `{ kind, detail? }`. */
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
