import React from "react";

import { exhaustedQuotaWindow } from "../accounts/quotaFormat";
import { INACTIVE_VEIL_CLASS } from "../components/status/inactiveVeil";
import { useNow } from "../format/useNow";
import type { Agent } from "../protocol";
import { AccountNameLine, AccountQuotaSummary } from "./AccountQuotaSummary";
import { ResetCreditsLine } from "./ResetCredits";
import { useAccountQuota } from "./useAccountQuota";

/** Asks to use a reset credit of an account of `agent` (its id, null for the default account) under the name the panel shows it by. */
export type ResetRequest = (agent: Agent, account: string | null, name: string) => void;

/** One account of the panel: its name and plan, and its quota beneath. An account that is used up recedes, its
 * name and meters muted and a "Limit reached" badge saying so, so the accounts that can still be used stand out.
 * `dimmed` is the other, passing cue: a veil of the panel's own colour laid over the row, not `opacity`: under a
 * translucent ancestor WebKit paints descendants that fade or sit on a GPU layer as blank tiles. The veil is
 * visual only and lets pointer events through. It takes the dimmed text below WCAG AA on purpose, by the user's
 * decision: a de-emphasis cue, lifted when the mouse enters the panel or the panel closes. A turned-off account, which
 * only the selected running session's exception lists, carries a warning-coloured "Disabled" badge. */
export function QuotaAccount({ agent, account, name, plainName, rowRef, dimmed, disabled, onUseReset }: { agent: Agent; account: string | null; name: React.ReactNode; plainName: string; rowRef?: React.Ref<HTMLDivElement>; dimmed: boolean; disabled: boolean; onUseReset: ResetRequest }): React.ReactElement {
  const quota = useAccountQuota(agent, account);
  const now = useNow();
  const usedUp = exhaustedQuotaWindow(quota, now) !== undefined;
  // The last row has no divider; a departing row's visual after it is not a row.
  return (
    <div ref={rowRef} data-list-motion={account ?? `default:${agent}`} data-dimmed={dimmed || undefined} className={`${INACTIVE_VEIL_CLASS} flex flex-col gap-2 groove-b py-3 data-dimmed:after:bg-overlay/55 [&:not(:has(~[data-list-motion]))]:border-b-0`}>
      <AccountNameLine name={name} quota={quota} usedUp={usedUp} disabled={disabled} />
      <AccountQuotaSummary quota={quota} single usedUp={usedUp} />
      <ResetCreditsLine quota={quota} accountName={plainName} onUse={() => onUseReset(agent, account, plainName)} preventFocusOnPress />
    </div>
  );
}
