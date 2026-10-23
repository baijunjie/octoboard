import { Description, Label, Radio, RadioGroup } from "@heroui/react";
import React, { useState } from "react";

import { canSwitchTo } from "../accountSwitchBatch";
import { AGENT_LABEL } from "../agents";
import { ErrorAlert } from "../components/ErrorAlert";
import { InlineReference } from "../components/InlineReference";
import { Message, useT } from "../i18n/react";
import type { Session } from "../protocol";
import { accountQuotaOf } from "../quota/accountQuotaOf";
import { quotaUsedUpCopy } from "../quota/quotaCopy";
import { exhaustedQuotaWindow } from "../quota/quotaFormat";
import { sessionDisplayTitle } from "../sessionLabel";
import { sessionWithDescendants } from "../sidebar/order";
import { useDaemon, useDaemonStore } from "../store";
import { useNow } from "../useNow";
import { ConfirmDialog } from "./ConfirmDialog";
import { useConfirmIncompleteLaunch } from "./useConfirmIncompleteLaunch";

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
  const sessions = useDaemonStore((s) => s.sessions);
  const quotas = useDaemonStore((s) => s.accountQuotas);
  const now = useNow();
  // The account being switched to may have no quota left: the daemon refuses the switch, so Confirm is held.
  const quota = accountQuotaOf(quotas, session.agent, account);
  const exhaustedBy = exhaustedQuotaWindow(quota, now);
  const [scope, setScope] = useState<Scope>("session");
  // A session an account switch already holds is left out: it is on its way, and cannot be claimed twice.
  const eligible = (candidate: Session) => candidate.console_id === session.console_id
    && candidate.agent === session.agent && canSwitchTo(candidate, account) && candidate.switching_account !== true;
  const targetsIn = (all: Map<string, Session>, chosen: Scope): Session[] => (
    chosen === "console"
      ? Array.from(all.values()).filter(eligible)
      : sessionWithDescendants(all.values(), all.get(session.id) ?? session).filter(eligible)
  );
  const counts = { session: targetsIn(sessions, "session").length, console: targetsIn(sessions, "console").length };
  const { confirmLaunch, confirmation } = useConfirmIncompleteLaunch(onClose);
  const usedUp = exhaustedBy && quota && quotaUsedUpCopy(t, quota, accountName, exhaustedBy, now);
  const agent = <InlineReference dir="auto" wrap>{AGENT_LABEL[session.agent]}</InlineReference>;
  const several = scope === "console" || counts.session > 1;
  return (
    <ConfirmDialog
      title={<Message id="dialog.switchAccount.title" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference>, account: <InlineReference dir="auto" wrap>{accountName}</InlineReference> }} />}
      message={<>
        {usedUp && <ErrorAlert title={usedUp.title} announce>{usedUp.description}</ErrorAlert>}
        <p>{t(several ? "dialog.switchAccount.messageAll" : session.has_conversation ? "dialog.switchAccount.message" : "dialog.switchAccount.messageFresh")}</p>
      </>}
      confirmLabel={t("dialog.switchAccount.confirm")}
      // Nothing left to switch in the chosen scope (its sessions began switching after the dialog opened).
      confirmDisabled={exhaustedBy !== undefined || counts[scope] === 0}
      onCancel={onClose}
      // The set is read once Confirm is pressed, and fixed from then on: sessions opened later never join it.
      onConfirm={() => confirmLaunch(account, () => {
        onSwitchAccounts(targetsIn(store.getState().sessions, scope).map((target) => target.id), account, accountName);
        onClose();
      })}
      extraControl={
        <>
          <RadioGroup variant="secondary" value={scope} onChange={(value) => setScope(value as Scope)}>
            <Label className="sr-only">{t("dialog.switchAccount.scopeLabel")}</Label>
            <Radio value="session">
              <Radio.Content>
                <Radio.Control><Radio.Indicator /></Radio.Control>
                <Label>{t("dialog.switchAccount.scopeSession")}</Label>
              </Radio.Content>
              <Description>{t("dialog.switchAccount.countSession", { count: counts.session })}</Description>
            </Radio>
            <Radio value="console">
              <Radio.Content>
                <Radio.Control><Radio.Indicator /></Radio.Control>
                <Label>{t("dialog.switchAccount.scopeConsole")}</Label>
              </Radio.Content>
              <Description>{t("dialog.switchAccount.countConsole", { count: counts.console })}</Description>
            </Radio>
          </RadioGroup>
          <p className="text-sm text-muted"><Message id="dialog.switchAccount.eligibility" params={{ agent }} /></p>
          {/* Inside this dialog's tree, though it shows in its own layer: a focus scope that is not
              nested in the one beneath it is bounced back to it. */}
          {confirmation}
        </>
      }
    />
  );
}
