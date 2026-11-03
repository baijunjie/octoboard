import { Gauge } from "lucide-react";
import React, { useMemo, useRef, useState } from "react";

import { pickableAgents } from "../agents/agents";
import { keepsEscapeFromPanel } from "../components/hint/Hint";
import { STATUS_MARK_POPOVER } from "../components/status/StatusMark";
import { useT } from "../i18n/react";
import type { OpenSettings } from "../settings/settingsLauncher";
import { useDaemon, useDaemonStore } from "../store";
import { UseResetCreditDialog, useResetFocusReturn } from "../quota/ResetCredits";
import { accountInUse, accountQuotaLabel, sessionQuotaBarOf } from "../quota/sessionQuota";
import { nestedPopovers } from "./nestedPopover";
import { RailPanelEntry } from "./RailPanelEntry";
import { QuotaPanel } from "../quota/QuotaPanel";
import { RailQuotaBars } from "../quota/RailQuotaBars";
import { useResetClock } from "../quota/useResetClock";

/** The one of the status marks' popovers that a mouse hover opened, which Escape closes before it closes the panel. */
const NESTED_TOPMOST = `${STATUS_MARK_POPOVER}[data-escape-topmost]`;
const OWNER = "rail-quota";

const nested = nestedPopovers(OWNER);

/** The rail's quota button and the floating panel it anchors, which lists each added and installed agent's accounts
 * in the saved display order, bar the ones the user turned off, with their plan and quota (see "Opening the panel" in
 * `docs/product/account-quota-panel.md`). The panel is a `Hint` of kind `popover`, the same behaviour as a status mark's
 * popover: resting the mouse on the button opens it without moving focus; a mouse click opens nothing; a tap, a pen,
 * assistive technology and the keyboard open it with focus in it. Each opening asks the daemon to refresh the quotas
 * that are due. */
export function RailQuotaEntry({ selectedSessionId, onOpenSettingsAt }: {
  selectedSessionId?: string;
  onOpenSettingsAt: OpenSettings;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const session = useDaemonStore((s) => s.sessions.get(selectedSessionId ?? ""));
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const accountQuotas = useDaemonStore((s) => s.accountQuotas);
  const disabledDefaults = useDaemonStore((s) => s.settings.disabled_default_accounts);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  // Memoized rather than computed in a store selector, since its result is a new object on each call.
  const bar = useMemo(() => sessionQuotaBarOf(session, { accounts, disabledDefaultAccounts: disabledDefaults, agentAvailability, accountQuotas }), [session, accounts, disabledDefaults, agentAvailability, accountQuotas]);
  // The panel stands the session's account out only if its row is listed. The bar already leaves out a turned-off account
  // unless the session runs on it, which is the panel's exception too; but an agent that is not added has a bar and no tab.
  const inUse = accountInUse(session);
  const shownListed = session !== undefined && bar !== undefined && pickableAgents(agentAvailability, addedAgents).includes(session.agent);
  const account = accounts.find((account) => account.id === session?.account_id);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const now = useResetClock(bar);
  // The mouse has come to the button and not yet into the panel, which is when the panel stands out the account
  // the button shows; it ends on entering the panel, on the panel's closing, or on leaving the button before the
  // panel has opened.
  const [onButton, setOnButton] = useState(false);
  const label = session && bar ? accountQuotaLabel(t, session.agent, account?.name, bar, defaultNames, now) : t("rail.quota");

  const [resetting, setResetting] = useState<{ account: string | null; name: string }>();
  // The panel closes on a press of Redeem reset, so with the confirmation gone focus has nowhere to go but this button.
  const railButton = useRef<HTMLButtonElement>(null);
  const handBack = useResetFocusReturn(() => railButton.current?.focus());
  return (
    <>
      <RailPanelEntry
        owner={OWNER}
        label={t("rail.quota")}
        triggerLabel={label}
        triggerRef={railButton}
        triggerClassName="relative"
        glyph={
          <>
            <Gauge aria-hidden="true" className={!session || !bar ? "size-4" : bar.thin ? "size-4 -translate-y-1" : "size-4 -translate-y-0.5"} />
            {session && bar && <RailQuotaBars bar={bar} now={now} />}
          </>
        }
        // A status mark's popover is portalled out of the panel but belongs to it, and Escape with one
        // open (by hover, which holds no focus) is the mark's to take first, where it would take it.
        yieldsEscape={(target) => nested.opened().some((popover) => popover.matches(NESTED_TOPMOST)) && !keepsEscapeFromPanel(target)}
        onHoverChange={(hovered, expanded) => {
          if (hovered) setOnButton(true);
          else if (!expanded) setOnButton(false);
        }}
        // The quotas already held are shown whether or not this succeeds, and the daemon's own poller
        // keeps them current, so a failed request has nothing to report.
        onOpenChange={(open) => {
          if (open) void request({ type: "refresh_account_quotas" }).catch(() => {});
          else setOnButton(false);
        }}
      >
        {({ close, form }) => (
          <QuotaPanel
            // The tab it opens on: the agent whose quota the button shows, if any.
            shownAgent={session && bar ? session.agent : undefined}
            // The account the panel scrolls to on that tab: the selected session's, the default account when the session names none.
            shownAccount={session?.account_id ?? null}
            // The one account a turned-off account's exception is for: the selected session's, while it still runs on it.
            inUse={inUse}
            // A press opens a panel that floats outside the rail's tab order, so focus is brought to its
            // first control; one a mouse rests on leaves focus where it was.
            focusSelectedTab={form === "press"}
            // The account the button shows stands out and the others dim from when the mouse is on the button
            // until it enters the panel or the panel closes; only a mouse does it, so keyboard focus on the
            // button (which does not open the panel) leaves them alone.
            emphasiseShown={onButton && shownListed}
            onMouseEnter={() => setOnButton(false)}
            onOpenAccounts={() => {
              close();
              onOpenSettingsAt("accounts");
            }}
            onAddAgent={() => {
              close();
              onOpenSettingsAt("accounts", { addAgent: true });
            }}
            // The confirmation is held out here: a modal opening closes the panel (`CloseWhenCovered`), and a dialog
            // held inside it would go with it.
            onUseReset={(account, name) => {
              close();
              setResetting({ account, name });
            }}
          />
        )}
      </RailPanelEntry>
      {resetting && <UseResetCreditDialog account={resetting.account} accountName={resetting.name} onClose={() => {
        setResetting(undefined);
        handBack();
      }} />}
    </>
  );
}

