// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { setLanguage } from "../i18n/language";
import { useInterfaceLanguageSync } from "./useInterfaceLanguageSync";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);

/** A stand-in for the store slice the hook reads, mutable per test. */
let fakeState: { connectionState: string; snapshotEpoch: number; settings: { interface_language?: string | null } };

vi.mock("../store", () => ({
  useDaemon: () => ({ request }),
  useDaemonStore: (selector: (state: typeof fakeState) => unknown) => selector(fakeState),
}));

function Host() {
  useInterfaceLanguageSync();
  return null;
}

function mount() {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Host />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { rerender: () => act(() => root.render(<Host />)) };
}

beforeEach(() => {
  request.mockClear();
  setLanguage("zh-Hans");
  fakeState = { connectionState: "open", snapshotEpoch: 1, settings: { interface_language: null } };
});

it("seeds a daemon that holds no language with this client's", () => {
  mount();
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", interface_language: "zh-Hans" });
});

it("leaves a language the daemon already holds alone, whatever this client's is", () => {
  fakeState.settings.interface_language = "de";
  mount();
  expect(request).not.toHaveBeenCalled();
});

it("sends nothing before the first snapshot or while the connection is down", () => {
  fakeState.snapshotEpoch = 0;
  const { rerender } = mount();
  fakeState.snapshotEpoch = 1;
  fakeState.connectionState = "reconnecting";
  rerender();
  expect(request).not.toHaveBeenCalled();
});

it("does not send again for a re-render, only once the daemon still has none after a new snapshot", () => {
  const { rerender } = mount();
  rerender();
  expect(request).toHaveBeenCalledTimes(1);
  fakeState.snapshotEpoch = 2;
  rerender();
  expect(request).toHaveBeenCalledTimes(2);
  fakeState.settings.interface_language = "zh-Hans";
  fakeState.snapshotEpoch = 3;
  rerender();
  expect(request).toHaveBeenCalledTimes(2);
});
