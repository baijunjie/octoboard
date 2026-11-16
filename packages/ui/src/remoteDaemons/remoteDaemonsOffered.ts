import { windowKind, type PlatformAdapter } from "../platform";

/** Whether the window has the Remote devices button in its rail, and the page it shows: only the desktop
 * application's own window, whose shell gives it the capability. A browser (whose adapter has none of the shell's
 * capabilities) and the window of a remote daemon (which the shell gives none either) never have it. */
export function remoteDaemonsOffered(platform: PlatformAdapter): boolean {
  // Also by the window's kind, for a platform that has the capability whatever window it is in (a test's, the
  // gallery's).
  return platform.remoteDaemons !== undefined && windowKind() === "local";
}
