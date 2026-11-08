// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { shellSettingsFromQuery } from "./shellSettings";

const query = (value: unknown) => `?port=1&settings=${encodeURIComponent(typeof value === "string" ? value : JSON.stringify(value))}`;

describe("shellSettingsFromQuery", () => {
  it("reads the values and the stored marker the shell put in the query", () => {
    expect(shellSettingsFromQuery(query({ run_in_background: true, show_menu_bar_icon: false, stored: true }))).toEqual({
      values: { runInBackground: true, showMenuBarIcon: false },
      stored: true,
    });
    expect(shellSettingsFromQuery(query({ run_in_background: false, show_menu_bar_icon: true, stored: false })).stored).toBe(false);
  });

  it("falls back to the defaults, as not stored, for anything else", () => {
    const defaults = { values: { runInBackground: false, showMenuBarIcon: true }, stored: false };
    expect(shellSettingsFromQuery("")).toEqual(defaults);
    expect(shellSettingsFromQuery("?port=1")).toEqual(defaults);
    expect(shellSettingsFromQuery(query("{not json"))).toEqual(defaults);
    expect(shellSettingsFromQuery(query("null"))).toEqual(defaults);
    expect(shellSettingsFromQuery(query("[]"))).toEqual(defaults);
    expect(shellSettingsFromQuery(query({ run_in_background: "true", show_menu_bar_icon: false, stored: true }))).toEqual(defaults);
    expect(shellSettingsFromQuery(query({ run_in_background: true, stored: true }))).toEqual(defaults);
    expect(shellSettingsFromQuery(query({ run_in_background: true, show_menu_bar_icon: true }))).toEqual(defaults);
  });
});

/** The modules as a page load finds them: the window's query and `localStorage` are read as they are evaluated. */
async function load(search: string, legacy: Record<string, string> = {}) {
  localStorage.clear();
  for (const [key, value] of Object.entries(legacy)) localStorage.setItem(key, value);
  window.history.replaceState(null, "", `/${search}`);
  vi.resetModules();
  const settings = await import("./shellSettings");
  const { runInBackground } = await import("./runInBackground");
  const { showMenuBarIcon } = await import("./showMenuBarIcon");
  return { ...settings, runInBackground, showMenuBarIcon };
}

const STORED = query({ run_in_background: true, show_menu_bar_icon: false, stored: true });
const NOT_STORED = query({ run_in_background: false, show_menu_bar_icon: true, stored: false });

beforeEach(() => {
  vi.resetModules();
});

afterEach(() => {
  localStorage.clear();
  window.history.replaceState(null, "", "/");
  vi.resetModules();
});

describe("the preferences", () => {
  it("start from the shell's values", async () => {
    const { runInBackground, showMenuBarIcon } = await load(STORED);
    expect([runInBackground.get(), showMenuBarIcon.get()]).toEqual([true, false]);
  });

  it("start from the defaults with no query, as in a browser", async () => {
    const { runInBackground, showMenuBarIcon } = await load("");
    expect([runInBackground.get(), showMenuBarIcon.get()]).toEqual([false, true]);
  });

  it("notify subscribers on a change and keep it in memory without a shell", async () => {
    const { runInBackground } = await load("");
    const listener = vi.fn();
    const unsubscribe = runInBackground.subscribe(listener);
    runInBackground.set(true);
    expect(runInBackground.get()).toBe(true);
    expect(listener).toHaveBeenCalledTimes(1);
    unsubscribe();
    runInBackground.set(false);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem(PREFERENCE_KEYS.runInBackground)).toBeNull();
  });

  it("notify a subscriber only of its own setting's changes", async () => {
    const { runInBackground, showMenuBarIcon } = await load("");
    const onBackground = vi.fn();
    const onIcon = vi.fn();
    runInBackground.subscribe(onBackground);
    showMenuBarIcon.subscribe(onIcon);
    runInBackground.set(true);
    expect([onBackground.mock.calls.length, onIcon.mock.calls.length]).toEqual([1, 0]);
    showMenuBarIcon.set(false);
    expect([onBackground.mock.calls.length, onIcon.mock.calls.length]).toEqual([1, 1]);
  });

  it("keep a change made with persist off in memory until persist is called", async () => {
    const { runInBackground, connectShellSettings } = await load(STORED);
    const toBackground = vi.fn(() => Promise.resolve());
    connectShellSettings({ runInBackground: toBackground });
    const listener = vi.fn();
    runInBackground.subscribe(listener);
    runInBackground.set(false, { persist: false });
    runInBackground.set(true, { persist: false });
    runInBackground.set(false, { persist: false });
    expect(runInBackground.get()).toBe(false);
    expect(listener).toHaveBeenCalledTimes(3);
    await Promise.resolve();
    expect(toBackground).not.toHaveBeenCalled();
    runInBackground.persist();
    await vi.waitFor(() => expect(toBackground).toHaveBeenCalledExactlyOnceWith(false));
  });

  it("hand a change to the shell through the connected sink", async () => {
    const { runInBackground, showMenuBarIcon, connectShellSettings } = await load(STORED);
    const toBackground = vi.fn(() => Promise.resolve());
    const toIcon = vi.fn(() => Promise.resolve());
    connectShellSettings({ runInBackground: toBackground, showMenuBarIcon: toIcon });
    runInBackground.set(false);
    showMenuBarIcon.set(true);
    await vi.waitFor(() => expect(toIcon).toHaveBeenCalledExactlyOnceWith(true));
    expect(toBackground).toHaveBeenCalledExactlyOnceWith(false);
  });

  it("reach the shell one push at a time, in the order they were made", async () => {
    const { runInBackground, connectShellSettings } = await load(STORED);
    const settle: (() => void)[] = [];
    const sent: boolean[] = [];
    connectShellSettings({
      runInBackground: (value) => {
        sent.push(value);
        return new Promise<void>((resolve) => settle.push(resolve));
      },
    });
    runInBackground.set(false);
    runInBackground.set(true);
    await Promise.resolve();
    expect(sent).toEqual([false]);
    settle[0]!();
    await vi.waitFor(() => expect(sent).toEqual([false, true]));
  });

  it("keep the value when a push fails, and go on with the pushes after it", async () => {
    const { runInBackground, connectShellSettings } = await load(STORED);
    const toBackground = vi.fn().mockRejectedValueOnce(new Error("refused")).mockResolvedValue(undefined);
    connectShellSettings({ runInBackground: toBackground });
    runInBackground.set(false);
    runInBackground.set(true);
    await vi.waitFor(() => expect(toBackground).toHaveBeenCalledTimes(2));
    expect(runInBackground.get()).toBe(true);
  });
});

