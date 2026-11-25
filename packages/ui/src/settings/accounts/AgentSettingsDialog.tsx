import React from "react";

import { ProviderMessage } from "../../agents/ProviderMessage";
import { providerMessageKey } from "../../agents/providers";
import { Dialog } from "../../components/dialog/Dialog";
import { ProviderName } from "../../components/text/ProviderName";
import { Message, useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { useProviders } from "../../store";
import { AgentHeading, useAgentDialogGone } from "./agentDialogSubject";
import { AgentProviderSettings } from "./AgentProviderSettings";
import { AgentTagsSetting } from "./AgentTagsSetting";

/** The provider's general note about its sessions (`sessionNote` of its presentation module), as secondary text of the
 * dialog's opening lines: it holds whatever the rows below are set to, so it stands with the line that speaks for all of
 * them and not in one row's help. Nothing is drawn for a provider whose module has none. */
function AgentSessionNote({ agent }: { agent: Agent }): React.ReactElement | null {
  const providers = useProviders();
  if (!providerMessageKey(agent, "sessionNote")) return null;
  return <p className="text-sm text-app-muted"><ProviderMessage providers={providers} id={agent} name="sessionNote" /></p>;
}

/**
 * One agent's settings in a modal of its own, opened from the agent tab's actions menu: the Tags row and the rows of the
 * settings its descriptor declares, so the tab itself holds the table of accounts alone. Each of them applies to every
 * account of the agent, which one line under the agent's name says for all of them; a provider's note about its sessions
 * (`AgentSessionNote`) follows that line. The title is plain ("Agent
 * settings"); the body's first line names the agent, as `AccountSettingsDialog`'s names its account. It has no footer: the header's close button, Escape and a
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
        <div className="flex flex-col gap-1">
          <AgentHeading agent={agent} />
          <p className="text-sm text-app-muted"><Message id="settings.accounts.agentSettings.scope" params={{ agent: <ProviderName agent={agent} /> }} /></p>
          <AgentSessionNote agent={agent} />
        </div>
        <div>
          <AgentTagsSetting agent={agent} />
          <AgentProviderSettings agent={agent} />
        </div>
      </div>
    </Dialog>
  );
}
