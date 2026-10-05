import type { PlatformAdapter } from "./index";

/**
 * A plain browser has no shell: no quit flow (closing the tab is not quitting anything — the
 * daemon outlives it), no application icon to badge. Desktop notifications map onto the Web
 * Notifications API, absent where the browser does not have it.
 */
export function browserPlatform(): PlatformAdapter {
  return {
    kind: "browser",
    notifications: typeof Notification === "undefined" ? undefined : webNotifications(),
  };
}

function webNotifications(): NonNullable<PlatformAdapter["notifications"]> {
  return {
    async ensurePermission() {
      if (Notification.permission === "granted") return true;
      // A denial is final in a browser: asking again would only be ignored.
      if (Notification.permission === "denied") return false;
      return (await Notification.requestPermission().catch(() => "denied")) === "granted";
    },
    async notify({ title, body }) {
      new Notification(title, { body });
    },
  };
}
