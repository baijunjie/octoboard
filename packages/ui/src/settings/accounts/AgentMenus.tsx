import { ArrowLeft, ArrowRight, CircleArrowUp, Settings2, Trash2 } from "lucide-react";
import React, { useMemo } from "react";

import { providerLabel } from "../../agents/providers";
import { ActionMenu } from "../../components/menus/ActionMenu";
import type { ActionMenuEntry } from "../../components/menus/menuEntries";
import type { Translate } from "../../i18n/catalog";
import { isolate } from "../../i18n/isolate";
import { textDirection } from "../../i18n/languages";
import { useCurrentLanguage, useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { Badge } from "../../primitives";
import { useProviders } from "../../store";

/** The Version entry of an agent's menu and its badge, for an agent whose CLI is installed (`undefined` for one that is
 * not, which has no entry): `update` is whether an update of the CLI is waiting (`hasCliUpdate`). */
export interface AgentVersionEntry {
  update: boolean;
  /** The installed version as the daemon read it, shown at the entry's end; `null` while unknown. */
  installed: string | null;
}

/** What an agent's menu needs to build its entries: where its tab stands in the strip, the Version entry and what each
 * entry does. */
interface AgentMenuActions {
  index: number;
  count: number;
  version: AgentVersionEntry | undefined;
  onSettings: () => void;
  onVersion: () => void;
  onMove: (by: -1 | 1) => void;
  onRemove: () => void;
}

/** What an agent's menu holds, in its tab's context menu and in the strip's more menu alike: first the Settings entry,
 * which opens the agent's settings in a modal, as an account's menu opens its settings, and the Version entry (none for
 * an agent that is not installed), which opens the agent's CLI version and its update in a modal of its own and shows
 * the installed version at its end, after a dot while an update is waiting, which its description puts in words; then
 * the ways to move the agent's tab a step along (the keyboard's counterpart to dragging it; "earlier" and "later" hold
 * in either reading direction, where "left" and "right" do not) and to remove the agent. A further action is one more
 * entry here. */
function agentMenu(t: Translate, { index, count, version, onSettings, onVersion, onMove, onRemove }: AgentMenuActions): ActionMenuEntry[] {
  return [
    { label: t("settings.accounts.settingsItem"), icon: Settings2, onClick: onSettings },
    ...(version ? [{
      label: t("settings.accounts.cli.label"),
      icon: CircleArrowUp,
      onClick: onVersion,
      ...(version.installed !== null ? {
        end: (
          <span aria-hidden="true" className="flex items-center gap-1.5 text-xs tabular-nums text-app-muted">
            {version.update && <span className="size-1.5 rounded-full bg-accent-soft-foreground" />}
            <span dir="ltr">{version.installed}</span>
          </span>
        ),
        description: version.update ? t("settings.accounts.versionItemUpdate", { version: isolate(version.installed) }) : version.installed,
      } : {}),
    }] : []),
    "separator",
    { label: t("settings.accounts.moveEarlier"), icon: ArrowLeft, iconClassName: "rtl:-scale-x-100", disabled: index <= 0, onClick: () => onMove(-1) },
    { label: t("settings.accounts.moveLater"), icon: ArrowRight, iconClassName: "rtl:-scale-x-100", disabled: index >= count - 1, onClick: () => onMove(1) },
    "separator",
    { label: t("settings.accounts.removeAgent"), icon: Trash2, destructive: true, onClick: onRemove },
  ];
}

/** An agent tab's menu, opened from the tab by a right-click or, from the keyboard, by the Menu key or Shift+F10 on the
 * focused tab. Each tab's `AgentTabActions` button holds the same entries for whoever does not find this one. */
export function AgentTabMenu({ agent, target, ...actions }: AgentMenuActions & { agent: Agent; target: HTMLElement }): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  // A new object when the tab's element changes, which is what has the menu listen to the new element.
  const targetRef = useMemo(() => ({ current: target }), [target]);
  return (
    <ActionMenu
      contextOnly
      contextTargetRef={targetRef}
      label={t("settings.accounts.agentActions", { agent: isolate(providerLabel(t, providers, agent)) })}
      className="hidden"
      items={agentMenu(t, actions)}
    />
  );
}

/** An agent tab's own actions button, for the strip to place at the end of that tab (`AgentTabList`'s `tabActions`): the
 * same entries as the tab's context menu, for the pointer, the keyboard and a screen reader to find without knowing about
 * the right-click. It is named for its agent, as each account row's menu is for its account, and, while an update of the
 * agent's CLI is waiting, says so in its name and tooltip and wears a badge beside it: the tab itself carries no such
 * mark. */
export function AgentTabActions({ agent, ...actions }: AgentMenuActions & { agent: Agent }): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  const rtl = textDirection(useCurrentLanguage()) === "rtl";
  const name = isolate(providerLabel(t, providers, agent));
  const update = actions.version?.update;
  return (
    <ActionMenu
      label={t(update ? "settings.accounts.agentActionsUpdate" : "settings.accounts.agentActions", { agent: name })}
      tooltip={update ? t("settings.accounts.moreActionsUpdate") : undefined}
      badge={update ? <Badge color="accent" placement={rtl ? "top-left" : "top-right"} /> : undefined}
      items={agentMenu(t, actions)}
    />
  );
}
