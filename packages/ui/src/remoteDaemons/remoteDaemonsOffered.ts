import { windowKind, type PlatformAdapter } from "../platform";

/** Whether the rail has the group of connected remote daemons: only in the desktop application's own window. The
 * capability is in every window of the application, but the group is the application's, not a daemon's, so a browser
 * (whose adapter has none of the shell's capabilities) and the window of a remote daemon never have it. */
export function remoteDaemonsOffered(platform: PlatformAdapter): boolean {
  // A remote daemon's window has the capability too, which is why the window's kind is checked.
  return platform.remoteDaemons !== undefined && windowKind() === "local";
}
