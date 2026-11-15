import { useEffect, useState } from "react";

import { currentLanguage } from "../i18n/language";
import { useDaemon, useDaemonStore } from "../store";

/**
 * Gives this window's daemon its first agent language (`Settings.agent_language`, the language it tells sessions to
 * talk to the user in): when the daemon holds none, it is seeded once per snapshot with the interface language this
 * window shows, which is the only time a client reports one on its own. A daemon that holds one is left as it is, and
 * the window's own language never follows the daemon's: the interface language is the client's (see
 * `i18n/language.ts`). The window of a daemon elsewhere seeds that daemon as the local window seeds its own.
 *
 * It waits for a `snapshot` of the connection that is open now: before it the store's settings are the defaults, or on
 * a reconnect the ones the previous connection last saw, and neither says what the daemon holds. `connectionState`
 * turns "open" at the hello, while `snapshotEpoch` still counts the previous connection's snapshots, so the epoch the
 * connection opened at is remembered and a larger one is waited for.
 */
export function useAgentLanguageSeed(): void {
  const { request } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const held = useDaemonStore((s) => s.settings.agent_language);
  // The epoch the current state began at, taken during render so that no render sees the new state with the
  // old baseline. A hook mounted while the connection is already open has no snapshot to rule out, so its
  // baseline is zero.
  const [began, setBegan] = useState({ state: connectionState, epoch: connectionState === "open" ? 0 : snapshotEpoch });
  if (began.state !== connectionState) setBegan({ state: connectionState, epoch: snapshotEpoch });
  const synced = connectionState === "open" && began.state === "open" && snapshotEpoch > began.epoch;

  useEffect(() => {
    if (!synced || held) return;
    request({ type: "update_settings", agent_language: currentLanguage() }).catch(() => {
      // Background upkeep nobody asked for: the next snapshot, or a pick in Settings, tries again.
    });
  }, [synced, snapshotEpoch, held, request]);
}
