import { Popover, Tabs } from "@heroui/react";
import { Bot, Gauge, KeyRound } from "lucide-react";
import React, { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { type PressEvent } from "react-aria";

import { Button } from "../components/Button";
import { accountsByAgent, defaultAccountName, noAgentState, pickableAgents } from "../agents";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { AgentTabList } from "../components/AgentTabList";
import { ChromeButton } from "../components/ChromeButton";
import { FadeOverflow } from "../components/FadeOverflow";
import { EmptyPanel } from "../components/EmptyPanel";
import { FADE_SIZE, FadedScroller, useFadeRegion } from "../components/scrollFade";
import { useStableHandler } from "../components/useLatest";
import { Message, useT } from "../i18n/react";
import type { Account, Agent, QuotaWindow } from "../protocol";
import type { SettingsSectionId } from "../settings/SettingsDialog";
import { useDaemon, useDaemonStore } from "../store";
import { useNow } from "../useNow";
import { AccountNameLine, AccountQuotaSummary } from "./AccountQuotaSummary";
import { MODAL_OPEN } from "../layout/useRegionCycle";
import { useAccountQuota } from "./useAccountQuota";
import { exhaustedQuotaWindow, remainingPercentAt, type QuotaBar } from "./quotaFormat";
import { accountQuotaLabel, sessionQuotaBarOf } from "./sessionQuota";

/** How long the pointer rests on the button before the panel floats in. */
const HOVER_OPEN_DELAY_MS = 250;
/** How long the pointer may be off both the button and the panel before the panel goes, so that
 * crossing the gap between them does not close it. */
const HOVER_CLOSE_DELAY_MS = 150;

/** The popover of a `FailureMark` in the panel, which is portalled out of it but belongs to it. */
const NESTED_POPOVER = "[data-failure-mark-popover]";

/** Whether `target` is inside either of `elements`. */
function insideAny(target: EventTarget | null, ...elements: Array<Element | null>): boolean {
  return target instanceof Node && elements.some((element) => element?.contains(target));
}

/** The longest the clock goes unread, so a reset that passes while the system sleeps (when timers may
 * pause) is noticed within a minute of waking. */
const MAX_CLOCK_WAIT_MS = 60_000;

/** The time the button's bars are drawn as of: re-read when a reset time among the windows they draw
 * passes, which is when their fill goes full (and at least once a minute). Not `useNow`, whose ticker
 * would run for as long as the rail is mounted, quota or not. */
function useResetClock(bar: QuotaBar | undefined): number {
  const [now, setNow] = useState(() => Date.now());
  // The resets rather than `bar`, which is a new object whenever the session or the quotas update.
  const mainReset = bar?.main.resets_at;
  const thinReset = bar?.thin?.resets_at;
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const arm = () => {
      const at = Date.now();
      setNow(at);
      const ahead = [mainReset, thinReset].filter((reset): reset is number => reset != null && reset > at);
      if (ahead.length > 0) timer = setTimeout(arm, Math.min(Math.min(...ahead) - at, MAX_CLOCK_WAIT_MS));
    };
    arm();
    return () => clearTimeout(timer);
  }, [mainReset, thinReset]);
  return now;
}

/** The rail button's compact indicator under its gauge: an outlined capsule 18 × 5 px filled by what
 * remains of the main window. With a weekly window too, the main capsule is 4 px tall and one 3 px tall
 * sits a pixel beneath it for that; the gauge rises 2 px further and the pair's bottom sits a pixel
 * lower, so the group stays near the button's middle. Decorative; the button's name states the numbers.
 * They draw in the button's text colour, so they suit a light and a dark rail. */
