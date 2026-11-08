// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { runInBackground } from "../lifecycle/runInBackground";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import { connectShellSettings } from "../lifecycle/shellSettings";
import type { PlatformAdapter } from "../platform";
import { mountHost } from "../test/mountHost";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { AppBehaviorSection } from "./AppBehaviorSection";
import { BROWSER_HOST } from "../test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rows beside it read the daemon's settings, which this test has no use for.
vi.mock("./DaemonSwitchSetting", () => ({ DaemonSwitchSetting: () => null }));

function mount(platform: PlatformAdapter) {
  return mountHost(
    <LanguageProvider>
      <AppBehaviorSection />
    </LanguageProvider>,
    { platform },
  );
}

beforeEach(() => {
  // The preference is a module-wide singleton that outlives a test, so put the in-memory value back
  // to the default; clearing storage alone would leave it as the last test set it.
  runInBackground.set(false);
  localStorage.clear();
});

// Leave nothing behind for a later file, should vitest ever run files without isolation: a
// module-level read there would pick up what the last test stored.
afterEach(() => {
  connectShellSettings({});
  localStorage.clear();
});

it("has no row without the capability", () => {
  const container = mount({ kind: "browser", host: BROWSER_HOST });
  expect(container.querySelector("[role=switch], input[type=checkbox]")).toBeNull();
});

it("pushes the choice to the shell when the switch is toggled", async () => {
  const set = vi.fn(() => Promise.resolve());
  connectShellSettings({ runInBackground: set });
  const container = mount({ kind: "tauri", host: BROWSER_HOST, backgroundRun: { set } });
  await act(async () => {});
  const toggle = container.querySelector<HTMLInputElement>("input[type=checkbox]")!;
  expect(toggle.checked).toBe(false);
  expect(set).not.toHaveBeenCalled();
  await act(async () => {
    toggle.click();
  });
  expect(toggle.checked).toBe(true);
  expect(set).toHaveBeenCalledExactlyOnceWith(true);
  // The shell keeps the choice, so nothing is stored by the page.
  expect(localStorage.getItem(PREFERENCE_KEYS.runInBackground)).toBeNull();
});

it("names the menu bar icon as the way back only while it is shown", async () => {
  const container = mount({ kind: "tauri", host: BROWSER_HOST, backgroundRun: { set: async () => {} }, statusItem: { setMenu: async () => {}, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} } });
  await act(async () => {
    showMenuBarIcon.set(true);
  });
  expect(container.textContent).toContain("brings the window back");
  await act(async () => {
    showMenuBarIcon.set(false);
  });
  expect(container.textContent).not.toContain("brings the window back");
  expect(container.textContent).toContain("Open Octoboard again or click one of its notifications");
  showMenuBarIcon.set(true);
});
