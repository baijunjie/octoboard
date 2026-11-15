// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { currentLanguage, setLanguage } from "../i18n/language";
import { DEFAULT_SETTINGS, type Daemon, type State } from "../store";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { useAgentLanguageSeed } from "./useAgentLanguageSeed";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);

let daemon: Daemon;
/** Puts `patch` to the store, as the daemon connection does. */
const set = (patch: Partial<State>) => act(() => daemon.store.setState(patch));
const setAgentLanguage = (agent_language: string | null) => set({ settings: { ...daemon.store.getState().settings, agent_language } });

function Host() {
  useAgentLanguageSeed();
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
  daemon = testDaemon({ state: { connectionState: "open", snapshotEpoch: 1, settings: { ...DEFAULT_SETTINGS, agent_language: null } }, request });
});

it("seeds a daemon that holds no agent language with the interface language this window shows", () => {
  mount();
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", agent_language: "zh-Hans" });
  expect(currentLanguage()).toBe("zh-Hans");
});

it("leaves the window's language as it is whatever agent language the daemon holds, and sends nothing", () => {
  setAgentLanguage("de");
  mount();
  setAgentLanguage("ja");
  expect(currentLanguage()).toBe("zh-Hans");
  expect(request).not.toHaveBeenCalled();
});

it("does not report a change of the window's language to a daemon that holds an agent language", () => {
  setAgentLanguage("de");
  const { rerender } = mount();
  act(() => setLanguage("ja"));
  rerender();
  set({ snapshotEpoch: 2 });
  rerender();
  expect(request).not.toHaveBeenCalled();
  expect(currentLanguage()).toBe("ja");
});

it("changes nothing before the first snapshot or while the connection is down", () => {
  set({ snapshotEpoch: 0 });
  const { rerender } = mount();
  set({ snapshotEpoch: 1, connectionState: "reconnecting" });
  rerender();
  expect(request).not.toHaveBeenCalled();
});

it("does not seed again for a re-render, only once the daemon still has none after a new snapshot", () => {
  const { rerender } = mount();
  rerender();
  expect(request).toHaveBeenCalledTimes(1);
  set({ snapshotEpoch: 2 });
  rerender();
  expect(request).toHaveBeenCalledTimes(2);
  setAgentLanguage("zh-Hans");
  set({ snapshotEpoch: 3 });
  rerender();
  expect(request).toHaveBeenCalledTimes(2);
});

it("trusts nothing the previous connection left until a snapshot arrives after a reconnect", () => {
  setAgentLanguage("de");
  const { rerender } = mount();
  act(() => setLanguage("ja"));
  set({ connectionState: "reconnecting" });
  rerender();
  set({ connectionState: "open" });
  rerender();
  setAgentLanguage(null);
  expect(request).not.toHaveBeenCalled();
  set({ snapshotEpoch: 2 });
  rerender();
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", agent_language: "ja" });
});
