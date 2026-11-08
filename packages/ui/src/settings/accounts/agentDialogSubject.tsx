import React, { useEffect } from "react";

import { isAgentAdded, isAgentUnavailable } from "../../agents/agents";
import { providerLabel } from "../../agents/providers";
import { AgentIcon } from "../../components/icons/AgentIcon";
import { useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { useDaemonStore, useProviders } from "../../store";

/**
 * Whether the agent an agent dialog is about is gone, read from the store as it is now so a change from another window
 * shows: it has left the list of added agents or, with `needsInstalled`, its CLI is no longer installed. `onGone` is
 * called once it is, and the dialog renders nothing.
 */
export function useAgentDialogGone(agent: Agent, onGone: () => void, needsInstalled = false): boolean {
  const gone = useDaemonStore((s) => !isAgentAdded(s.settings.added_agents, agent) || (needsInstalled && isAgentUnavailable(s.agentAvailability, agent)));
  useEffect(() => {
    if (gone) onGone();
  }, [gone]);
  return gone;
}

/** The agent's icon and name, the first line of an agent dialog's body, which names the agent as the plain title does
 * not. */
export function AgentHeading({ agent }: { agent: Agent }): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  return (
    <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
      <AgentIcon agent={agent} />
      <span dir="auto" className="min-w-0 wrap-anywhere">{providerLabel(t, providers, agent)}</span>
    </div>
  );
}
