import type {
  AppMenuCapability,
  BadgeCapability,
  ExitCapability,
  NativeWindowCapability,
  NotificationCapability,
  PlatformAdapter,
  WindowChromeCapability,
} from "./index";

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
    appMenu: tauriAppMenu(),
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

/** What the traffic lights cover at the bar's leading edge: `TRAFFIC_LIGHT_X` in
 * `apps/desktop/src-tauri/src/lib.rs` plus three buttons and a gap. */
const TRAFFIC_LIGHT_INSET = 80;

/** Present only on macOS, where `lib.rs` hides the native titlebar and floats the traffic lights
 * over the page; they leave the bar in fullscreen, and with them the inset. */
function tauriWindowChrome(): WindowChromeCapability | undefined {
  if (!navigator.userAgent.includes("Mac")) return undefined;
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
    async setLabels(labels) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_menu_labels", { labels });
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

        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const { listen } = await import("@tauri-apps/api/event");
        const win = getCurrentWindow();
        track(
          await win.onCloseRequested((event) => {
            event.preventDefault();
            onQuitRequested();
          }),
        );
        track(await listen("exit-requested", () => onQuitRequested()));
        track(await listen<string>("daemon-exited", (event) => onDaemonExited(event.payload)));
      })();

      return undo;
    },
    async heartbeat() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("frontend_exit_heartbeat");
    },
    async confirmQuit() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("confirm_quit");
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
      // macOS this clears the app's own `NSAppearance` override rather than freezing it at
      // whatever the OS happened to be at the moment of the call.
      await getCurrentWindow().setTheme(theme ?? null);
    },
    async reveal() {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().show();
    },
  };
}