describe("the migration of what the page kept in localStorage", () => {
  const LEGACY = { [PREFERENCE_KEYS.runInBackground]: "true", [PREFERENCE_KEYS.showMenuBarIcon]: "false" };

  it("takes the legacy values at once when the shell has nothing stored", async () => {
    const { runInBackground, showMenuBarIcon } = await load(NOT_STORED, LEGACY);
    expect([runInBackground.get(), showMenuBarIcon.get()]).toEqual([true, false]);
  });

  it("reads a legacy key that is missing as its default", async () => {
    const { runInBackground, showMenuBarIcon } = await load(NOT_STORED, { [PREFERENCE_KEYS.runInBackground]: "true" });
    expect([runInBackground.get(), showMenuBarIcon.get()]).toEqual([true, true]);
  });

  it("ignores the legacy values when the shell has its own", async () => {
    const { runInBackground, showMenuBarIcon, connectShellSettings } = await load(STORED, { [PREFERENCE_KEYS.runInBackground]: "false", [PREFERENCE_KEYS.showMenuBarIcon]: "true" });
    const toBackground = vi.fn(() => Promise.resolve());
    connectShellSettings({ runInBackground: toBackground });
    await Promise.resolve();
    expect([runInBackground.get(), showMenuBarIcon.get()]).toEqual([true, false]);
    expect(toBackground).not.toHaveBeenCalled();
    expect(localStorage.getItem(PREFERENCE_KEYS.runInBackground)).toBe("false");
  });

  it("pushes them once at startup and removes the legacy keys after the pushes succeed", async () => {
    const { connectShellSettings, migrateLegacyShellSettings } = await load(NOT_STORED, LEGACY);
    const toBackground = vi.fn(() => Promise.resolve());
    const toIcon = vi.fn(() => Promise.resolve());
    connectShellSettings({ runInBackground: toBackground, showMenuBarIcon: toIcon });
    expect(localStorage.getItem(PREFERENCE_KEYS.runInBackground)).toBe("true");
    await vi.waitFor(() => expect(localStorage.getItem(PREFERENCE_KEYS.runInBackground)).toBeNull());
    expect(localStorage.getItem(PREFERENCE_KEYS.showMenuBarIcon)).toBeNull();
    expect(toBackground).toHaveBeenCalledExactlyOnceWith(true);
    expect(toIcon).toHaveBeenCalledExactlyOnceWith(false);
    await migrateLegacyShellSettings();
    expect(toBackground).toHaveBeenCalledTimes(1);
  });

  it("keeps the legacy keys when a push fails", async () => {
    const { connectShellSettings, migrateLegacyShellSettings } = await load(NOT_STORED, LEGACY);
    connectShellSettings({ runInBackground: () => Promise.reject(new Error("refused")), showMenuBarIcon: () => Promise.resolve() });
    await migrateLegacyShellSettings();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(localStorage.getItem(PREFERENCE_KEYS.runInBackground)).toBe("true");
    expect(localStorage.getItem(PREFERENCE_KEYS.showMenuBarIcon)).toBe("false");
  });

  it("pushes nothing where the platform has no capability for either setting", async () => {
    const { connectShellSettings } = await load(NOT_STORED, LEGACY);
    connectShellSettings({});
    expect(localStorage.getItem(PREFERENCE_KEYS.runInBackground)).toBe("true");
  });
});
