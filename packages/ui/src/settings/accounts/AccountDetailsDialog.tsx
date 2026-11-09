import React, { useId, useRef, useState } from "react";

import { isAgentUnavailable } from "../../agents/agents";
import { HelpTip } from "../../components/hint/HelpTip";
import { PATH_VAR_PARAMS } from "../../components/text/literalParams";
import { MarkedPath } from "../../components/text/MarkedPath";
import { focusOpenDialog } from "../../components/focus/modalFocus";
import { Dialog } from "../../components/dialog/Dialog";
import { Message, useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { AccountQuotaSummary, LoginStateBadge, NoUsageReading, QuotaFailureMark, QuotaPlan, QuotaUpdated } from "../../quota/AccountQuotaSummary";
import { loginStateOf } from "../../accounts/quotaFormat";
import { ResetCreditsLine, UseResetCreditDialog, useResetFocusReturn } from "../../quota/ResetCredits";
import { useAccountQuota } from "../../quota/useAccountQuota";
import { useDaemonStore } from "../../store";
import { ProviderName } from "../../components/text/ProviderName";
import { AccountHeading, useAccountDialogSubject } from "./accountDialogSubject";
import { AccountLoginAnnouncer, useSucceededAccountLogin } from "./AccountLogin";

/** A titled part of the dialog, the title standing over what it holds and naming it. */
function Part({ title, aside, children }: { title: React.ReactNode; aside?: React.ReactNode; children: React.ReactNode }): React.ReactElement {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="flex min-w-0 flex-col gap-1.5">
      <div className="flex items-center gap-1">
        <h3 id={headingId} className="text-sm font-medium text-foreground">{title}</h3>
        {aside}
      </div>
      {children}
    </section>
  );
}

/**
 * One account's details in a modal of its own, opened from its row's actions menu (`account` null for the agent's
 * default account): what the row shows in full and what it leaves out, read-only but for redeeming a reset credit. The
 * title is plain ("Account details"); the body's first line names the account, with its plan and login state after the
 * name, the email under it on a line of its own, and the next says where its configuration lives, the default
 * account's explanation after the path. Then the account's quota as the rail's panel draws it (the two-layer 5-hour
 * and weekly meter, each model's window after it) with when it was read, and, for an account holding a reset credit of a provider
 * that offers them, the line that offers to redeem it (`ResetCreditsLine`), whose confirmation opens over this dialog.
 * Besides those controls it holds the header's close button, which Escape and a click outside do the same as; the
 * account's switches are in `AccountSettingsDialog`, its connectors and MCP servers in `AccountMcpDialog`, and logging
 * in is the row's and the menu's. It reads the account from the store as it is now, so a change from another window
 * shows, and closes if the account is removed.
 *
 * Closing the confirmation hands focus back when it fell to the page (the confirmation was opened by a press that took
 * no focus, or the button went with the last credit): to the Redeem reset button while it is there, else to this
 * dialog's opening focus (the same handoff as `useLoginFocusReturn`).
 */
export function AccountDetailsDialog({ agent, account, onClose }: {
  agent: Agent;
  account: string | null;
  onClose: () => void;
}): React.ReactElement | null {
  const t = useT();
  const { stored, name, gone } = useAccountDialogSubject(agent, account, onClose);
  const succeeded = useSucceededAccountLogin(agent, account);
  const defaultDir = useDaemonStore((s) => s.agentAvailability.get(agent)?.default_account_dir);
  const unusable = useDaemonStore((s) => isAgentUnavailable(s.agentAvailability, agent));
  const quota = useAccountQuota(agent, account);
  const [resetting, setResetting] = useState(false);
  const redeem = useRef<HTMLDivElement>(null);
  const handBack = useResetFocusReturn(() => void focusOpenDialog(redeem.current?.querySelector("button"), { opening: true }));
  const closeReset = () => {
    setResetting(false);
    handBack();
  };
  if (gone) return null;
  const path = stored ? stored.config_dir : defaultDir;
  const known = loginStateOf(quota) !== undefined;
  const showsUsage = quota === "loading" || (typeof quota === "object" && (quota.windows.length > 0 || loginStateOf(quota) === "out"));
  return (
    <>
      <Dialog
        size="lg"
        title={t("settings.accounts.detailsDialogTitle")}
        onClose={onClose}
        footer={null}
      >
        <div className="flex flex-col gap-4">
          {/* The title is plain, so what the badges qualify is the account named here, ahead of them. */}
          <AccountHeading name={name} quota={quota}>
            <QuotaPlan quota={quota} />
            {known
              ? <LoginStateBadge quota={quota} />
              : <span className="text-app-muted">{t(quota === "loading" ? "settings.accounts.loginChecking" : "settings.accounts.loginUnknown")}</span>}
            <QuotaFailureMark quota={quota} />
          </AccountHeading>
          {path && (
            <div className="flex items-center gap-1">
              <div className="min-w-0"><MarkedPath path={path} /></div>
              {!stored && <HelpTip label={t("settings.accounts.defaultHelp")}>{t("settings.accounts.defaultSetup")}</HelpTip>}
            </div>
          )}
          {stored && unusable && (
            <div className="text-sm text-app-muted">
              <Message id="settings.accounts.notUsable" params={{ agent: <ProviderName agent={agent} />, ...PATH_VAR_PARAMS }} />
            </div>
          )}
          <AccountLoginAnnouncer login={succeeded} name={name} />
          <Part title={t("settings.accounts.table.usage")} aside={<QuotaUpdated quota={quota} />}>
            {showsUsage ? <AccountQuotaSummary quota={quota} single /> : <NoUsageReading />}
            <div ref={redeem} className="contents">
              <ResetCreditsLine quota={quota} accountName={name} onUse={() => setResetting(true)} />
            </div>
          </Part>
        </div>
      </Dialog>
      {resetting && <UseResetCreditDialog agent={agent} account={account} accountName={name} onClose={closeReset} />}
    </>
  );
}
