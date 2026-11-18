import type { Agent } from "../protocol";
import type { Daemon } from "./connection";
import { setNativeApps } from "./actions";
import { useDaemon, useDaemonStore } from "./react";

/** Whether `agent`'s own desktop application is installed on the daemon's host, as the last probe said. */
export function useNativeAppInstalled(agent: Agent): boolean {
  return useDaemonStore((s) => s.nativeApps.includes(agent));
}

/** Whether the last probe that asked about `session` found that the applications cannot see its conversation. */
export function useNativeAppUnseen(session: string): boolean {
  return useDaemonStore((s) => s.nativeAppsUnseen.has(session));
}

/** Asks the daemon which agents' desktop applications are installed and, for `sessions` (at most 200), whether the
 * applications can see their conversations (`probe_native_apps`), and records the answer for the sessions it judged.
 * Asked as a window connects and as a menu or view offering the action opens, so an application installed or removed
 * meanwhile is found; the judging of sessions walks record directories, so only the rows being offered it are asked
 * about. A failed probe leaves the last answer. */
export function useNativeAppsRefresh(): (sessions?: readonly string[]) => void {
  const { request, store } = useDaemon();
  return (sessions = []) => {
    void refreshNativeApps(request, store, sessions);
  };
}

async function refreshNativeApps(request: Daemon["request"], store: Daemon["store"], sessions: readonly string[]): Promise<void> {
  try {
    const reply = await request({ type: "probe_native_apps", sessions: [...sessions] });
    // A stand-in daemon that answers every request alike says nothing of the applications.
    // Only the sessions the daemon judged are recorded, those it found seen and those it found unseen.
    if (reply?.type === "native_apps") setNativeApps(store, reply.agents, [...reply.sessions_seen, ...(reply.sessions_unseen ?? [])], reply.sessions_seen);
  } catch {
    // Background upkeep nobody asked for: the next probe tries again.
  }
}
