// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { sessionOf } from "../fixtures/builders";
import { LanguageProvider } from "../i18n/react";
import { keepAwake, keepAwakeWhen } from "../lifecycle/keepAwake";
import { useKeepAwakeSync } from "../lifecycle/useKeepAwakeSync";
import type { PlatformAdapter } from "../platform";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { BROWSER_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { AppBehaviorSection } from "./AppBehaviorSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rows beside it read the daemon's settings, which this test has no use for.
vi.mock("./DaemonSwitchSetting", () => ({ DaemonSwitchSetting: () => null }));

function Sync() {
  useKeepAwakeSync();
  return null;
}

function mount(platform: PlatformAdapter, daemon = testDaemon()) {
  return mountHost(
    <LanguageProvider>
      <Sync />
      <AppBehaviorSection />
    </LanguageProvider>,
    { platform, daemon },
  );
}

beforeEach(() => {
  // The preferences are module-wide singletons that outlive a test, so put the in-memory values back to their defaults.
  keepAwake.set(false);
  keepAwakeWhen.set("working");
  localStorage.clear();
});

afterEach(() => {
  localStorage.clear();
});

it("has no row without the capability", () => {
  const container = mount({ kind: "browser", host: BROWSER_HOST });
  expect(container.textContent).not.toContain("Prevent sleep");
});

it("tells the shell to hold nothing at startup", async () => {
  const set = vi.fn(() => Promise.resolve());
  mount({ kind: "tauri", host: BROWSER_HOST, keepAwake: { set } });
  await act(async () => {});
  expect(set).toHaveBeenCalledExactlyOnceWith(false);
});

it("shows the choice of when only while the switch is on, and stores both", async () => {
  const set = vi.fn(() => Promise.resolve());
  const container = mount({ kind: "tauri", host: BROWSER_HOST, keepAwake: { set } });
  await act(async () => {});
  expect(container.textContent).not.toContain("When to prevent sleep");
  const toggle = container.querySelector<HTMLInputElement>("input[type=checkbox]:not([aria-checked=true])");
  await act(async () => {
    toggle!.click();
  });
  expect(localStorage.getItem(PREFERENCE_KEYS.keepAwake)).toBe("true");
  expect(container.textContent).toContain("When to prevent sleep");
  // With no session working, "while sessions are working" holds nothing.
  expect(set).toHaveBeenLastCalledWith(false);
  await act(async () => {
    keepAwakeWhen.set("always");
  });
  expect(set).toHaveBeenLastCalledWith(true);
});

it("holds while a session is working and lets go when none is, under the choice of working", async () => {
  const set = vi.fn(() => Promise.resolve());
  keepAwake.set(true);
  const daemon = testDaemon();
  mount({ kind: "tauri", host: BROWSER_HOST, keepAwake: { set } }, daemon);
  await act(async () => {});
  expect(set).toHaveBeenLastCalledWith(false);
  await act(async () => {
    daemon.store.setState({ sessions: new Map([["s", sessionOf("s", "c", "p", "Title", "working")]]) });
  });
  expect(set).toHaveBeenLastCalledWith(true);
  await act(async () => {
    daemon.store.setState({ sessions: new Map([["s", sessionOf("s", "c", "p", "Title", "waiting_user")]]) });
  });
  expect(set).toHaveBeenLastCalledWith(false);
});

it("hands focus to the switch when an import turns it off while focus is on the choice of when", async () => {
  keepAwake.set(true);
  const container = mount({ kind: "tauri", host: BROWSER_HOST, keepAwake: { set: vi.fn(() => Promise.resolve()) } });
  await act(async () => {});
  const choice = container.querySelector<HTMLElement>('button[aria-label="When to prevent sleep"], [aria-label="When to prevent sleep"] button');
  act(() => choice!.focus());
  expect(document.activeElement).toBe(choice);
  await act(async () => {
    keepAwake.set(false);
  });
  expect(container.textContent).not.toContain("When to prevent sleep");
  expect(document.activeElement).toBe(container.querySelector('input[role="switch"]'));
});
