// Locates the daemon and the key it requires. The daemon binds an OS-assigned port and makes a new
// key for every launch (see `apps/daemon/PROTOCOL.md`), so the UI is never allowed to hardcode
// either. The address, in priority order:
//
//   1. a `?port=NNNN` query parameter — this is how the Tauri shell hands the sidecar's port to
//      the window it opens, with no Tauri IPC call involved at all;
//   2. `VITE_DAEMON_PORT`, a dev-time constant for running `vite dev` straight against a daemon
//      started by hand (read the printed `octoboardd listening on 127.0.0.1:<port> with key <key>`
//      line, or `$TMPDIR/octoboardd.port` for the port, and pass it as an env var);
//   3. the page's own origin, for a UI the daemon itself serves (or the dev server proxies to one,
//      see `vite.config.ts`). Not in the Tauri shell: its page origin is the shell's asset
//      handler, never the daemon, so a missing `?port=` there is a failure to report. Not on a
//      dev server without the proxy either: it leaves a WebSocket upgrade hanging without ever
//      closing it, so the page would sit at "connecting" with no hint of what is missing.
//
// The key comes from `?key=` (the shell hands it over beside the port) or `VITE_DAEMON_KEY`, in
// that order, whichever way the address was found: the daemon refuses every connection without it.
// It is on the page's URL, which is why `index.html` sends no referrer.
//
// None of them is a hard failure the UI should guess its way around; a missing address or key is
// surfaced to the user instead of defaulting to something.

import type { PlatformAdapter } from "./platform";

/** Where the daemon's WebSocket endpoints are, and the key every connection to them carries. */
export interface DaemonAddress {
  /** The `ws:`/`wss:` origin the endpoints hang off. */
  origin: string;
  key: string;
}

function parsePort(raw: string | null | undefined): number | undefined {
  if (!raw) return undefined;
  const parsed = Number(raw);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : undefined;
}

export function resolveDaemonAddress(platform: PlatformAdapter): DaemonAddress | undefined {
  const query = new URLSearchParams(window.location.search);
  const key = query.get("key") || import.meta.env.VITE_DAEMON_KEY;
  if (!key) return undefined;
  const port = parsePort(query.get("port")) ?? parsePort(import.meta.env.VITE_DAEMON_PORT);
  if (port) return { origin: `ws://127.0.0.1:${port}`, key };
  const { protocol, host } = window.location;
  if (platform.kind === "tauri" || (protocol !== "http:" && protocol !== "https:")) return undefined;
  if (import.meta.env.DEV && !__OCTOBOARD_DEV_PROXY__) return undefined;
  return { origin: `${protocol === "https:" ? "wss:" : "ws:"}//${host}`, key };
}

export function daemonWsUrl(address: DaemonAddress, path: string): string {
  return `${address.origin}${path}?key=${encodeURIComponent(address.key)}`;
}

/**
 * When the daemon failed to start at all (another instance already holds its lock, a corrupt
 * database, ...), the Tauri shell still opens the window rather than leaving the user staring at
 * nothing — it passes the failure message along as `?error=` instead of `?port=`, percent-encoded
 * since the message is free text. Checked before `resolveDaemonAddress`.
 */
export function resolveStartupError(): string | undefined {
  const raw = new URLSearchParams(window.location.search).get("error");
  return raw ?? undefined;
}

/**
 * Whether the shell reloaded this page after restarting a daemon that had died, which it marks with
 * `?recovered=1`. The page is then not an opening but a reload of a window the user may have closed
 * into the background, so it must not reveal the window.
 */
export function resolveRecovered(): boolean {
  return new URLSearchParams(window.location.search).get("recovered") === "1";
}
