import React from "react";

import { DaemonSwitchSetting } from "./DaemonSwitchSetting";

/** The automatic cleanup the application does: whether archived sessions whose conversation is gone are deleted. */
export function CleanupSection(): React.ReactElement {
  return <DaemonSwitchSetting field="auto_delete_unresumable_archives" />;
}
