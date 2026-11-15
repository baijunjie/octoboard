import { useSyncExternalStore } from "react";

export interface PersistedPreference<T> {
  /** Makes `value` current for every subscriber; with `persist: false` it is kept in memory only,
   * until `persist()` writes it (a value changing many times in a row, such as a drag). */
  set(value: T, options?: { persist?: boolean }): void;
  /** Writes the current value to storage. */
  persist(): void;
  /** The current value, for code outside React. */
  get(): T;
  /** Calls `listener` after every change of the value; returns the function that unsubscribes. */
  subscribe(listener: () => void): () => void;
  /** The current value, shared module-wide, and re-rendering the caller whenever it changes. */
  useValue(): T;
}

/**
 * Whose a preference is, when the application shows several daemons, each in a window of its own:
 *
 * - `application`: the application's, shared by every window. A change one window makes is taken up
 *   by the others as it is stored (the `storage` event, which the browser sends every other page of
 *   the same origin).
 * - `daemon`: what a window remembers of its daemon (the selected console, the file browsers' state),
 *   kept under that daemon's identity (`daemonScopedKey`), so the window of another daemon neither
 *   changes nor removes it. `inherit` reads the application's value under the bare key when the
 *   daemon has none of its own yet, for a layout a new daemon's window starts from.
 */
export type PreferenceScope = { scope?: "application" } | { scope: "daemon"; inherit?: boolean };

let windowDaemonIdentity: string | undefined;

/**
 * Sets the identity this window's daemon-scoped preferences are kept under (`daemonIdentity` of the platform layer,
 * which the entry hands in, `src/preferenceIdentity.ts`): `undefined`, the default, for the local daemon's window.
 * Called before any preference is created, since each reads its stored value as it is created.
 */
export function setPreferenceDaemonIdentity(identity: string | undefined): void {
  windowDaemonIdentity = identity;
}

/**
 * The `localStorage` key a preference of `scope` kept under `key` is stored at in this window: the key
 * itself for the application's, and for the local daemon's (whose window has no identity, so what it
 * kept before there were several daemons still applies); `key@identity` for another daemon's.
 */
export function storageKeyFor(key: string, scope: PreferenceScope["scope"], identity: string | undefined = windowDaemonIdentity): string {
  return scope === "daemon" && identity !== undefined ? `${key}@${identity}` : key;
}

function readStorage(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/**
 * A user preference kept in `localStorage` under `key` (one of `PREFERENCE_KEYS` in `preferenceKeys.ts`)
 * and shared by every caller in the page; `options` says whose it is (`PreferenceScope`, the application's
 * by default).
 * `parse` turns the stored text (`null` when there is none, or storage is unavailable) into the
 * value, falling back to the default itself; `serialize` returns the text to store, or `null` to
 * remove the entry. Every storage access is guarded: without storage a choice lasts for the page
 * load only.
 */
export function createPersistedPreference<T>(
  key: string,
  parse: (raw: string | null) => T,
  serialize: (value: T) => string | null,
  options: PreferenceScope = {},
): PersistedPreference<T> {
  const storageKey = storageKeyFor(key, options.scope);
  let stored = readStorage(storageKey);
  if (stored === null && options.scope === "daemon" && options.inherit && storageKey !== key) stored = readStorage(key);
  let value = parse(stored);
  const listeners = new Set<() => void>();

  const subscribe = (listener: () => void) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  };
  const getValue = () => value;

  const persist = () => {
    try {
      const raw = serialize(value);
      if (raw === null) localStorage.removeItem(storageKey);
      else localStorage.setItem(storageKey, raw);
    } catch {
      // Storage unavailable: the choice then lasts for this page load only.
    }
  };

  if (options.scope !== "daemon" && typeof window !== "undefined") {
    window.addEventListener("storage", (event) => {
      if (event.key !== storageKey) return;
      try {
        if (event.storageArea !== localStorage) return;
      } catch {
        return;
      }
      value = parse(event.newValue);
      for (const listener of listeners) listener();
    });
  }

  return {
    set(next, { persist: write = true } = {}) {
      value = next;
      if (write) persist();
      for (const listener of listeners) listener();
    },
    persist,
    get: getValue,
    subscribe,
    useValue: () => useSyncExternalStore(subscribe, getValue),
  };
}