function RailQuotaBars({ bar, now }: { bar: QuotaBar; now: number }): React.ReactElement {
  if (!bar.thin) {
    return (
      <span aria-hidden="true" className="absolute inset-x-[7px] bottom-1.5">
        <QuotaCapsule window={bar.main} height={5} now={now} />
      </span>
    );
  }
  return (
    <span aria-hidden="true" className="absolute inset-x-[7px] bottom-[5px] flex flex-col gap-px">
      <QuotaCapsule window={bar.main} height={4} now={now} />
      <QuotaCapsule window={bar.thin} height={3} now={now} />
    </span>
  );
}

/** The width every capsule spans, in pixels: the 32 px button less 7 on each side. Centred in its
 * row, so it stays centred where the button is wider. */
const CAPSULE_WIDTH = 18;

/** One outlined capsule, `height` pixels tall, with fully round ends, filling from the reading
 * direction's start. It is drawn as a vector, a square-ended fill clipped to the capsule and the
 * outline stroked over it: an HTML border beside a filled box leaves a pale antialiased seam between
 * them wherever the button sits at a fractional offset, as it does while scaled down on a press. */
function QuotaCapsule({ window, height, now }: { window: QuotaWindow; height: number; now: number }): React.ReactElement {
  const radius = height / 2;
  const clipId = useId();
  const fill = (CAPSULE_WIDTH * remainingPercentAt(window, now)) / 100;
  return (
    <svg data-slot="quota-capsule" viewBox={`0 0 ${CAPSULE_WIDTH} ${height}`} className="mx-auto my-0 block rtl:-scale-x-100" style={{ width: CAPSULE_WIDTH, height }}>
      <clipPath id={clipId}>
        <rect width={CAPSULE_WIDTH} height={height} rx={radius} />
      </clipPath>
      {fill > 0 && <rect width={fill} height={height} clipPath={`url(#${clipId})`} fill="currentColor" />}
      <rect x={0.5} y={0.5} width={CAPSULE_WIDTH - 1} height={height - 1} rx={radius - 0.5} fill="none" stroke="currentColor" />
    </svg>
  );
}

