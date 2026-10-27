import { Tabs } from "@heroui/react";
import { Bot, Gauge, KeyRound } from "lucide-react";
import React, { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";

import { Button } from "../components/Button";
import { accountsByAgent, defaultAccountName, noAgentState, pickableAgents } from "../agents";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { AgentTabList } from "../components/AgentTabList";
import { ChromeButton } from "../components/ChromeButton";
import { Hint, keepsEscapeFromPanel } from "../components/Hint";
import { FadeOverflow } from "../components/FadeOverflow";
import { EmptyPanel } from "../components/EmptyPanel";
import { FADE_SIZE, FadedScroller, type FadeRegion, useFadeRegion } from "../components/scrollFade";
import { useListMotion } from "../components/useListMotion";
import { Message, useT } from "../i18n/react";
import type { Account, Agent, QuotaWindow } from "../protocol";
import type { SettingsSectionId } from "../settings/SettingsDialog";
import { INACTIVE_VEIL_CLASS } from "../components/inactiveVeil";
import { useDaemon, useDaemonStore } from "../store";
import { useNow } from "../useNow";
import { AccountNameLine, AccountQuotaSummary } from "./AccountQuotaSummary";
import { MODAL_OPEN } from "../layout/useRegionCycle";
import { useAccountQuota } from "./useAccountQuota";
import { exhaustedQuotaWindow, remainingPercentAt, type QuotaBar } from "./quotaFormat";
import { accountQuotaLabel, sessionQuotaBarOf } from "./sessionQuota";

/** The popover of a `StatusMark` in the panel, which is portalled out of it but belongs to it. */
const NESTED_POPOVER = "[data-status-mark-popover]";
/** The one of those that a mouse hover opened, which Escape closes before it closes the panel. */
const NESTED_TOPMOST = `${NESTED_POPOVER}[data-escape-topmost]`;

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

/** The rail's quota button and the floating panel it anchors, which lists the accounts of each agent that is added and
 * installed, in the order they were added, with their plan and quota (see "Opening the panel" in
 * `docs/product/account-quota-panel.md`). The panel is a `Hint` of kind `popover`, the same behaviour as a status
 * mark's popover: resting the mouse on the button opens it without moving focus; a mouse click opens nothing; a tap, a
 * pen, assistive technology and the keyboard open it with focus in it. Each opening asks the daemon to refresh the
 * quotas that are due. */
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
  // The mouse has come to the button and not yet into the panel, which is when the panel stands out the account
  // the button shows; it ends on entering the panel, on the panel's closing, or on leaving the button before the
  // panel has opened.
  const [onButton, setOnButton] = useState(false);
  const label = session && bar ? accountQuotaLabel(t, session.agent, account?.name, bar, defaultNames, now) : t("rail.quota");

  return (
    <Hint
      kind="popover"
      label={t("rail.quota")}
      placement="end bottom"
      offset={8}
      // A panel, not a hint: the surface, shadow and corners are the tooltip's, but it pads its own content edge to
      // edge and keeps the text size it had.
      contentClassName="w-96 max-w-[calc(100vw-4rem)] p-0 text-sm"
      // A status mark's popover is portalled out of the panel but belongs to it, and Escape with one
      // open (by hover, which holds no focus) is the mark's to take first, where it would take it.
      isInside={(target) => target instanceof Element && target.closest(NESTED_POPOVER) != null}
      yieldsEscape={(event) => {
        const target = event.target instanceof Element ? event.target : null;
        if (target?.closest(NESTED_POPOVER)) return true;
        return document.querySelector(NESTED_TOPMOST) != null && !keepsEscapeFromPanel(target);
      }}
      // The quotas already held are shown whether or not this succeeds, and the daemon's own poller
      // keeps them current, so a failed request has nothing to report.
      onOpenChange={(open) => {
        if (open) void request({ type: "refresh_account_quotas" }).catch(() => {});
        else setOnButton(false);
      }}
      trigger={({ ref, onHoverStart: _start, onHoverEnd: _end, preventFocusOnPress: _prevent, onMouseHoverChange, ...wiring }) => (
        <ChromeButton
          label={label}
          className="relative"
          buttonRef={ref}
          noTooltip
          {...wiring}
          onMouseHoverChange={(hovered) => {
            if (hovered) setOnButton(true);
            else if (!wiring.expanded) setOnButton(false);
            onMouseHoverChange(hovered);
          }}
        >
          <Gauge aria-hidden="true" className={!session || !bar ? "size-4" : bar.thin ? "size-4 -translate-y-1" : "size-4 -translate-y-0.5"} />
          {session && bar && <RailQuotaBars bar={bar} now={now} />}
        </ChromeButton>
      )}
    >
      {({ close, form, popupId }) => (
        <>
          <CloseWhenCovered close={close} own={popupId} />
          <QuotaPanel
            // The tab it opens on: the agent whose quota the button shows, if any.
            shownAgent={session && bar ? session.agent : undefined}
            // The account the panel scrolls to on that tab: the selected session's, the default account when the session names none.
            shownAccount={session?.account_id ?? null}
            // A press opens a panel that floats outside the rail's tab order, so focus is brought to its
            // first control; one a mouse rests on leaves focus where it was.
            focusSelectedTab={form === "press"}
            // The account the button shows stands out and the others dim from when the mouse is on the button
            // until it enters the panel or the panel closes; only a mouse does it, so keyboard focus on the
            // button (which does not open the panel) leaves them alone.
            emphasiseShown={onButton && session !== undefined && bar !== undefined}
            onMouseEnter={() => setOnButton(false)}
            onOpenAccounts={() => {
              close();
              onOpenSettingsAt("accounts");
            }}
            onAddAgent={() => {
              close();
              onOpenSettingsAt("accounts", { addAgent: true });
            }}
          />
        </>
      )}
    </Hint>
  );
}

