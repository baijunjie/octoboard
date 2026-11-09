import { useRef } from "react";

import { alertAppDataFailure } from "../components/alerts/raiseError";
import { baseVersions } from "../errors/writeConflict";
import { useT } from "../i18n/react";
import type { ClientRequestBody } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";

type SettingsChanges = Omit<Extract<ClientRequestBody, { type: "update_settings" }>, "type" | "base_versions">;

/**
 * Sends the write of a settings control that changes `paths` (see "Checked writes" in `apps/daemon/PROTOCOL.md`),
 * based on the version the store holds of each. A write of this window's own is not answered until the daemon replies,
 * and the store shows it only once its broadcast arrives, which may come after the reply; a second write of the same
 * field before then would be based on the version the first is about to replace, and be refused. So a press is dropped
 * while the store still holds a path at the version this window last wrote it from: the control shows the store's
 * value, so the press that counts is the next one. A write that fails frees its paths, as the store will not move.
 */
export function useSettingsWrite(): (changes: SettingsChanges, paths: readonly string[]) => void {
  const t = useT();
  const { request } = useDaemon();
  const versions = useDaemonStore((s) => s.settings.versions);
  // The version of each path when this window last wrote it, until the store has moved past it. A path the store holds
  // no version of is not checked, so it has none to wait for.
  const written = useRef(new Map<string, number>());
  return (changes, paths) => {
    for (const [path, version] of written.current) {
      if (versions[path] !== version) written.current.delete(path);
    }
    if (paths.some((path) => written.current.has(path))) return;
    for (const path of paths) {
      const version = versions[path];
      if (version !== undefined) written.current.set(path, version);
    }
    void request({ type: "update_settings", ...changes, base_versions: baseVersions(versions, paths) }).catch((error: unknown) => {
      for (const path of paths) written.current.delete(path);
      alertAppDataFailure(t("settings.saveFailedTitle"))(error);
    });
  };
}
