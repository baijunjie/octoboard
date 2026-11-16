// Imports nothing that reads a stored preference as it loads: `frame.tsx` imports this before `prepareStorage` has run.
import type { RemoteFilesCapability, RemoteForward, RemoteForwardsCapability } from "../platform";

/** The ports a remote window's shell forwards, as a scenario lists them: the rail's entry and its popover drive them as
 * they drive the desktop app's. Stop lets a port go and tells the listener, as the shell does. */
export function fixtureRemoteForwards(forwards: RemoteForward[]): RemoteForwardsCapability {
  let list = forwards.map((forward) => ({ ...forward }));
  const listeners = new Set<() => void>();
  return {
    list: async () => list.map((forward) => ({ ...forward })),
    stop: async (remotePort) => {
      list = list.filter((forward) => forward.remotePort !== remotePort);
      listeners.forEach((listener) => listener());
    },
    onChange: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

/** The shell's copy of a remote file, which takes every part and opens the copy without showing anything: how far a
 * copy comes is the daemon's side (a binary's `answeredParts`), not this one's. */
export function fixtureRemoteFiles(): RemoteFilesCapability {
  return {
    begin: async () => "copy",
    write: async () => {},
    open: async () => {},
    discard: async () => {},
  };
}
