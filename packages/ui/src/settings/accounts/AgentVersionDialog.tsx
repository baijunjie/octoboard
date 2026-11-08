import React from "react";

import { Dialog } from "../../components/dialog/Dialog";
import { useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { AgentHeading, useAgentDialogGone } from "./agentDialogSubject";
import { AgentCliVersionSetting } from "./AgentCliVersionSetting";

/**
 * One agent's CLI version in a modal of its own, opened from the Version entry of the agent tab's actions menu: the
 * `AgentCliVersionSetting` row, which is the only content. The title is plain ("Agent version"); the body's first line
 * names the agent, as `AgentSettingsDialog`'s does. It has no footer: the header's close button, Escape and a click
 * outside close it, as does the agent leaving the list or its CLI no longer being installed. The row reads the store as
 * it is now, so an update another window started shows. The error alert of an update started from the row is raised by
 * `AgentCliUpdateAlerts`, so it outlives this dialog.
 */
export function AgentVersionDialog({ agent, onClose, onGone }: { agent: Agent; onClose: () => void; onGone: () => void }): React.ReactElement | null {
  const t = useT();
  const gone = useAgentDialogGone(agent, onGone, true);
  if (gone) return null;
  return (
    <Dialog size="lg" title={t("settings.accounts.agentVersion.title")} onClose={onClose} footer={null}>
      <div className="flex flex-col gap-4">
        <AgentHeading agent={agent} />
        <AgentCliVersionSetting agent={agent} />
      </div>
    </Dialog>
  );
}
