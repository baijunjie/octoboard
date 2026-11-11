import { createContext, useContext } from "react";

import type { SettingsSearchContext } from "./settingsSearch";
import type { LaunchAtLoginRead } from "./useLaunchAtLogin";
import type { MacosPermissionsRead } from "./useMacosPermissions";

/** The reads of the system that settings rows wait on, held by the Settings dialog (`SettingsReadsProvider`) so that
 * the search and the rows work from the same answers (see `searchReads`). */
export interface SettingsReads {
  launchAtLogin: LaunchAtLoginRead;
  macosPermissions: MacosPermissionsRead;
}

export const SettingsReadsContext = createContext<SettingsReads | undefined>(undefined);

/** The reads of the Settings dialog the caller is in. */
export function useSettingsReads(): SettingsReads {
  const reads = useContext(SettingsReadsContext);
  if (!reads) throw new Error("useSettingsReads is used outside a SettingsReadsProvider");
  return reads;
}

/** What the search is told of `reads` (see `SettingsSearchContext.reads`): the answers the rows themselves appear on. */
export function searchReads({ launchAtLogin, macosPermissions }: SettingsReads): SettingsSearchContext["reads"] {
  return { launchAtLogin: launchAtLogin.shown, macosPermissions: macosPermissions.shown, macosPermissionsUnreadable: macosPermissions.unreadable };
}
