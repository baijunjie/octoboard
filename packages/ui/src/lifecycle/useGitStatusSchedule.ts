import { useEffect, useMemo, useRef } from "react";

import { useDaemon, useDaemonStore, type State } from "../store";

/** How often every console's projects are re-checked. A constant, not a setting. */
const REFRESH_INTERVAL_MS = 5 * 60_000;

/** A count comparison would miss a project replaced by another in the same tick. */
export function hasNewProjectId(known: ReadonlySet<string>, current: readonly string[]): boolean {
  return current.some((id) => !known.has(id));
}

function projectRoster({ consoles, projects }: Pick<State, "consoles" | "projects">): Map<string, Set<string>> {
  const roster = new Map(Array.from(consoles.keys(), (id) => [id, new Set<string>()]));
  for (const project of projects.values()) roster.get(project.console_id)?.add(project.id);
  return roster;
}

/** Keeps every console current while connected, independently of sidebar selection. Roster
 * changes refresh their affected consoles without restarting the five-minute clock. */
export function useGitStatusSchedule(): void {
  const { request, store } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const roster = useMemo(() => projectRoster({ consoles, projects }), [consoles, projects]);
  const knownRoster = useRef<Map<string, Set<string>>>(new Map());

  useEffect(() => {
    if (connectionState !== "open") return;
    // A reconnect or snapshot already refreshes everything; roster changes in that same render
    // must not request the newly arrived projects a second time.
    knownRoster.current = projectRoster(store.getState());
    const sweep = () => {
      for (const consoleId of store.getState().consoles.keys()) {
        void request({ type: "refresh_git_status", console: consoleId }).catch(() => {
          // Background upkeep retries on its next sweep; a failed console must not stop the rest.
        });
      }
    };
    sweep();
    const interval = setInterval(sweep, REFRESH_INTERVAL_MS);
    return () => clearInterval(interval);
  }, [connectionState, snapshotEpoch, request, store]);

  useEffect(() => {
    if (connectionState !== "open") return;
    for (const [consoleId, ids] of roster) {
      const known = knownRoster.current.get(consoleId);
      if (!known || hasNewProjectId(known, Array.from(ids))) {
        void request({ type: "refresh_git_status", console: consoleId }).catch(() => {
          // Same retry policy as the periodic sweep.
        });
      }
    }
    knownRoster.current = roster;
  }, [connectionState, roster, request]);
}