/** Closes the panel it is rendered in when a modal is opened by something other than a press here
 * (Settings from the native menu), which covers the panel and would otherwise leave it up under the
 * modal, taking the modal's Escape. Modals and menus portal in as direct children of the body, so only
 * those are watched. Not covering: a hover-opened panel (`aria-modal="false"`), the panel's own dialog
 * in its press form (`own`), and a status mark's popover opened from inside it, whose portal would
 * otherwise read as such a modal and close the panel it came from. */
function CloseWhenCovered({ close, own }: { close: () => void; own: string }): null {
  useEffect(() => {
    const modalWatch = new MutationObserver(() => {
      const covering = Array.from(document.querySelectorAll(MODAL_OPEN)).some((modal) => !modal.closest(NESTED_POPOVER) && modal.id !== own);
      if (covering) close();
    });
    modalWatch.observe(document.body, { childList: true });
    return () => modalWatch.disconnect();
  }, []);
  return null;
}

/** The panel's contents: one tab per agent that is added and installed, in the order they were added, each listing the agent's accounts, and the
 * way into Settings' accounts section. The panel mounts afresh on each opening, so the tab starts on `shownAgent`
 * (the first tab if it is not listed or none is given) and the user's own choice lasts only until it closes. */
function QuotaPanel({ shownAgent, shownAccount, focusSelectedTab, emphasiseShown, onMouseEnter, onOpenAccounts, onAddAgent }: { shownAgent?: Agent; shownAccount: string | null; focusSelectedTab: boolean; emphasiseShown: boolean; onMouseEnter: () => void; onOpenAccounts: () => void; onAddAgent: () => void }): React.ReactElement {
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
    <div ref={root} className="flex flex-col" onPointerEnter={(event) => event.pointerType === "mouse" && onMouseEnter()}>
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
              <AgentPanel key={agent} agent={agent} owned={owned} currentAccount={agent === shownAgent ? shownAccount : undefined} dimOthers={emphasiseShown} />
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
 * Only the region is held here, and the list is the panel's content (`AgentAccountList`): `Tabs`
 * renders its children a second time in the hidden copy it builds its collection from, where the panel
 * renders nothing but this component renders in full, so the list's hooks would run there too. */
function AgentPanel({ agent, owned, currentAccount, dimOthers }: { agent: Agent; owned: Account[]; currentAccount?: string | null; dimOthers: boolean }): React.ReactElement {
  const t = useT();
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
      <AgentAccountList agent={agent} owned={owned} currentAccount={currentAccount} dimOthers={dimOthers} region={region} />
    </Tabs.Panel>
  );
}

