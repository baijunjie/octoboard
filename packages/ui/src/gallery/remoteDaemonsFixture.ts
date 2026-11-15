// Imports nothing that reads a stored preference as it loads: `frame.tsx` imports this before `prepareStorage` has run.
import type { RemoteDaemonEntry, RemoteDaemonsCapability, RemoteDaemonStatus } from "../platform";
import type { RemoteDaemonsFixture } from "./scenario";

const NEVER = new Promise<never>(() => {});

/** The shell's list of remote daemons as a scenario describes it (`RemoteDaemonsFixture`), which the rail's group
 * then drives exactly as it drives the desktop app's. Saving and removing change the list kept here and tell the
 * listener, as the shell does; a connection ends as the scenario's `outcomes` say, and otherwise never, so the entry
 * that was pressed stays pending. One held for the browser is reported as such until it is disconnected, as is one a
 * window holds. The shell's windows are not drawn here: opening one does nothing. */
export function fixtureRemoteDaemons({ entries, status = [], outcomes = {} }: RemoteDaemonsFixture): RemoteDaemonsCapability {
  let list: RemoteDaemonEntry[] = entries.map((entry) => ({ ...entry }));
  const states: RemoteDaemonStatus[] = status.map((one) => ({ ...one }));
  const listeners = new Set<() => void>();
  let created = 0;
  const changed = () => listeners.forEach((listener) => listener());
  return {
    list: async () => list.map((entry) => ({ ...entry })),
    status: async () => states.filter(({ id }) => list.some((entry) => entry.id === id)).map((one) => ({ ...one })),
    onChanged: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    save: async (input) => {
      const saved: RemoteDaemonEntry = { ...input, id: input.id ?? `remote-new-${++created}` };
      list = input.id === null ? [...list, saved] : list.map((entry) => (entry.id === saved.id ? saved : entry));
      changed();
      return saved;
    },
    remove: async (id) => {
      list = list.filter((entry) => entry.id !== id);
      changed();
    },
    connect: async ({ id, browser }) => {
      if (browser || states.some((one) => one.id === id && one.via === "browser")) {
        if (!states.some((one) => one.id === id)) states.push({ id, state: "connected", via: "browser" });
        changed();
        return { outcome: "opened_in_browser" };
      }
      return outcomes[id] ?? NEVER;
    },
    disconnect: async (id) => {
      const at = states.findIndex((one) => one.id === id);
      if (at >= 0) states.splice(at, 1);
      changed();
    },
    newWindow: async () => {},
    openWindow: async () => {},
    showMainWindow: async () => {},
    showUpdates: async () => {},
  };
}
