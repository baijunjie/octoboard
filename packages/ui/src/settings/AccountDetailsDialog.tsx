import React from "react";

import { AGENT_LABEL, isAgentUnavailable } from "../agents";
import { HelpTip } from "../components/HelpTip";
import { InlineReference } from "../components/InlineReference";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { MarkedPath } from "../components/MarkedPath";
import { Dialog } from "../dialogs/Dialog";
import { Message, useT } from "../i18n/react";
import type { Agent } from "../protocol";
import { AccountQuotaSummary, LoginStateBadge, NoUsageReading, QuotaFailureMark, QuotaPlan, QuotaUpdated } from "../quota/AccountQuotaSummary";
import { loginStateOf } from "../quota/quotaFormat";
import { useAccountQuota } from "../quota/useAccountQuota";
import { useDaemonStore } from "../store";
import { AccountHeading, useAccountDialogSubject } from "./accountDialogSubject";
import { AccountLoginAnnouncer, useSucceededAccountLogin } from "./AccountLogin";

/** A titled part of the dialog, the title standing over what it holds. */
function Part({ title, aside, children }: { title: React.ReactNode; aside?: React.ReactNode; children: React.ReactNode }): React.ReactElement {
  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <div className="flex items-center gap-1">
        <h3 className="text-sm font-medium text-foreground">{title}</h3>
        {aside}
      </div>
      {children}
    </div>
  );
}

/**
 * One account's details in a modal of its own, opened from its row's actions menu (`account` null for the agent's
 * default account): what the row shows in full and what it leaves out, read-only. The title is plain ("Account
 * details"); the body's first line names the account, with its plan and login state after the name, the email under it
 * on a line of its own, and the next says where its configuration lives, the default account's explanation after the
 * path. Then the account's quota as the rail's panel draws it (the two-layer 5-hour and weekly meter, each model's
 * window after it) with when it was read. It holds no control but the header's close button, which Escape and a click
 * outside do the same as; the account's switches are in `AccountSettingsDialog`, and logging in is the row's and the
 * menu's. It reads the account from the store as it is now, so a change from another window shows, and closes if the
 * account is removed.
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
  if (gone) return null;
  const path = stored ? stored.config_dir : defaultDir;
  const known = loginStateOf(quota) !== undefined;
  const showsUsage = quota === "loading" || (typeof quota === "object" && (quota.windows.length > 0 || loginStateOf(quota) === "out"));
  return (
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
            <Message id="settings.accounts.notUsable" params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[agent]}</InlineReference>, ...PATH_VAR_PARAMS }} />
          </div>
        )}
        <AccountLoginAnnouncer login={succeeded} name={name} />
        <Part title={t("settings.accounts.table.usage")} aside={<QuotaUpdated quota={quota} />}>
          {showsUsage ? <AccountQuotaSummary quota={quota} single /> : <NoUsageReading />}
        </Part>
      </div>
    </Dialog>
  );
}
