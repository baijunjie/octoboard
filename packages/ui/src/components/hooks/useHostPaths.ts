import { useShallow } from "zustand/react/shallow";

import { type HostPaths, hostPathsOf } from "../../format/hostPath";
import { useDaemonStore, useDaemonStoreOrEmpty } from "../../store";

/** The daemon host's home directory and path style, for showing and judging its paths (`format/hostPath`); the
 * style is `unix` until the first snapshot. */
export function useHostPaths(): HostPaths {
  return useDaemonStore(useShallow(hostPathsOf));
}

/** `useHostPaths` for what also renders outside a `DaemonProvider` (see `useDaemonStoreOrEmpty`). */
export function useHostPathsOrEmpty(): HostPaths {
  return useDaemonStoreOrEmpty(useShallow(hostPathsOf));
}
