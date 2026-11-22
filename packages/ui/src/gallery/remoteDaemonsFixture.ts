// Imports nothing that reads a stored preference as it loads: `frame.tsx` imports this before `prepareStorage` has run.
import type { RemoteDaemonEntry, RemoteDaemonsCapability, RemoteDaemonStatus, RemoteDeviceKeyRemoval } from "../platform";
import type { RemoteDaemonsFixture } from "./scenario";

const NEVER = new Promise<never>(() => {});

/** The shell's answer when an entry was removed while its page was open (`REMOVED` in `saved.rs`). */
const REMOVED = "This remote daemon has been removed.";

const NOT_INSTALLED: RemoteDeviceKeyRemoval = { line: { result: "not_installed" }, privateKeyLeft: null };
const REMOVED_KEY: RemoteDeviceKeyRemoval = { line: { result: "removed" }, privateKeyLeft: null };

/** An optional text as the shell saves it: trimmed, and none where it is blank. */
const trimmed = (value: string | null): string | null => value?.trim() || null;

/** Where an entry signs in to, as the shell compares one entry's destination with another's (`Entry::identity`). */
const destinationOf = ({ user, host, port }: Pick<RemoteDaemonEntry, "user" | "host" | "port">) => `${user}@${host}:${port}`;

/** The shell's list of remote daemons as a scenario describes it (`RemoteDaemonsFixture`), which the rail's button and
 * the Remote devices page then drive exactly as they drive the desktop app's. Saving and removing change the list kept
 * here and tell the listener, as the shell does (`entries.rs`, `saved.rs`): a save trims the name, host and user (a
 * blank name becomes the host) and the optional texts, and rejects an entry that has been removed; an
 * entry that is new, or that is saved for another destination, signs in with a password even where the form asked for
 * this computer's key. A connection ends as the scenario's `outcomes` say, and otherwise never, so the entry that was
 * pressed stays pending. One held for the browser is reported as such until it is disconnected, as is one a window
 * holds. This computer's key an entry has installed is taken back by removing the entry, turning the key off, or a save
 * that leaves it (another destination or another sign-in): the entry then signs in with a password and keeps no line,
 * but keeps the owner's no, and the removal is what the scenario's `keyRemovals` says for it, otherwise the line
 * removed. A connection asked again with the answer to the question about the key (`deviceKey`) never ends either. The
 * shell's windows are not drawn here: opening one does nothing. */
export function fixtureRemoteDaemons({ entries, status = [], outcomes = {}, keyRemovals = {} }: RemoteDaemonsFixture): RemoteDaemonsCapability {
  let list: RemoteDaemonEntry[] = entries.map((entry) => ({ ...entry, deviceKey: { ...entry.deviceKey } }));
  const states: RemoteDaemonStatus[] = status.map((one) => ({ ...one }));
  const listeners = new Set<() => void>();
  let created = 0;
  const changed = () => listeners.forEach((listener) => listener());
  const takeBack = (entry: RemoteDaemonEntry): RemoteDeviceKeyRemoval => (entry.auth === "device" ? (keyRemovals[entry.id] ?? REMOVED_KEY) : NOT_INSTALLED);
  /** The entry as it is once it has no key installed: it signs in with a password where it used the key, and keeps no
   * line, while the owner's no stays. */
  const withoutKey = (entry: RemoteDaemonEntry): RemoteDaemonEntry => ({ ...entry, auth: entry.auth === "device" ? "password" : entry.auth, deviceKey: { ...entry.deviceKey, file: null, mark: null } });
  return {
    list: async () => list.map((entry) => ({ ...entry })),
    status: async () => states.filter(({ id }) => list.some((entry) => entry.id === id)).map((one) => ({ ...one })),
    onChanged: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    save: async (input) => {
      const previous = input.id === null ? undefined : list.find((entry) => entry.id === input.id);
      if (input.id !== null && !previous) throw new Error(REMOVED);
      const host = input.host.trim();
      const entry: RemoteDaemonEntry = {
        ...input,
        id: previous?.id ?? `remote-new-${++created}`,
        name: input.name.trim() || host,
        host,
        user: input.user.trim(),
        keyPath: input.auth === "key" ? trimmed(input.keyPath) : null,
        fingerprint: trimmed(input.fingerprint),
        octoboarddPath: trimmed(input.octoboarddPath),
        deviceKey: previous?.deviceKey ?? { declined: false, file: null, mark: null },
      };
      // Only an entry that already has the key at this destination keeps it; the form cannot ask for one.
      const keeps = input.auth === "device" && previous?.auth === "device" && destinationOf(previous) === destinationOf(entry);
      const saved = keeps ? entry : withoutKey({ ...entry, auth: input.auth === "device" ? "password" : input.auth });
      const keyRemoval = previous?.auth === "device" && saved.auth !== "device" ? takeBack(previous) : NOT_INSTALLED;
      list = previous === undefined ? [...list, saved] : list.map((one) => (one.id === saved.id ? saved : one));
      changed();
      return { entry: saved, keyRemoval };
    },
    remove: async (id) => {
      const entry = list.find((one) => one.id === id);
      const keyRemoval = entry ? takeBack(entry) : NOT_INSTALLED;
      list = list.filter((one) => one.id !== id);
      changed();
      return keyRemoval;
    },
    setDeviceKey: async (id, enabled) => {
      const entry = list.find((one) => one.id === id);
      if (!entry) throw new Error(REMOVED);
      const keyRemoval = enabled ? NOT_INSTALLED : takeBack(entry);
      const turned = enabled ? entry : withoutKey(entry);
      list = list.map((one) => (one.id === id ? { ...turned, deviceKey: { ...turned.deviceKey, declined: !enabled } } : one));
      changed();
      return keyRemoval;
    },
    connect: async ({ id, browser, deviceKey }) => {
      if (browser || states.some((one) => one.id === id && one.via === "browser")) {
        if (!states.some((one) => one.id === id)) states.push({ id, state: "connected", via: "browser" });
        changed();
        return { outcome: "opened_in_browser" };
      }
      const outcome = outcomes[id];
      return outcome === undefined || (outcome.outcome === "device_key_offered" && deviceKey !== undefined) ? NEVER : outcome;
    },
    disconnect: async (id) => {
      const at = states.findIndex((one) => one.id === id);
      if (at >= 0) states.splice(at, 1);
      changed();
    },
    onHandedOver: () => () => {},
    onPageRequested: () => () => {},
    showMainWindow: async () => {},
    showUpdates: async () => {},
  };
}
