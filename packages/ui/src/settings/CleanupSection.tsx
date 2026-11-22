import React from "react";

import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { UninstallCleanup } from "./UninstallCleanup";

/** The cleaning up the application does, two rows: the automatic one, whether archived sessions whose conversation is
 * gone are deleted, and the cleanup before the application is uninstalled, last since it is used once at most. */
export function CleanupSection(): React.ReactElement {
  return (
    <>
      <DaemonSwitchSetting field="auto_delete_unresumable_archives" />
      <UninstallCleanup />
    </>
  );
}
