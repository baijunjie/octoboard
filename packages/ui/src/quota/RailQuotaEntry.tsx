import { Button, Popover, Tabs } from "@heroui/react";
import { Gauge } from "lucide-react";
import React, { useEffect, useId, useRef, useState } from "react";
import { useFocusVisible, type PressEvent } from "react-aria";

import { accountsByAgent, defaultAccountName } from "../agents";
import { AgentTabList } from "../components/AgentTabList";
import { ChromeButton } from "../components/ChromeButton";
import { FadeOverflow } from "../components/FadeOverflow";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { EmptyPanel } from "../components/EmptyPanel";
import { FadedScroller, useFadeRegion } from "../components/scrollFade";
import { useStableHandler } from "../components/useLatest";
import { Message, useT } from "../i18n/react";
import type { Account, Agent } from "../protocol";
import type { SettingsSectionId } from "../settings/SettingsDialog";
import { useDaemon, useDaemonStore } from "../store";
import { AccountNameLine, AccountQuotaSummary } from "./AccountQuotaSummary";
import { MODAL_OPEN } from "../layout/useRegionCycle";
import { useAccountQuota } from "./useAccountQuota";
import { remainingPercent } from "./quotaFormat";
import { accountQuotaLabel, sessionQuotaWindowOf } from "./sessionQuota";

/** How long the pointer rests on the button before the panel floats in. It is shorter than the
 * tooltip's wait, so the panel is what a resting pointer meets and the tooltip is held back. */
const HOVER_OPEN_DELAY_MS = 250;
/** How long the pointer may be off both the button and the panel before the panel goes, so that
 * crossing the gap between them does not close it. */
const HOVER_CLOSE_DELAY_MS = 150;

/** Whether `target` is inside either of `elements`. */
function insideAny(target: EventTarget | null, ...elements: Array<Element | null>): boolean {
  return target instanceof Node && elements.some((element) => element?.contains(target));
}

/** The rail's quota button and the floating panel it anchors, which lists the accounts of each
 * available agent with their plan and quota (see "The account quota panel on the rail" in
 * `docs/product/account-quotas.md`).
 *
 * The panel has two modes. Resting the mouse on the button, or on the panel, shows it and leaving
 * both hides it; pressing the button, with the mouse or the keyboard, pins it open until Escape, a
 * press outside, or another press of the button. The panel is not modal and the document's presses
 * and keys are watched here, since react-aria's non-modal popover dismisses on neither. Focus goes
 * back on closing as react-aria's overlay does it: to the element that held it when the panel
 * opened, which is the button for a keyboard press and the terminal for a mouse one. Each opening
 * asks the daemon to refresh the quotas that are due. */
