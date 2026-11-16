import extensions from "../../../config/launchable-extensions.json";

/** The extensions of what the system launches or acts on rather than shows, from `config/launchable-extensions.json`,
 * which the daemon and the desktop shell read as well. The first two lists hold for every system; the Windows list
 * (`.js`, `.lnk`, ...) only for a Windows one, where those files launch and elsewhere are plain data. */
const EVERYWHERE = new Set([...extensions.anyComponent, ...extensions.finalFile]);
const WINDOWS = new Set(extensions.windowsFinalFile);

/** Whether a file of this name is one the system could launch rather than show, by what follows the last dot, a leading
 * one included, and without regard to case. `windows` says whether the system that would open it is a Windows one: the
 * daemon's host for a file its default application opens, this computer for the copy of a file Download and open makes. Only
 * the name is judged: a file with an executable bit and no extension is the daemon's to tell, and it refuses to open it
 * all the same. */
export function isLaunchableName(name: string, windows: boolean): boolean {
  // What follows the last dot, a leading one included: the system's opener takes `.bat` for a batch file.
  const dot = name.replace(/[. ]+$/, "").lastIndexOf(".");
  if (dot < 0) return false;
  const extension = name.slice(dot + 1).replace(/[. ]+$/, "").toLowerCase();
  return EVERYWHERE.has(extension) || (windows && WINDOWS.has(extension));
}
