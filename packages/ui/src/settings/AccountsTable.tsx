import React, { useLayoutEffect, useRef, useState } from "react";

import { AGENT_LABEL } from "../agents";
import { ActionMenu, type ActionMenuEntry } from "../components/ActionMenu";
import { FadeOverflow } from "../components/FadeOverflow";
import { HelpTip } from "../components/HelpTip";
import { FadedScroller, useFadeRegion } from "../components/scrollFade";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Account, Agent } from "../protocol";
import { AccountUsageCell } from "../quota/AccountUsageCell";
import { AccountNameLine, LoginStateBadge } from "../quota/AccountQuotaSummary";
import { useAccountQuota } from "../quota/useAccountQuota";
import { useDaemonStore } from "../store";
import { AccountDisableSwitch } from "./AccountDisableSwitch";
import { AccountLoginAnnouncer, accountLoginOf, useClearSucceededLogin } from "./AccountLogin";
import { AccountLoginButton, type ReturnLoginFocus } from "./AccountLoginButton";

/** The pinned cell's start edge while the table scrolls: a hairline, as a shadow since a cell's border is the
 * groove's. */
const PINNED_EDGE = "group-data-[overflowing]/accounts:shadow-[inset_1px_0_0_var(--separator)] group-data-[overflowing]/accounts:rtl:shadow-[inset_-1px_0_0_var(--separator)]";

const HEADER = "px-2 pb-2 text-start text-xs font-medium whitespace-nowrap text-app-muted groove-b";

/** One agent's accounts as a table, a row for each account (`AccountTableRow`, as `children`) under column headers
 * said once: the account (name and plan), its status (login state, with the Log in icon while it is signed out), its
 * usage, whether it is turned off (the Disable switch), and an actions column that holds the actions menu, whose first
 * entry opens the details. Narrower than its columns need, the table scrolls sideways (a focusable region while it
 * does) and the actions column stays pinned at the end edge. The scroller draws no fade, which would dim that column,
 * and keeps clear of it when focus scrolls a control into view: its end scroll padding takes the column's measured
 * width. */
export function AccountsTable({ agent, children }: { agent: Agent; children: React.ReactNode }): React.ReactElement {
  const t = useT();
  const region = useFadeRegion({ label: t("settings.accounts.table.scrollLabel", { agent: AGENT_LABEL[agent] }) });
  const pinned = useRef<HTMLTableCellElement>(null);
  const [pinnedWidth, setPinnedWidth] = useState(0);
  useLayoutEffect(() => {
    const cell = pinned.current;
    if (!cell) return;
    const measure = () => setPinnedWidth(cell.offsetWidth);
    const resize = new ResizeObserver(measure);
    resize.observe(cell);
    measure();
    return () => resize.disconnect();
  }, []);
  return (
    <div className={`group/accounts rounded-lg ${region.focused ? "ring-2 ring-focus" : ""}`} data-overflowing={region.overflows ? "" : undefined}>
      {/* The top padding is room for the focus ring of the help tip in the header. */}
      <FadedScroller orientation="horizontal" region={region} fade={false} endInset={pinnedWidth} className="pt-1">
        <table aria-label={t("settings.accounts.table.label", { agent: AGENT_LABEL[agent] })} className="w-full min-w-[38rem] border-separate border-spacing-0 text-sm">
          <thead>
            <tr>
              <th scope="col" className={HEADER}>{t("settings.accounts.table.account")}</th>
              <th scope="col" className={HEADER}>{t("settings.accounts.table.status")}</th>
              <th scope="col" className={HEADER}>{t("settings.accounts.table.usage")}</th>
              <th scope="col" className={HEADER}>
                <span className="inline-flex items-center gap-1">
                  {t("settings.accounts.disable")}
                  <HelpTip label={t("settings.accounts.disableHelp")}>{t("settings.accounts.disableExplanation")}</HelpTip>
                </span>
              </th>
              <th ref={pinned} scope="col" className={`${HEADER} ${PINNED_EDGE} sticky end-0 z-1 bg-overlay`}>
                <span className="sr-only">{t("settings.accounts.table.actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>{children}</tbody>
        </table>
      </FadedScroller>
    </div>
  );
}

/** The actions menu button of the row of `row` (an account id, or `default:<agent>`), found by the row's own marks and
 * not by the button's name, which is in the interface language. */
export function rowActionsTrigger(row: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-account-row="${CSS.escape(row)}"] [data-row-actions] button`);
}

const CELL = "px-2 py-3 align-middle groove-b group-last/row:border-b-0";

/** One account's row (`account` null for the agent's default account): its `name` with the plan (the row's header
 * cell), its login state with the Log in icon while it is signed out (the Status cell), a compact usage indicator, the
 * Disable switch, and the actions `menu`, whose first entry opens the details. A turned-off account's row looks like
 * any other but for its switch. The hidden status region that says a login succeeded is the row's, which also clears
 * that login once it has been shown. */
export function AccountTableRow({ agent, account, name, menu, onLogin }: {
  agent: Agent;
  account: Account | null;
  name: string;
  menu: ActionMenuEntry[];
  onLogin: (returnFocus: ReturnLoginFocus) => void;
}): React.ReactElement {
  const t = useT();
  const id = account?.id ?? null;
  const quota = useAccountQuota(agent, id);
  const login = useDaemonStore((s) => accountLoginOf(s.accountLogins, agent, id));
  const succeeded = login?.status === "succeeded" ? login : undefined;
  useClearSucceededLogin(succeeded);
  const actions = useRef<HTMLDivElement>(null);
  return (
    <tr data-account-row={id ?? `default:${agent}`} className="group/row">
      <th scope="row" className={`${CELL} text-start font-normal`}>
        <div className="max-w-56 min-w-28">
          <AccountNameLine name={<FadeOverflow dir="auto" titleWhenClipped={name}>{name}</FadeOverflow>} quota={quota} showUpdated={false} showFailure={false} />
          <AccountLoginAnnouncer login={succeeded} name={name} />
        </div>
      </th>
      <td className={`${CELL} whitespace-nowrap`}>
        <div className="flex items-center gap-1">
          <LoginStateBadge quota={quota} />
          <AccountLoginButton agent={agent} account={id} name={name} quota={quota} onLogin={onLogin} fallback={() => actions.current?.querySelector<HTMLElement>("button")} />
        </div>
      </td>
      <td className={CELL}><AccountUsageCell quota={quota} /></td>
      <td className={CELL}><AccountDisableSwitch agent={agent} account={id} name={name} /></td>
      <td className={`${CELL} ${PINNED_EDGE} sticky end-0 z-1 bg-overlay`}>
        <div ref={actions} data-row-actions="" className="flex items-center justify-end gap-1">
          <ActionMenu label={t("settings.accounts.actionsLabel", { name: isolate(name), agent: AGENT_LABEL[agent] })} items={menu} />
        </div>
      </td>
    </tr>
  );
}
