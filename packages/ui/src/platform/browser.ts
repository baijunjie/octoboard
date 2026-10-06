import type { PlatformAdapter } from "./index";

/**
 * A plain browser has no shell: no quit flow (closing the tab is not quitting anything — the
 * daemon outlives it), no application icon to badge, no native window to theme or reveal, no
 * window controls of its own to keep clear and no menu bar to answer. Desktop notifications map
 * onto the Web Notifications API, absent where the browser does not have it.
 */
export function browserPlatform(): PlatformAdapter {
  return {
    kind: "browser",
    notifications: typeof Notification === "undefined" ? undefined : webNotifications(),
  };
}

function webNotifications(): NonNullable<PlatformAdapter["notifications"]> {
  return {
    // Never asks: a browser ignores or denies a request made without a user gesture, and Chrome
    // would ask again for every newly waiting session while the answer is undecided.
    // `useNotificationPermission` asks through `permissionPrompt` instead.
    async ensurePermission() {
      return Notification.permission === "granted";
    },
    permissionPrompt: {
      status() {
        return Notification.permission === "default" ? "undecided" : Notification.permission;
      },
      async request() {
        await Notification.requestPermission().catch(() => {});
      },
    },
    async notify({ title, body }) {
      new Notification(title, { body });
    },
  };
}
