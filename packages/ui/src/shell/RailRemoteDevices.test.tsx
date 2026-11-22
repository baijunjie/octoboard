// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter, RemoteDaemonStatus } from "../platform";
import { BROWSER_HOST, MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { RemoteDevicesTestHost } from "../remoteDaemons/remoteDevicesTestSupport";
import { RailRemoteDevices } from "./RailRemoteDevices";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const status = vi.fn(async (): Promise<RemoteDaemonStatus[]> => []);
let changed: (() => void) | undefined;
beforeEach(() => {
  status.mockReset().mockResolvedValue([]);
  changed = undefined;
});

const DESKTOP: PlatformAdapter = {
  kind: "tauri",
  host: MAC_HOST,
  remoteDaemons: {
    list: async () => [],
    status: () => status(),
    onChanged: (listener) => {
      changed = listener;
      return () => {};
    },
    save: async (entry) => ({ entry: { ...entry, id: entry.id ?? "new", deviceKey: { declined: false, file: null, mark: null } }, keyRemoval: { line: { result: "not_installed" as const }, privateKeyLeft: null } }),
    remove: async () => ({ line: { result: "not_installed" as const }, privateKeyLeft: null }),
    setDeviceKey: async () => ({ line: { result: "not_installed" as const }, privateKeyLeft: null }),
    connect: async () => ({ outcome: "opened" }),
    disconnect: async () => {},
    onHandedOver: () => () => {},
    onPageRequested: () => () => {},
    showMainWindow: async () => {},
    showUpdates: async () => {},
  },
};
const BROWSER: PlatformAdapter = { kind: "browser", host: BROWSER_HOST };

async function mount(platform: PlatformAdapter = DESKTOP, shown = false, onPress: () => void = () => {}) {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  onTestFinished(() => void vi.unstubAllGlobals());
  let container!: HTMLElement;
  await act(async () => {
    container = mountHost(<LanguageProvider><RemoteDevicesTestHost capability={platform.remoteDaemons}><RailRemoteDevices shown={shown} onPress={onPress} /></RemoteDevicesTestHost></LanguageProvider>, { platform });
  });
  return container;
}

const button = (container: ParentNode) => container.querySelector<HTMLButtonElement>("button")!;

it("is absent in a browser", async () => {
  const container = await mount(BROWSER);
  expect(container.innerHTML).toBe("");
});

it("is absent in a remote daemon's window, which has the capability too", async () => {
  window.history.replaceState({}, "", "/?remote=server");
  onTestFinished(() => window.history.replaceState({}, "", "/"));
  const container = await mount();
  expect(container.innerHTML).toBe("");
});

it("is always there while remote daemons are offered, named Remote devices, with no badge while nothing is connected", async () => {
  const container = await mount();
  expect(button(container).getAttribute("aria-label")).toBe("Remote devices");
  // The glyph alone: no count.
  expect(container.textContent).toBe("");
});

it("counts the held connections in the badge and in the button's name, and follows the shell's changes", async () => {
  status.mockResolvedValue([
    { id: "staging", state: "connected", via: "window" },
    { id: "production", state: "reconnecting", via: "browser" },
  ]);
  const container = await mount();
  expect(button(container).getAttribute("aria-label")).toBe("Remote devices, connected: 2");
  expect(container.textContent).toBe("2");
  status.mockResolvedValue([]);
  await act(async () => changed!());
  expect(button(container).getAttribute("aria-label")).toBe("Remote devices");
  expect(container.textContent).toBe("");
});

it("is a plain button: a press is the caller's, and it announces no menu and no key", async () => {
  const onPress = vi.fn();
  const container = await mount(DESKTOP, false, onPress);
  await act(async () => button(container).click());
  expect(onPress).toHaveBeenCalledTimes(1);
  expect(button(container).hasAttribute("aria-haspopup")).toBe(false);
  expect(button(container).hasAttribute("aria-keyshortcuts")).toBe(false);
});

it("is the current item while its page shows", async () => {
  expect(button(await mount(DESKTOP, false)).hasAttribute("aria-current")).toBe(false);
  expect(button(await mount(DESKTOP, true)).getAttribute("aria-current")).toBe("true");
});

it("opens no menu from the keyboard: not on ArrowDown, the Menu key or Shift+F10", async () => {
  const container = await mount();
  const target = button(container);
  act(() => target.focus());
  for (const init of [{ key: "ArrowDown" }, { key: "ContextMenu" }, { key: "F10", shiftKey: true }]) {
    await act(async () => void target.dispatchEvent(new KeyboardEvent("keydown", { ...init, bubbles: true, cancelable: true })));
    expect(document.querySelector("[role=menu]")).toBeNull();
  }
  expect(target.hasAttribute("aria-expanded")).toBe(false);
});

it("opens no menu from a right click either", async () => {
  const container = await mount();
  await act(async () => void button(container).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true })));
  expect(document.querySelector("[role=menu]")).toBeNull();
});
