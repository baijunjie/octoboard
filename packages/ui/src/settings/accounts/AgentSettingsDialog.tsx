import React from "react";

import { Dialog } from "../../components/dialog/Dialog";
import { useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { AgentHeading, useAgentDialogGone } from "./agentDialogSubject";
import { AgentProviderSettings } from "./AgentProviderSettings";
import { AgentTagsSetting } from "./AgentTagsSetting";

/**
 * One agent's settings in a modal of its own, opened from the agent tab's actions menu: the Tags row and the rows of the
 * settings its descriptor declares, each of which applies to every account of the agent, so the
 * tab itself holds the table of accounts alone. The title is plain ("Agent settings"); the body's first line names the
 * agent, as `AccountSettingsDialog`'s names its account. It has no footer: the header's close button, Escape and a
 * click outside close it, as does the agent leaving the list of added agents. Its rows read the store as it is now, as
 * they did in the tab, so a change from another window shows.
 */
export function AgentSettingsDialog({ agent, onClose, onGone }: { agent: Agent; onClose: () => void; onGone: () => void }): React.ReactElement | null {
  const t = useT();
  const gone = useAgentDialogGone(agent, onGone);
  if (gone) return null;
  return (
    <Dialog size="lg" title={t("settings.accounts.agentSettings.title")} onClose={onClose} footer={null}>
      <div className="flex flex-col gap-4">
        <AgentHeading agent={agent} />
        <div>
          <AgentTagsSetting agent={agent} />
          <AgentProviderSettings agent={agent} />
        </div>
      </div>
    </Dialog>
  );
}
