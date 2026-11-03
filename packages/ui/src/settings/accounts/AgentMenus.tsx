import { ArrowLeft, ArrowRight, Trash2 } from "lucide-react";
import React, { useMemo } from "react";

import { AGENT_LABEL } from "../../agents/agents";
import { ActionMenu } from "../../components/menus/ActionMenu";
import type { ActionMenuEntry } from "../../components/menus/menuEntries";
import type { Translate } from "../../i18n/catalog";
import { useT } from "../../i18n/react";
import type { Agent } from "../../protocol";

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
