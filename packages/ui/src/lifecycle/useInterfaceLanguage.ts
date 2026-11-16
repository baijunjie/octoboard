import { useEffect, useState } from "react";

import { currentLanguage, setLanguage } from "../i18n/language";
import { isLanguage } from "../i18n/languages";
import { useDaemon, useDaemonStore } from "../store";

/**
 * Keeps this window in the language the daemon holds (`Settings.interface_language`), the one setting
 * every client shares: whenever the daemon's value is a language this build offers, the window shows
 * it, which also refreshes the cache the next start renders from before it has connected. A value
 * this build does not offer leaves the window as it is and is never overwritten.
 *
 * It reacts to a change of the daemon's value only, never to the window's own language, so a pick in
 * Settings that has not come back as a `settings_updated` yet is not undone (see `LanguageSetting`).
 *
 * When the daemon holds none, it is seeded once per snapshot with the language this window shows,
 * which is the only time a client reports one on its own: it picks it from the cache or the system.
 *
 * It waits for a `snapshot` of the connection that is open now: before it the store's settings are the
 * defaults, or on a reconnect the ones the previous connection last saw, and neither says what the daemon
 * holds. `connectionState` turns "open" at the hello, while `snapshotEpoch` still counts the previous
 * connection's snapshots, so the epoch the connection opened at is remembered and a larger one is waited for.
 */
export function useInterfaceLanguage(): void {
  const { request } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const held = useDaemonStore((s) => s.settings.interface_language);
  // The epoch the current state began at, taken during render so that no render sees the new state with the
  // old baseline. A hook mounted while the connection is already open has no snapshot to rule out, so its
  // baseline is zero.
  const [began, setBegan] = useState({ state: connectionState, epoch: connectionState === "open" ? 0 : snapshotEpoch });
  if (began.state !== connectionState) setBegan({ state: connectionState, epoch: snapshotEpoch });
  const synced = connectionState === "open" && began.state === "open" && snapshotEpoch > began.epoch;

  useEffect(() => {
    if (synced && isLanguage(held) && held !== currentLanguage()) setLanguage(held);
  }, [synced, held]);

  useEffect(() => {
    if (!synced || held) return;
    request({ type: "update_settings", interface_language: currentLanguage() }).catch(() => {
      // Background upkeep nobody asked for: the next snapshot, or a pick in Settings, tries again.
    });
  }, [synced, snapshotEpoch, held, request]);
}
