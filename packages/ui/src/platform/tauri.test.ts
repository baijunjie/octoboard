// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { afterQueuedWork } from "../test/clock";
import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { ConfigurationFileError } from "./configurationFileError";
import { browserHost } from "./host";
import { tauriPlatform } from "./tauri";
import { ForwardedLinkOpenError, LinkForwardError } from "./linkForwardError";
import { RemoteFileError } from "./remoteFileError";
import { UpdaterError } from "./updaterError";

const invoke = vi.fn();
/** The IPC channel the shell reports download progress through: the test calls `onmessage` itself. */
class FakeChannel<T> {
  onmessage: (message: T) => void = () => {};
}
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args), Channel: FakeChannel }));
/** The shell's events by name, as the adapter listens to them: the test emits one by calling its handler. */
const listeners = new Map<string, (event: { payload: unknown }) => void>();
/** The window the adapter listens on: only its own events, never another window's. */
const listenedOn = vi.fn();
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => {
    listenedOn();
    return {
      listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
        listeners.set(name, handler);
        return () => listeners.delete(name);
      },
    };
  },
}));

/** The shell's drag and drop of files over the webview: the test calls `dragDrop` itself, as the shell would. */
let dragDrop: ((event: { payload: unknown }) => void) | undefined;
const unlistenDragDrop = vi.fn();
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: async (handler: (event: { payload: unknown }) => void) => {
      dragDrop = handler;
      return unlistenDragDrop;
    },
  }),
}));

const shellOpen = vi.fn(async (_address: string) => {});
vi.mock("@tauri-apps/plugin-shell", () => ({ open: (address: string) => shellOpen(address) }));

const readText = vi.fn();
const readImage = vi.fn();
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ readText: () => readText(), readImage: () => readImage() }));

/** Puts the shell's `host` query parameter on the page, as the desktop shell does when it opens the window. */
function reportHost(facts: unknown): void {
  window.history.replaceState(null, "", `/?host=${encodeURIComponent(JSON.stringify(facts))}`);
}

beforeEach(() => reportHost(MAC_HOST));

afterEach(() => {
  readImage.mockReset();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  invoke.mockReset();
  shellOpen.mockReset();
  shellOpen.mockResolvedValue(undefined);
  listeners.clear();
  dragDrop = undefined;
  unlistenDragDrop.mockReset();
  vi.unstubAllGlobals();
  window.history.replaceState(null, "", "/");
});

describe("the host facts", () => {
  it("are read from the window's query", () => {
    reportHost(WINDOWS_HOST);
    expect(tauriPlatform().host).toEqual(WINDOWS_HOST);
    reportHost(MAC_HOST);
    expect(tauriPlatform().host).toEqual(MAC_HOST);
  });

  it("fall back to what the browser says when the shell reported none, or something that is not a set of them", () => {
    for (const search of ["/", "/?host=", "/?host=%7Bnot-json", `/?host=${encodeURIComponent(JSON.stringify({ ...MAC_HOST, chrome: "wide" }))}`, `/?host=${encodeURIComponent(JSON.stringify({ ...MAC_HOST, terms: undefined }))}`]) {
      window.history.replaceState(null, "", search);
      expect(tauriPlatform().host).toEqual(browserHost());
    }
  });

  it("decide which capabilities the adapter has: the traffic lights' inset, the material, the menu, the status item, the badge, background running, the updater, the notification pane", () => {
    const mac = tauriPlatform();
    expect(mac.windowChrome?.leftInset()).toBe(MAC_HOST.trafficLightInset);
    expect(mac.translucentWindow).toBe(true);
    expect([mac.appMenu, mac.statusItem, mac.badge, mac.backgroundRun, mac.updater, mac.notifications?.openSystemSettings].map((capability) => capability !== undefined)).toEqual([true, true, true, true, true, true]);

    reportHost(WINDOWS_HOST);
    const windows = tauriPlatform();
    expect(windows.windowChrome).toBeUndefined();
    expect(windows.translucentWindow).toBe(false);
    expect([windows.appMenu, windows.badge, windows.notifications?.openSystemSettings].map((capability) => capability !== undefined)).toEqual([false, false, true]);
    expect([windows.statusItem, windows.backgroundRun, windows.updater].map((capability) => capability !== undefined)).toEqual([true, true, true]);
  });
});

it.each([
  { shell: { kind: "too_large" }, kind: "too_large", detail: "" },
  { shell: { kind: "not_text" }, kind: "not_text", detail: "" },
  { shell: { kind: "io", detail: "Permission denied" }, kind: "io", detail: "Permission denied" },
  { shell: "something else", kind: "io", detail: "something else" },
])("tells a configuration file the shell refused by its kind: $shell", async ({ shell, kind, detail }) => {
  invoke.mockRejectedValue(shell);
  const error = await tauriPlatform().configurationFile!.open().catch((err: unknown) => err);
  expect(error).toBeInstanceOf(ConfigurationFileError);
  expect(error).toMatchObject({ kind, message: detail });
});

it("tells a configuration file the shell could not save by its kind too", async () => {
  invoke.mockRejectedValue({ kind: "io", detail: "Disk full" });
  const error = await tauriPlatform().configurationFile!.save("a.json", "{}").catch((err: unknown) => err);
  expect(error).toBeInstanceOf(ConfigurationFileError);
  expect(error).toMatchObject({ kind: "io", message: "Disk full" });
});

it("answers nothing for a dialog the user dismissed", async () => {
  invoke.mockResolvedValue(null);
  await expect(tauriPlatform().configurationFile!.open()).resolves.toBeUndefined();
  await expect(tauriPlatform().configurationFile!.save("a.json", "{}")).resolves.toBeUndefined();
});

it("returns the system directory choice and sends the starting path and localized title", async () => {
  invoke.mockResolvedValue("/Users/me/code");
  const options = { title: "Choose directory", initialPath: "~/code" };
  await expect(tauriPlatform().directoryPicker!.pick(options)).resolves.toBe("/Users/me/code");
  expect(invoke).toHaveBeenCalledWith("pick_directory", options);
  invoke.mockResolvedValue(null);
  await expect(tauriPlatform().directoryPicker!.pick(options)).resolves.toBeUndefined();
});

