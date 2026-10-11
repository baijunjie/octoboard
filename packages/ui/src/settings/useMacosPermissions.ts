import { useCallback, useEffect, useRef, useState } from "react";

import type { MacosPermissionKind, MacosPermissionsStatus } from "../platform";
import { usePlatform } from "../platform/react";

type PerKind = Partial<Record<MacosPermissionKind, boolean>>;

/**
 * The macOS privacy permissions as the shell reports them, with the actions that change them. The
 * grant can change entirely outside the app, so it is read on mount, whenever the window regains
 * focus (the user flips the switch in System Settings and comes back, which is the only way
 * Accessibility is ever granted) and right after either action.
 *
 * - `status`: `undefined` until the first read is in, and where there is nothing to report — no
 *   capability, or a shell that is not on macOS.
 * - `loaded`: the first read has come back, whatever it said, so `undefined` can be told from a
 *   read still in flight.
 * - `unreadable`: the shell refused the read. Separate from `undefined` because a shell that
 *   cannot be asked is not a shell with nothing to report, and the two want different words.
 * - `refused`: a `request` for that permission came back without granting, so the section points
 *   at System Settings instead of offering the prompt again. Held for as long as the section is
 *   mounted, which the settings tabs end on a switch away: the button comes back, and a press
 *   that macOS answers silently puts the guidance back with it.
 * - `pending`: a `request` is in flight. Only Screen Recording's is ever observably pending —
 *   Accessibility's returns at once, while its dialog is still coming up.
 */
export function useMacosPermissions(): {
  status: MacosPermissionsStatus | undefined;
  loaded: boolean;
  unreadable: boolean;
  refused: PerKind;
  pending: PerKind;
  request: (kind: MacosPermissionKind) => Promise<void>;
  openSystemSettings: (kind: MacosPermissionKind) => Promise<void>;
} {
  const { macosPermissions } = usePlatform();
  const [status, setStatus] = useState<MacosPermissionsStatus | undefined>();
  const [loaded, setLoaded] = useState(false);
  const [unreadable, setUnreadable] = useState(false);
  const [refused, setRefused] = useState<PerKind>({});
  const [pending, setPending] = useState<PerKind>({});
  const mounted = useRef(true);
  // Reads overlap: pressing Grant starts one when the request resolves and another as the window
  // regains focus behind the closing prompt. Only the newest may land, or an older answer could
  // overwrite it.
  const reads = useRef(0);

  const refresh = useCallback(async () => {
    const read = ++reads.current;
    let next: MacosPermissionsStatus | undefined;
    let failed = false;
    try {
      next = await macosPermissions?.status();
    } catch {
      failed = true;
    }
    if (!mounted.current || read !== reads.current) return;
    setUnreadable(failed);
    setStatus(next);
    setLoaded(true);
  }, [macosPermissions]);

  useEffect(() => {
    mounted.current = true;
    void refresh();
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      mounted.current = false;
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh]);

  const request = useCallback(
    async (kind: MacosPermissionKind) => {
      if (!macosPermissions) return;
      setPending((p) => ({ ...p, [kind]: true }));
      try {
        const granted = await macosPermissions.request(kind);
        if (mounted.current && !granted) setRefused((r) => ({ ...r, [kind]: true }));
      } catch {
        // The prompt could not be raised; the refresh below shows where things stand.
      }
      if (mounted.current) setPending((p) => ({ ...p, [kind]: false }));
      await refresh();
    },
    [macosPermissions, refresh],
  );

  const openSystemSettings = useCallback(
    async (kind: MacosPermissionKind) => {
      try {
        await macosPermissions?.openSystemSettings(kind);
      } catch {
        // Only an IPC failure reaches here. The shell does not report back on the pane itself, so a
        // pane that failed to open is indistinguishable from one that did.
      }
      await refresh();
    },
    [macosPermissions, refresh],
  );

  return { status, loaded, unreadable, refused, pending, request, openSystemSettings };
}
