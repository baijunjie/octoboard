import type { StoreApi } from "zustand";

import { baseVersions } from "../errors/writeConflict";
import type { ClientRequestBody } from "../protocol";
import type { RequestSender } from "../requests";
import type { State } from "./state";

/** What a write of settings changes: the fields it sets, without the envelope and the versions it is based on. */
export type SettingsChanges = Omit<Extract<ClientRequestBody, { type: "update_settings" }>, "type" | "base_versions">;

/**
 * The checked write of the settings controls (see "Checked writes" in `apps/daemon/PROTOCOL.md`): `write(changes, paths)`
 * sends `update_settings` for the fields that change `paths`, based on the version the store holds of each, and settles
 * once the daemon has answered; a failure is handed to `onFailure`, and settles it too. A write of this window's own is
 * not answered until the daemon replies, and the store shows it only once its broadcast arrives, which may come after the
 * reply; a second write of the same field before then would be based on the version the first is about to replace, and be
 * refused. So a write is dropped (it settles at once, sending nothing) while the store still holds a path at the version
 * this window last wrote it from: the control shows the store's value, so the press that counts is the next one. A write
 * that fails frees its paths, as the store will not move. Used by `useSettingsWrite`, and by the daemon's update store.
 */
export function createSettingsWriter({ store, request, onFailure }: {
  store: StoreApi<State>;
  request: RequestSender;
  onFailure: (error: unknown) => void;
}): (changes: SettingsChanges, paths: readonly string[]) => Promise<void> {
  // The version of each path when this window last wrote it, until the store has moved past it. A path the store holds
  // no version of is not checked, so it has none to wait for.
  const written = new Map<string, number>();
  return async (changes, paths) => {
    const versions = store.getState().settings.versions;
    for (const [path, version] of written) {
      if (versions[path] !== version) written.delete(path);
    }
    if (paths.some((path) => written.has(path))) return;
    for (const path of paths) {
      const version = versions[path];
      if (version !== undefined) written.set(path, version);
    }
    try {
      await request({ type: "update_settings", ...changes, base_versions: baseVersions(versions, paths) });
    } catch (error) {
      for (const path of paths) written.delete(path);
      onFailure(error);
    }
  };
}
