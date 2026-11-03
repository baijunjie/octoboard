import { useEffect, useMemo, useState } from "react";

import { listedAgents, servedAgents } from "../../agents/agents";
import { alertAppDataFailure } from "../../components/alerts/raiseError";
import { movedAgent } from "../../components/tabs/tabDrag";
import { useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { useDaemon, useProviders } from "../../store";
import { useOptimisticOrder } from "./useOptimisticOrder";

/** The agents the strip lists, the tabs' elements and the means to reorder the agents. The strip shows the order a
 * move or a drop has just asked for at once, not once the daemon's `settings_updated` has come back: until that
 * arrives, or the request is refused and the order falls back. The tab that was moved takes keyboard focus back
 * once the strip has followed the new order, since moving a focused element in the document drops its focus; call
 * this before anything that hands lost focus elsewhere (`useSectionRefocus`), so the tab wins. */
export function useAgentList(added: Agent[] | null): {
  agents: Agent[];
  /** The element of an agent's tab, once the strip has drawn it. */
  tabs: Partial<Record<Agent, HTMLElement>>;
  /** The ref callback of an agent's tab. */
  tabRef: (agent: Agent) => (element: HTMLElement | null) => void;
  /** Asks for the whole new order, `moved` being the agent that was put elsewhere. */
  reorder: (order: Agent[], moved: Agent) => void;
  /** Moves an agent a step earlier (-1) or later (1). */
  move: (agent: Agent, by: -1 | 1) => void;
} {
  const t = useT();
  const { request } = useDaemon();
  const providers = useProviders();
  const { order: agents, propose } = useOptimisticOrder(added, listedAgents(added, servedAgents(providers)));
  const [tabs, setTabs] = useState<Partial<Record<Agent, HTMLElement>>>({});
  const tabRefs = useMemo(() => Object.fromEntries(providers.map(({ id: agent }) => [
    agent,
    (element: HTMLElement | null) => setTabs((current) => {
      if ((current[agent] ?? null) === element) return current;
      const next = { ...current };
      if (element) next[agent] = element;
      else delete next[agent];
      return next;
    }),
  ])), [providers]);
  // The tab to give focus back to, until it has been given.
  const [moved, setMoved] = useState<Agent>();
  useEffect(() => {
    const tab = moved && tabs[moved];
    if (!moved || !tab) return;
    if (document.activeElement === document.body) tab.focus();
    setMoved(undefined);
  }, [moved, agents, tabs]);

  const reorder = (order: Agent[], agent: Agent) => {
    setMoved(agent);
    const revert = propose(order);
    void request({ type: "reorder_agents", agents: order }).catch((err) => {
      revert();
      setMoved(agent);
      alertAppDataFailure(t("settings.accounts.reorderAgentsFailedTitle"))(err);
    });
  };
  const move = (agent: Agent, by: -1 | 1) => reorder(movedAgent(agents, agent, agents.indexOf(agent) + by), agent);
  return { agents, tabs, tabRef: (agent) => tabRefs[agent], reorder, move };
}
