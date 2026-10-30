import React, { useEffect, useId, useState } from "react";

import { AGENT_LABEL } from "../agents";
import { DaemonRequestError } from "../daemon-client";
import { ErrorAlert } from "../components/ErrorAlert";
import { FailureAlert } from "../components/FailureAlert";
import { InlineReference } from "../components/InlineReference";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { ConfirmConsequences } from "../dialogs/ConfirmConsequences";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { ConfirmOption } from "../dialogs/ConfirmOption";
import { KeepFocusInDialog } from "../dialogs/KeepFocusInDialog";
import { useAfterErrorAlert } from "../dialogs/useAfterErrorAlert";
import { errorPresentation, type ErrorPresentation } from "../errorPresentation";
import { Message, useT } from "../i18n/react";
import { isLive, type Account, type AccountTrashOutcome } from "../protocol";
import { compareSessions, ordersByStatus } from "../sidebar/order";
import { useDaemon, useDaemonStore } from "../store";

/** What removing account `accountId` with its directory moved to the Trash would be let or refused now, asked afresh
 * each time `asking` turns on (the option ticked) and each time `reinspect` is called, so what changed while it was off
 * or since the last answer is found. An answer to an earlier question, or to one that was dropped by unticking, is
 * ignored. `pending` while the answer is on its way. A failed inspection stays as `failure` while `retry` has asked
 * again, so what shows it and the Try again that retried are not taken away under focus; `answered` is the attempt that
 * the shown answer is of, for the failure to be announced again when a retry fails in the same words. */
function useAccountTrashInspection(accountId: string, asking: boolean): { pending: boolean; outcome?: AccountTrashOutcome; failure?: ErrorPresentation; answered?: number; retry: () => void; reinspect: () => void } {
  const t = useT();
  const { request } = useDaemon();
  const [tick, setTick] = useState(0);
  const [attempt, setAttempt] = useState(0);
  const [answer, setAnswer] = useState<{ tick: number; attempt: number; outcome?: AccountTrashOutcome; failure?: ErrorPresentation }>();
  const [asked, setAsked] = useState(false);
  if (asking !== asked) {
    setAsked(asking);
    if (asking) {
      setTick(tick + 1);
      setAttempt(0);
    }
  }
  useEffect(() => {
    if (!asking) return;
    let active = true;
    request({ type: "inspect_account_trash", account: accountId }).then(
      (result) => active && setAnswer(result.type === "account_trash_inspected" ? { tick, attempt, outcome: result.outcome } : { tick, attempt, failure: errorPresentation(new Error(t("error.unexpectedReply"))) }),
      (err: unknown) => active && setAnswer({ tick, attempt, failure: errorPresentation(err) }),
    );
    return () => { active = false; };
  }, [accountId, request, asking, tick, attempt]);
  const current = asking && answer?.tick === tick ? answer : undefined;
  return {
    pending: asking && current?.attempt !== attempt,
    outcome: current?.outcome,
    failure: current?.failure,
    answered: current?.attempt,
    retry: () => setAttempt((value) => value + 1),
    reinspect: () => {
      if (!asking) return;
      setTick((value) => value + 1);
      setAttempt(0);
    },
  };
}

/** The refusals of removing an account with its directory that the directory's own state may decide, which the inspection
 * answers too: after one, the inspection is asked again, so the reason stands in the dialog once the error is closed.
 * `account_directory_trash_failed` also covers a failed move, which the inspection may find nothing wrong with, so it
 * stands for the same only as far as the inspection can say (`asksAgain`). `account_has_running_sessions` is not among
 * them: the dialog lists those sessions live. */
const DIRECTORY_REFUSALS = new Set(["account_directory_shared", "account_directory_protected", "account_directory_trash_failed"]);

/** Whether the inspection is asked again after the removal was refused with `err`: not for a login that did not end
 * (`signing_in`), which the inspection does not look at, so asking again would only repeat an answer it gave before. */
const asksAgain = (err: DaemonRequestError): boolean => DIRECTORY_REFUSALS.has(err.code) && err.params.cause !== "signing_in";

/** Why the directory cannot be moved to the Trash, said only as the cause, or nothing when `outcome` has none. */
function trashRefusal(outcome: AccountTrashOutcome | undefined): React.ReactElement | undefined {
  switch (outcome?.status) {
    case "protected":
      return <Message id="settings.accounts.removeTrashRefused.protected" />;
    case "shared":
      return outcome.kind === "account"
        ? <Message id="settings.accounts.removeTrashRefused.sharedAccount" params={{ agent: <InlineReference dir="auto" wrap>{outcome.agent ?? ""}</InlineReference>, name: <InlineReference dir="auto" wrap>{outcome.name ?? ""}</InlineReference> }} />
        : outcome.kind === "project"
          ? <Message id="settings.accounts.removeTrashRefused.sharedProject" params={{ name: <InlineReference dir="auto" wrap>{outcome.name ?? ""}</InlineReference> }} />
          : <Message id="settings.accounts.removeTrashRefused.sharedDefaultAccount" params={{ agent: <InlineReference dir="auto" wrap>{outcome.agent ?? ""}</InlineReference> }} />;
    case "unlookable":
      return <Message id={outcome.cause === "unreadable" ? "settings.accounts.removeTrashRefused.unreadable" : outcome.cause === "other_unlooked" ? "settings.accounts.removeTrashRefused.otherUnlooked" : "settings.accounts.removeTrashRefused.unlookable"} />;
    default:
      return undefined;
  }
}

