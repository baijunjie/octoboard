// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { browserPlatform } from "./browser";
import { ConfigurationFileError } from "./configurationFileError";
import { fakeTimers } from "../test/clock";

afterEach(() => {
  vi.restoreAllMocks();
});
// Registered before any test's `fakeTimers()`, so the globals stay stubbed while its run-out fires the timers still pending.
beforeEach(() => {
  onTestFinished(() => void vi.unstubAllGlobals());
});

it("opens a link in a new tab with no access to the page and no referrer", async () => {
  const open = vi.spyOn(window, "open").mockReturnValue(null);
  await browserPlatform().links!.open("https://example.com/");
  expect(open).toHaveBeenCalledWith("https://example.com/", "_blank", "noopener,noreferrer");
});

// `noopener` makes `window.open` return null whether or not a tab opened, so a blocked popup cannot be
// told apart, and the capability resolves.
it("resolves even when the browser gives no window back", async () => {
  vi.spyOn(window, "open").mockReturnValue(null);
  await expect(browserPlatform().links!.open("https://example.com/")).resolves.toBeUndefined();
});

describe("the configuration file capability", () => {
  it("saves as a download under the suggested name, and says only that the file was handed over", async () => {
    const created = vi.fn(() => "blob:fixture");
    Object.assign(URL, { createObjectURL: created, revokeObjectURL: vi.fn() });
    const clicked: { download: string; href: string }[] = [];
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) {
      clicked.push({ download: this.download, href: this.href });
    });
    await expect(browserPlatform().configurationFile!.save("config.json", "{}")).resolves.toEqual({ kind: "handedOver" });
    expect(clicked).toEqual([{ download: "config.json", href: "blob:fixture" }]);
    expect(await (created.mock.calls[0] as unknown as [Blob])[0].text()).toBe("{}");
  });

  /** Runs `open` against an input the test controls, and returns it with the promise. */
  function opening() {
    const input = document.createElement("input");
    const create = document.createElement.bind(document);
    vi.spyOn(document, "createElement").mockImplementation((tag: string) => (tag === "input" ? input : create(tag)));
    vi.spyOn(input, "click").mockImplementation(() => {});
    return { input, opened: browserPlatform().configurationFile!.open() };
  }

  it("opens the file the user picked, as its name and text", async () => {
    const { input, opened } = opening();
    Object.defineProperty(input, "files", { value: [new File(['{"a":1}'], "mine.json")] });
    input.dispatchEvent(new Event("change"));
    await expect(opened).resolves.toEqual({ name: "mine.json", text: '{"a":1}' });
  });

  it("rejects with a ConfigurationFileError when the picked file cannot be read", async () => {
    const { input, opened } = opening();
    const unreadable = new File(["x"], "mine.json");
    vi.spyOn(unreadable, "text").mockRejectedValue(new Error("not readable"));
    Object.defineProperty(input, "files", { value: [unreadable] });
    input.dispatchEvent(new Event("change"));
    await expect(opened).rejects.toMatchObject({ name: "Error", kind: "io", message: "not readable" });
    await expect(opened).rejects.toBeInstanceOf(ConfigurationFileError);
  });

  it("resolves with nothing when the picker is dismissed", async () => {
    const { input, opened } = opening();
    input.dispatchEvent(new Event("cancel"));
    await expect(opened).resolves.toBeUndefined();
  });
});

describe("saving the configuration file as a download", () => {
  it("puts the link in the document for the click and revokes the URL only after it", async () => {
    fakeTimers();
    const revoked = vi.fn();
    Object.assign(URL, { createObjectURL: () => "blob:fixture", revokeObjectURL: revoked });
    let inDocument: boolean | undefined;
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) {
      inDocument = this.isConnected;
      expect(revoked).not.toHaveBeenCalled();
    });
    await browserPlatform().configurationFile!.save("config.json", "{}");
    expect(inDocument).toBe(true);
    expect(document.querySelector("a[download]")).toBeNull();
    expect(revoked).not.toHaveBeenCalled();
    vi.runAllTimers();
    expect(revoked).toHaveBeenCalledExactlyOnceWith("blob:fixture");
  });
});

it("offers clipboard reads only when the browser provides them, without reading at setup", async () => {
  const writeText = vi.fn(async () => {});
  const readText = vi.fn(async () => "clipboard text");
  vi.stubGlobal("navigator", { clipboard: { writeText, readText } });
  const clipboard = browserPlatform().clipboard!;
  expect(readText).not.toHaveBeenCalled();
  await expect(clipboard.readText!()).resolves.toBe("clipboard text");
  expect(readText).toHaveBeenCalledOnce();
});

it("keeps clipboard writing available in a browser without clipboard reading", () => {
  vi.stubGlobal("navigator", { clipboard: { writeText: async () => {} } });
  expect(browserPlatform().clipboard?.readText).toBeUndefined();
  expect(browserPlatform().clipboard?.writeText).toBeDefined();
});

