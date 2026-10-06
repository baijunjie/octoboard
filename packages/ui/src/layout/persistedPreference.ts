import { useSyncExternalStore } from "react";

export interface PersistedPreference<T> {
  /** Makes `value` current for every subscriber; with `persist: false` it is kept in memory only,
   * until `persist()` writes it (a value changing many times in a row, such as a drag). */
  set(value: T, options?: { persist?: boolean }): void;
  /** Writes the current value to storage. */
  persist(): void;
  /** The current value, shared module-wide, and re-rendering the caller whenever it changes. */
  useValue(): T;
}

/**
 * A user preference kept in `localStorage` under `key` and shared by every caller in the page.
 * `parse` turns the stored text (`null` when there is none, or storage is unavailable) into the
 * value, falling back to the default itself; `serialize` returns the text to store, or `null` to
 * remove the entry. Every storage access is guarded: without storage a choice lasts for the page
 * load only.
 */
export function createPersistedPreference<T>(
  key: string,
  parse: (raw: string | null) => T,
  serialize: (value: T) => string | null,
): PersistedPreference<T> {
  let stored: string | null = null;
  try {
    stored = localStorage.getItem(key);
  } catch {
    // Storage unavailable: the default applies.
  }
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
      if (raw === null) localStorage.removeItem(key);
      else localStorage.setItem(key, raw);
    } catch {
      // Storage unavailable: the choice then lasts for this page load only.
    }
  };

  return {
    set(next, { persist: write = true } = {}) {
      value = next;
      if (write) persist();
      for (const listener of listeners) listener();
    },
    persist,
    useValue: () => useSyncExternalStore(subscribe, getValue),
  };
}
