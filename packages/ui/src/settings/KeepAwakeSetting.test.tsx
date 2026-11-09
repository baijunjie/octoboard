// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { sessionOf } from "../fixtures/builders";
import { LanguageProvider } from "../i18n/react";
import { useKeepAwakeSync } from "../lifecycle/useKeepAwakeSync";
import type { PlatformAdapter } from "../platform";
import type { Event } from "../protocol";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { BROWSER_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { AppBehaviorSection } from "./AppBehaviorSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rows beside it read the daemon's settings, which this test has no use for; Prevent sleep's own switch stays real.
vi.mock("./DaemonSwitchSetting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./DaemonSwitchSetting")>();
  return {
    DaemonSwitchSetting: (props: Parameters<typeof actual.DaemonSwitchSetting>[0]) =>
      props.field === "keep_awake" ? <actual.DaemonSwitchSetting {...props} /> : null,
  };
});

const HOST = { os: "linux", path_style: "unix", file_links: true, trash: "trash", settings: "settings" } as const;

function Sync() {
  useKeepAwakeSync();
  return null;
}

function mount(daemon: ReturnType<typeof testDaemon>, platform: PlatformAdapter = { kind: "browser", host: BROWSER_HOST }) {
  return mountHost(
    <LanguageProvider>
      <Sync />
      <AppBehaviorSection />
    </LanguageProvider>,
    { platform, daemon },
  );
}

const request = vi.fn(async (_body: unknown): Promise<Event> => ({ type: "ack", id: null }));
beforeEach(() => request.mockClear());
const withHost = (keep_awake: boolean, settings = DEFAULT_SETTINGS) => testDaemon({ request, state: { connectionState: "open", host: { ...HOST, keep_awake }, settings } });

it("has no row where the daemon's host cannot be held awake", () => {
  expect(mount(withHost(false)).textContent).not.toContain("Prevent sleep");
  expect(mount(testDaemon()).textContent).not.toContain("Prevent sleep");
});

it("shows the row in a browser too, where the daemon's host can be held awake, whatever the window runs on", () => {
  const container = mount(withHost(true));
  expect(container.textContent).toContain("Prevent sleep");
  expect(container.textContent).not.toContain("When to prevent sleep");
});

it("changes the daemon's settings, showing the choice of when only while the switch is on", async () => {
  const daemon = withHost(true);
  const container = mount(daemon);
  const toggle = container.querySelector<HTMLInputElement>("input[type=checkbox]:not([aria-checked=true])");
  await act(async () => {
    toggle!.click();
  });
  expect(request).toHaveBeenCalledWith({ type: "update_settings", keep_awake: true });
  await act(async () => {
    daemon.store.setState({ settings: { ...DEFAULT_SETTINGS, keep_awake: true } });
  });
  expect(container.textContent).toContain("When to prevent sleep");
});

it("sends the choice of when to prevent sleep to the daemon", async () => {
  const container = mount(withHost(true, { ...DEFAULT_SETTINGS, keep_awake: true }));
  const choice = container.querySelector<HTMLElement>('button[aria-label="When to prevent sleep"], [aria-label="When to prevent sleep"] button');
  await act(async () => choice!.click());
  await act(async () => Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]')).find((option) => option.textContent?.includes("Always"))!.click());
  expect(request).toHaveBeenCalledWith({ type: "update_settings", keep_awake_when: "always" });
});

it("hands focus to the switch when an import turns it off while focus is on the choice of when", async () => {
  const daemon = withHost(true, { ...DEFAULT_SETTINGS, keep_awake: true });
  const container = mount(daemon);
  const choice = container.querySelector<HTMLElement>('button[aria-label="When to prevent sleep"], [aria-label="When to prevent sleep"] button');
  act(() => choice!.focus());
  expect(document.activeElement).toBe(choice);
  await act(async () => {
    daemon.store.setState({ settings: DEFAULT_SETTINGS });
  });
  expect(container.textContent).not.toContain("When to prevent sleep");
  expect(document.activeElement).toBe(container.querySelector('input[role="switch"]'));
});

it("tells the shell whether the daemon is keeping the computer awake, and not while the connection is down", async () => {
  const set = vi.fn(() => Promise.resolve());
  const daemon = withHost(true);
  mount(daemon, { kind: "tauri", host: BROWSER_HOST, keepAwake: { set } });
  await act(async () => {});
  expect(set).toHaveBeenLastCalledWith(false);
  await act(async () => {
    daemon.store.setState({ keepAwakeHeld: true });
  });
  expect(set).toHaveBeenLastCalledWith(true);
  await act(async () => {
    daemon.store.setState({ connectionState: "closed" });
  });
  expect(set).toHaveBeenLastCalledWith(false);
});

it("goes on telling the shell when one push is rejected", async () => {
  // The shell answers with an error where it has no menu bar icon.
  const set = vi.fn().mockRejectedValueOnce(new Error("the menu bar icon does not exist")).mockResolvedValue(undefined);
  const daemon = withHost(true);
  mount(daemon, { kind: "tauri", host: BROWSER_HOST, keepAwake: { set } });
  await act(async () => {});
  await act(async () => {
    daemon.store.setState({ keepAwakeHeld: true });
  });
  expect(set).toHaveBeenCalledTimes(2);
  expect(set).toHaveBeenLastCalledWith(true);
});

it("leaves what the shell is told to the daemon, whatever the sessions are doing", async () => {
  const set = vi.fn(() => Promise.resolve());
  const daemon = withHost(true, { ...DEFAULT_SETTINGS, keep_awake: true });
  mount(daemon, { kind: "tauri", host: BROWSER_HOST, keepAwake: { set } });
  await act(async () => {
    daemon.store.setState({ sessions: new Map([["s", sessionOf("s", "c", "p", "Title", "working")]]) });
  });
  expect(set).toHaveBeenLastCalledWith(false);
});
