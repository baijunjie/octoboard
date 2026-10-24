import { ArrowLeft, ArrowRight, Trash2 } from "lucide-react";
import React, { useEffect, useMemo, useState } from "react";

import { AGENT_LABEL, listedAgents } from "../agents";
import { ActionMenu, type ActionMenuEntry } from "../components/ActionMenu";
import { InlineReference } from "../components/InlineReference";
import { movedAgent } from "../components/tabDrag";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { errorPresentation } from "../errorPresentation";
import type { Translate } from "../i18n/catalog";
import { joinNodes } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { Agent } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { useDaemon, useDaemonStoreEqual } from "../store";
import { agentUsage } from "./agentUsage";

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
  const { request, toastError } = useDaemon();
  // The order asked for, good for as long as the list it was asked against is the one held.
  const [pending, setPending] = useState<{ base: Agent[] | null; order: Agent[] }>();
  const agents = useMemo(() => (pending && pending.base === added ? pending.order : listedAgents(added)), [added, pending]);
  const [tabs, setTabs] = useState<Partial<Record<Agent, HTMLElement>>>({});
  const tabRefs = useMemo(() => Object.fromEntries(listedAgents(null).map((agent) => [
    agent,
    (element: HTMLElement | null) => setTabs((current) => {
      if ((current[agent] ?? null) === element) return current;
      const { [agent]: _, ...rest } = current;
      return element ? { ...rest, [agent]: element } : rest;
    }),
  ])) as Record<Agent, (element: HTMLElement | null) => void>, []);
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
    setPending({ base: added, order });
    void request({ type: "reorder_agents", agents: order }).catch((err) => {
      setPending(undefined);
      setMoved(agent);
      toastError(errorPresentation(err as Error));
    });
  };
  const move = (agent: Agent, by: -1 | 1) => reorder(movedAgent(agents, agent, agents.indexOf(agent) + by), agent);
  return { agents, tabs, tabRef: (agent) => tabRefs[agent], reorder, move };
}

/** What an agent's menu holds, in its tab's context menu and in the strip's more menu alike: the ways to move the
 * agent's tab a step along (the keyboard's counterpart to dragging it; "earlier" and "later" hold in either reading
 * direction, where "left" and "right" do not) and to remove the agent. A further action is one more entry here. */
function agentMenu(t: Translate, index: number, count: number, onMove: (by: -1 | 1) => void, onRemove: () => void): ActionMenuEntry[] {
  return [
    { label: t("settings.accounts.moveEarlier"), icon: ArrowLeft, iconClassName: "rtl:-scale-x-100", disabled: index <= 0, onClick: () => onMove(-1) },
    { label: t("settings.accounts.moveLater"), icon: ArrowRight, iconClassName: "rtl:-scale-x-100", disabled: index >= count - 1, onClick: () => onMove(1) },
    "separator",
    { label: t("settings.accounts.removeAgent"), icon: Trash2, destructive: true, onClick: onRemove },
  ];
}

/** An agent tab's menu, opened from the tab by a right-click or, from the keyboard, by the Menu key or Shift+F10 on the
 * focused tab. Each tab's `AgentTabActions` button holds the same entries for whoever does not find this one. */
export function AgentTabMenu({ agent, target, index, count, onMove, onRemove }: {
  agent: Agent;
  target: HTMLElement;
  index: number;
  count: number;
  onMove: (by: -1 | 1) => void;
  onRemove: () => void;
}): React.ReactElement {
  const t = useT();
  // A new object when the tab's element changes, which is what has the menu listen to the new element.
  const targetRef = useMemo(() => ({ current: target }), [target]);
  return (
    <ActionMenu
      contextOnly
      contextTargetRef={targetRef}
      label={t("settings.accounts.agentActions", { agent: AGENT_LABEL[agent] })}
      className="hidden"
      items={agentMenu(t, index, count, onMove, onRemove)}
    />
  );
}

/** An agent tab's own actions button, for the strip to place at the end of that tab (`AgentTabList`'s `tabActions`): the
 * same entries as the tab's context menu, for the pointer, the keyboard and a screen reader to find without knowing about
 * the right-click. It is named for its agent, as each account row's menu is for its account. */
export function AgentTabActions({ agent, index, count, onMove, onRemove }: {
  agent: Agent;
  index: number;
  count: number;
  onMove: (by: -1 | 1) => void;
  onRemove: () => void;
}): React.ReactElement {
  const t = useT();
  return <ActionMenu label={t("settings.accounts.agentActions", { agent: AGENT_LABEL[agent] })} items={agentMenu(t, index, count, onMove, onRemove)} />;
}

/** The confirmation of taking an agent off the added list. It says the agent's accounts are kept and, when consoles,
 * projects or sessions still use the agent, names them and says they keep their setting. Each line says how its
 * kind uses the agent: a console or project has it as its default agent, which is not the same as running sessions of
 * it there. */
export function RemoveAgentDialog({ agent, onRemoved, onCancel }: {
  agent: Agent;
  onRemoved: () => void;
  onCancel: () => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { request } = useDaemon();
  const usage = useDaemonStoreEqual((s) => agentUsage(agent, s.consoles, s.projects, s.sessions, (session) => sessionDisplayTitle(t, session)));
  // Names are listed while they are few enough to read in a line, else only counted.
  const phrase = (names: string[], count: string): React.ReactNode => (names.length > 0 && names.length <= 5
    ? <Message id="settings.accounts.removeAgentNamed" params={{ what: count, names: joinNodes(language, names, (name, key) => <InlineReference key={key} dir="auto" wrap>{name}</InlineReference>) }} />
    : count);
  const uses = [
    usage.consoles.length > 0 && phrase(usage.consoles, t("settings.accounts.removeAgentConsolesDefault", { count: usage.consoles.length })),
    usage.projects.length > 0 && phrase(usage.projects, t("settings.accounts.removeAgentProjectsDefault", { count: usage.projects.length })),
    usage.sessions.length > 0 && phrase(usage.sessions, t("settings.accounts.removeAgentSessionsUsing", { count: usage.sessions.length })),
  ].filter((use): use is React.ReactNode => use !== false);
  return (
    <ConfirmDialog
      title={<Message id="settings.accounts.removeAgentTitle" params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[agent]}</InlineReference> }} />}
      message={
        <>
          <p><Message id="settings.accounts.removeAgentMessage" params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[agent]}</InlineReference> }} /></p>
          {uses.length > 0 && (
            <>
              <p><Message id="settings.accounts.removeAgentInUse" params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[agent]}</InlineReference> }} /></p>
              <ul className="list-disc ps-5">
                {uses.map((use, at) => <li key={at}>{use}</li>)}
              </ul>
            </>
          )}
        </>
      }
      confirmLabel={t("common.remove")}
      destructive
      resetKey={agent}
      onConfirm={async () => {
        await request({ type: "remove_agent", agent });
        onRemoved();
      }}
      onCancel={onCancel}
    />
  );
}
