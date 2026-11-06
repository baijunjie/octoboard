import React, { useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { accountDisabled, quotaSignedOut } from "../accounts/choiceEntries";
import { canSwitchTo } from "./accountSwitchBatch";
import { InlineReference } from "../i18n/InlineReference";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { Session } from "../protocol";
import { accountQuotaOf } from "../accounts/accountQuotaOf";
import { quotaUsedUpMessage } from "../quota/quotaCopy";
import { exhaustedQuotaWindow, quotaNaming, quotaUsedUpSentence } from "../accounts/quotaFormat";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { sessionWithDescendants } from "../sessions/order";
import { holdsAccountSwitch } from "../sessions/sessionStatus";
import { useDaemon, useDaemonStore, useProviders, type State } from "../store";
import { ProviderName } from "../components/text/ProviderName";
import { useNow } from "../format/useNow";
import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { useConfirmIncompleteLaunch } from "./useConfirmIncompleteLaunch";
import { Radio, RadioGroup } from "../primitives";

/** Which sessions Confirm switches: the session with everything bound under it, or every session
 * of its agent in its console. */
type Scope = "session" | "console";

export function SwitchAccountDialog({
  session,
  account,
  accountName,
  onClose,
  onSwitchAccounts,
}: {
  session: Session;
  account: string | null;
  accountName: string;
  onClose: () => void;
  /** Starts switching `sessionIds` to `account`, called `accountName`, in the background and returns
   * at once; the dialog closes meanwhile, and the outcome is reported where the sessions are shown. */
  onSwitchAccounts: (sessionIds: string[], account: string | null, accountName: string) => void;
}): React.ReactElement {
  const t = useT();
  const { store } = useDaemon();
  const quotas = useDaemonStore((s) => s.accountQuotas);
  const providers = useProviders();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const disabledDefaults = useDaemonStore((s) => s.settings.disabled_default_accounts);
  const now = useNow();
  // The account being switched to may have been turned off, found not logged in, or have no quota left, since the dialog opened: the
  // daemon would refuse the switch either way, so Confirm says why in the error alert dialog and leaves the dialog
  // open.
  const disabled = accountDisabled(accounts, disabledDefaults, session.agent, account);
  const quota = accountQuotaOf(quotas, session.agent, account);
  const exhaustedBy = exhaustedQuotaWindow(quota, now);
  const signedOut = quotaSignedOut(quota);
  const [scope, setScope] = useState<Scope>("session");
  // A session an account switch already holds is left out: it is on its way, and cannot be claimed twice.
  const targetsIn = ({ sessions: all, queuedAccountSwitches }: Pick<State, "sessions" | "queuedAccountSwitches">, chosen: Scope): Session[] => {
    const eligible = (candidate: Session) => candidate.console_id === session.console_id
      && candidate.agent === session.agent && canSwitchTo(candidate, account) && !holdsAccountSwitch(candidate, queuedAccountSwitches);
    return chosen === "console"
      ? Array.from(all.values()).filter(eligible)
      : sessionWithDescendants(all.values(), all.get(session.id) ?? session).filter(eligible);
  };
  const counts = useDaemonStore(useShallow((s) => ({ session: targetsIn(s, "session").length, console: targetsIn(s, "console").length })));
  const { confirmLaunch, confirmation } = useConfirmIncompleteLaunch(onClose);
  const agent = <ProviderName agent={session.agent} />;
  const several = scope === "console" || counts.session > 1;
  return (
    <ConfirmDialog
      title={t("dialog.switchAccount.title")}
      subject={<Message id="dialog.switchAccount.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference>, account: <InlineReference dir="auto" wrap>{accountName}</InlineReference> }} />}
      message={<p>{t(several ? "dialog.switchAccount.messageAll" : session.has_conversation ? "dialog.switchAccount.message" : "dialog.switchAccount.messageFresh")}</p>}
      confirmLabel={t("dialog.switchAccount.confirm")}
      // Nothing left to switch in the chosen scope (its sessions began switching after the dialog opened).
      confirmDisabled={counts[scope] === 0}
      onCancel={onClose}
      errorTitle={t("dialog.switchAccount.failedTitle")}
      // The set is read once Confirm is pressed, and fixed from then on: sessions opened later never join it.
      onConfirm={() => {
        const refusal = disabled
          ? {
            message: t("accountChoice.disabled.refusal", { name: isolate(accountName) }),
            content: <Message id="accountChoice.disabled.refusal" params={{ name: <InlineReference dir="auto" wrap>{accountName}</InlineReference> }} />,
            recovery: "accountDisabled" as const,
          }
          : signedOut
            ? {
              message: t("accountChoice.signedOut.refusal", { name: isolate(accountName) }),
              content: <Message id="accountChoice.signedOut.refusal" params={{ name: <InlineReference dir="auto" wrap>{accountName}</InlineReference> }} />,
              recovery: "accountNotSignedIn" as const,
            }
            : quota && exhaustedBy && { message: quotaUsedUpSentence(t, quotaNaming(quota, providers), accountName, exhaustedBy, now), content: quotaUsedUpMessage(t, quotaNaming(quota, providers), accountName, exhaustedBy, now), recovery: "none" as const };
        if (refusal) {
          errorAlerts.report({ ...refusal, title: <Message id="dialog.switchAccount.failedTitle" />, details: null });
          return Promise.resolve();
        }
        return confirmLaunch(account, () => {
          onSwitchAccounts(targetsIn(store.getState(), scope).map((target) => target.id), account, accountName);
          onClose();
        });
      }}
      extraControl={
        <>
          <RadioGroup variant="secondary" label={t("dialog.switchAccount.scopeLabel")} hideLabel value={scope} onChange={(value) => setScope(value as Scope)}>
            <Radio value="session" description={t("dialog.switchAccount.countSession", { count: counts.session })}>
              {t("dialog.switchAccount.scopeSession")}
            </Radio>
            <Radio value="console" description={t("dialog.switchAccount.countConsole", { count: counts.console })}>
              {t("dialog.switchAccount.scopeConsole")}
            </Radio>
          </RadioGroup>
          <p className="text-sm text-app-muted"><Message id="dialog.switchAccount.eligibility" params={{ agent }} /></p>
          {/* Inside this dialog's tree, though it shows in its own layer: a focus scope that is not
              nested in the one beneath it is bounced back to it. */}
          {confirmation}
        </>
      }
    />
  );
}
