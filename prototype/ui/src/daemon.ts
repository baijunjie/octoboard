// Locates the daemon. The daemon binds an OS-assigned port (see prototype/PROTOCOL.md), so the
// UI is never allowed to hardcode one. Two sources, in priority order:
//
//   1. a `?port=NNNN` query parameter — this is how the Tauri shell hands the sidecar's port to
//      the page it loads, with no Tauri IPC call involved at all (see prototype/README.md);
//   2. `VITE_DAEMON_PORT`, a dev-time constant for running `vite dev` straight against a daemon
//      started by hand (read the printed `obd-proto listening on 127.0.0.1:<port>` line, or
//      `$TMPDIR/obd-proto.port`, and pass it as an env var).
//
// Neither present is a hard failure the UI should guess its way around; it is surfaced to the
// control strip instead.
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
