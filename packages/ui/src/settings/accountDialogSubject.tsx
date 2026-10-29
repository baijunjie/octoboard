import React, { useEffect } from "react";

import { defaultAccountName } from "../agents";
import { useT } from "../i18n/react";
import type { Account, Agent } from "../protocol";
import type { QuotaReading } from "../quota/useAccountQuota";
import { useDaemonStore } from "../store";
import { AccountEmail } from "./AccountEmail";

/**
 * The account an account dialog is about (`account` null for the agent's default account), read from the store as it
 * is now so a change from another window shows. `gone` is true once the account has been removed; `onClose` is then
 * called, and the dialog renders nothing.
 */
export function useAccountDialogSubject(agent: Agent, account: string | null, onClose: () => void): { stored: Account | null; name: string; gone: boolean } {
  const t = useT();
  const stored = useDaemonStore((s) => (account === null ? null : s.settings.accounts.find((held) => held.id === account) ?? null));
  const defaultName = useDaemonStore((s) => defaultAccountName(t, s.settings.default_account_names, agent));
  const gone = account !== null && stored === null;
  useEffect(() => {
    if (gone) onClose();
  }, [gone]);
  return { stored, name: stored?.name ?? defaultName, gone };
}

/** The account's name with `children` after it on the first line, and the email it is signed in with, when its reading
 * gives one, muted on a line of its own under it, as in the accounts table's row. Names the account an account dialog
 * is about. */
export function AccountHeading({ name, quota, children }: { name: string; quota: QuotaReading; children?: React.ReactNode }): React.ReactElement {
  return (
    <div className="flex flex-col gap-0.5 text-sm">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span dir="auto" className="min-w-0 font-semibold text-foreground wrap-anywhere">{name}</span>
        {children}
      </div>
      <AccountEmail quota={quota} />
    </div>
  );
}