it("changes the settings the shell keeps one at a time, and reveals the window with no argument", async () => {
  invoke.mockResolvedValue(undefined);
  const platform = tauriPlatform();
  await platform.backgroundRun!.set(true);
  expect(invoke).toHaveBeenLastCalledWith("set_app_settings", { settings: { run_in_background: true } });
  await platform.statusItem!.setVisible!(false);
  expect(invoke).toHaveBeenLastCalledWith("set_app_settings", { settings: { show_menu_bar_icon: false } });
  await platform.nativeWindow!.reveal();
  expect(invoke).toHaveBeenLastCalledWith("reveal_main_window");
});

it("turns a directory command failure into an Error for the shared error route", async () => {
  invoke.mockRejectedValue({ kind: "io", detail: "Could not open picker" });
  await expect(tauriPlatform().directoryPicker!.pick({ title: "Choose directory", initialPath: "~" })).rejects.toThrow("Could not open picker");
});

it("tells the shell a quit has begun with its own command", async () => {
  invoke.mockResolvedValue(undefined);
  await tauriPlatform().exit!.beginQuit();
  expect(invoke.mock.calls).toEqual([["begin_quit"]]);
});

it("asks the shell to relaunch only when told to", async () => {
  invoke.mockResolvedValue(undefined);
  await tauriPlatform().exit!.confirmQuit();
  await tauriPlatform().exit!.confirmQuit({ restart: true });
  expect(invoke.mock.calls).toEqual([
    ["confirm_quit", { restart: undefined }],
    ["confirm_quit", { restart: true }],
  ]);
});

it("hands the shell the channel to check and answers the version and holder floor it found", async () => {
  invoke.mockResolvedValue({ version: "0.2.0", holder_floor: "0.2.0" });
  await expect(tauriPlatform().updater!.check("stable")).resolves.toEqual({ version: "0.2.0", holderFloor: "0.2.0" });
  expect(invoke).toHaveBeenCalledWith("check_for_update", { channel: "stable" });
  invoke.mockResolvedValue({ version: "0.3.0" });
  await expect(tauriPlatform().updater!.check("beta")).resolves.toEqual({ version: "0.3.0", holderFloor: null });
});

it("answers nothing for a check that found no newer version", async () => {
  invoke.mockResolvedValue(null);
  await expect(tauriPlatform().updater!.check("beta")).resolves.toBeUndefined();
});

it.each([
  { shell: { kind: "busy" }, kind: "busy", detail: "" },
  { shell: { kind: "installed" }, kind: "installed", detail: "" },
  { shell: { kind: "nothing_to_install" }, kind: "nothing_to_install", detail: "" },
  { shell: { kind: "failed", detail: "offline" }, kind: "failed", detail: "offline" },
  { shell: "something else", kind: "failed", detail: "something else" },
  { shell: { kind: "failed" }, kind: "failed", detail: "" },
  { shell: new Error("ipc down"), kind: "failed", detail: "ipc down" },
])("tells an update the shell refused by its kind: $shell", async ({ shell, kind, detail }) => {
  invoke.mockRejectedValue(shell);
  const error = await tauriPlatform().updater!.check("beta").catch((err: unknown) => err);
  expect(error).toBeInstanceOf(UpdaterError);
  expect(error).toMatchObject({ kind, message: detail });
});

it("passes the download's progress on while the shell installs", async () => {
  const progress = vi.fn();
  invoke.mockImplementation(async (_command: string, args: { onProgress: FakeChannel<{ downloaded: number; total: number | null }> }) => {
    args.onProgress.onmessage({ downloaded: 10, total: 100 });
    args.onProgress.onmessage({ downloaded: 20, total: null });
  });
  await tauriPlatform().updater!.install(progress);
  expect(invoke).toHaveBeenCalledWith("install_update", expect.anything());
  expect(progress.mock.calls).toEqual([[{ downloaded: 10, total: 100 }], [{ downloaded: 20, total: undefined }]]);
});

it("ignores progress the shell sends after the install has settled", async () => {
  const progress = vi.fn();
  let channel: FakeChannel<{ downloaded: number; total: number | null }> | undefined;
  invoke.mockImplementation(async (_command: string, args: { onProgress: FakeChannel<{ downloaded: number; total: number | null }> }) => {
    channel = args.onProgress;
  });
  await tauriPlatform().updater!.install(progress);
  channel!.onmessage({ downloaded: 30, total: 100 });
  expect(progress).not.toHaveBeenCalled();
});

/** Registers for the exit flow with the shell answering `daemon_exit_detail` with `answer`, and returns what was
 * reported once the registration has settled. */
async function registerExit(answer: Promise<string | null>) {
  invoke.mockImplementation((command: string) => (command === "daemon_exit_detail" ? answer : Promise.resolve()));
  const onDaemonExited = vi.fn();
  const undo = tauriPlatform().exit!.register({ onQuitRequested: () => {}, onDaemonExited });
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("daemon_exit_detail"));
  return { onDaemonExited, undo, emitExited: (detail: string) => listeners.get("daemon-exited")?.({ payload: detail }) };
}

it("reports a daemon end the shell gave up on before the page was listening", async () => {
  const { onDaemonExited } = await registerExit(Promise.resolve("restarted 3 times"));
  await vi.waitFor(() => expect(onDaemonExited.mock.calls).toEqual([["restarted 3 times"]]));
});

it("reports the end once when the event arrives as well, in either order", async () => {
  let answer!: (detail: string) => void;
  const first = await registerExit(new Promise((resolve) => (answer = resolve)));
  first.emitExited("restarted 3 times");
  answer("restarted 3 times");
  await vi.waitFor(() => expect(first.onDaemonExited.mock.calls).toEqual([["restarted 3 times"]]));

  const second = await registerExit(Promise.resolve("exit code 0"));
  await vi.waitFor(() => expect(second.onDaemonExited).toHaveBeenCalledTimes(1));
  second.emitExited("exit code 0");
  expect(second.onDaemonExited.mock.calls).toEqual([["exit code 0"]]);
});

