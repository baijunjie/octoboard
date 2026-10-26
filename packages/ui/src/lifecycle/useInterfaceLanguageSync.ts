import { useEffect } from "react";

import { currentLanguage } from "../i18n/language";
import { useDaemon, useDaemonStore } from "../store";

/**
 * Tells the daemon which language this client's interface is in, once, when the daemon has none
 * stored yet: that seeds an install from before the daemon kept it. It sends only then, never on
 * every connect, since two clients with different languages would otherwise overwrite each other's
 * choice at every start; after that only a pick in Settings sends (see `LanguageSetting`).
 *
 * It waits for the first `snapshot` (`snapshotEpoch` above zero) because before it the store's
 * settings are the defaults, which say nothing about what the daemon holds.
 */
export function useInterfaceLanguageSync(): void {
  const { request } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const stored = useDaemonStore((s) => s.settings.interface_language);

  useEffect(() => {
    if (connectionState !== "open" || snapshotEpoch === 0 || stored) return;
    request({ type: "update_settings", interface_language: currentLanguage() }).catch(() => {
      // Background upkeep nobody asked for: the next snapshot, or a pick in Settings, tries again.
    });
  }, [connectionState, snapshotEpoch, stored, request]);
}
