import React from "react";

import { SettingsReadsContext, type SettingsReads } from "./settingsReads";
import { useLaunchAtLogin } from "./useLaunchAtLogin";
import { useMacosPermissionsRead } from "./useMacosPermissions";

/** Reads the system for the Settings dialog inside it, once, however many sections look at the answers. */
export function SettingsReadsProvider({ children }: { children: React.ReactNode }): React.ReactElement {
  const reads: SettingsReads = { launchAtLogin: useLaunchAtLogin(), macosPermissions: useMacosPermissionsRead() };
  return <SettingsReadsContext.Provider value={reads}>{children}</SettingsReadsContext.Provider>;
}
