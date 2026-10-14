/** Each iframe gets its own ephemeral preferences, even when opened directly on the site's origin.
 * Install before importing any application module: several preferences are read at evaluation. */
export function installPreviewStorage(): void {
  function memoryStorage(): Storage {
    const entries = new Map<string, string>();
    return {
      get length() {
        return entries.size;
      },
      clear: () => entries.clear(),
      getItem: (key) => entries.get(String(key)) ?? null,
      key: (index) => [...entries.keys()][index] ?? null,
      removeItem: (key) => {
        entries.delete(String(key));
      },
      setItem: (key, value) => {
        entries.set(String(key), String(value));
      },
    };
  }
  for (const name of ["localStorage", "sessionStorage"]) {
    Object.defineProperty(window, name, {
      configurable: true,
      value: memoryStorage(),
    });
  }
}