describe("reading an image from the clipboard", () => {
  const item = (types: Record<string, Blob>) => ({ types: Object.keys(types), getType: vi.fn(async (type: string) => types[type]) });

  it("is offered only where the browser can read the clipboard's items", () => {
    vi.stubGlobal("navigator", { clipboard: { writeText: async () => {}, readText: async () => "" } });
    expect(browserPlatform().clipboard?.readImage).toBeUndefined();
    vi.stubGlobal("navigator", { clipboard: { writeText: async () => {}, read: async () => [] } });
    expect(browserPlatform().clipboard?.readImage).toBeDefined();
  });

  it("does not read at setup", () => {
    const read = vi.fn(async () => []);
    vi.stubGlobal("navigator", { clipboard: { writeText: async () => {}, read } });
    browserPlatform();
    expect(read).not.toHaveBeenCalled();
  });

  it("answers the first image among the items, in the format the browser holds it in", async () => {
    const jpeg = new Blob(["jpeg"], { type: "image/jpeg" });
    const png = new Blob(["png"], { type: "image/png" });
    const text = item({ "text/plain": new Blob(["text"]) });
    const first = item({ "text/html": new Blob(["html"]), "image/jpeg": jpeg });
    const second = item({ "image/png": png });
    vi.stubGlobal("navigator", { clipboard: { writeText: async () => {}, read: async () => [text, first, second] } });
    await expect(browserPlatform().clipboard!.readImage!()).resolves.toBe(jpeg);
    expect(first.getType).toHaveBeenCalledExactlyOnceWith("image/jpeg");
    expect(second.getType).not.toHaveBeenCalled();
  });

  it("answers nothing for a clipboard with no image, or with no items", async () => {
    vi.stubGlobal("navigator", { clipboard: { writeText: async () => {}, read: async () => [item({ "text/plain": new Blob(["text"]) })] } });
    await expect(browserPlatform().clipboard!.readImage!()).resolves.toBeUndefined();
    vi.stubGlobal("navigator", { clipboard: { writeText: async () => {}, read: async () => [] } });
    await expect(browserPlatform().clipboard!.readImage!()).resolves.toBeUndefined();
  });

  it("rejects when the browser refuses the read, as a denied permission does", async () => {
    const denied = new DOMException("Read permission denied.", "NotAllowedError");
    vi.stubGlobal("navigator", { clipboard: { writeText: async () => {}, read: async () => { throw denied; } } });
    await expect(browserPlatform().clipboard!.readImage!()).rejects.toBe(denied);
  });
});

describe("the host facts", () => {
  it.each([
    ["MacIntel", "macos", "meta"],
    ["iPad", "macos", "meta"],
    ["Win32", "windows", "control"],
    ["Linux x86_64", "linux", "control"],
    ["", "linux", "control"],
  ] as const)("take the system and the modifier from the platform %j and claim nothing a shell provides", (platform, os, modifier) => {
    vi.stubGlobal("navigator", { platform });
    const host = browserPlatform().host;
    expect(host).toMatchObject({ os, modifier, chrome: "none", translucent: false, appMenu: false, statusItem: "none", badge: false, runInBackground: false, keepAwake: false, launchAtLogin: false, commandLine: false, permissionsPane: false, notificationSettings: "none", updater: "none" });
    expect(host.trafficLightInset).toBeUndefined();
  });

  it("name the system's own words for each system", () => {
    vi.stubGlobal("navigator", { platform: "Win32" });
    expect(browserPlatform().host.terms).toEqual({ trash: "recycleBin", settings: "settings", menuBarIcon: "notificationArea" });
    vi.stubGlobal("navigator", { platform: "MacIntel" });
    expect(browserPlatform().host.terms).toEqual({ trash: "trash", settings: "systemSettings", menuBarIcon: "menuBar" });
  });
});

describe("the notification capability", () => {
  /** The notifications a stand-in `Notification` was asked to show, with the click and close of each. */
  function stubNotification(permission: NotificationPermission) {
    const shown: { title: string; body?: string; onclick: (() => void) | null; close: ReturnType<typeof vi.fn> }[] = [];
    class FakeNotification {
      static permission = permission;
      onclick: (() => void) | null = null;
      close = vi.fn();
      constructor(
        public title: string,
        options?: NotificationOptions,
      ) {
        this.body = options?.body;
        shown.push(this);
      }
      body?: string;
    }
    vi.stubGlobal("Notification", FakeNotification);
    return shown;
  }

  it("has none where the browser has no notifications", () => {
    vi.stubGlobal("Notification", undefined);
    expect(browserPlatform().notifications).toBeUndefined();
  });

  it("shows a notification with its title and body, and hands nothing back when it is clicked", async () => {
    const shown = stubNotification("granted");
    await browserPlatform().notifications!.notify({ title: "Waiting", body: "Fix the build" });
    expect(shown).toHaveLength(1);
    expect(shown[0]).toMatchObject({ title: "Waiting", body: "Fix the build", onclick: null });
  });

  it("hands the target of a notification that is clicked to each listener, brings the window forward and closes it", async () => {
    const shown = stubNotification("granted");
    const focus = vi.spyOn(window, "focus").mockImplementation(() => {});
    const { notify, onChosen } = browserPlatform().notifications!;
    const first = vi.fn();
    const second = vi.fn();
    onChosen!(first);
    const stop = onChosen!(second);
    await notify({ title: "Failed", body: "Morning report", target: "scheduled-task:t-1" });
    shown[0]!.onclick!();
    expect(first).toHaveBeenCalledWith("scheduled-task:t-1");
    expect(second).toHaveBeenCalledWith("scheduled-task:t-1");
    expect(focus).toHaveBeenCalledOnce();
    expect(shown[0]!.close).toHaveBeenCalledOnce();

    stop();
    shown[0]!.onclick!();
    expect(first).toHaveBeenCalledTimes(2);
    expect(second).toHaveBeenCalledOnce();
  });

  it("is allowed to notify only while the permission is granted, and never asks for it itself", async () => {
    stubNotification("denied");
    const { ensurePermission, permissionPrompt } = browserPlatform().notifications!;
    expect(await ensurePermission()).toBe(false);
    expect(permissionPrompt!.status()).toBe("denied");
    stubNotification("granted");
    expect(await browserPlatform().notifications!.ensurePermission()).toBe(true);
    stubNotification("default");
    const undecided = browserPlatform().notifications!;
    expect(await undecided.ensurePermission()).toBe(false);
    expect(undecided.permissionPrompt!.status()).toBe("undecided");
  });
});
