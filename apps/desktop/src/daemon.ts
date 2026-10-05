// Locates the daemon. The daemon binds an OS-assigned port (see `daemon/PROTOCOL.md`), so the UI
// is never allowed to hardcode one. Two sources, in priority order:
//
//   1. a `?port=NNNN` query parameter — this is how the Tauri shell hands the sidecar's port to
//      the window it opens, with no Tauri IPC call involved at all;
//   2. `VITE_DAEMON_PORT`, a dev-time constant for running `vite dev` straight against a daemon
//      started by hand (read the printed `octoboardd listening on 127.0.0.1:<port>` line, or
//      `$TMPDIR/octoboardd.port`, and pass it as an env var).
//
// Neither present is a hard failure the UI should guess its way around; it is surfaced to the user
// instead of defaulting to some port.
export function resolveDaemonPort(): number | undefined {
  const fromQuery = new URLSearchParams(window.location.search).get("port");
  if (fromQuery) {
    const parsed = Number(fromQuery);
    if (Number.isInteger(parsed) && parsed > 0) return parsed;
  }
  const fromEnv = import.meta.env.VITE_DAEMON_PORT;
  if (fromEnv) {
    const parsed = Number(fromEnv);
    if (Number.isInteger(parsed) && parsed > 0) return parsed;
  }
  return undefined;
}

export function daemonWsUrl(port: number, path: string): string {
  return `ws://127.0.0.1:${port}${path}`;
}

/**
 * When the daemon failed to start at all (another instance already holds its lock, a corrupt
 * database, ...), the Tauri shell still opens the window rather than leaving the user staring at
 * nothing — it passes the failure message along as `?error=` instead of `?port=`, percent-encoded
 * since the message is free text. Checked before `resolveDaemonPort`.
 */
export function resolveStartupError(): string | undefined {
  const raw = new URLSearchParams(window.location.search).get("error");
  return raw ?? undefined;
}
