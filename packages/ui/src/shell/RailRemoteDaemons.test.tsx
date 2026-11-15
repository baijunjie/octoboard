// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter, RemoteDaemonConnectOutcome, RemoteDaemonEntry, RemoteDaemonStatus } from "../platform";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { BROWSER_HOST, MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { RailRemoteDaemons } from "./RailRemoteDaemons";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const entryOf = (id: string, name: string): RemoteDaemonEntry => ({
  id,
  name,
  host: `${id}.example.com`,
  port: 22,
  user: "deploy",
  auth: "device",
  keyPath: null,
  fingerprint: null,
  octoboarddPath: null,
});
const STAGING = entryOf("staging", "Staging");
const PRODUCTION = entryOf("production", "Production");
const LAB = entryOf("lab", "Home lab");

const list = vi.fn(async (): Promise<RemoteDaemonEntry[]> => [STAGING, PRODUCTION, LAB]);
const status = vi.fn(async (): Promise<RemoteDaemonStatus[]> => []);
const connect = vi.fn(async (_request: unknown): Promise<RemoteDaemonConnectOutcome> => ({ outcome: "opened" }));
const disconnect = vi.fn(async (_id: string) => {});
const openWindow = vi.fn(async (_id: string) => {});
let changed: (() => void) | undefined;
beforeEach(() => {
  acknowledgeAllAlerts();
  list.mockReset().mockResolvedValue([STAGING, PRODUCTION, LAB]);
  status.mockReset().mockResolvedValue([]);
  connect.mockReset().mockResolvedValue({ outcome: "opened" });
  disconnect.mockClear();
  openWindow.mockClear();
  changed = undefined;
});

const DESKTOP: PlatformAdapter = {
  kind: "tauri",
  host: MAC_HOST,
  remoteDaemons: {
    list: () => list(),
    status: () => status(),
    onChanged: (listener) => {
      changed = listener;
      return () => {};
    },
    save: async (entry) => ({ ...entry, id: entry.id ?? "new" }),
    remove: async () => {},
    connect: (request) => connect(request),
    disconnect: (id) => disconnect(id),
    newWindow: async () => {},
    openWindow: (id) => openWindow(id),
    showMainWindow: async () => {},
    showUpdates: async () => {},
  },
};
const BROWSER: PlatformAdapter = { kind: "browser", host: BROWSER_HOST };

async function mount(platform: PlatformAdapter = DESKTOP) {
  let container!: HTMLElement;
  await act(async () => {
    container = mountHost(<LanguageProvider><RailRemoteDaemons /></LanguageProvider>, { platform });
  });
  return container;
}

/** A button's accessible name without the isolates around the names substituted into it. */
const nameOf = (button: Element) => (button.getAttribute("aria-label") ?? "").replace(/[⁨⁩]/g, "");
const tile = (container: ParentNode, name: string) =>
  Array.from(container.querySelectorAll<HTMLButtonElement>("button")).find((button) => nameOf(button).startsWith(`${name}, `));
const menuItem = (name: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === name);
const openMenu = (container: ParentNode, name: string) =>
  act(async () => tile(container, name)!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 10, clientY: 10 })));

it("is absent in a browser", async () => {
  const container = await mount(BROWSER);
  expect(container.innerHTML).toBe("");
});

it("is absent in a remote daemon's window, which has the capability too", async () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  window.history.replaceState({}, "", "/?remote=server");
  try {
    status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
    const container = await mount();
    expect(container.innerHTML).toBe("");
  } finally {
    window.history.replaceState({}, "", "/");
    vi.unstubAllGlobals();
  }
});

it("has no group and no groove while nothing is connected, though daemons are saved", async () => {
  const container = await mount();
  expect(container.innerHTML).toBe("");
});

it("shows only the connected daemons, each named with its address and state, with no Add button", async () => {
  status.mockResolvedValue([
    { id: "staging", state: "connected", via: "window" },
    { id: "production", state: "connected", via: "browser" },
  ]);
  const container = await mount();
  expect(nameOf(tile(container, "Staging")!)).toBe("Staging, deploy@staging.example.com:22, Connected");
  expect(nameOf(tile(container, "Production")!)).toBe("Production, deploy@production.example.com:22, Opened in browser");
  expect(tile(container, "Home lab")).toBeUndefined();
  // The two tiles and nothing else to press: each tile's menu opens from a right-click on it, its trigger not drawn.
  const shown = Array.from(container.querySelectorAll("button")).filter((button) => !button.closest(".hidden"));
  expect(shown).toHaveLength(2);
  expect(container.querySelector("[role=group]")!.getAttribute("aria-label")).toBe("Remote daemons");
});

it("shows a reconnecting connection as such", async () => {
  status.mockResolvedValue([{ id: "staging", state: "reconnecting", via: "window" }]);
  const container = await mount();
  expect(nameOf(tile(container, "Staging")!)).toContain("Reconnecting");
});