export function RailQuotaEntry({ selectedSessionId, onOpenSettingsAt }: {
  selectedSessionId?: string;
  onOpenSettingsAt: (section: SettingsSectionId) => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const session = useDaemonStore((s) => s.sessions.get(selectedSessionId ?? ""));
  const window = useDaemonStore((s) => sessionQuotaWindowOf(session, {
    accounts: s.settings.accounts, agentAvailability: s.agentAvailability, accountQuotas: s.accountQuotas,
  }));
  const account = useDaemonStore((s) => s.settings.accounts.find((account) => account.id === session?.account_id));
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const [hovered, setHovered] = useState(false);
  const [pinned, setPinned] = useState(false);
  const [keyboardPinned, setKeyboardPinned] = useState(false);
  // Set when the panel closes over a button the pointer or focus is on, and cleared when they leave,
  // so that the tooltip does not pop up on a button the panel has just closed over.
  const [justClosed, setJustClosed] = useState(false);
  const [onButton, setOnButton] = useState(false);
  const { isFocusVisible } = useFocusVisible();
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const panelId = useId();
  const openTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const closeTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const label = session && window ? accountQuotaLabel(t, session.agent, account?.name, window, defaultNames) : t("rail.quota");

  const clearTimers = () => {
    clearTimeout(openTimer.current);
    clearTimeout(closeTimer.current);
  };
  // Stable, since the document listeners below are subscribed once per opening.
  const close = useStableHandler((holdTooltip = true) => {
    clearTimers();
    setHovered(false);
    setPinned(false);
    setKeyboardPinned(false);
    // With `holdTooltip`, only where the tooltip would reopen at once: the pointer is on the
    // button, or focus is on it or about to be returned to it from the panel (a keyboard pin).
    const focus = document.activeElement;
    if (holdTooltip && (onButton || focus === button.current || (keyboardPinned && insideAny(focus, panel.current)))) setJustClosed(true);
  });
  const open = hovered || pinned;
  useEffect(() => clearTimers, []);

  // The quotas already held are shown whether or not this succeeds, and the daemon's own poller
  // keeps them current, so a failed request has nothing to report.
  useEffect(() => {
    if (open) void request({ type: "refresh_account_quotas" }).catch(() => {});
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (insideAny(event.target, button.current, panel.current)) return;
      // The press moves focus to what it lands on, so none comes back to the button to hold the
      // tooltip for. A target that takes no focus on a press (`preventFocusOnPress`) gets focus
      // returned to the button under pointer modality, where its tooltip does not open either.
      close(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      // The key is this panel's: it neither reaches the agent in the terminal nor closes a drawer
      // under it (which leaves Escape alone while the panel is marked `data-escape-topmost`).
      event.preventDefault();
      event.stopPropagation();
      close();
    };
    // A modal opened by something other than a press here (Settings from the native menu) covers
    // the panel, which would otherwise stay up under it and take the modal's Escape. Modals and
    // menus portal in as direct children of the body, so only those are watched; this panel is
    // `aria-modal="false"` and does not match.
    const modalWatch = new MutationObserver(() => {
      if (document.querySelector(MODAL_OPEN)) close();
    });
    modalWatch.observe(document.body, { childList: true });
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown, true);
    return () => {
      modalWatch.disconnect();
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown, true);
    };
  }, [open]);

  const onHoverChange = (isHovered: boolean) => {
    setOnButton(isHovered);
    if (!isHovered) setJustClosed(false);
    clearTimeout(openTimer.current);
    clearTimeout(closeTimer.current);
    if (isHovered) {
      if (!open) openTimer.current = setTimeout(() => setHovered(true), HOVER_OPEN_DELAY_MS);
    } else if (!pinned) {
      closeTimer.current = setTimeout(() => setHovered(false), HOVER_CLOSE_DELAY_MS);
    }
  };

  const onPress = (event: PressEvent) => {
    if (pinned) {
      close();
      return;
    }
    clearTimers();
    setJustClosed(false);
    setKeyboardPinned(event.pointerType === "keyboard" || event.pointerType === "virtual");
    setPinned(true);
  };

  return (
    // The popover root only supplies the panel's styling and the trigger's popup attributes: the
    // state is held here, since the trigger's own press would toggle it where this pins instead.
    <Popover isOpen={open} onOpenChange={() => {}}>
      <ChromeButton
        label={label}
        className="relative"
        buttonRef={button}
        expanded={open}
        tooltipDisabled={open || (justClosed && (onButton || isFocusVisible))}
        popupId={panelId}
        onBlur={() => setJustClosed(false)}
        onPress={onPress}
        onMouseHoverChange={onHoverChange}
        tooltipPlacement="end"
      >
        <Gauge aria-hidden="true" className={session && window ? "size-4 -translate-y-0.5" : "size-4"} />
        {session && window && (
          <span
            aria-hidden="true"
            className="absolute inset-x-[7px] bottom-1.5 h-[5px] overflow-hidden rounded-full border border-current"
          >
            <span className="block h-full rounded-full bg-current" style={{ width: `${remainingPercent(window)}%` }} />
          </span>
        )}
      </ChromeButton>
      <Popover.Content
        isOpen={open}
        isNonModal
        triggerRef={button}
        placement="end bottom"
        offset={8}
        onOpenChange={(isOpen) => {
          if (!isOpen) close();
        }}
        className="w-96 max-w-[calc(100vw-4rem)]"
      >
        {/* Not `Popover.Dialog`: react-aria's dialog takes focus when it mounts, which a panel the
            pointer merely rests on must not, since focus would leave the terminal. This one is
            given focus only by a keyboard pin (`focusSelectedTab`). */}
        <div
          ref={panel}
          id={panelId}
          role="dialog"
          aria-label={t("rail.quota")}
          // Not modal: the window's shortcuts, F6 and floating panes go on working beside it.
          aria-modal="false"
          // Only while open, so that Escape during the exit animation reaches the drawer as usual.
          data-escape-topmost={open ? "" : undefined}
          onPointerEnter={(event) => {
            if (event.pointerType !== "mouse") return;
            clearTimeout(closeTimer.current);
          }}
          onPointerLeave={(event) => {
            if (event.pointerType !== "mouse" || pinned) return;
            closeTimer.current = setTimeout(() => setHovered(false), HOVER_CLOSE_DELAY_MS);
          }}
        >
          <QuotaPanel
            // A keyboard user pins a panel that floats outside the rail's tab order, so focus is
            // brought to it; a pointer's pin leaves focus where it was.
            focusSelectedTab={keyboardPinned}
            onOpenAccounts={() => {
              close();
              onOpenSettingsAt("accounts");
            }}
          />
        </div>
      </Popover.Content>
    </Popover>
  );
}

/** The panel's contents: one tab per available agent, each listing the agent's accounts, and the
 * way into Settings' accounts section. */