/** Confirms removing an account the user owns. Its directory stays on disk unless the user opts in to moving it to the
 * Trash, which the daemon does before removing the account and refuses (keeping the account) when it cannot. With the
 * option ticked, the sessions of the account that are open or starting are listed and Confirm is disabled until none is
 * left, as the daemon refuses the removal while any is: they run on that directory. The daemon is asked each time the
 * option is ticked whether the directory may go, and Confirm is held while that is out and disabled while it says no
 * (or could not say), with the reason under the option: the removal is prevented rather than refused. The refusals the
 * daemon still answers the removal with are for what changed since. */
export function RemoveAccountDialog({ account, onRemoved, onCancel }: { account: Account; onRemoved: () => void; onCancel: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const [trash, setTrash] = useState(false);
  const sessions = useDaemonStore((s) => s.sessions);
  const byStatus = useDaemonStore((s) => ordersByStatus(s.settings));
  // Read live: a session that starts or ends while the dialog is open joins or leaves the list.
  const running = Array.from(sessions.values())
    .filter((s) => s.agent === account.agent && isLive(s.status) && (s.account_id === account.id || s.config_dir === account.config_dir))
    .sort((a, b) => compareSessions(a, b, byStatus));
  const inspection = useAccountTrashInspection(account.id, trash);
  const afterErrorAlert = useAfterErrorAlert();
  const alertId = useId();
  const runningId = useId();
  const refusal = trash ? trashRefusal(inspection.outcome) : undefined;
  const problem = trash && (refusal !== undefined || inspection.failure !== undefined);
  const blocked = trash && running.length > 0;
  const agent = <InlineReference dir="auto" wrap>{AGENT_LABEL[account.agent]}</InlineReference>;
  return (
    <ConfirmDialog
      title={t("settings.accounts.removeTitle")}
      errorTitle={t("settings.accounts.removeFailedTitle")}
      subject={<Message id="settings.accounts.removeSubject" params={{ name: <InlineReference dir="auto" wrap>{account.name}</InlineReference> }} />}
      message={<p><Message id={trash && inspection.outcome?.status !== "missing" ? "settings.accounts.removeMessageTrash" : "settings.accounts.removeMessage"} params={{ agent }} /></p>}
      consequences={blocked ? <ConfirmConsequences title={t("settings.accounts.removeRunning", { count: running.length })} titleId={runningId} announceTitle sessions={running} /> : undefined}
      confirmLabel={t("common.remove")}
      destructive
      confirmDisabled={blocked || problem}
      waiting={trash && inspection.pending}
      resetKey={account.id}
      extraControl={(busy) => (
        <>
          <ConfirmOption
            isSelected={trash}
            onChange={setTrash}
            isReadOnly={busy}
            label={t("settings.accounts.removeTrash")}
            hint={t(trash && inspection.outcome?.status === "missing" ? "settings.accounts.removeTrashHintMissing" : "settings.accounts.removeTrashHint")}
            describedBy={[problem ? alertId : undefined, blocked ? runningId : undefined].filter(Boolean).join(" ") || undefined}
          />
          {/* A reason the removal cannot go ahead is an Alert under the option, announced as it appears. */}
          {trash && refusal !== undefined && (
            <ErrorAlert textId={alertId} status="warning" announce announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} title={refusal} />
          )}
          {/* A retry that succeeds takes the alert, and the Try again focus was on, away. */}
          <KeepFocusInDialog>
            {trash && inspection.failure !== undefined && (
              <FailureAlert
                textId={alertId}
                title={t("settings.accounts.removeTrashInspectFailed")}
                message={inspection.failure}
                onRetry={inspection.retry}
                pending={inspection.pending}
                attempt={inspection.answered}
                announce
                announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}
              />
            )}
          </KeepFocusInDialog>
        </>
      )}
      onConfirm={async () => {
        try {
          await request({ type: "delete_account", account: account.id, trash_directory: trash });
        } catch (err) {
          // What the directory is now is asked again once the alert has closed, so the reason stands under the option, and
          // is announced and handed focus (Remove to Cancel) with the dialog back in view.
          if (trash && err instanceof DaemonRequestError && asksAgain(err)) afterErrorAlert(inspection.reinspect);
          throw err;
        }
        onRemoved();
      }}
      onCancel={onCancel}
    />
  );
}
