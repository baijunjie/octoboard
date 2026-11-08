// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { runInBackground } from "../lifecycle/runInBackground";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import { useBackgroundRunSync } from "../lifecycle/useBackgroundRunSync";
import type { PlatformAdapter } from "../platform";
import { mountHost } from "../test/mountHost";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { AppBehaviorSection } from "./AppBehaviorSection";
import { BROWSER_HOST } from "../test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rows beside it read the daemon's settings, which this test has no use for.
vi.mock("./DaemonSwitchSetting", () => ({ DaemonSwitchSetting: () => null }));

function Sync() {
  useBackgroundRunSync();
  return null;
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
  // The preference is a module-wide singleton that outlives a test, so put the in-memory value back
  // to the default; clearing storage alone would leave it as the last test set it.
  runInBackground.set(false);
  localStorage.clear();
});

// Leave nothing behind for a later file, should vitest ever run files without isolation: a
// module-level read there would pick up what the last test stored.
afterEach(() => {
  localStorage.clear();
});

it("has no row without the capability", () => {
  const container = mount({ kind: "browser", host: BROWSER_HOST });
  expect(container.querySelector("[role=switch], input[type=checkbox]")).toBeNull();
});

it("tells the shell the default choice at startup", async () => {
  const set = vi.fn(() => Promise.resolve());
  mount({ kind: "tauri", host: BROWSER_HOST, backgroundRun: { set } });
  await act(async () => {});
  expect(set).toHaveBeenCalledExactlyOnceWith(false);
});

// The preference reads storage as its module loads, so a stored choice is only seen by a fresh load.
it("tells the shell a stored choice at startup", async () => {
  localStorage.setItem(PREFERENCE_KEYS.runInBackground, "true");
  vi.resetModules();
  onTestFinished(() => {
    vi.resetModules();
  });
  const { useBackgroundRunSync: freshSync } = await import("../lifecycle/useBackgroundRunSync");
  const { PlatformProvider: FreshProvider } = await import("../platform/react");
  const set = vi.fn(() => Promise.resolve());
  const Host = () => {
    freshSync();
    return null;
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <FreshProvider value={{ kind: "tauri", host: BROWSER_HOST, backgroundRun: { set } }}>
        <Host />
      </FreshProvider>,
    ),
  );
  await act(async () => {});
  expect(set).toHaveBeenCalledExactlyOnceWith(true);
});

it("pushes and stores the choice when the switch is toggled", async () => {
  const set = vi.fn(() => Promise.resolve());
  const container = mount({ kind: "tauri", host: BROWSER_HOST, backgroundRun: { set } });
  await act(async () => {});
  const toggle = container.querySelector<HTMLInputElement>("input[type=checkbox]")!;
  expect(toggle.checked).toBe(false);
  await act(async () => {
    toggle.click();
  });
  expect(toggle.checked).toBe(true);
  expect(set).toHaveBeenLastCalledWith(true);
  expect(localStorage.getItem(PREFERENCE_KEYS.runInBackground)).toBe("true");
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
