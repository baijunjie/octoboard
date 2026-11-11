import { useCallback, useEffect, useRef, useState } from "react";

import { onWindowFocusReturn } from "../components/hooks/windowFocus";
import type { MacosPermissionKind, MacosPermissionsStatus } from "../platform";
import { usePlatform } from "../platform/react";
import { useSettingsReads } from "./settingsReads";

type PerKind = Partial<Record<MacosPermissionKind, boolean>>;

/** What the shell reports of the macOS privacy permissions, as `useMacosPermissionsRead` holds it. */
export interface MacosPermissionsRead {
  /** `undefined` until the first read is in, and where there is nothing to report: no capability, or a shell that is
   * not on macOS. */
  status: MacosPermissionsStatus | undefined;
  /** The first read has come back, whatever it said, so `undefined` can be told from a read still in flight. */
  loaded: boolean;
  /** The permission rows are on screen: a read has reported the grants. The rows and the search both go by this. */
  shown: boolean;
  /** The shell refused the read. Separate from `undefined` because a shell that cannot be asked is not a shell with
   * nothing to report, and the two want different words. */
  unreadable: boolean;
  /** Reads again; the read of a newer call wins over an older one still out. */
  refresh: () => Promise<void>;
  /** Has the window's return to focus read again for as long as the caller stays (until the returned function is
   * called), and reads once at the start if the window regained focus while nothing followed; the read on mount is made
   * regardless. See `useFollowMacosPermissions`. */
  follow: () => () => void;
}

/**
 * The macOS privacy permissions as the shell reports them. The grant can change entirely outside the app, so it is read
 * on mount and, while something follows it (`follow`), whenever the window regains focus (the user flips the switch in
 * System Settings and comes back, which is the only way Accessibility is ever granted); `refresh` reads it again after
 * an action (`useMacosPermissions`). A read starts a process that can take seconds, so the focus read is made only for
 * a reader that would show the answer: the section, or a search with words in it. Held by the Settings dialog
 * (`SettingsReadsProvider`), not by the section, because the search counts the permission rows only once this has
 * answered, and a section the search takes out of the list is not mounted to ask.
 */
export function useMacosPermissionsRead(): MacosPermissionsRead {
  const { macosPermissions } = usePlatform();
  const [status, setStatus] = useState<MacosPermissionsStatus | undefined>();
  const [loaded, setLoaded] = useState(false);
  const [unreadable, setUnreadable] = useState(false);
  const mounted = useRef(true);
  const followers = useRef(0);
  // The window came back to focus while nobody followed, so the read was skipped: the next follower reads once.
  const missed = useRef(false);
  // Reads overlap: pressing Grant starts one right after the request, while the prompt is still up,
  // and another as the window regains focus once it closes, which can come before the first has
  // returned. Only the newest may land, or an older answer could overwrite it.
  const reads = useRef(0);

  const refresh = useCallback(async () => {
    const read = ++reads.current;
    let next: MacosPermissionsStatus | undefined;
    let failed = false;
    // Started inside a promise chain, so a platform method that throws synchronously fails the read like a rejection.
    const reading = Promise.resolve().then(() => macosPermissions?.status());
    try {
      next = await reading;
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
    const undoFocus = onWindowFocusReturn(() => {
      if (followers.current > 0) void refresh();
      else missed.current = true;
    });
    return () => {
      mounted.current = false;
      undoFocus();
    };
  }, [refresh]);

  const follow = useCallback(() => {
    followers.current++;
    // `missed` is only set while nobody follows, so a flag found here was set before this follower.
    if (missed.current) {
      missed.current = false;
      void refresh();
    }
    return () => {
      followers.current--;
    };
  }, [refresh]);

  return { status, loaded, shown: status !== undefined, unreadable, refresh, follow };
}

/** Has the dialog's read of the macOS permissions follow the window's focus (see `MacosPermissionsRead.follow`) while
 * `active`. */
export function useFollowMacosPermissions(active: boolean): void {
  const { follow } = useSettingsReads().macosPermissions;
  useEffect(() => (active ? follow() : undefined), [active, follow]);
}

/**
 * The macOS privacy permissions as the dialog's read reports them (`useMacosPermissionsRead`), with the actions that
 * change them, each followed by a read.
 *
 * - `status`, `loaded`, `shown`, `unreadable`: see `MacosPermissionsRead`. The section follows the window's focus
 *   (`useFollowMacosPermissions`) for as long as it is mounted.
 * - `refused`: a `request` for that permission came back without granting, so the section points
 *   at System Settings instead of offering the prompt again. Held for as long as the section is
 *   mounted, which the settings tabs end on a switch away: the button comes back, and a press
 *   that macOS answers silently puts the guidance back with it.
 * - `pending`: a `request` is in flight. Not observably shown: both requests return at once,
 *   while the system's prompt is still coming up.
 * - `request`: rejects when the request could not be made, after the read and with `pending` cleared,
 *   for the caller to show. A request macOS answers without granting does not reject; it sets `refused`.
 * - `openSystemSettings`: rejects when the pane did not open, after the read, for the caller to show.
 */
export function useMacosPermissions(): Omit<MacosPermissionsRead, "refresh" | "follow"> & {
  refused: PerKind;
  pending: PerKind;
  request: (kind: MacosPermissionKind) => Promise<void>;
  openSystemSettings: (kind: MacosPermissionKind) => Promise<void>;
} {
  const { macosPermissions } = usePlatform();
  const { status, loaded, shown, unreadable, refresh } = useSettingsReads().macosPermissions;
  useFollowMacosPermissions(true);
  const [refused, setRefused] = useState<PerKind>({});
  const [pending, setPending] = useState<PerKind>({});
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const request = useCallback(
    async (kind: MacosPermissionKind) => {
      if (!macosPermissions) return;
      setPending((p) => ({ ...p, [kind]: true }));
      // Promise chains, not `try`/`finally`: the React Compiler leaves a function with one uncompiled.
      await Promise.resolve().then(() => macosPermissions.request(kind)).then((granted) => {
        if (mounted.current && !granted) setRefused((r) => ({ ...r, [kind]: true }));
      }).finally(async () => {
        if (mounted.current) setPending((p) => ({ ...p, [kind]: false }));
        await refresh();
      });
    },
    [macosPermissions, refresh],
  );

  const openSystemSettings = useCallback(
    async (kind: MacosPermissionKind) => {
      await Promise.resolve().then(() => macosPermissions?.openSystemSettings(kind)).finally(refresh);
    },
    [macosPermissions, refresh],
  );

  return { status, loaded, shown, unreadable, refused, pending, request, openSystemSettings };
}
