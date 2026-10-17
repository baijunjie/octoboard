import { ConfigurationFileError } from "./configurationFileError";
import type { PlatformAdapter } from "./index";

/**
 * A plain browser has no shell: no quit flow (closing the tab is not quitting anything — the
 * daemon outlives it), no application icon to badge, no native window to theme or reveal, no
 * window controls of its own to keep clear and no menu bar to answer. Desktop notifications map
 * onto the Web Notifications API, absent where the browser does not have it; the clipboard is the
 * async Clipboard API, absent where the page is not a secure context; a link opens in a new tab;
 * the configuration file is saved as a download and opened through a file input.
 */
export function browserPlatform(): PlatformAdapter {
  return {
    kind: "browser",
    notifications: typeof Notification === "undefined" ? undefined : webNotifications(),
    clipboard: webClipboard(),
    configurationFile: webConfigurationFile(),
    links: webLinks(),
  };
}

/** The configuration file. Saving is a download of a Blob, which the browser files where its own
 * settings say, under a name of its own choosing, or asks about if the user told it to; a blocked
 * or dismissed download is not reported either, so all this can say is that the file was handed
 * over. Opening is a file input made on the spot and clicked, since a browser opens its picker only
 * from a user gesture, which the press that called this is. `cancel` is how a dismissed picker is
 * told from one still open; a browser without the event leaves the promise pending for good, so the
 * caller must not hold anything it needs back while it waits. */
function webConfigurationFile(): NonNullable<PlatformAdapter["configurationFile"]> {
  return {
    async save(suggestedName, text) {
      const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
      const link = document.createElement("a");
      link.href = url;
      link.download = suggestedName;
      // In the document for the click, and the URL kept past it: Firefox and WebKit fetch the blob
      // after the click returns, so revoking it at once can cancel the download.
      document.body.append(link);
      link.click();
      link.remove();
      setTimeout(() => URL.revokeObjectURL(url), 0);
      return { kind: "handedOver" };
    },
    open() {
      return new Promise((resolve, reject) => {
        const input = document.createElement("input");
        input.type = "file";
        input.accept = ".json,application/json";
        input.addEventListener("cancel", () => resolve(undefined));
        input.addEventListener("change", () => {
          const file = input.files?.[0];
          if (!file) resolve(undefined);
          else {
            file.text().then(
              (text) => resolve({ name: file.name, text }),
              (err) => reject(new ConfigurationFileError("io", err instanceof Error ? err.message : String(err))),
            );
          }
        });
        input.click();
      });
    },
  };
}

/** A new tab with no access back to this page (`noopener`) and no referrer: the address comes from
 * a file's own text, so it is not trusted with either. `noopener` makes `window.open` return `null`
 * whether or not a tab opened, so a blocked popup cannot be told from an opened one and this never
 * rejects. */
function webLinks(): NonNullable<PlatformAdapter["links"]> {
  return {
    async open(url) {
      window.open(url, "_blank", "noopener,noreferrer");
    },
  };
}

/** The async Clipboard API, which writes text from a user gesture without a permission prompt;
 * absent where the page is not a secure context. */
function webClipboard(): NonNullable<PlatformAdapter["clipboard"]> | undefined {
  if (typeof navigator === "undefined" || !navigator.clipboard?.writeText) return undefined;
  return {
    writeText: (text) => navigator.clipboard.writeText(text),
    ...(navigator.clipboard.readText && { readText: () => navigator.clipboard.readText() }),
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
