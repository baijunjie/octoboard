// What kind of window of the desktop application this page is, as the shell says on the page's URL. Outside the shell
// every page is a local one: in a browser anyone can put these parameters on the address, and the page would then take
// itself for a window the shell never opened.

import { inTauri } from "./host";

/**
 * - `local`: the local daemon's window, and every page outside the desktop shell.
 * - `remote`: a window the shell opened on a daemon elsewhere (`?remote=<name>`), reached at a loopback address through
 *   the connection the application holds to it.
 * - `flyout`: the Windows notification-area flyout (`?flyout=1`), which the shell creates hidden and shows on a click
 *   of the icon, with no daemon behind it.
 */
export type WindowKind = "local" | "remote" | "flyout";

function query(): URLSearchParams {
  return new URLSearchParams(window.location.search);
}

export function windowKind(): WindowKind {
  if (!inTauri()) return "local";
  const params = query();
  if (params.get("flyout") === "1") return "flyout";
  if (params.get("remote")) return "remote";
  return "local";
}

/** The name the shell gives the daemon of a remote window (`?remote=`, the remote host's), which the window shows as
 * its own; `undefined` in any other window. */
export function remoteDaemonName(): string | undefined {
  return windowKind() === "remote" ? query().get("remote") || undefined : undefined;
}

/**
 * The identity what a remote window remembers of its daemon is kept under (`?daemon=`, given with `?remote=`), or
 * `undefined` in any other window: the local daemon's window keeps the bare keys.
 */
export function daemonIdentity(): string | undefined {
  return windowKind() === "remote" ? query().get("daemon") || undefined : undefined;
}
