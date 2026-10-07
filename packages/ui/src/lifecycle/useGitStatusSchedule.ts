import { useEffect, useRef } from "react";
import { useShallow } from "zustand/react/shallow";

import { useDaemon, useDaemonStore } from "../store";

/** How often the shown console's projects are re-checked. A constant, not a setting. */
const REFRESH_INTERVAL_MS = 5 * 60_000;

/** Whether `current` holds an id `known` does not — the signal that a project newly appeared in
 * the shown console. Pure, so it is tested on its own rather than through the hook; a plain count
 * comparison would miss a project removed and another added in the same tick, since the count
 * does not change. */
export function hasNewProjectId(known: ReadonlySet<string>, current: readonly string[]): boolean {
  return current.some((id) => !known.has(id));
}

/**
 * Keeps the shown console's git statuses current: sends `refresh_git_status` for `consoleId` as
 * soon as it is shown, again every `REFRESH_INTERVAL_MS` for as long as it stays shown, and once
 * more on a `snapshotEpoch` bump, since the daemon may have restarted and lost the statuses it
 * was holding in memory. Sends nothing while no console is shown (`consoleId` undefined) or the
 * control connection is down; switching consoles drops the previous one's interval instead of
 * running several at once.
 *
 * The interval is driven only by `consoleId`, `connectionState` and `snapshotEpoch`: a project
 * being added to or removed from the shown console must not restart the 5-minute clock, so that
 * roster change is handled by the second effect below instead, which refreshes without touching
 * the timer.
 */
export function useGitStatusSchedule(consoleId: string | undefined): void {
  const { request } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const projectIds = useDaemonStore(
    useShallow((s) =>
      consoleId
        ? Array.from(s.projects.values())
            .filter((p) => p.console_id === consoleId)
            .map((p) => p.id)
        : [],
    ),
  );

  useEffect(() => {
    if (!consoleId || connectionState !== "open") return;
    const sweep = () =>
      request({ type: "refresh_git_status", console: consoleId }).catch(() => {
        // A sweep's failure (offline, an unknown console after a race, anything else) is not the
        // user's problem to act on: it is background upkeep nobody asked for, and the next sweep —
        // on this timer, or the roster-change effect below — retries on its own.
      });
    void sweep();
    const interval = setInterval(() => void sweep(), REFRESH_INTERVAL_MS);
    return () => clearInterval(interval);
  }, [consoleId, connectionState, snapshotEpoch, request]);

  // A project appearing in the shown console — added, or moved in from another console — gets
  // checked right away rather than waiting for the next sweep. `knownIds` tracks the roster this
  // hook has already seen for the *current* console; switching consoles resyncs it without
  // sending a refresh of its own, since the effect above already sends a full one for the switch.
  const lastConsoleId = useRef<string | undefined>(undefined);
  const knownIds = useRef<Set<string>>(new Set());
  useEffect(() => {
    if (!consoleId || connectionState !== "open") return;
    if (lastConsoleId.current !== consoleId) {
      lastConsoleId.current = consoleId;
      knownIds.current = new Set(projectIds);
      return;
    }
    const added = hasNewProjectId(knownIds.current, projectIds);
    knownIds.current = new Set(projectIds);
    if (added) {
      void request({ type: "refresh_git_status", console: consoleId }).catch(() => {
        // Same reasoning as the periodic sweep above: nothing the user can act on.
      });
    }
  }, [consoleId, connectionState, projectIds, request]);
}
