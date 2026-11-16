import type { PlainMessageKey } from "../i18n/catalog";
import type { RemoteDaemonStatus } from "../platform";

/** Where a connected entry stands, as the Remote devices page shows it: its window is open, or the
 * connection is held for the browser, or it is being brought back after it dropped. */
export type ConnectionState = "connected" | "browser" | "reconnecting";

/** The words for each state. */
export const CONNECTION_STATE_WORDS = {
  connected: "remoteDaemons.state.connected",
  browser: "remoteDaemons.state.browser",
  reconnecting: "remoteDaemons.state.reconnecting",
} as const satisfies Record<ConnectionState, PlainMessageKey>;

/** The state of an entry the shell reports as connected. */
export function connectionState(status: RemoteDaemonStatus): ConnectionState {
  if (status.state === "reconnecting") return "reconnecting";
  return status.via === "browser" ? "browser" : "connected";
}
