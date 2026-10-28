import React, { useEffect, useRef } from "react";

import { AGENT_LABEL, defaultAccountName, isAgentUnavailable } from "../agents";
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
import { accountDisabled } from "../accountChoices";
import { useDaemonStore } from "../store";
import { ringForAutomaticFocus } from "../touchInput";
import { AccountDisableSwitch } from "./AccountDisableSwitch";
import { AccountLoginAnnouncer, accountLoginOf } from "./AccountLogin";
import { AccountLoginButton, type ReturnLoginFocus } from "./AccountLoginButton";
import { AccountSharingSwitch } from "./AccountSharingSwitch";
import { SettingRow } from "./SettingRow";

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

/** The Disable account row: its description says what the switch does now, off or on, following the daemon's recorded
 * choice as the switch does. */
function DisableAccountRow({ agent, account, name }: { agent: Agent; account: string | null; name: string }): React.ReactElement {
  const t = useT();
  const disabled = useDaemonStore((s) => accountDisabled(s.settings.accounts, s.settings.disabled_default_accounts, agent, account));
  return (
    <SettingRow label={t("settings.accounts.disableAccount")} description={t(disabled ? "settings.accounts.disableHiddenDescriptionOnInUse" : "settings.accounts.disableHiddenDescriptionOffInUse")}>
      <AccountDisableSwitch agent={agent} account={account} name={name} label="settings.accounts.disableAccountLabel" />
    </SettingRow>
  );
}

/**
 * One account's details in a modal of its own, opened from its row's actions menu (`account` null for the agent's
 * default account): everything the row shows in full and what it leaves out. The title is plain ("Account settings");
 * the body's first line names the account, with its plan and login state and the Log in icon (while it is not logged
 * in) after the name, and the next says where its configuration lives, the default account's explanation after the
 * path. Then the account's quota as the rail's panel draws it (the two-layer 5-hour and weekly meter, each model's
 * window after it) with when it was read, and the account's settings as the Settings page's rows are: for an account
 * the user owns, the Share default configuration switch with the lock reason, the progress and the unfinished-change
 * warning it carries, and last the Disable account switch, whose description follows the switch (off or on). The row
 * menu's Settings entry opens it. It has no footer: the header's close button, Escape and a click outside close it. It
 * reads the account from the store as it is now, so a change from another window shows, and closes if the account is
 * removed. `focusSharing` puts focus on the sharing switch on opening, for a request that Settings show that switch.
 */
export function AccountDetailsDialog({ agent, account, focusSharing = false, onLogin, onClose }: {
  agent: Agent;
  account: string | null;
  focusSharing?: boolean;
  /** Opens the login dialog, handed what returns focus here when it closes. */
  onLogin: (returnFocus: ReturnLoginFocus) => void;
  onClose: () => void;
}): React.ReactElement | null {
  const t = useT();
  const stored = useDaemonStore((s) => (account === null ? null : s.settings.accounts.find((held) => held.id === account) ?? null));
  const defaultName = useDaemonStore((s) => defaultAccountName(t, s.settings.default_account_names, agent));
  const defaultDir = useDaemonStore((s) => s.agentAvailability.get(agent)?.default_account_dir);
  const unusable = useDaemonStore((s) => isAgentUnavailable(s.agentAvailability, agent));
  const login = useDaemonStore((s) => accountLoginOf(s.accountLogins, agent, account));
  const quota = useAccountQuota(agent, account);
  const body = useRef<HTMLDivElement>(null);
  const gone = account !== null && stored === null;
  useEffect(() => {
    if (gone) onClose();
  }, [gone]);
  useEffect(() => {
    if (!focusSharing || !stored) return;
    // The switch is there once the dialog has mounted its body, a frame on.
    const frame = requestAnimationFrame(() => {
      const control = body.current?.querySelector<HTMLElement>('[data-sharing-account] input[role="switch"]');
      if (!control) return;
      control.scrollIntoView?.({ block: "center" });
      // Focus lands from a press in another dialog, so the destination shows its ring.
      ringForAutomaticFocus();
      control.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, []);
  if (gone) return null;
  const name = stored?.name ?? defaultName;
  const path = stored ? stored.config_dir : defaultDir;
  const known = loginStateOf(quota) !== undefined;
  const showsUsage = quota === "loading" || (typeof quota === "object" && (quota.windows.length > 0 || loginStateOf(quota) === "out"));
  return (
    <Dialog
      size="lg"
      title={t("settings.accounts.details.title")}
      onClose={onClose}
      footer={null}
    >
      <div ref={body} className="flex flex-col gap-4">
        {/* The title is a plain name, so what the badges qualify is the account named here, ahead of them. */}
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm">
          <span dir="auto" className="min-w-0 font-semibold text-foreground wrap-anywhere">{name}</span>
          <QuotaPlan quota={quota} />
          {known
            ? <LoginStateBadge quota={quota} />
            : <span className="text-app-muted">{t(quota === "loading" ? "settings.accounts.loginChecking" : "settings.accounts.loginUnknown")}</span>}
          <QuotaFailureMark quota={quota} />
          <AccountLoginButton agent={agent} account={account} name={name} quota={quota} onLogin={onLogin} fallback={() => body.current?.querySelector<HTMLElement>('input[role="switch"]')} />
        </div>
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
        <AccountLoginAnnouncer login={login?.status === "succeeded" ? login : undefined} name={name} />
        <Part title={t("settings.accounts.table.usage")} aside={<QuotaUpdated quota={quota} />}>
          {showsUsage ? <AccountQuotaSummary quota={quota} single /> : <NoUsageReading />}
        </Part>
        <div>
          {stored && <AccountSharingSwitch account={stored} opensFocused={focusSharing} />}
          <DisableAccountRow agent={agent} account={account} name={name} />
        </div>
      </div>
    </Dialog>
  );
}
