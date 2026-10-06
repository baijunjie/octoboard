// Locates the daemon. The daemon binds an OS-assigned port (see `apps/daemon/PROTOCOL.md`), so the UI
// is never allowed to hardcode one. Sources, in priority order:
//
//   1. a `?port=NNNN` query parameter — this is how the Tauri shell hands the sidecar's port to
//      the window it opens, with no Tauri IPC call involved at all;
//   2. `VITE_DAEMON_PORT`, a dev-time constant for running `vite dev` straight against a daemon
//      started by hand (read the printed `octoboardd listening on 127.0.0.1:<port>` line, or
//      `$TMPDIR/octoboardd.port`, and pass it as an env var);
//   3. the page's own origin, for a UI the daemon itself serves (or the dev server proxies to one,
//      see `vite.config.ts`). Not in the Tauri shell: its page origin is the shell's asset
//      handler, never the daemon, so a missing `?port=` there is a failure to report. Not on a
//      dev server without the proxy either: it leaves a WebSocket upgrade hanging without ever
//      closing it, so the page would sit at "connecting" with no hint of what is missing.
//
// None of them is a hard failure the UI should guess its way around; it is surfaced to the user
// instead of defaulting to some port.

import type { PlatformAdapter } from "./platform";

/** The `ws:`/`wss:` origin the daemon's WebSocket endpoints hang off. */
export type DaemonOrigin = string;

function parsePort(raw: string | null | undefined): number | undefined {
  if (!raw) return undefined;
  const parsed = Number(raw);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : undefined;
}

export function resolveDaemonOrigin(platform: PlatformAdapter): DaemonOrigin | undefined {
  const port =
    parsePort(new URLSearchParams(window.location.search).get("port")) ??
    parsePort(import.meta.env.VITE_DAEMON_PORT);
  if (port) return `ws://127.0.0.1:${port}`;
  const { protocol, host } = window.location;
  if (platform.kind === "tauri" || (protocol !== "http:" && protocol !== "https:")) return undefined;
  if (import.meta.env.DEV && !__OCTOBOARD_DEV_PROXY__) return undefined;
  return `${protocol === "https:" ? "wss:" : "ws:"}//${host}`;
}

export function daemonWsUrl(origin: DaemonOrigin, path: string): string {
  return `${origin}${path}`;
}

/**
 * When the daemon failed to start at all (another instance already holds its lock, a corrupt
 * database, ...), the Tauri shell still opens the window rather than leaving the user staring at
 * nothing — it passes the failure message along as `?error=` instead of `?port=`, percent-encoded
 * since the message is free text. Checked before `resolveDaemonOrigin`.
 */
export function resolveStartupError(): string | undefined {
  const raw = new URLSearchParams(window.location.search).get("error");
  return raw ?? undefined;
}
