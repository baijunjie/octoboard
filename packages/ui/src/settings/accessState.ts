import type { ConnectionDetails, RemoteAccess } from "../protocol";

/** The state of a daemon that serves the browser at a port of its own, with that port. */
export type BrowserPortAccess = RemoteAccess & { browser_port: number };

/** Whether the daemon serves the browser at a port of its own (`browser_port` is set: the macOS and Windows
 * applications'), so that the section has its This computer's browser group and its Browser tab. */
export function hasBrowserPort(remoteAccess: RemoteAccess | null): remoteAccess is BrowserPortAccess {
  return remoteAccess?.browser_port != null;
}

/** The address a browser on this computer opens as it is, while the daemon serves the browser at its browser port
 * (macOS, Windows): browser access is on and the port is open, which a failure to listen on it (taken by another program
 * or user) says it is not. Null otherwise. */
export function servedBrowserAddress(remoteAccess: RemoteAccess | null): string | null {
  return remoteAccess !== null && remoteAccess.browser_state === "serving" && remoteAccess.browser_port !== null ? remoteAccess.address : null;
}

/** Whether browser access is on but the daemon could not open its port (taken by another program or user, say). */
export function browserPortFailed(remoteAccess: RemoteAccess | null): boolean {
  return remoteAccess !== null && remoteAccess.browser_port !== null && remoteAccess.browser_state === "failed";
}

/** The ssh command that forwards a port of another computer to the browser port, while the daemon serves the browser at it. */
export function servedForwardCommand(remoteAccess: RemoteAccess | null): string | null {
  return servedBrowserAddress(remoteAccess) !== null ? (remoteAccess?.forward_command ?? null) : null;
}

/** The connection details another computer's app fills its Add remote daemon form from, while remote access serves. */
export function servedConnection(remoteAccess: RemoteAccess | null): ConnectionDetails | null {
  return remoteAccess?.state === "serving" ? remoteAccess.connection : null;
}

/** Whether the daemon serves anything a key opens right now: remote access or browser access. */
export function anyServing(remoteAccess: RemoteAccess | null): boolean {
  return remoteAccess !== null && (remoteAccess.state === "serving" || remoteAccess.browser_state === "serving");
}