function QuotaPanel({ focusSelectedTab, onOpenAccounts }: { focusSelectedTab: boolean; onOpenAccounts: () => void }): React.ReactElement {
  const t = useT();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const availability = useDaemonStore((s) => s.agentAvailability);
  const [chosen, setChosen] = useState<Agent>();
  // The tab elements exist only after the tabs have built their collection, so focus is given as
  // the selected one mounts, once.
  const focused = useRef(false);
  const available = accountsByAgent(accounts).filter(({ agent }) => availability.get(agent)?.availability === "available");
  const selected = available.find(({ agent }) => agent === chosen)?.agent ?? available[0]?.agent;
  const focusOnce = (tab: HTMLElement | null) => {
    if (!tab || focused.current) return;
    focused.current = true;
    tab.focus();
  };
  return (
    <div className="flex flex-col">
      {selected === undefined ? (
        <div className="p-4">
          <EmptyPanel icon={Gauge} message={t("rail.quota.none")} description={<Message id="rail.quota.noneHint" params={PATH_VAR_PARAMS} />} />
        </div>
      ) : (
        <Tabs variant="secondary" selectedKey={selected} onSelectionChange={(key) => setChosen(key as Agent)}>
          <AgentTabList
            agents={available.map(({ agent }) => agent)}
            label={t("rail.quota.agents")}
            tabRef={(agent) => (focusSelectedTab && agent === selected ? focusOnce : undefined)}
            className="groove-b"
          />
          {/* The panel is anchored by its bottom edge to the rail's button and closes once the pointer
              is off it, so a tab with fewer accounts must not make it shorter: its top edge, and the
              tab strip with it, would drop out from under the pointer that just chose the tab. Every
              tab's panel therefore stands in this one grid cell, which is as tall as the tallest list
              the scroll limit allows, and the cell carries the padding the panels go without. */}
          <div className="grid grid-cols-1 py-2">
            {available.map(({ agent, accounts: owned }) => (
              <AgentPanel key={agent} agent={agent} owned={owned} />
            ))}
          </div>
        </Tabs>
      )}
      <div className="groove-t flex justify-end p-2">
        <Button size="sm" variant="ghost" className="text-app-muted hover:text-foreground data-[focus-visible=true]:text-foreground" preventFocusOnPress onPress={onOpenAccounts}>
          {t("rail.quota.manage")}
        </Button>
      </div>
    </div>
  );
}

/** An agent's tab panel: its accounts, the default one first, in a faded scroller that is a region of
 * its own while it overflows. Each panel keeps its own region, since the scroller reports its overflow
 * only when it changes, so one shared by the panels would follow whichever panel last reported, not the
 * selected one.
 *
 * The region is held by the component that renders the panel, not by one inside it. react-aria makes a
 * panel with nothing tabbable inside it a tab stop of its own, and checks that when the panel renders
 * or something is added inside it, never when an attribute such as `tabindex` changes: the region's
 * `tabindex` arriving with the overflow report goes unnoticed unless the panel renders with it. Left a stop in front of the region, the panel takes Tab from the tab strip and then,
 * rendering on that focus, finds the region and gives up its `tabindex`, which drops focus to the body
 * in WebKit. */
function AgentPanel({ agent, owned }: { agent: Agent; owned: Account[] }): React.ReactElement {
  const t = useT();
  const defaultName = defaultAccountName(t, useDaemonStore((s) => s.settings.default_account_names), agent);
  const region = useFadeRegion({ label: t("rail.quota.accounts") });
  return (
    <Tabs.Panel
      id={agent}
      // Kept mounted while unselected, in the grid cell every panel shares, and invisible (react-aria
      // also makes it inert, out of the tab order and of assistive technology), so the cell keeps the
      // height of the tallest list.
      shouldForceMount
      // No padding or margin of its own, which would make this panel's height differ from the others',
      // and not the library's exit placement out of the flow, which would drop the cell's height.
      className="col-start-1 row-start-1 m-0 self-start p-0 data-[exiting=true]:static data-[inert=true]:invisible"
    >
      <div className={region.focused ? "ring-2 ring-inset ring-app-focus" : ""}>
        <FadedScroller region={region} className="max-h-[min(24rem,60vh)]">
          <div className="flex flex-col px-4">
            <QuotaAccount agent={agent} account={null} name={defaultName} />
            {owned.map((account) => (
              <QuotaAccount
                key={account.id}
                agent={agent}
                account={account.id}
                name={<FadeOverflow dir="auto" titleWhenClipped={account.name}>{account.name}</FadeOverflow>}
              />
            ))}
          </div>
        </FadedScroller>
      </div>
    </Tabs.Panel>
  );
}

/** One account of the panel: its name and plan, and its quota beneath. */
function QuotaAccount({ agent, account, name }: { agent: Agent; account: string | null; name: React.ReactNode }): React.ReactElement {
  const quota = useAccountQuota(agent, account);
  return (
    <div className="flex flex-col gap-2 groove-b py-3 last:border-b-0">
      <AccountNameLine name={name} quota={quota} />
      <AccountQuotaSummary quota={quota} single />
    </div>
  );
}