it("reads the entries and their states again when the shell says they changed", async () => {
  const container = await mount();
  expect(container.innerHTML).toBe("");
  status.mockResolvedValue([{ id: "lab", state: "connected", via: "window" }]);
  await act(async () => changed!());
  expect(tile(container, "Home lab")).toBeDefined();
  status.mockResolvedValue([]);
  await act(async () => changed!());
  expect(container.innerHTML).toBe("");
});

it("calls connect on a press, which the shell answers by focusing the window or opening the browser", async () => {
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
  const container = await mount();
  await act(async () => tile(container, "Staging")!.click());
  expect(connect).toHaveBeenCalledWith({ id: "staging", secret: null, confirmFingerprint: null, action: null, browser: false });
  expect(openWindow).not.toHaveBeenCalled();
});

it("opens the daemon's window through the shell when connect no longer finds it connected", async () => {
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
  connect.mockResolvedValueOnce({ outcome: "secret_needed", kind: "password" });
  const container = await mount();
  await act(async () => tile(container, "Staging")!.click());
  expect(openWindow).toHaveBeenCalledWith("staging");
});

it("offers Open and Disconnect from a tile's context menu, and nothing else", async () => {
  status.mockResolvedValue([{ id: "production", state: "connected", via: "browser" }]);
  const container = await mount();
  await openMenu(container, "Production");
  expect([...document.querySelectorAll("[role=menuitem]")].map((item) => item.textContent)).toEqual(["Open", "Disconnect"]);
  await act(async () => menuItem("Disconnect")!.click());
  expect(disconnect).toHaveBeenCalledWith("production");
});

it("shows an alert when a press fails", async () => {
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
  connect.mockRejectedValueOnce(new Error("no such command"));
  const container = await mount();
  await act(async () => tile(container, "Staging")!.click());
  expect(errorAlerts.getSnapshot()[0]!.title).toBe("Could not connect to the remote daemon");
});

it("opens the daemon's window from the context menu's Open as a press does", async () => {
  status.mockResolvedValue([{ id: "production", state: "connected", via: "browser" }]);
  connect.mockResolvedValueOnce({ outcome: "opened_in_browser" });
  const container = await mount();
  await openMenu(container, "Production");
  await act(async () => menuItem("Open")!.click());
  expect(connect).toHaveBeenCalledWith({ id: "production", secret: null, confirmFingerprint: null, action: null, browser: false });
  expect(openWindow).not.toHaveBeenCalled();
});

it("reads the states again after Disconnect, so that the tile goes", async () => {
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
  const container = await mount();
  status.mockResolvedValue([]);
  await openMenu(container, "Staging");
  await act(async () => menuItem("Disconnect")!.click());
  expect(disconnect).toHaveBeenCalledWith("staging");
  expect(container.innerHTML).toBe("");
});

it("shows an alert when Disconnect fails, and keeps the tile", async () => {
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
  disconnect.mockRejectedValueOnce(new Error("busy"));
  const container = await mount();
  await openMenu(container, "Staging");
  await act(async () => menuItem("Disconnect")!.click());
  expect(errorAlerts.getSnapshot()[0]!.title).toBe("Could not disconnect from the remote daemon");
  expect(tile(container, "Staging")).toBeDefined();
});

it("shows an alert when the window cannot be opened after connect found the daemon no longer connected", async () => {
  status.mockResolvedValue([{ id: "staging", state: "connected", via: "window" }]);
  connect.mockResolvedValueOnce({ outcome: "secret_needed", kind: "password" });
  openWindow.mockRejectedValueOnce(new Error("no window"));
  const container = await mount();
  await act(async () => tile(container, "Staging")!.click());
  expect(errorAlerts.getSnapshot()[0]!.title).toBe("Could not connect to the remote daemon");
});

it("tells a connection that is being brought back from one that is up, in words as well as in the badge", async () => {
  status.mockResolvedValue([
    { id: "staging", state: "connected", via: "window" },
    { id: "production", state: "reconnecting", via: "window" },
  ]);
  const container = await mount();
  expect(nameOf(tile(container, "Staging")!)).toMatch(/, Connected$/);
  expect(nameOf(tile(container, "Production")!)).toMatch(/, Reconnecting$/);
});

it("lists a daemon held for the browser and one with a window side by side", async () => {
  status.mockResolvedValue([
    { id: "lab", state: "connected", via: "window" },
    { id: "staging", state: "connected", via: "browser" },
  ]);
  const container = await mount();
  const names = Array.from(container.querySelectorAll("button")).filter((button) => !button.closest(".hidden")).map((button) => nameOf(button).split(",")[0]);
  expect([...names].sort()).toEqual(["Home lab", "Staging"]);
});
