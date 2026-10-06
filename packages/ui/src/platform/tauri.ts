import type {
  BadgeCapability,
  ExitCapability,
  NativeWindowCapability,
  NotificationCapability,
  PlatformAdapter,
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
  };
}

function tauriExit(): ExitCapability {
  return {
    register({ onQuitRequested, onDaemonExited }) {
      // Every listener is registered after an `await`, so the registration can be undone before any
      // of them exists. Collecting them through `track` — which unsubscribes immediately once the
      // registration is gone — is what keeps an undo during that window from leaking one: a leaked
      // listener fires for the rest of the window's life, which shows up as a duplicated quit
      // prompt and a duplicated toast for every daemon event.
      let cancelled = false;
      const unlisteners: Array<() => void> = [];
      const track = (unlisten: () => void) => {
        if (cancelled) unlisten();
        else unlisteners.push(unlisten);
      };

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

      return () => {
        cancelled = true;
        for (const unlisten of unlisteners) unlisten();
        unlisteners.length = 0;
      };
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