it("reports nothing while the shell has not given up, and the event still once it does", async () => {
  const { onDaemonExited, emitExited } = await registerExit(Promise.resolve(null));
  await afterQueuedWork();
  expect(onDaemonExited).not.toHaveBeenCalled();
  emitExited("restarted 3 times");
  expect(onDaemonExited.mock.calls).toEqual([["restarted 3 times"]]);
});

it("reports nothing for an answer that comes after the registration was undone", async () => {
  let answer!: (detail: string) => void;
  const { onDaemonExited, undo } = await registerExit(new Promise((resolve) => (answer = resolve)));
  undo();
  answer("restarted 3 times");
  await afterQueuedWork();
  expect(onDaemonExited).not.toHaveBeenCalled();
});

it("reports nothing when the shell cannot be asked", async () => {
  const { onDaemonExited } = await registerExit(Promise.reject(new Error("not allowed")));
  await afterQueuedWork();
  expect(onDaemonExited).not.toHaveBeenCalled();
});

it("reads a clipboard without text as empty text, as a native paste does", async () => {
  readText.mockRejectedValue("The clipboard contents were not available in the requested format or the clipboard is empty.");
  await expect(tauriPlatform().clipboard!.readText!()).resolves.toBe("");
});

it("still rejects a clipboard read that failed for another reason", async () => {
  readText.mockRejectedValue("The native clipboard is not accessible due to being held by another party.");
  await expect(tauriPlatform().clipboard!.readText!()).rejects.toBe("The native clipboard is not accessible due to being held by another party.");
});

const NOTHING_ON_CLIPBOARD = "The clipboard contents were not available in the requested format or the clipboard is empty.";

describe("reading an image from the clipboard", () => {
  /** What the plugin hands back: an image that reads its size and its raw RGBA pixels, and is closed once read. */
  const close = vi.fn();
  beforeEach(() => close.mockClear());
  const pluginImage = (width: number, height: number, rgba: Uint8Array) => ({ size: async () => ({ width, height }), rgba: async () => rgba, close: () => { close(); return Promise.resolve(); } });

  /** A canvas that records what is drawn on it and encodes to `encoded` (null for an encoding that fails), and an
   * `ImageData` that keeps what it is made of: jsdom has neither. */
  function stubCanvas(encoded: Blob | null, context: object | null = {}) {
    const drawn: { data: Uint8ClampedArray; width: number; height: number; at: [number, number] }[] = [];
    const sizes: [number, number][] = [];
    vi.stubGlobal("ImageData", class { constructor(readonly data: Uint8ClampedArray, readonly width: number, readonly height: number) {} });
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(function (this: HTMLCanvasElement) {
      sizes.push([this.width, this.height]);
      return context && ({ putImageData: (image: { data: Uint8ClampedArray; width: number; height: number }, x: number, y: number) => drawn.push({ ...image, at: [x, y] }) } as unknown as CanvasRenderingContext2D);
    } as typeof HTMLCanvasElement.prototype.getContext);
    const toBlob = vi.spyOn(HTMLCanvasElement.prototype, "toBlob").mockImplementation((callback, type) => callback(encoded && new Blob([encoded], { type: type ?? "" })));
    return { drawn, sizes, toBlob };
  }

  it("answers the pixels the plugin read, drawn at their own size, as a PNG", async () => {
    const rgba = Uint8Array.from([1, 2, 3, 255, 4, 5, 6, 255]);
    readImage.mockResolvedValue(pluginImage(2, 1, rgba));
    const canvas = stubCanvas(new Blob(["png bytes"]));
    const image = await tauriPlatform().clipboard!.readImage!();
    expect(image?.type).toBe("image/png");
    expect(await image!.text()).toBe("png bytes");
    expect(canvas.sizes).toEqual([[2, 1]]);
    expect(canvas.drawn).toHaveLength(1);
    expect(canvas.drawn[0]).toMatchObject({ width: 2, height: 1, at: [0, 0] });
    expect(Array.from(canvas.drawn[0].data)).toEqual(Array.from(rgba));
    expect(close).toHaveBeenCalledOnce();
  });

  it("closes the image after reading it, and a close that fails changes nothing", async () => {
    const image = { ...pluginImage(1, 1, Uint8Array.from([0, 0, 0, 255])), close: () => { close(); return Promise.reject(new Error("already closed")); } };
    readImage.mockResolvedValue(image);
    stubCanvas(new Blob(["png bytes"]));
    await expect(tauriPlatform().clipboard!.readImage!()).resolves.toBeInstanceOf(Blob);
    expect(close).toHaveBeenCalledOnce();
  });

  it("closes the image when its pixels cannot be read", async () => {
    readImage.mockResolvedValue({ ...pluginImage(1, 1, new Uint8Array(4)), rgba: async () => { throw new Error("unreadable"); } });
    stubCanvas(new Blob(["x"]));
    await expect(tauriPlatform().clipboard!.readImage!()).rejects.toThrow("unreadable");
    expect(close).toHaveBeenCalledOnce();
  });

  it("answers nothing for a clipboard that holds no image", async () => {
    readImage.mockRejectedValue(NOTHING_ON_CLIPBOARD);
    await expect(tauriPlatform().clipboard!.readImage!()).resolves.toBeUndefined();
  });

  it("still rejects a read that failed for another reason", async () => {
    readImage.mockRejectedValue("The native clipboard is not accessible due to being held by another party.");
    await expect(tauriPlatform().clipboard!.readImage!()).rejects.toBe("The native clipboard is not accessible due to being held by another party.");
  });

  it("rejects when the pixels cannot be drawn or encoded, rather than answering no image", async () => {
    readImage.mockResolvedValue(pluginImage(1, 1, Uint8Array.from([0, 0, 0, 255])));
    stubCanvas(new Blob(["x"]), null);
    await expect(tauriPlatform().clipboard!.readImage!()).rejects.toBeInstanceOf(Error);
    vi.restoreAllMocks();
    stubCanvas(null);
    await expect(tauriPlatform().clipboard!.readImage!()).rejects.toBeInstanceOf(Error);
    // Closed whether or not the pixels could be encoded, so the shell's copy of them does not leak.
    expect(close).toHaveBeenCalledTimes(2);
  });
});

