// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { setLanguage } from "../i18n/language";
import { DEFAULT_SETTINGS, type Daemon, type State } from "../store";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { useInterfaceLanguageSync } from "./useInterfaceLanguageSync";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);

let daemon: Daemon;
/** Puts `patch` to the store, as the daemon connection does. */
const set = (patch: Partial<State>) => act(() => daemon.store.setState(patch));
const setLanguageOnDaemon = (interface_language: string | null) => set({ settings: { ...daemon.store.getState().settings, interface_language } });

function Host() {
  useInterfaceLanguageSync();
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

it("seeds a daemon that holds no language with this client's", () => {
  mount();
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", interface_language: "zh-Hans" });
});

it("leaves a language the daemon already holds alone, whatever this client's is", () => {
  setLanguageOnDaemon("de");
  mount();
  expect(request).not.toHaveBeenCalled();
});

it("sends nothing before the first snapshot or while the connection is down", () => {
  set({ snapshotEpoch: 0 });
  const { rerender } = mount();
  set({ snapshotEpoch: 1, connectionState: "reconnecting" });
  rerender();
  expect(request).not.toHaveBeenCalled();
});

it("does not send again for a re-render, only once the daemon still has none after a new snapshot", () => {
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