/** The rail's quota button and the floating panel it anchors, which lists the accounts of each
 * agent that is added and installed, in the order they were added, with their plan and quota (see "The account quota panel on the rail" in
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
  onOpenSettingsAt: (section: SettingsSectionId, options?: { addAgent?: boolean }) => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const session = useDaemonStore((s) => s.sessions.get(selectedSessionId ?? ""));
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const accountQuotas = useDaemonStore((s) => s.accountQuotas);
  // Memoized rather than computed in a store selector, since its result is a new object on each call.
  const bar = useMemo(() => sessionQuotaBarOf(session, { accounts, agentAvailability, accountQuotas }), [session, accounts, agentAvailability, accountQuotas]);
  const account = accounts.find((account) => account.id === session?.account_id);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const now = useResetClock(bar);
  const [hovered, setHovered] = useState(false);
  const [pinned, setPinned] = useState(false);
  const [keyboardPinned, setKeyboardPinned] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const panelId = useId();
  const openTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const closeTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const label = session && bar ? accountQuotaLabel(t, session.agent, account?.name, bar, defaultNames, now) : t("rail.quota");

  const clearTimers = () => {
    clearTimeout(openTimer.current);
    clearTimeout(closeTimer.current);
  };
  // Stable, since the document listeners below are subscribed once per opening.
  const close = useStableHandler(() => {
    clearTimers();
    setHovered(false);
    setPinned(false);
    setKeyboardPinned(false);
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
      if (insideAny(event.target, button.current, panel.current) || (event.target instanceof Element && event.target.closest(NESTED_POPOVER))) return;
      close();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      // Escape in a failure mark's popover closes that one, and the panel stays.
      if (event.key !== "Escape" || (event.target instanceof Element && event.target.closest(NESTED_POPOVER))) return;
      // The key is this panel's: it neither reaches the agent in the terminal nor closes a drawer
      // under it (which leaves Escape alone while the panel is marked `data-escape-topmost`).
      event.preventDefault();
      event.stopPropagation();
      close();
    };
    // A modal opened by something other than a press here (Settings from the native menu) covers
    // the panel, which would otherwise stay up under it and take the modal's Escape. Modals and
    // menus portal in as direct children of the body, so only those are watched; this panel is
    // `aria-modal="false"` and does not match, and neither does a failure mark's popover opened from
    // inside it, whose portal would otherwise read as such a modal and close the panel it came from.
    const modalWatch = new MutationObserver(() => {
      const covering = Array.from(document.querySelectorAll(MODAL_OPEN)).some((modal) => !modal.closest(NESTED_POPOVER));
      if (covering) close();
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
        noTooltip
        popupId={panelId}
        onPress={onPress}
        onMouseHoverChange={onHoverChange}
      >
        <Gauge aria-hidden="true" className={!session || !bar ? "size-4" : bar.thin ? "size-4 -translate-y-1" : "size-4 -translate-y-0.5"} />
        {session && bar && <RailQuotaBars bar={bar} now={now} />}
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
            // The tab it opens on: the agent whose quota the button shows, if any.
            shownAgent={session && bar ? session.agent : undefined}
            // The account the panel scrolls to on that tab: the selected session's, the default account when the session names none.
            shownAccount={session?.account_id ?? null}
            // A keyboard user pins a panel that floats outside the rail's tab order, so focus is
            // brought to it; a pointer's pin leaves focus where it was.
            focusSelectedTab={keyboardPinned}
            onOpenAccounts={() => {
              close();
              onOpenSettingsAt("accounts");
            }}
            onAddAgent={() => {
              close();
              onOpenSettingsAt("accounts", { addAgent: true });
            }}
          />
        </div>
      </Popover.Content>
    </Popover>
  );
}

/** The panel's contents: one tab per agent that is added and installed, in the order they were added, each listing the agent's accounts, and the
 * way into Settings' accounts section. The panel mounts afresh on each opening, so the tab starts on `shownAgent`
 * (the first tab if it is not listed or none is given) and the user's own choice lasts only until it closes. */
