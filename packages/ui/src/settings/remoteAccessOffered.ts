import type { PlatformAdapter } from "../platform";
import type { RemoteAccess } from "../protocol";

/** Whether Settings has the Access section: only in the desktop application's own window, for a daemon that
 * offers remote access (`remoteAccess` is set). The section configures this computer's own daemon, so a browser, which
 * reaches a daemon through the very access the section switches, never has it: the daemon would refuse a browser's
 * change anyway (`remote_access_refused_to_browser`), and the browser's platform adapter has none of the shell's
 * capabilities, so the presence of `remoteLogin` is what tells the window from a browser. */
export function remoteAccessOffered(platform: PlatformAdapter, remoteAccess: RemoteAccess | null): boolean {
  // A remote daemon's window has it neither: its adapter leaves `remoteLogin` out too, as the section would configure
  // that daemon, not this computer's.
  return remoteAccess !== null && platform.remoteLogin !== undefined;
}