/** An agent's accounts in its panel's faded scroller, which is the `region` the panel holds.
 *
 * With a `currentAccount` (`null` for the default account) the list opens scrolled so that account's row
 * is in view, as the selected session's account is the one the user came to read. The row is not marked
 * in any other way, except that with `dimOthers` (from the mouse on the rail button until it enters the panel
 * or the panel closes) every row but that one, on every agent's panel, is dimmed. Accounts added or removed
 * while the panel is open move with the shared list motion. */
function AgentAccountList({ agent, owned, currentAccount, dimOthers, region }: { agent: Agent; owned: Account[]; currentAccount?: string | null; dimOthers: boolean; region: FadeRegion }): React.ReactElement {
  const t = useT();
  const defaultName = defaultAccountName(t, useDaemonStore((s) => s.settings.default_account_names), agent);
  const scroller = useRef<HTMLDivElement>(null);
  const currentRow = useRef<HTMLDivElement>(null);
  const listRef = useListMotion<HTMLDivElement>();
  // `scrollTop` rather than `scrollIntoView`, which would scroll the popover's ancestors too. The row's
  // `offsetTop` is measured from the list's content (its wrapper is the offset parent) and is not
  // affected by the popover's opening transform.
  useLayoutEffect(() => {
    if (scroller.current && currentRow.current) scroller.current.scrollTop = Math.max(0, currentRow.current.offsetTop - FADE_SIZE);
  }, []);
  return (
    <div className={region.focused ? "ring-2 ring-inset ring-focus" : ""}>
      <FadedScroller ref={scroller} region={region} className="max-h-[min(24rem,60vh)]">
        <div ref={listRef} className="relative flex flex-col px-4">
          <QuotaAccount agent={agent} account={null} name={defaultName} rowRef={currentAccount === null ? currentRow : undefined} dimmed={dimOthers && currentAccount !== null} />
          {owned.map((account) => (
            <QuotaAccount
              key={account.id}
              agent={agent}
              account={account.id}
              name={<FadeOverflow dir="auto" titleWhenClipped={account.name}>{account.name}</FadeOverflow>}
              rowRef={currentAccount === account.id ? currentRow : undefined}
              dimmed={dimOthers && currentAccount !== account.id}
            />
          ))}
        </div>
      </FadedScroller>
    </div>
  );
}

/** One account of the panel: its name and plan, and its quota beneath. An account that is used up recedes, its
 * name and meters muted and a "Limit reached" badge saying so, so the accounts that can still be used stand out.
 * `dimmed` is the other, passing cue: a veil of the panel's own colour laid over the row, not `opacity`: under a
 * translucent ancestor WebKit paints descendants that fade or sit on a GPU layer as blank tiles. The veil is
 * visual only and lets pointer events through. It takes the dimmed text below WCAG AA on purpose, by the user's
 * decision: a de-emphasis cue, lifted when the mouse enters the panel or the panel closes. */
function QuotaAccount({ agent, account, name, rowRef, dimmed }: { agent: Agent; account: string | null; name: React.ReactNode; rowRef?: React.Ref<HTMLDivElement>; dimmed: boolean }): React.ReactElement {
  const quota = useAccountQuota(agent, account);
  const now = useNow();
  const usedUp = exhaustedQuotaWindow(quota, now) !== undefined;
  // The last row has no divider; a departing row's visual after it is not a row.
  return (
    <div ref={rowRef} data-list-motion={account ?? `default:${agent}`} data-dimmed={dimmed || undefined} className={`${INACTIVE_VEIL_CLASS} flex flex-col gap-2 groove-b py-3 data-dimmed:after:bg-overlay/55 [&:not(:has(~[data-list-motion]))]:border-b-0`}>
      <AccountNameLine name={name} quota={quota} usedUp={usedUp} />
      <AccountQuotaSummary quota={quota} single usedUp={usedUp} />
    </div>
  );
}