describe("the window of a daemon elsewhere", () => {
  function openRemote(): void {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    window.history.replaceState(null, "", `/?host=${encodeURIComponent(JSON.stringify(MAC_HOST))}&daemon=me%40host&remote=host`);
  }

  it("has no exit flow, directory picker or privacy grants, all of which are this machine's", () => {
    openRemote();
    const platform = tauriPlatform();
    expect(platform.exit).toBeUndefined();
    expect(platform.directoryPicker).toBeUndefined();
    expect(platform.macosPermissions).toBeUndefined();
    expect(platform.appMenu).toBeDefined();
  });

  it("keeps them in the local daemon's window", () => {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    const platform = tauriPlatform();
    expect(platform.exit).toBeDefined();
    expect(platform.directoryPicker).toBeDefined();
    expect(platform.macosPermissions).toBeDefined();
    expect(platform.statusItem?.setVisible).toBeDefined();
  });

  it("has a menu bar icon and a share of the badge of its own, but not the switch that shows the icons", () => {
    openRemote();
    const platform = tauriPlatform();
    expect(platform.statusItem).toBeDefined();
    expect(platform.statusItem!.setVisible).toBeUndefined();
    expect(platform.badge).toBeDefined();
  });
});

it("reports the window's share of the badge and raises a notification through the shell, which knows the window", async () => {
  invoke.mockResolvedValue(undefined);
  const platform = tauriPlatform();
  await platform.badge!.set(3);
  expect(invoke).toHaveBeenLastCalledWith("set_badge_count", { count: 3 });
  await platform.badge!.set(0);
  expect(invoke).toHaveBeenLastCalledWith("set_badge_count", { count: 0 });
  await platform.notifications!.notify({ title: "Waiting", body: "Fix the build" });
  expect(invoke).toHaveBeenLastCalledWith("show_notification", { title: "Waiting", body: "Fix the build", target: null });
  // A target goes with it, which the shell hands back when the notification is chosen.
  await platform.notifications!.notify({ title: "Failed", body: "Morning report", target: "scheduled-task:t-1" });
  expect(invoke).toHaveBeenLastCalledWith("show_notification", { title: "Failed", body: "Morning report", target: "scheduled-task:t-1" });
});

it("listens to the shell's events on its own window, so what is sent to another window does not reach it", async () => {
  const handler = vi.fn();
  const undo = tauriPlatform().appMenu!.onSettingsRequested(handler);
  await afterQueuedWork();
  expect(listenedOn).toHaveBeenCalled();
  listeners.get("settings-requested")!({ payload: undefined });
  expect(handler).toHaveBeenCalledOnce();
  undo();
});

it("hands the target the shell reports for a chosen notification to the listener, until it stops listening", async () => {
  const chosen = vi.fn();
  const stop = tauriPlatform().notifications!.onChosen!(chosen);
  await afterQueuedWork();
  listeners.get("notification-chosen")!({ payload: "scheduled-task:t-1" });
  expect(chosen).toHaveBeenCalledExactlyOnceWith("scheduled-task:t-1");
  stop();
  await afterQueuedWork();
  expect(listeners.has("notification-chosen")).toBe(false);
});

describe("what each kind of window of the application has", () => {
  function openAs(query: string): void {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    window.history.replaceState(null, "", `/?host=${encodeURIComponent(JSON.stringify(MAC_HOST))}${query}`);
  }

  it("gives only the local daemon's window the remote daemons and the computer's own Remote Login", () => {
    openAs("");
    expect(tauriPlatform().remoteDaemons).toBeDefined();
    expect(tauriPlatform().remoteLogin).toBeDefined();
    openAs("&daemon=me%40host&remote=host");
    expect(tauriPlatform().remoteDaemons).toBeUndefined();
    expect(tauriPlatform().remoteLogin).toBeUndefined();
  });

  it("gives the notification-area flyout's page its own capability, and no other window has it", () => {
    openAs("&flyout=1");
    const flyout = tauriPlatform();
    expect(flyout.trayFlyout).toBeDefined();
    expect(flyout.exit).toBeUndefined();
    expect(flyout.directoryPicker).toBeUndefined();
    expect(flyout.macosPermissions).toBeUndefined();
    expect(flyout.remoteLogin).toBeUndefined();
    openAs("");
    expect(tauriPlatform().trayFlyout).toBeUndefined();
    openAs("&daemon=me%40host&remote=host");
    expect(tauriPlatform().trayFlyout).toBeUndefined();
  });

  it("reads none of it outside the shell, where the query is anyone's", () => {
    window.history.replaceState(null, "", `/?host=${encodeURIComponent(JSON.stringify(MAC_HOST))}&flyout=1`);
    const platform = tauriPlatform();
    expect(platform.trayFlyout).toBeUndefined();
    expect(platform.exit).toBeDefined();
  });
});

describe("the jump list", () => {
  it("is on Windows alone, and takes its labels through the menu's own command", async () => {
    reportHost(MAC_HOST);
    expect(tauriPlatform().jumpList).toBeUndefined();
    reportHost(WINDOWS_HOST);
    const platform = tauriPlatform();
    invoke.mockResolvedValue(undefined);
    await platform.jumpList!.setLabels({ "menu.remote": "Remote" });
    expect(invoke).toHaveBeenCalledExactlyOnceWith("set_menu_labels", { labels: { "menu.remote": "Remote" }, settingsAccelerator: null });
  });
});

describe("the badge", () => {
  it("is reported as a whole, non-negative count, which the shell adds to the other windows'", async () => {
    invoke.mockResolvedValue(undefined);
    const { badge } = tauriPlatform();
    await badge!.set(2.6);
    expect(invoke).toHaveBeenLastCalledWith("set_badge_count", { count: 3 });
    await badge!.set(-4);
    expect(invoke).toHaveBeenLastCalledWith("set_badge_count", { count: 0 });
  });

  it("is absent where the platform shows no badge", () => {
    reportHost(WINDOWS_HOST);
    expect(tauriPlatform().badge).toBeUndefined();
  });
});

