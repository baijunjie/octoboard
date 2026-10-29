import React, { useEffect, useRef } from "react";

import { Dialog } from "../dialogs/Dialog";
import { useT } from "../i18n/react";
import type { Agent } from "../protocol";
import { useAccountQuota } from "../quota/useAccountQuota";
import { ringForAutomaticFocus } from "../touchInput";
import { AccountDisableSwitch } from "./AccountDisableSwitch";
import { AccountLoginAnnouncer, useSucceededAccountLogin } from "./AccountLogin";
import { AccountSharingSwitch } from "./AccountSharingSwitch";
import { AccountHeading, useAccountDialogSubject } from "./accountDialogSubject";
import { SettingRow } from "./SettingRow";
import { useAccountDisabled } from "./useAccountDisabled";

/** The Disable account row: its description says what the switch does now, off or on, following the daemon's recorded
 * choice as the switch does. */
function DisableAccountRow({ agent, account, name }: { agent: Agent; account: string | null; name: string }): React.ReactElement {
  const t = useT();
  const disabled = useAccountDisabled(agent, account);
  return (
    <SettingRow label={t("settings.accounts.disableAccount")} description={t(disabled ? "settings.accounts.disableHiddenDescriptionOnInUse" : "settings.accounts.disableHiddenDescriptionOffInUse")}>
      <AccountDisableSwitch agent={agent} account={account} name={name} label="settings.accounts.disableAccountLabel" />
    </SettingRow>
  );
}

/**
 * One account's settings in a modal of its own, opened from its row's actions menu (`account` null for the agent's
 * default account): the account's switches as the Settings page's rows are. The title is plain ("Account settings");
 * the body's first line names the account, with its email under it, so it is clear which one the switches are for.
 * Then, for an account the user owns, the Share default configuration switch with the lock reason, the progress and the
 * unfinished-change warning it carries, and last the Disable account switch, whose description follows the switch (off
 * or on). What the account is, its quota and where its configuration lives are in `AccountDetailsDialog`. It has no
 * footer: the header's close button, Escape and a click outside close it. It reads the account from the store as it is
 * now, so a change from another window shows, and closes if the account is removed. `focusSharing` puts focus on the
 * sharing switch on opening, for a request that Settings show that switch.
 */
export function AccountSettingsDialog({ agent, account, focusSharing = false, onClose }: {
  agent: Agent;
  account: string | null;
  focusSharing?: boolean;
  onClose: () => void;
}): React.ReactElement | null {
  const t = useT();
  const { stored, name, gone } = useAccountDialogSubject(agent, account, onClose);
  const succeeded = useSucceededAccountLogin(agent, account);
  const quota = useAccountQuota(agent, account);
  const body = useRef<HTMLDivElement>(null);
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
  return (
    <Dialog
      size="lg"
      title={t("settings.accounts.details.title")}
      onClose={onClose}
      footer={null}
    >
      <div ref={body} className="flex flex-col gap-4">
        <AccountHeading name={name} quota={quota} />
        <AccountLoginAnnouncer login={succeeded} name={name} />
        <div>
          {stored && <AccountSharingSwitch account={stored} opensFocused={focusSharing} />}
          <DisableAccountRow agent={agent} account={account} name={name} />
        </div>
      </div>
    </Dialog>
  );
}
