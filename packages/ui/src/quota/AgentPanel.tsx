import { Tabs } from "@heroui/react";
import React, { useLayoutEffect, useRef } from "react";

import { defaultAccountName } from "../agents/agents";
import { FADE_SIZE, FadedScroller, type FadeRegion, useFadeRegion } from "../components/scroll/scrollFade";
import { useListMotion } from "../components/motion/useListMotion";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { useT } from "../i18n/react";
import type { Agent } from "../protocol";
import type { QuotaRow } from "./quotaListing";
import { useDaemonStore } from "../store";
import { QuotaAccount, type ResetRequest } from "./QuotaAccount";

/** An agent's tab panel: its accounts in the saved account order, in a faded scroller that is a region of
 * its own while it overflows. Each panel keeps its own region, since the scroller reports its overflow
 * only when it changes, so one shared by the panels would follow whichever panel last reported, not the
 * selected one.
 *
 * The region is held by the component that renders the panel, not by one inside it. react-aria makes a
 * panel with nothing tabbable inside it a tab stop of its own, and checks that when the panel renders
 * or something is added inside it, never when an attribute such as `tabindex` changes: the region's
 * `tabindex` arriving with the overflow report goes unnoticed unless the panel renders with it. Left a
 * stop in front of the region, the panel takes Tab from the tab strip and then, rendering on that focus,
 * finds the region and gives up its `tabindex`, which drops focus to the body in WebKit.
 *
 * Only the region is held here, and the list is the panel's content (`AgentAccountList`): `Tabs`
 * renders its children a second time in the hidden copy it builds its collection from, where the panel
 * renders nothing but this component renders in full, so the list's hooks would run there too. */
export function AgentPanel({ agent, rows, currentAccount, dimOthers, onUseReset }: { agent: Agent; rows: QuotaRow[]; currentAccount?: string | null; dimOthers: boolean; onUseReset: ResetRequest }): React.ReactElement {
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
      <AgentAccountList agent={agent} rows={rows} currentAccount={currentAccount} dimOthers={dimOthers} region={region} onUseReset={onUseReset} />
    </Tabs.Panel>
  );
}

/** An agent's accounts in its panel's faded scroller, which is the `region` the panel holds.
 *
 * With a `currentAccount` (`null` for the default account) the list opens scrolled so that account's row
 * is in view, as the selected session's account is the one the user came to read. The row is not marked
 * in any other way, except that with `dimOthers` (from the mouse on the rail button until it enters the panel
 * or the panel closes) every row but that one, on every agent's panel, is dimmed. The `rows` are what the panel lists
 * (`listedQuotaAccounts`); accounts added or removed while it is open move with the shared list motion. */
function AgentAccountList({ agent, rows, currentAccount, dimOthers, region, onUseReset }: { agent: Agent; rows: QuotaRow[]; currentAccount?: string | null; dimOthers: boolean; region: FadeRegion; onUseReset: ResetRequest }): React.ReactElement {
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
    <div className={region.insetRing}>
      <FadedScroller ref={scroller} region={region} className="max-h-[min(24rem,60vh)]">
        <div ref={listRef} className="relative flex flex-col px-4">
          {rows.map(({ account, disabled }) => (
            <QuotaAccount
              key={account?.id ?? "default"}
              agent={agent}
              account={account?.id ?? null}
              name={account ? <FadeOverflow dir="auto" titleWhenClipped={account.name}>{account.name}</FadeOverflow> : defaultName}
              plainName={account?.name ?? defaultName}
              onUseReset={onUseReset}
              rowRef={currentAccount === (account?.id ?? null) ? currentRow : undefined}
              dimmed={dimOthers && currentAccount !== (account?.id ?? null)}
              disabled={disabled}
            />
          ))}
        </div>
      </FadedScroller>
    </div>
  );
}
