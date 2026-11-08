// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { runInBackground } from "../lifecycle/runInBackground";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import { useMenuBarIconSync } from "../lifecycle/useMenuBarIconSync";
import type { PlatformAdapter, StatusItemCapability } from "../platform";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { BROWSER_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { AppBehaviorSection } from "./AppBehaviorSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rows beside it read the daemon's settings, which this test has no use for.
vi.mock("./DaemonSwitchSetting", () => ({ DaemonSwitchSetting: () => null }));

function Sync() {
  useMenuBarIconSync();
  return null;
}

function statusItem(setVisible: StatusItemCapability["setVisible"]): StatusItemCapability {
  return { setMenu: async () => {}, setVisible, onSessionChosen: () => () => {}, onOpened: () => () => {} };
}

function mount(platform: PlatformAdapter) {
  return mountHost(
    <LanguageProvider>
      <Sync />
      <AppBehaviorSection />
    </LanguageProvider>,
    { platform },
  );
}

beforeEach(() => {
  // The preferences are module-wide singletons that outlive a test, so put the in-memory values back to their defaults.
  showMenuBarIcon.set(true);
  runInBackground.set(false);
  localStorage.clear();
});

afterEach(() => {
  localStorage.clear();
});

it("has no row without the capability", () => {
  const container = mount({ kind: "browser", host: BROWSER_HOST });
  expect(container.textContent).not.toContain("icon");
});

it("tells the shell the default choice at startup", async () => {
  const setVisible = vi.fn(() => Promise.resolve());
  mount({ kind: "tauri", host: BROWSER_HOST, statusItem: statusItem(setVisible) });
  await act(async () => {});
  expect(setVisible).toHaveBeenCalledExactlyOnceWith(true);
});

it("pushes and stores the choice when the switch is toggled", async () => {
  const setVisible = vi.fn(() => Promise.resolve());
  const container = mount({ kind: "tauri", host: BROWSER_HOST, statusItem: statusItem(setVisible) });
  await act(async () => {});
  const toggle = container.querySelector<HTMLInputElement>("input[type=checkbox]")!;
  expect(toggle.checked).toBe(true);
  await act(async () => {
    toggle.click();
  });
  expect(toggle.checked).toBe(false);
  expect(setVisible).toHaveBeenLastCalledWith(false);
  expect(localStorage.getItem(PREFERENCE_KEYS.showMenuBarIcon)).toBe("false");
});

it("says how the window comes back only while the icon is hidden and the app keeps running in the background", async () => {
  const container = mount({ kind: "tauri", host: BROWSER_HOST, statusItem: statusItem(async () => {}) });
  const hint = "open Octoboard again or click one of its notifications";
  await act(async () => {
    showMenuBarIcon.set(false);
  });
  expect(container.textContent).not.toContain(hint);
  await act(async () => {
    runInBackground.set(true);
  });
  expect(container.textContent).toContain(hint);
  await act(async () => {
    showMenuBarIcon.set(true);
  });
  expect(container.textContent).not.toContain(hint);
});
