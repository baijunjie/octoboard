import React, { useEffect, useRef, useState } from "react";

import { Button } from "../components/controls/Button";
import { InlineReference } from "../i18n/InlineReference";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { focusFellToPage } from "../components/focus/regionFocus";
import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { ConfirmConsequences } from "../components/dialog/ConfirmConsequences";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { AccountQuota, ResetCreditOutcome } from "../protocol";
import { useDaemon } from "../store";
import { ringForAutomaticFocus } from "../input/touchInput";
import { useNow } from "../format/useNow";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { AccountQuotaSummary, QuotaStaleMark } from "./AccountQuotaSummary";
import { accountQuotaOf } from "../accounts/accountQuotaOf";
import { expiryLine, nextResetCredit, resetCreditsSummary, usageBarSummary, usageBarWindows } from "../accounts/quotaFormat";
import { useAccountQuota, type QuotaReading } from "./useAccountQuota";

/** An account's rate limit reset credits (Codex's "usage limit resets") in one line, with the action that redeems
 * one, beneath the account's quota windows: "1 usage limit reset available · expires in 6d 3h" and "Redeem reset".
 * Nothing for an account with no credit to use, which is every account but a Codex one holding at least one. `onUse`
 * opens `UseResetCreditDialog`, which the surface holds itself (a panel that closes when a modal opens cannot hold
 * it). `preventFocusOnPress` is for a surface that closes on a press here, so the press moves no focus into it. */
export function ResetCreditsLine({ quota, accountName, onUse, preventFocusOnPress }: {
  quota: QuotaReading;
  accountName: string;
  onUse: () => void;
  preventFocusOnPress?: boolean;
}): React.ReactElement | null {
  const t = useT();
  const now = useNow();
  if (typeof quota !== "object" || !quota.reset_credits) return null;
  const summary = resetCreditsSummary(t, quota.reset_credits, now);
  return (
    <div className="flex min-w-0 items-center gap-3 text-sm">
      <FadeOverflow className="min-w-0 flex-1 text-app-muted" titleWhenClipped={summary}>{summary}</FadeOverflow>
      <Button size="xs" variant="secondary" className="shrink-0" preventFocusOnPress={preventFocusOnPress} aria-label={t("quota.resetCredits.useFor", { account: isolate(accountName) })} onPress={onUse}>
        {t("quota.resetCredits.use")}
      </Button>
    </div>
  );
}

/** Hands focus on once `UseResetCreditDialog` has closed, for the surface that opened it: returns the function to call
 * when it closes, which two frames later (React Aria restores focus in the frame after the dialog unmounts) runs
 * `restore` if focus fell to the page, the way `useLoginFocusReturn` does for the login dialog. The user did not ask for
 * the move, so the destination shows its ring. Nothing runs once the surface is gone. */
export function useResetFocusReturn(restore: () => void): () => void {
  const frame = useRef(0);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);
  return () => {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => {
      frame.current = requestAnimationFrame(() => {
        if (!focusFellToPage()) return;
        ringForAutomaticFocus();
        restore();
      });
    });
  };
}

/** The notice of an outcome that ends the question at once; a use that worked is told in the dialog itself. */
const OUTCOME_NOTICE: Partial<Record<ResetCreditOutcome, "quota.resetCredits.nothingToReset" | "quota.resetCredits.noCredit">> = {
  nothing_to_reset: "quota.resetCredits.nothingToReset",
  no_credit: "quota.resetCredits.noCredit",
};

/** How long the dialog waits for the reading the daemon takes after a use before it stops: as long as a reading may
 * take (`PROBE_TIMEOUT` in the daemon). */
const READING_WAIT_MS = 60_000;

/** Where the dialog stands: asking; waiting for the reading that follows a use that worked (`frozen` is the reading the
 * meters keep showing meanwhile, `baseline` the one held when the use was answered, which the next reading must be newer
 * than); or finished, with whether the reading arrived (`updated`) or the wait ended without one. */
type Stage =
  | { kind: "ask" }
  | { kind: "waiting"; frozen: AccountQuota; baseline: AccountQuota }
  | { kind: "done"; updated: boolean; frozen: AccountQuota };

/** The confirmation of redeeming one of a Codex account's reset credits (`account` null for the default account): it
 * names the credit that would be used (Codex's own title and description when it gives them), when it expires, and
 * that it resets the account's eligible usage windows now, over the account's usage meters as they are. Confirming asks
 * the daemon to use it. A use that worked keeps the dialog open: the meters hold the values they had at the confirm
 * until a reading newer than the one held when the use was answered arrives, then fill to the new ones (the meter's
 * width transition, which reduced motion removes), and the dialog says the usage was reset, with Done as its only
 * action. A reading that fails, or does not arrive within `READING_WAIT_MS`, leaves the meters as they were (a failed
 * one marked out of date). The outcomes that used nothing are told in a notice and close the dialog; a failure is the
 * error alert's, with the dialog still open. Before it is used, the dialog reads the account's quota as it is now, so a
 * credit that is used up or expires meanwhile shows, and Confirm is held while none is left. Focus stays where it is
 * (Done after a use); what changes is said through a `StatusAnnouncer` that stands with the dialog, while the visible
 * status line is `aria-hidden`. */