it("raises each notification through the shell, whatever platform the window is on", async () => {
  invoke.mockResolvedValue(undefined);
  reportHost(WINDOWS_HOST);
  await tauriPlatform().notifications!.notify({ title: "Waiting", body: "Fix the build" });
  expect(invoke).toHaveBeenLastCalledWith("show_notification", { title: "Waiting", body: "Fix the build", target: null });
});

it("lets a failed notification reach the caller", async () => {
  invoke.mockRejectedValue(new Error("no permission"));
  await expect(tauriPlatform().notifications!.notify({ title: "t", body: "b" })).rejects.toThrow("no permission");
});

describe("Remote Login", () => {
  it("reads whether it is on, and answers nothing where the shell cannot tell", async () => {
    const { remoteLogin } = tauriPlatform();
    invoke.mockResolvedValueOnce(true);
    expect(await remoteLogin!.status()).toBe(true);
    expect(invoke).toHaveBeenLastCalledWith("remote_login_status");
    invoke.mockResolvedValueOnce(false);
    expect(await remoteLogin!.status()).toBe(false);
    invoke.mockResolvedValueOnce(null);
    expect(await remoteLogin!.status()).toBeUndefined();
  });

  it("opens the page of the system's settings that turns it on", async () => {
    invoke.mockResolvedValue(undefined);
    await tauriPlatform().remoteLogin!.openSystemSettings();
    expect(invoke).toHaveBeenLastCalledWith("open_remote_login_settings");
  });
});

