import { useLayoutEffect } from "react";

import { announceDaemonHost, type DaemonHost } from "../i18n/platformTerms";
import type { HostInfo } from "../protocol";
import { useDaemonStoreOrEmpty } from "./react";

/** The catalog's term for each value of a `snapshot.host` field. */
const OS: Record<HostInfo["os"], NonNullable<DaemonHost["os"]>> = { macos: "macos", windows: "windows", linux: "linux" };
const TRASH: Record<HostInfo["trash"], NonNullable<DaemonHost["terms"]["trash"]>> = { trash: "trash", recycle_bin: "recycleBin" };
const SETTINGS: Record<HostInfo["settings"], NonNullable<DaemonHost["terms"]["settings"]>> = { system_settings: "systemSettings", settings: "settings" };

/** The catalog's term for `value`, or `undefined` for a value `terms` does not name (a newer daemon's). */
function termOf<T>(terms: Record<string, T>, value: unknown): T | undefined {
  return typeof value === "string" && Object.hasOwn(terms, value) ? terms[value] : undefined;
}

/** What the copy takes from the daemon's host (`snapshot.host`): its operating system and its words for the Trash and
 * the settings app, in the catalog's terms. A daemon older than a word sends none, and a newer one may send a value this
 * client does not know; either is left out, and the client's own stands in. */
export function daemonHostOf(host: HostInfo): DaemonHost {
  const os = termOf(OS, host.os);
  const trash = termOf(TRASH, host.trash);
  const settings = termOf(SETTINGS, host.settings);
  return {
    ...(os !== undefined && { os }),
    terms: {
      ...(trash !== undefined && { trash }),
      ...(settings !== undefined && { settings }),
    },
  };
}

/**
 * Announces the host of the daemon in context to the catalog (`announceDaemonHost`), from its first snapshot on and
 * again whenever a snapshot names another, so what the copy says happens on the daemon's machine is worded in that
 * machine's terms whatever the client runs on. Mounted once per window, under its `DaemonProvider`; with no daemon, or
 * once unmounted, none is announced and the client's own words stand in.
 *
 * A layout effect, so the messages already rendered are worded anew before the window is painted.
 */
export function DaemonHostAnnouncement(): null {
  const host = useDaemonStoreOrEmpty((state) => state.host);
  useLayoutEffect(() => announceDaemonHost(host ? daemonHostOf(host) : undefined), [host]);
  useLayoutEffect(() => () => announceDaemonHost(undefined), []);
  return null;
}
