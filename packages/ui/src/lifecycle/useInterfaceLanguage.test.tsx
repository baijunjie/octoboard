// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { currentLanguage, setLanguage } from "../i18n/language";
import { DEFAULT_SETTINGS, type Daemon, type State } from "../store";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { useInterfaceLanguage } from "./useInterfaceLanguage";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);

let daemon: Daemon;
/** Puts `patch` to the store, as the daemon connection does. */
const set = (patch: Partial<State>) => act(() => daemon.store.setState(patch));
const setLanguageOnDaemon = (interface_language: string | null) => set({ settings: { ...daemon.store.getState().settings, interface_language } });

function Host() {
  useInterfaceLanguage();
  return null;
}

function mount() {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = () => act(() => root.render(<TestHost daemon={daemon}><Host /></TestHost>));
  render();
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { rerender: render };
}

beforeEach(() => {
  request.mockClear();
  setLanguage("zh-Hans");
  daemon = testDaemon({ state: { connectionState: "open", snapshotEpoch: 1, settings: { ...DEFAULT_SETTINGS, interface_language: null } }, request });
});

it("seeds a daemon that holds no language with the one this window shows", () => {
  mount();
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", interface_language: "zh-Hans" });
  expect(currentLanguage()).toBe("zh-Hans");
});

it("shows the language the daemon holds, whatever this window showed, and sends nothing", () => {
  setLanguageOnDaemon("de");
  mount();
  expect(currentLanguage()).toBe("de");
  expect(request).not.toHaveBeenCalled();
});

it("follows the daemon when another client changes its language", () => {
  setLanguageOnDaemon("de");
  mount();
  setLanguageOnDaemon("ja");
  expect(currentLanguage()).toBe("ja");
  expect(request).not.toHaveBeenCalled();
});

it("keeps the language it shows, and writes nothing, for a tag this build does not offer", () => {
  setLanguageOnDaemon("tlh");
  mount();
  expect(currentLanguage()).toBe("zh-Hans");
  expect(request).not.toHaveBeenCalled();
});

it("does not undo a pick of this window while the daemon still holds the old language", () => {
  setLanguageOnDaemon("de");
  const { rerender } = mount();
  act(() => setLanguage("ja"));
  rerender();
  set({ snapshotEpoch: 2 });
  rerender();
  expect(currentLanguage()).toBe("ja");
  setLanguageOnDaemon("ja");
  expect(currentLanguage()).toBe("ja");
});

it("changes nothing before the first snapshot or while the connection is down", () => {
  setLanguageOnDaemon("de");
  set({ snapshotEpoch: 0 });
  const { rerender } = mount();
  expect(currentLanguage()).toBe("zh-Hans");
  set({ snapshotEpoch: 1, connectionState: "reconnecting" });
  rerender();
  expect(currentLanguage()).toBe("zh-Hans");
  expect(request).not.toHaveBeenCalled();
  setLanguageOnDaemon(null);
  expect(request).not.toHaveBeenCalled();
});

it("does not seed again for a re-render, only once the daemon still has none after a new snapshot", () => {
  const { rerender } = mount();
  rerender();
  expect(request).toHaveBeenCalledTimes(1);
  set({ snapshotEpoch: 2 });
  rerender();
  expect(request).toHaveBeenCalledTimes(2);
  setLanguageOnDaemon("zh-Hans");
  set({ snapshotEpoch: 3 });
  rerender();
  expect(request).toHaveBeenCalledTimes(2);
});

it("trusts nothing the previous connection left until a snapshot arrives after a reconnect", () => {
  setLanguageOnDaemon("de");
  const { rerender } = mount();
  act(() => setLanguage("ja"));
  set({ connectionState: "reconnecting" });
  rerender();
  set({ connectionState: "open" });
  rerender();
  expect(currentLanguage()).toBe("ja");
  expect(request).not.toHaveBeenCalled();
  setLanguageOnDaemon(null);
  expect(request).not.toHaveBeenCalled();
  set({ snapshotEpoch: 2 });
  rerender();
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", interface_language: "ja" });
});