describe("opening an address", () => {
  function openAs(query: string): void {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    window.history.replaceState(null, "", `/?host=${encodeURIComponent(JSON.stringify(MAC_HOST))}${query}`);
  }

  it("hands the shell plugin the address as it is in the main window, which asks the shell for nothing", async () => {
    openAs("");
    await tauriPlatform().links!.open("http://localhost:3000/");
    expect(shellOpen).toHaveBeenCalledExactlyOnceWith("http://localhost:3000/");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("opens in a remote window what the shell answers for the address, the forwarded one for a loopback address", async () => {
    openAs("&daemon=me%40host&remote=host");
    invoke.mockResolvedValueOnce("http://localhost:4000/app");
    await tauriPlatform().links!.open("http://localhost:3000/app");
    expect(invoke).toHaveBeenLastCalledWith("remote_link_address", { url: "http://localhost:3000/app" });
    expect(shellOpen).toHaveBeenLastCalledWith("http://localhost:4000/app");
  });

  it("treats an IPv6 literal address like any other: as it is in the main window, through the shell in a remote one", async () => {
    openAs("");
    await tauriPlatform().links!.open("http://[::1]:59302/remote.txt");
    expect(shellOpen).toHaveBeenLastCalledWith("http://[::1]:59302/remote.txt");
    expect(invoke).not.toHaveBeenCalled();
    openAs("&daemon=me%40host&remote=host");
    invoke.mockResolvedValueOnce("http://localhost:4000/remote.txt");
    await tauriPlatform().links!.open("http://[::1]:59302/remote.txt");
    expect(invoke).toHaveBeenLastCalledWith("remote_link_address", { url: "http://[::1]:59302/remote.txt" });
    expect(shellOpen).toHaveBeenLastCalledWith("http://localhost:4000/remote.txt");
  });

  it("rejects with the kind and the port where the shell could not forward, and opens nothing", async () => {
    openAs("&daemon=me%40host&remote=host");
    for (const kind of ["nothing_listening", "forwarding_refused", "disconnected"] as const) {
      invoke.mockRejectedValueOnce({ kind, port: 3000 });
      const error = await tauriPlatform().links!.open("http://localhost:3000/").catch((cause: unknown) => cause);
      expect(error).toBeInstanceOf(LinkForwardError);
      expect(error).toMatchObject({ kind, port: 3000 });
    }
    invoke.mockRejectedValueOnce({ kind: "failed", port: 3000, detail: "cannot listen" });
    expect(await tauriPlatform().links!.open("http://localhost:3000/").catch((cause: unknown) => cause)).toMatchObject({ kind: "failed", message: "cannot listen" });
    expect(shellOpen).not.toHaveBeenCalled();
  });

  it("lets a failure that is not one of those through, and says which address a browser failed to open", async () => {
    openAs("&daemon=me%40host&remote=host");
    invoke.mockRejectedValueOnce(new Error("no such command"));
    await expect(tauriPlatform().links!.open("http://localhost:3000/")).rejects.toThrow("no such command");
    invoke.mockResolvedValueOnce("http://localhost:4000/");
    shellOpen.mockRejectedValueOnce(new Error("no browser"));
    const error = await tauriPlatform().links!.open("http://localhost:3000/").catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(ForwardedLinkOpenError);
    expect(error).toMatchObject({ address: "http://localhost:4000/" });
    // An address that was not rewritten fails as it is.
    invoke.mockResolvedValueOnce("https://example.com/");
    shellOpen.mockRejectedValueOnce(new Error("no browser"));
    await expect(tauriPlatform().links!.open("https://example.com/")).rejects.toThrow("no browser");
  });
});

describe("the forwarded ports", () => {
  function openAs(query: string): void {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    window.history.replaceState(null, "", `/?host=${encodeURIComponent(JSON.stringify(MAC_HOST))}${query}`);
  }

  it("are a remote window's alone", () => {
    openAs("&daemon=me%40host&remote=host");
    expect(tauriPlatform().remoteForwards).toBeDefined();
    expect(tauriPlatform().remoteConnection).toBeDefined();
    openAs("");
    expect(tauriPlatform().remoteForwards).toBeUndefined();
    expect(tauriPlatform().remoteConnection).toBeUndefined();
  });

  it("has the shell make the SSH connection again at once for Try again, and hears it say the connection is up", async () => {
    openAs("&daemon=me%40host&remote=host");
    const { remoteConnection } = tauriPlatform();
    invoke.mockResolvedValueOnce(undefined);
    await remoteConnection!.retryNow();
    expect(invoke).toHaveBeenLastCalledWith("remote_connection_retry");
    const listener = vi.fn();
    const undo = remoteConnection!.onUp(listener);
    await afterQueuedWork();
    listeners.get("remote-connection-up")!({ payload: null });
    expect(listener).toHaveBeenCalledTimes(1);
    undo();
    expect(listeners.has("remote-connection-up")).toBe(false);
  });

  it("are listed and stopped by the shell's commands, and the shell's event is heard in this window", async () => {
    openAs("&daemon=me%40host&remote=host");
    const forward = { remotePort: 3000, localPort: 3000, address: "http://localhost:3000/", localAddress: "http://localhost:3000/" };
    const { remoteForwards } = tauriPlatform();
    invoke.mockResolvedValueOnce([forward]);
    expect(await remoteForwards!.list()).toEqual([forward]);
    expect(invoke).toHaveBeenLastCalledWith("remote_forwards_list");
    invoke.mockResolvedValueOnce(undefined);
    await remoteForwards!.stop(3000);
    expect(invoke).toHaveBeenLastCalledWith("remote_forward_stop", { remotePort: 3000 });
    const listener = vi.fn();
    const undo = remoteForwards!.onChange(listener);
    await afterQueuedWork();
    listeners.get("remote-forwards-changed")!({ payload: null });
    expect(listener).toHaveBeenCalledTimes(1);
    undo();
    expect(listeners.has("remote-forwards-changed")).toBe(false);
  });
});

describe("the files copied from a remote daemon", () => {
  function openAs(query: string): void {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    window.history.replaceState(null, "", `/?host=${encodeURIComponent(JSON.stringify(MAC_HOST))}${query}`);
  }

  it("are a remote window's alone", () => {
    openAs("&daemon=me%40host&remote=host");
    expect(tauriPlatform().remoteFiles).toBeDefined();
    openAs("");
    expect(tauriPlatform().remoteFiles).toBeUndefined();
  });

  it("are begun, written as the raw body of the call behind the copy's id, opened and discarded by the shell's commands", async () => {
    openAs("&daemon=me%40host&remote=host");
    const { remoteFiles } = tauriPlatform();
    invoke.mockResolvedValueOnce("copy-1");
    expect(await remoteFiles!.begin({ name: "a.pdf", size: 12, launchable: false })).toBe("copy-1");
    expect(invoke).toHaveBeenLastCalledWith("remote_file_begin", { name: "a.pdf", size: 12, launchable: false });
    invoke.mockResolvedValueOnce(undefined);
    await remoteFiles!.write("copy-1", Uint8Array.from([1, 2, 3]));
    // No header: the id's length, the id, the part, whichever channel carries the bytes.
    const [command, body, ...rest] = invoke.mock.lastCall!;
    expect(command).toBe("remote_file_write");
    expect(body).toBeInstanceOf(Uint8Array);
    expect(Array.from(body as Uint8Array)).toEqual([6, ...new TextEncoder().encode("copy-1"), 1, 2, 3]);
    expect(rest).toEqual([]);
    invoke.mockResolvedValueOnce(undefined);
    await remoteFiles!.write("copy-1", new Uint8Array());
    expect(Array.from(invoke.mock.lastCall![1] as Uint8Array)).toEqual([6, ...new TextEncoder().encode("copy-1")]);
    invoke.mockResolvedValueOnce(undefined);
    await remoteFiles!.open("copy-1");
    expect(invoke).toHaveBeenLastCalledWith("remote_file_open", { id: "copy-1" });
    invoke.mockResolvedValueOnce(undefined);
    await remoteFiles!.discard("copy-1");
    expect(invoke).toHaveBeenLastCalledWith("remote_file_discard", { id: "copy-1" });
  });

  it("reject with the kind the shell gave, and with anything else as it came", async () => {
    openAs("&daemon=me%40host&remote=host");
    const { remoteFiles } = tauriPlatform();
    for (const kind of ["launchable", "storage", "incomplete", "open"] as const) {
      invoke.mockRejectedValueOnce({ kind, detail: "text" });
      const error = await remoteFiles!.open("copy-1").catch((cause: unknown) => cause);
      expect(error).toBeInstanceOf(RemoteFileError);
      expect(error).toMatchObject({ kind, message: "text" });
    }
    invoke.mockRejectedValueOnce({ kind: "launchable" });
    expect(await remoteFiles!.begin({ name: "a.exe", size: 1, launchable: false }).catch((cause: unknown) => cause)).toMatchObject({ kind: "launchable", message: "" });
    invoke.mockRejectedValueOnce(new Error("no such command"));
    await expect(remoteFiles!.write("copy-1", new Uint8Array())).rejects.toThrow("no such command");
  });

  it("let a discard that fails pass in silence: the copy is already given up on", async () => {
    openAs("&daemon=me%40host&remote=host");
    invoke.mockRejectedValueOnce({ kind: "storage", detail: "gone" });
    await expect(tauriPlatform().remoteFiles!.discard("copy-1")).resolves.toBeUndefined();
  });
});

describe("the remote daemons", () => {
  const ENTRY = { id: "a", name: "Staging", host: "h", port: 22, user: "u", auth: "password", keyPath: null, fingerprint: null, octoboarddPath: null } as const;

  it("lists the entries and the connections through the shell and returns what it answers", async () => {
    const { remoteDaemons } = tauriPlatform();
    invoke.mockResolvedValueOnce([ENTRY]);
    expect(await remoteDaemons!.list()).toEqual([ENTRY]);
    expect(invoke).toHaveBeenLastCalledWith("remote_daemons_list");
    invoke.mockResolvedValueOnce([{ id: "a", state: "connected", via: "window" }]);
    expect(await remoteDaemons!.status()).toEqual([{ id: "a", state: "connected", via: "window" }]);
    expect(invoke).toHaveBeenLastCalledWith("remote_daemons_status");
  });

  it("saves and removes an entry by the shell's commands, and gives back what the shell saved", async () => {
    const { remoteDaemons } = tauriPlatform();
    const input = { ...ENTRY, id: null };
    invoke.mockResolvedValueOnce(ENTRY);
    expect(await remoteDaemons!.save(input)).toEqual(ENTRY);
    expect(invoke).toHaveBeenLastCalledWith("remote_daemon_save", { entry: input });
    invoke.mockResolvedValueOnce(undefined);
    await remoteDaemons!.remove("a");
    expect(invoke).toHaveBeenLastCalledWith("remote_daemon_remove", { id: "a" });
  });

  it("sends a connection's request whole, the password with it, and resolves with how it ended", async () => {
    const request = { id: "a", secret: "hunter2", confirmFingerprint: "SHA256:abc", action: "start", browser: false } as const;
    invoke.mockResolvedValueOnce({ outcome: "no_daemon" });
    expect(await tauriPlatform().remoteDaemons!.connect(request)).toEqual({ outcome: "no_daemon" });
    expect(invoke).toHaveBeenLastCalledWith("remote_daemon_connect", { request });
  });

  it("lets a failure of the connect command itself reach the caller", async () => {
    invoke.mockRejectedValueOnce(new Error("no such command"));
    await expect(tauriPlatform().remoteDaemons!.connect({ id: "a", secret: null, confirmFingerprint: null, action: null, browser: false })).rejects.toThrow("no such command");
  });

  it("disconnects and shows this computer's window by the shell's commands", async () => {
    const { remoteDaemons } = tauriPlatform();
    invoke.mockResolvedValue(undefined);
    await remoteDaemons!.disconnect("a");
    expect(invoke).toHaveBeenLastCalledWith("remote_daemon_disconnect", { id: "a" });
    await remoteDaemons!.showMainWindow();
    expect(invoke).toHaveBeenLastCalledWith("main_window_show");
  });

  it("takes what was handed over once it listens, and again whenever the shell says there is more", async () => {
    const taken = [{ id: "a", outcome: { outcome: "no_daemon" } }];
    invoke.mockImplementation(async (command: string) => (command === "remote_connect_pending_take" ? taken : undefined));
    const listener = vi.fn();
    const undo = tauriPlatform().remoteDaemons!.onHandedOver(listener);
    await afterQueuedWork();
    expect(listeners.has("remote-connect-pending")).toBe(true);
    expect(invoke).toHaveBeenCalledWith("remote_connect_pending_take");
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenLastCalledWith(taken);
    listeners.get("remote-connect-pending")!({ payload: undefined });
    await afterQueuedWork();
    expect(listener).toHaveBeenCalledTimes(2);
    // An empty answer is not passed on.
    invoke.mockImplementation(async () => []);
    listeners.get("remote-connect-pending")!({ payload: undefined });
    await afterQueuedWork();
    expect(listener).toHaveBeenCalledTimes(2);
    undo();
  });

  it("stops hearing what was handed over once undone, even where undone before the shell was listened to", async () => {
    invoke.mockImplementation(async () => [{ id: "a", outcome: { outcome: "no_daemon" } }]);
    const listener = vi.fn();
    tauriPlatform().remoteDaemons!.onHandedOver(listener)();
    await afterQueuedWork();
    expect(listeners.has("remote-connect-pending")).toBe(false);
    const heard = vi.fn();
    const undo = tauriPlatform().remoteDaemons!.onHandedOver(heard);
    await afterQueuedWork();
    expect(listeners.has("remote-connect-pending")).toBe(true);
    undo();
    expect(listeners.has("remote-connect-pending")).toBe(false);
  });

  it("makes no take once undone, and drops what a take under way brings back", async () => {
    const takes = () => invoke.mock.calls.filter(([command]) => command === "remote_connect_pending_take" || command === "remote_page_requested_take").length;
    invoke.mockImplementation(async () => []);
    // Undone before the shell was listened to: nothing is taken at all.
    tauriPlatform().remoteDaemons!.onHandedOver(vi.fn())();
    tauriPlatform().remoteDaemons!.onPageRequested(vi.fn())();
    await afterQueuedWork();
    expect(takes()).toBe(0);
    // Undone while a take is under way: its answer reaches no one.
    let answer!: (items: unknown[]) => void;
    invoke.mockImplementation(() => new Promise((resolve) => { answer = resolve; }));
    const listener = vi.fn();
    const undo = tauriPlatform().remoteDaemons!.onHandedOver(listener);
    await afterQueuedWork();
    expect(takes()).toBe(1);
    undo();
    answer([{ id: "a", outcome: { outcome: "no_daemon" } }]);
    await afterQueuedWork();
    expect(listener).not.toHaveBeenCalled();
    const asked = vi.fn();
    invoke.mockImplementation(() => new Promise((resolve) => { answer = resolve as typeof answer; }));
    const undoAdd = tauriPlatform().remoteDaemons!.onPageRequested(asked);
    await afterQueuedWork();
    undoAdd();
    (answer as unknown as (asked: boolean) => void)(true);
    await afterQueuedWork();
    expect(asked).not.toHaveBeenCalled();
  });

  it("is not stopped by the shell failing to give what was handed over", async () => {
    invoke.mockRejectedValue(new Error("no such command"));
    const listener = vi.fn();
    const undo = tauriPlatform().remoteDaemons!.onHandedOver(listener);
    await afterQueuedWork();
    expect(listener).not.toHaveBeenCalled();
    invoke.mockResolvedValue([{ id: "a", outcome: { outcome: "no_daemon" } }]);
    listeners.get("remote-connect-pending")!({ payload: undefined });
    await afterQueuedWork();
    expect(listener).toHaveBeenCalledOnce();
    undo();
  });

  it("takes a request for the Remote devices page once it listens, and again whenever the shell says so", async () => {
    invoke.mockImplementation(async (command: string) => (command === "remote_page_requested_take" ? true : undefined));
    const listener = vi.fn();
    const undo = tauriPlatform().remoteDaemons!.onPageRequested(listener);
    await vi.waitFor(() => {
      expect(listeners.has("remote-page-requested")).toBe(true);
      expect(listener).toHaveBeenCalledTimes(1);
    });
    // Nothing asked for is not passed on.
    invoke.mockImplementation(async () => false);
    listeners.get("remote-page-requested")!({ payload: undefined });
    await afterQueuedWork();
    expect(listener).toHaveBeenCalledTimes(1);
    undo();
  });

  it("hears of a change on its own window, and stops hearing once undone", async () => {
    const listener = vi.fn();
    const undo = tauriPlatform().remoteDaemons!.onChanged(listener);
    await afterQueuedWork();
    expect(listenedOn).toHaveBeenCalled();
    listeners.get("remote-daemons-changed")!({ payload: undefined });
    expect(listener).toHaveBeenCalledOnce();
    undo();
    expect(listeners.has("remote-daemons-changed")).toBe(false);
  });
});

describe("the notification-area flyout's page", () => {
  const MODEL = { tabs: [{ key: "local", label: "This PC", remote: false, raised: false }], chosen: 0, panels: [[]], shared: [] };

  function openFlyout() {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    window.history.replaceState(null, "", `/?host=${encodeURIComponent(JSON.stringify(WINDOWS_HOST))}&flyout=1`);
    return tauriPlatform().trayFlyout!;
  }

  it("asks the shell for the model, and answers none where it has none", async () => {
    const flyout = openFlyout();
    invoke.mockResolvedValueOnce(MODEL);
    expect(await flyout.model()).toEqual(MODEL);
    expect(invoke).toHaveBeenLastCalledWith("tray_flyout_model");
    invoke.mockResolvedValueOnce(null);
    expect(await flyout.model()).toBeNull();
  });

  it("tells the shell its size, what was chosen and that it was dismissed, by the shell's commands", async () => {
    const flyout = openFlyout();
    invoke.mockResolvedValue(undefined);
    await flyout.setSize({ width: 320, height: 400 });
    expect(invoke).toHaveBeenLastCalledWith("tray_flyout_size", { width: 320, height: 400 });
    await flyout.choose("session:a");
    expect(invoke).toHaveBeenLastCalledWith("tray_flyout_choose", { id: "session:a" });
    await flyout.close();
    expect(invoke).toHaveBeenLastCalledWith("tray_flyout_close");
  });

  it("hears a newer model and the flyout being shown, with the tab to start on, on its own window", async () => {
    const flyout = openFlyout();
    const onModel = vi.fn();
    const onOpened = vi.fn();
    // One at a time: the mocked module of the window is resolved by the first import, which a second one racing it
    // would not see.
    const undoModel = flyout.onModel(onModel);
    await vi.waitFor(() => expect(listeners.size).toBe(1));
    const undoOpened = flyout.onOpened(onOpened);
    await vi.waitFor(() => expect(listeners.size).toBe(2));
    listeners.get("tray-flyout-model")!({ payload: MODEL });
    listeners.get("tray-flyout-opened")!({ payload: { chosen: 1 } });
    expect(onModel).toHaveBeenCalledExactlyOnceWith(MODEL);
    expect(onOpened).toHaveBeenCalledExactlyOnceWith({ chosen: 1 });
    undoModel();
    undoOpened();
    expect(listeners.size).toBe(0);
  });
});

describe("the files dragged over the window", () => {
  function openAs(query: string): void {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    window.history.replaceState(null, "", `/?host=${encodeURIComponent(JSON.stringify(MAC_HOST))}${query}`);
  }

  it("are the local daemon's window's to take, and no other kind of window has the capability", () => {
    openAs("");
    expect(tauriPlatform().fileDrop).toBeDefined();
    openAs("&daemon=me%40host&remote=host");
    expect(tauriPlatform().fileDrop).toBeUndefined();
    openAs("&flyout=1");
    expect(tauriPlatform().fileDrop).toBeUndefined();
  });

  it("are reported to the handler with the position in CSS pixels, the shell's physical pixels divided by the device pixel ratio", async () => {
    vi.stubGlobal("devicePixelRatio", 2);
    const handler = vi.fn();
    const stop = tauriPlatform().fileDrop!.listen(handler);
    await afterQueuedWork();
    dragDrop!({ payload: { type: "enter", paths: ["/a"], position: { x: 200, y: 101 } } });
    dragDrop!({ payload: { type: "over", position: { x: 300, y: 50 } } });
    dragDrop!({ payload: { type: "drop", paths: ["/Users/u/a.txt", "/Users/u/folder"], position: { x: 10, y: 20 } } });
    dragDrop!({ payload: { type: "leave" } });
    expect(handler.mock.calls.map(([event]) => event)).toEqual([
      { kind: "over", position: { x: 100, y: 50.5 } },
      { kind: "over", position: { x: 150, y: 25 } },
      { kind: "drop", position: { x: 5, y: 10 }, paths: ["/Users/u/a.txt", "/Users/u/folder"] },
      { kind: "leave" },
    ]);
    stop();
  });

  it("are positioned as they are where the device pixel ratio is 1", async () => {
    vi.stubGlobal("devicePixelRatio", 1);
    const handler = vi.fn();
    const stop = tauriPlatform().fileDrop!.listen(handler);
    await afterQueuedWork();
    dragDrop!({ payload: { type: "over", position: { x: 33, y: 44 } } });
    expect(handler).toHaveBeenCalledExactlyOnceWith({ kind: "over", position: { x: 33, y: 44 } });
    stop();
  });

  it("stop being reported when the listener is undone", async () => {
    const stop = tauriPlatform().fileDrop!.listen(() => {});
    await afterQueuedWork();
    expect(unlistenDragDrop).not.toHaveBeenCalled();
    stop();
    expect(unlistenDragDrop).toHaveBeenCalledOnce();
  });

  it("stop being reported even when the listener is undone before the shell has been subscribed to", async () => {
    const stop = tauriPlatform().fileDrop!.listen(() => {});
    stop();
    await afterQueuedWork();
    expect(unlistenDragDrop).toHaveBeenCalledOnce();
  });
});
