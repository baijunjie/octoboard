import { Bot, Gauge, KeyRound } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { noAgentState, pickableAgents } from "../agents/agents";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { Button, Tabs } from "../primitives";
import { AgentTabList } from "../components/tabs/AgentTabList";
import { PATH_VAR_PARAMS } from "../components/text/literalParams";
import { Message, useT } from "../i18n/react";
import type { Agent } from "../protocol";
import { listedQuotaAccounts } from "./quotaListing";
import { useDaemonStore, useProviders } from "../store";
import { AgentPanel } from "./AgentPanel";
import type { ResetRequest } from "./QuotaAccount";

/** The panel's contents: one tab per agent that is added and installed and has an account to list, in the order they were
 * added, each listing the agent's accounts that are not turned off (bar the selected running session's own, `inUse`), and
 * the way into Settings' accounts section. The panel mounts afresh on each opening, so the tab starts on `shownAgent`
 * (the first tab if it is not listed or none is given) and the user's own choice lasts only until it closes. */
export function QuotaPanel({ shownAgent, shownAccount, inUse, focusSelectedTab, emphasiseShown, onMouseEnter, onOpenAccounts, onAddAgent, onUseReset }: { shownAgent?: Agent; shownAccount: string | null; inUse?: { agent: Agent; account: string | null }; focusSelectedTab: boolean; emphasiseShown: boolean; onMouseEnter: () => void; onOpenAccounts: () => void; onAddAgent: () => void; onUseReset: ResetRequest }): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const availability = useDaemonStore((s) => s.agentAvailability);
  const accountOrder = useDaemonStore((s) => s.settings.account_order);
  const [chosen, setChosen] = useState<Agent | undefined>(shownAgent);
  // The tab elements exist only after the tabs have built their collection, so focus is given as
  // the selected one mounts, once.
  const focused = useRef(false);
  const disabledDefaults = useDaemonStore((s) => s.settings.disabled_default_accounts);
  // An agent whose accounts are all turned off has no tab, as it has no submenu in the menu bar, unless the selected running session is on one.
  const listed = listedQuotaAccounts(pickableAgents(availability, addedAgents, providers), { accounts, accountOrder, disabledDefaultAccounts: disabledDefaults }, inUse);
  const selected = listed.find(({ agent }) => agent === chosen)?.agent ?? listed[0]?.agent;
  const focusOnce = (tab: HTMLElement | null) => {
    if (!tab || focused.current) return;
    focused.current = true;
    tab.focus();
  };
  const root = useRef<HTMLDivElement>(null);
  // With no tab to take it, a keyboard pin's focus goes to the first button of the panel: the empty state's Add agent
  // or Manage accounts, or else the link to Settings' accounts.
  useEffect(() => {
    if (focusSelectedTab && selected === undefined) root.current?.querySelector("button")?.focus();
  }, []);
  // A tab that exists only for the selected running session's turned-off account goes when the session stops, taking
  // focus with it: the handoff puts it on the tab now selected, or on the panel's first button when there is none.
  const handoff = useFocusHandoff(() => {
    (root.current?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]') ?? root.current?.querySelector<HTMLElement>("button"))?.focus();
  });
  const noAgents = noAgentState(availability, addedAgents, providers);
  return (
    <div ref={root} className="flex flex-col" onFocus={handoff.onFocus} onBlur={handoff.onBlur} onPointerEnter={(event) => event.pointerType === "mouse" && onMouseEnter()}>
      {selected === undefined ? (
        <div className="p-4">
          {noAgents === "add" ? (
            <EmptyPanel icon={Bot} message={t("rail.quota.none")} description={t("rail.quota.noneHint")} action={{ label: t("agents.addAction"), onPress: onAddAgent }} />
          ) : noAgents === undefined ? (
            <EmptyPanel icon={Gauge} message={t("rail.quota.noEnabled")} description={t("rail.quota.noEnabledHint")} action={{ label: t("rail.quota.manage"), onPress: onOpenAccounts }} />
          ) : (
            <EmptyPanel icon={Gauge} message={t("rail.quota.noneInstalled")} description={<Message id="rail.quota.noneInstalledHint" params={PATH_VAR_PARAMS} />} />
          )}
        </div>
      ) : (
        <Tabs variant="underline" selectedKey={selected} onSelectionChange={(key) => setChosen(key as Agent)}>
          <AgentTabList
            providers={providers}
            agents={listed.map(({ agent }) => agent)}
            label={t("rail.quota.agents")}
            tabRef={(agent) => (focusSelectedTab && agent === selected ? focusOnce : undefined)}
            spanning
            className="groove-b"
          />
          {/* The panel is anchored by its bottom edge to the rail's button and closes once the pointer
              is off it, so a tab with fewer accounts must not make it shorter: its top edge, and the
              tab strip with it, would drop out from under the pointer that just chose the tab. Every
              tab's panel therefore stands in this one grid cell, which is as tall as the tallest list
              the scroll limit allows. Each account row's own padding is the list's only space at its
              ends, so the first and last rows sit as far from the tab strip and the foot as rows do
              from each other's dividers. */}
          <div className="grid grid-cols-1">
            {listed.map(({ agent, rows }) => (
              <AgentPanel key={agent} agent={agent} rows={rows} currentAccount={agent === shownAgent ? shownAccount : undefined} dimOthers={emphasiseShown} onUseReset={onUseReset} />
            ))}
          </div>
        </Tabs>
      )}
      {/* The empty states that ask to add an agent or to manage the accounts have that button already; this link would say the same. */}
      {!(selected === undefined && (noAgents === "add" || noAgents === undefined)) && (
        <div className="groove-t flex justify-end p-2">
          <Button size="sm" variant="ghost" className="text-app-muted hover:text-foreground ui-keyboard-focus:text-foreground" focusOnPress={false} onPress={onOpenAccounts}>
            <KeyRound aria-hidden="true" className="size-4 shrink-0" />
            {t("rail.quota.manage")}
          </Button>
        </div>
      )}
    </div>
  );
}