export function UseResetCreditDialog({ account, accountName, onClose }: {
  account: string | null;
  accountName: string;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const now = useNow();
  const { request, toastNotice, store } = useDaemon();
  const quota = useAccountQuota("codex", account);
  const [stage, setStage] = useState<Stage>({ kind: "ask" });
  // The reading the confirm was pressed on, from then until the request fails: the daemon takes a used credit out of
  // the held reading before it replies, and what the dialog asks and says meanwhile must not follow that.
  const [pressed, setPressed] = useState<AccountQuota>();
  // The reading after the use is the first one newer than the one held when the use was answered, or a failure of the
  // next attempt, which keeps the older windows and marks them.
  useEffect(() => {
    if (stage.kind !== "waiting" || typeof quota !== "object") return;
    const { baseline, frozen } = stage;
    if (quota.fetched_at > baseline.fetched_at || (quota.unavailable !== null && baseline.unavailable === null)) {
      setStage({ kind: "done", updated: quota.unavailable === null, frozen });
    }
  }, [stage, quota]);
  // Ends when the reading arrives, which leaves the waiting stage.
  useEffect(() => {
    if (stage.kind !== "waiting") return;
    const timer = setTimeout(() => setStage((current) => current.kind === "waiting" ? { kind: "done", updated: false, frozen: current.frozen } : current), READING_WAIT_MS);
    return () => clearTimeout(timer);
  }, [stage.kind]);
  const asked = pressed ?? quota;
  const credits = typeof asked === "object" ? asked.reset_credits : undefined;
  const credit = credits ? nextResetCredit(credits, now) : undefined;
  const shown = stage.kind === "waiting" || (stage.kind === "done" && !stage.updated) ? stage.frozen : quota;
  const nameChip = <InlineReference dir="auto" wrap>{accountName}</InlineReference>;
  const confirm = async () => {
    // Confirm is held while no reading holds a credit, so there is a reading to freeze.
    if (typeof quota !== "object") return;
    setPressed(quota);
    const reply = await request({ type: "use_codex_reset_credit", account }).catch((error: unknown) => {
      setPressed(undefined);
      throw error;
    });
    if (reply.type !== "reset_credit_used") {
      setPressed(undefined);
      return;
    }
    const key = OUTCOME_NOTICE[reply.outcome];
    if (key === undefined) {
      const held = accountQuotaOf(store.getState().accountQuotas, "codex", account);
      setStage({ kind: "waiting", frozen: quota, baseline: typeof held === "object" ? held : quota });
      return;
    }
    toastNotice(t(key, { account: isolate(accountName) }), <Message id={key} params={{ account: nameChip }} />);
    onClose();
  };
  const updatedBar = stage.kind === "done" && stage.updated && typeof quota === "object" ? usageBarWindows(quota) : undefined;
  const statusText = stage.kind === "waiting"
    ? t("quota.resetCredits.updating")
    : updatedBar ? t("quota.resetCredits.updated") : t("quota.resetCredits.updateFailed");
  // Said once per change, with the new values as the meters' names and values; not at every frame of the fill. Each
  // write begins with that the reset worked, so that a user whose first write was coalesced with the next still hears it.
  const resetDone = t("quota.resetCredits.done", { account: isolate(accountName) });
  const sentences = (...parts: string[]) => parts.join(t("common.sentenceSeparator"));
  const announcement = stage.kind === "ask"
    ? credits ? undefined : t("quota.resetCredits.confirm.none", { account: isolate(accountName) })
    : stage.kind === "waiting"
      ? resetDone
      : updatedBar ? sentences(resetDone, statusText, usageBarSummary(t, updatedBar, now)) : sentences(resetDone, statusText);
  const meters = (
    <div className="flex flex-col gap-2">
      <AccountQuotaSummary quota={shown} single />
      {stage.kind !== "ask" && (
        <div className="flex items-center gap-1 text-sm text-app-muted">
          <span aria-hidden="true">{statusText}</span>
          {stage.kind === "done" && <QuotaStaleMark quota={quota} />}
        </div>
      )}
      <StatusAnnouncer text={announcement} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />
    </div>
  );
  const asking = stage.kind === "ask";
  return (
    <ConfirmDialog
      title={asking ? t("quota.resetCredits.confirm.title") : t("quota.resetCredits.doneTitle")}
      subject={!asking
        ? <Message id="quota.resetCredits.done" params={{ account: nameChip }} />
        : credits
          ? <Message id="quota.resetCredits.confirm.subject" params={{ account: nameChip }} />
          : <Message id="quota.resetCredits.confirm.none" params={{ account: nameChip }} />}
      message={asking && credit && (credit.title || credit.description || credit.expires_at !== null) ? (
        <div className="flex flex-col gap-1">
          {credit.title && <p dir="auto" className="font-medium">{credit.title}</p>}
          {credit.description && <p dir="auto" className="text-app-muted">{credit.description}</p>}
          {credit.expires_at !== null && <p className="text-app-muted">{expiryLine(t, credit.expires_at, now)}</p>}
        </div>
      ) : null}
      extraControl={meters}
      consequences={asking && credits ? <ConfirmConsequences title={t("quota.resetCredits.confirm.effect")} /> : undefined}
      confirmLabel={asking ? t("quota.resetCredits.confirm.action") : t("quota.resetCredits.doneAction")}
      pendingLabel={t("quota.resetCredits.confirm.pending")}
      hideCancel={!asking}
      confirmDisabled={asking && !credits}
      errorTitle={t("quota.resetCredits.failedTitle")}
      onConfirm={asking ? confirm : async () => onClose()}
      onCancel={onClose}
    />
  );
}