function QuotaPanel({ shownAgent, shownAccount, focusSelectedTab, onOpenAccounts, onAddAgent }: { shownAgent?: Agent; shownAccount: string | null; focusSelectedTab: boolean; onOpenAccounts: () => void; onAddAgent: () => void }): React.ReactElement {
  const t = useT();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const availability = useDaemonStore((s) => s.agentAvailability);
  const [chosen, setChosen] = useState<Agent | undefined>(shownAgent);
  // The tab elements exist only after the tabs have built their collection, so focus is given as
  // the selected one mounts, once.
  const focused = useRef(false);
  const listed = accountsByAgent(accounts, pickableAgents(availability, addedAgents));
  const selected = listed.find(({ agent }) => agent === chosen)?.agent ?? listed[0]?.agent;
  const focusOnce = (tab: HTMLElement | null) => {
    if (!tab || focused.current) return;
    focused.current = true;
    tab.focus();
  };
  const root = useRef<HTMLDivElement>(null);
  // With no tab to take it, a keyboard pin's focus goes to the first button of the panel: Add agent, or the
  // link to Settings' accounts.
  useEffect(() => {
    if (focusSelectedTab && selected === undefined) root.current?.querySelector("button")?.focus();
  }, []);
  const noAgents = noAgentState(availability, addedAgents);
  return (
    <div ref={root} className="flex flex-col">
      {selected === undefined ? (
        <div className="p-4">
          {noAgents === "add" ? (
            <EmptyPanel icon={Bot} message={t("rail.quota.none")} description={t("rail.quota.noneHint")} action={{ label: t("agents.addAction"), onPress: onAddAgent }} />
          ) : (
            <EmptyPanel icon={Gauge} message={t("rail.quota.noneInstalled")} description={<Message id="rail.quota.noneInstalledHint" params={PATH_VAR_PARAMS} />} />
          )}
        </div>
      ) : (
        <Tabs variant="secondary" selectedKey={selected} onSelectionChange={(key) => setChosen(key as Agent)}>
          <AgentTabList
            agents={listed.map(({ agent }) => agent)}
            label={t("rail.quota.agents")}
            tabRef={(agent) => (focusSelectedTab && agent === selected ? focusOnce : undefined)}
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
            {listed.map(({ agent, accounts: owned }) => (
              <AgentPanel key={agent} agent={agent} owned={owned} currentAccount={agent === shownAgent ? shownAccount : undefined} />
            ))}
          </div>
        </Tabs>
      )}
      {/* The empty state that asks to add an agent has that button already; this link would say the same. */}
      {!(selected === undefined && noAgents === "add") && (
        <div className="groove-t flex justify-end p-2">
          <Button size="sm" variant="ghost" className="text-app-muted hover:text-foreground data-[focus-visible=true]:text-foreground" preventFocusOnPress onPress={onOpenAccounts}>
            <KeyRound aria-hidden="true" className="size-4 shrink-0" />
            {t("rail.quota.manage")}
          </Button>
        </div>
      )}
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
 * in WebKit.
 *
 * With a `currentAccount` (`null` for the default account) the list opens scrolled so that account's row
 * is in view, as the selected session's account is the one the user came to read. The row is not marked
 * in any other way. */
function AgentPanel({ agent, owned, currentAccount }: { agent: Agent; owned: Account[]; currentAccount?: string | null }): React.ReactElement {
  const t = useT();
  const defaultName = defaultAccountName(t, useDaemonStore((s) => s.settings.default_account_names), agent);
  const region = useFadeRegion({ label: t("rail.quota.accounts") });
  const scroller = useRef<HTMLDivElement>(null);
  const currentRow = useRef<HTMLDivElement>(null);
  // `scrollTop` rather than `scrollIntoView`, which would scroll the popover's ancestors too. The row's
  // `offsetTop` is measured from the list's content (its wrapper is the offset parent) and is not
  // affected by the popover's opening transform.
  useLayoutEffect(() => {
    if (scroller.current && currentRow.current) scroller.current.scrollTop = Math.max(0, currentRow.current.offsetTop - FADE_SIZE);
  }, []);
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
      <div className={region.focused ? "ring-2 ring-inset ring-focus" : ""}>
        <FadedScroller ref={scroller} region={region} className="max-h-[min(24rem,60vh)]">
          <div className="relative flex flex-col px-4">
            <QuotaAccount agent={agent} account={null} name={defaultName} rowRef={currentAccount === null ? currentRow : undefined} />
            {owned.map((account) => (
              <QuotaAccount
                key={account.id}
                agent={agent}
                account={account.id}
                name={<FadeOverflow dir="auto" titleWhenClipped={account.name}>{account.name}</FadeOverflow>}
                rowRef={currentAccount === account.id ? currentRow : undefined}
              />
            ))}
          </div>
        </FadedScroller>
      </div>
    </Tabs.Panel>
  );
}

/** One account of the panel: its name and plan, and its quota beneath. An account that is used up recedes, its
 * name and meters muted and a "Used up" badge saying so, so the accounts that can still be used stand out. */
function QuotaAccount({ agent, account, name, rowRef }: { agent: Agent; account: string | null; name: React.ReactNode; rowRef?: React.Ref<HTMLDivElement> }): React.ReactElement {
  const quota = useAccountQuota(agent, account);
  const now = useNow();
  const usedUp = exhaustedQuotaWindow(quota, now) !== undefined;
  return (
    <div ref={rowRef} className="flex flex-col gap-2 groove-b py-3 last:border-b-0">
      <AccountNameLine name={name} quota={quota} usedUp={usedUp} />
      <AccountQuotaSummary quota={quota} single usedUp={usedUp} />
    </div>
  );
}
