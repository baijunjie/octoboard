// Locates the daemon and the key it requires. The daemon binds an OS-assigned port and accepts
// only its launch's key or, run on its own, its owner's key until that is rotated (see
// `apps/daemon/PROTOCOL.md`), so the UI is never allowed to hardcode either. The address, in priority order:
//
//   1. a `?port=NNNN` query parameter — this is how the Tauri shell hands the sidecar's port to
//      the window it opens, with no Tauri IPC call involved at all;
//   2. `VITE_DAEMON_PORT`, a dev-time constant for running `vite dev` straight against a daemon
//      started by hand (read the printed `octoboardd listening on 127.0.0.1:<port> with key <key>`
//      line, or `~/.octoboard/octoboardd.port` for the port, and pass it as an env var);
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
  /** Set when the page was loaded from the daemon itself (the browser build the server form serves at its own address):
   * its `/key-check` is then on this page's origin, and a connection refused with a key the daemon no longer accepts can
   * be told from a daemon that is merely unreachable. Never set for the desktop shell, a dev server or an address given
   * as a port. */
  servedByDaemon?: true;
}

function parsePort(raw: string | null | undefined): number | undefined {
  if (!raw) return undefined;
  const parsed = Number(raw);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : undefined;
}

/** The key the page was given: `?key=`, else `VITE_DAEMON_KEY`. */
function pageKey(query: URLSearchParams): string | undefined {
  return query.get("key") || import.meta.env.VITE_DAEMON_KEY || undefined;
}

/** The port the page was given: `?port=`, else `VITE_DAEMON_PORT`. */
function pagePort(query: URLSearchParams): number | undefined {
  return parsePort(query.get("port")) ?? parsePort(import.meta.env.VITE_DAEMON_PORT);
}

/** Whether the page's own origin can be a daemon's: an http(s) page outside the Tauri shell. */
function originMayBeDaemon(platform: PlatformAdapter): boolean {
  const { protocol } = window.location;
  return platform.kind !== "tauri" && (protocol === "http:" || protocol === "https:");
}

export function resolveDaemonAddress(platform: PlatformAdapter): DaemonAddress | undefined {
  const query = new URLSearchParams(window.location.search);
  const key = pageKey(query);
  if (!key) return undefined;
  const port = pagePort(query);
  if (port) return { origin: `ws://127.0.0.1:${port}`, key };
  if (!originMayBeDaemon(platform)) return undefined;
  if (import.meta.env.DEV && !__OCTOBOARD_DEV_PROXY__) return undefined;
  const { protocol, host } = window.location;
  return {
    origin: `${protocol === "https:" ? "wss:" : "ws:"}//${host}`,
    key,
    ...(import.meta.env.DEV ? undefined : { servedByDaemon: true as const }),
  };
}

/**
 * Whether this page is the production browser build opened at an address with no key: what the daemon serves, opened
 * without the `?key=` its address carries. Not the desktop shell, whose missing key is a failure to report, nor a dev
 * server, whose missing address or key is the developer's to give, nor a page given a port, which is meant to reach
 * another daemon.
 */
export function isKeylessDaemonPage(platform: PlatformAdapter): boolean {
  const query = new URLSearchParams(window.location.search);
  return !pageKey(query) && !pagePort(query) && originMayBeDaemon(platform) && !import.meta.env.DEV;
}

/** How long the key check may take before the daemon is taken as unreachable. */
export const KEY_CHECK_TIMEOUT_MS = 5000;

/**
 * Asks the daemon that served this page (`GET /key-check`, on the page's own origin) whether it still accepts the page's
 * key, after a connection attempt failed. Only a `401` is a refusal: `204` says the key is accepted, a `404` is a daemon
 * older than the route, and any other answer, a network failure or a timeout leaves the failure as the unreachable
 * daemon it was taken for. Only `address.key` is read: the route is asked on this page's origin, which is the daemon's
 * only for an address marked `servedByDaemon`. Credentials go as for any same-origin request, so a proxy in front that
 * asks for its own is not mistaken for the daemon refusing the key.
 */
export async function daemonRefusedKey(address: DaemonAddress): Promise<boolean> {
  try {
    const response = await fetch(`/key-check?key=${encodeURIComponent(address.key)}`, {
      cache: "no-store",
      referrerPolicy: "no-referrer",
      signal: AbortSignal.timeout(KEY_CHECK_TIMEOUT_MS),
    });
    return response.status === 401;
  } catch {
    return false;
  }
}

export function daemonWsUrl(address: DaemonAddress, path: string): string {
  return `${address.origin}${path}?key=${encodeURIComponent(address.key)}`;
}

/** Only loopback addresses establish that the daemon shares this machine's filesystem. */
export function isLocalDaemon(address: DaemonAddress): boolean {
  try {
    const { hostname, protocol } = new URL(address.origin);
    return (protocol === "ws:" || protocol === "wss:") && ["localhost", "127.0.0.1", "[::1]"].includes(hostname);
  } catch {
    return false;
  }
}

/**
 * When the daemon failed to start at all (another instance already holds its lock, a corrupt
 * database, ...), the Tauri shell still opens the window rather than leaving the user staring at
 * nothing — it passes the failure message along as `?error=` instead of `?port=`, percent-encoded
 * since the message is free text. Checked before `resolveDaemonAddress`. Read only under the shell:
 * in a browser anyone can put text there, and the page would show it as the daemon's failure on the
 * daemon's own trusted origin.
 */
export function resolveStartupError(platform: PlatformAdapter): string | undefined {
  if (platform.kind !== "tauri") return undefined;
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
