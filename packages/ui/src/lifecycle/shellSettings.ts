import { useSyncExternalStore } from "react";

import type { PersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { createSerialQueue } from "../components/hooks/serialQueue";

/** The two choices the desktop shell keeps in its own file (`settings.json`,
 * `apps/desktop/src-tauri/src/app_settings.rs`) because it must know them before this page has loaded. */
export interface ShellSettings {
  runInBackground: boolean;
  showMenuBarIcon: boolean;
}

const DEFAULTS: ShellSettings = { runInBackground: false, showMenuBarIcon: true };

/** What the shell put in the window's query (`?settings=`): the values it holds, and whether they came from its
 * file. */
export interface ShellSettingsFromQuery {
  values: ShellSettings;
  /** False when the shell found no usable file, or put nothing in the query: there is nothing stored yet. */
  stored: boolean;
}

/** The settings the shell put in the window's query (`?settings=`, JSON), or the defaults, as not stored, when there
 * are none or they are not a whole, valid set (a browser, a dev server in a Tauri window, an older shell). */
export function shellSettingsFromQuery(search: string): ShellSettingsFromQuery {
  const none: ShellSettingsFromQuery = { values: DEFAULTS, stored: false };
  const raw = new URLSearchParams(search).get("settings");
  if (!raw) return none;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return none;
  }
  if (typeof value !== "object" || value === null) return none;
  const { run_in_background, show_menu_bar_icon, stored } = value as Record<string, unknown>;
  if (typeof run_in_background !== "boolean" || typeof show_menu_bar_icon !== "boolean" || typeof stored !== "boolean") return none;
  return { values: { runInBackground: run_in_background, showMenuBarIcon: show_menu_bar_icon }, stored };
}

/** What this page kept in `localStorage` before the shell held the settings, read as the preferences read it then: a
 * choice not made is the default. `undefined` when it kept neither, or storage is unavailable. */
function readLegacy(): ShellSettings | undefined {
  try {
    const background = localStorage.getItem(PREFERENCE_KEYS.runInBackground);
    const icon = localStorage.getItem(PREFERENCE_KEYS.showMenuBarIcon);
    if (background === null && icon === null) return undefined;
    return { runInBackground: background === "true", showMenuBarIcon: icon !== "false" };
  } catch {
    return undefined;
  }
}

function removeLegacy(): void {
  try {
    localStorage.removeItem(PREFERENCE_KEYS.runInBackground);
    localStorage.removeItem(PREFERENCE_KEYS.showMenuBarIcon);
  } catch {
    // Storage unavailable: nothing was read from it either.
  }
}

const fromQuery = shellSettingsFromQuery(window.location.search);
// Where the shell has no file yet, what the page kept before takes over at once, and `migrateLegacyShellSettings` hands
// it to the shell. Where it has one, the file is the truth and what the page kept is stale.
let legacy = fromQuery.stored ? undefined : readLegacy();
let values: ShellSettings = legacy ?? fromQuery.values;

// One listener set per setting, so a listener hears only about the setting it subscribed to.
const listeners: { [K in keyof ShellSettings]: Set<() => void> } = { runInBackground: new Set(), showMenuBarIcon: new Set() };

/** How each choice reaches the shell, once `connectShellSettings` has been given the platform. */
type Sinks = { [K in keyof ShellSettings]?: (value: ShellSettings[K]) => Promise<unknown> };
let sinks: Sinks = {};

// Pushes go one at a time, in the order they were made. A failed push is dropped: a shell left on its previous choice is
// not worth surfacing to the user.
const queue = createSerialQueue();

/** Hands the current value of `key` to the shell, when the platform has connected a sink for it; before that it stays in
 * memory only. */
function push<K extends keyof ShellSettings>(key: K): void {
  const sink = sinks[key] as ((value: ShellSettings[K]) => Promise<unknown>) | undefined;
  const value = values[key];
  if (sink) void queue(() => sink(value));
}

/** A setting the shell holds, with the interface of a persisted preference: `set` makes the value current for every
 * subscriber and, unless `persist` is false, hands it to the shell; `persist` hands the shell the current value. In a
 * browser, which has no shell, a choice lasts for the page load. */
export function createShellPreference<K extends keyof ShellSettings>(key: K): PersistedPreference<ShellSettings[K]> {
  const get = () => values[key];
  const subscribe = (listener: () => void) => {
    listeners[key].add(listener);
    return () => listeners[key].delete(listener);
  };
  return {
    set(value, { persist = true } = {}) {
      values = { ...values, [key]: value };
      if (persist) push(key);
      for (const listener of listeners[key]) listener();
    },
    persist: () => push(key),
    get,
    subscribe,
    useValue: () => useSyncExternalStore(subscribe, get),
  };
}

/** Connects the preferences to the shell: from here on a change reaches it through `sinks`, and what the page kept before
 * the shell held the settings is handed over once. Called once, at startup, with the capabilities the platform has; a
 * setting whose capability is missing stays in memory. */
export function connectShellSettings(connected: Sinks): void {
  sinks = connected;
  migrateLegacyShellSettings();
}

/** Hands the shell the settings this page kept in `localStorage` before the shell held them, when the shell had none
 * stored, and removes them from `localStorage` only once it has taken them all: a failed push leaves them for the next
 * launch, which finds the shell still with no file. */
export function migrateLegacyShellSettings(): Promise<void> {
  const migrating = legacy;
  const { runInBackground: toBackground, showMenuBarIcon: toIcon } = sinks;
  if (!migrating || (!toBackground && !toIcon)) return Promise.resolve();
  legacy = undefined;
  const pushes: Promise<boolean>[] = [];
  if (toBackground) pushes.push(queue(() => toBackground(migrating.runInBackground)));
  if (toIcon) pushes.push(queue(() => toIcon(migrating.showMenuBarIcon)));
  return Promise.all(pushes).then((results) => {
    if (results.every(Boolean)) removeLegacy();
  });
}
