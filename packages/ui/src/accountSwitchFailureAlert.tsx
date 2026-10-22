import { useId } from "react";

import { DaemonErrorMessage, ShellTimeoutCommand } from "./components/DaemonErrorMessage";
import { DiagnosticBlock } from "./components/DiagnosticBlock";
import { RecoveryGuidance } from "./components/ErrorGuidance";
import { InlineChip } from "./components/InlineChip";
import { DETAIL_BLOCK_CODES, messageCode, RECOVERY_CODES } from "./daemonMessage";
import { errorAlerts } from "./errorAlerts";
import { DaemonRequestError } from "./daemon-client";
import { hasCodeParams, hasShellTimeoutDetails } from "./errorPresentation";
import { additionalRecovery, classifyError, type ErrorRecovery } from "./errorRecovery";
import { errorText, unexpectedFailureDiagnostic } from "./errorText";
import { isMessageKey } from "./i18n/catalog";
import { t } from "./i18n/language";
import { Message, useT } from "./i18n/react";
import type { Session } from "./protocol";
import { sessionDisplayTitle } from "./sessionLabel";

/** A session whose switch failed, and why. */
export interface SwitchFailure {
  session: Session;
  error: DaemonRequestError;
}

/** Whether the catalog words this failure, so that its row can show the sentence rather than the raw message. */
function hasSentence(error: SwitchFailure["error"]): boolean {
  return isMessageKey(`daemon.${messageCode(error.code, error.params)}`);
}

/** Whether the failure is unexpected and has no sentence of the catalog's to show for it: its row says only that, and
 * its message goes to the details. */
function isUnexpectedWithoutSentence(error: SwitchFailure["error"]): boolean {
  return !hasSentence(error) && classifyError(error).kind === "unexpected";
}

/** What "Error details" holds for a failure, if anything: the diagnostic text the daemon keeps apart from the sentence,
 * a shell-environment failure's command, or what an unexpected failure with no sentence of its own said. A failure
 * whose message is the sentence already shown has none. */
function rawDetails(error: SwitchFailure["error"]): ((labelledBy: string) => React.ReactNode) | undefined {
  if (hasShellTimeoutDetails(error)) return () => <ShellTimeoutCommand params={error.params} />;
  const text = error.params.detail && DETAIL_BLOCK_CODES.has(error.code) ? error.params.detail : isUnexpectedWithoutSentence(error) ? unexpectedFailureDiagnostic(error) : undefined;
  return text ? (labelledBy) => <DiagnosticBlock labelledBy={labelledBy}>{text}</DiagnosticBlock> : undefined;
}

/** Whether the failure has a remedy sentence of its own (`daemon.<code>.recovery`), which says more than its kind's; a
 * shell-environment failure has one too. */
function hasSpecificRecovery(error: SwitchFailure["error"]): boolean {
  return hasShellTimeoutDetails(error) || (RECOVERY_CODES.has(error.code) && isMessageKey(`daemon.${error.code}.recovery`) && hasCodeParams(error));
}

function SessionTitle({ session }: { session: Session }): React.ReactElement {
  const t = useT();
  return <>{sessionDisplayTitle(t, session)}</>;
}

/** The short reason in a failed session's row: the catalog's sentence, or for an unexpected failure with none that
 * says so, the raw text staying in the details. */
function Reason({ error }: { error: SwitchFailure["error"] }): React.ReactElement {
  const t = useT();
  return <p>{isUnexpectedWithoutSentence(error) ? t("dialog.switchAccount.unexpectedReason") : <DaemonErrorMessage error={error} part="summary" />}</p>;
}

function SessionDetails({ session, content }: { session: Session; content: (labelledBy: string) => React.ReactNode }): React.ReactElement {
  const id = useId();
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <h3 id={id} dir="auto" className="text-sm font-semibold"><SessionTitle session={session} /></h3>
      {content(id)}
    </div>
  );
}

/** One modal error alert for a batch: what failed and where to, said once, each failed session with only the short
 * reason, then each remedy once (a code's own for the failures that have one, else their kind's), and every session's
 * raw details together behind "Error details", which is left out when no session has any. An unexpected failure adds
 * the alert's own report paragraph. */
export function reportSwitchFailures(failures: SwitchFailure[], accountName: string): void {
  const count = failures.length;
  // A code's own remedy is said once for the code, and the failures without one share their kind's, once.
  const specific = new Map<string, DaemonRequestError>();
  const generic = new Set<ErrorRecovery>();
  for (const { error } of failures) {
    const classification = classifyError(error);
    if (hasSpecificRecovery(error)) {
      if (!specific.has(error.code)) specific.set(error.code, error);
    } else if (classification.kind === "actionable" && classification.recovery !== "none") {
      generic.add(classification.recovery);
    }
    const also = additionalRecovery(error);
    if (also) generic.add(also);
  }
  // Saving an older item set aside comes before any other remedy: it is gone once its folder is cleared.
  const remedies = [...generic].sort((a, b) => Number(b === "notRestored") - Number(a === "notRestored"));
  const details = failures.flatMap(({ session, error }) => {
    const content = rawDetails(error);
    return content ? [{ session, content }] : [];
  });
  errorAlerts.report({
    // Plain text, which tells two batches to different accounts apart.
    message: [t("dialog.switchAccount.failed", { count, account: accountName }), ...failures.map(({ session, error }) => `${session.id}: ${errorText(error)}`)].join("\n"),
    title: <Message id="dialog.switchAccount.failed" params={{ count, account: <InlineChip dir="auto" wrap size="inherit">{accountName}</InlineChip> }} />,
    failures: failures.map(({ error }) => ({ error })),
    content: (
      <div className="flex min-w-0 flex-col gap-3 whitespace-normal">
        <p><Message id="dialog.switchAccount.failedIntro" params={{ count }} /></p>
        <ul className="flex flex-col gap-2">
          {failures.map(({ session, error }) => (
            <li key={session.id}>
              <p dir="auto" className="font-semibold"><SessionTitle session={session} /></p>
              <Reason error={error} />
            </li>
          ))}
        </ul>
        {[...specific].map(([code, error]) => <DaemonErrorMessage key={code} error={error} part="recovery" />)}
        {remedies.map((recovery) => <p key={recovery}><RecoveryGuidance recovery={recovery} /></p>)}
      </div>
    ),
    details: details.length ? details.map(({ session, content }) => <SessionDetails key={session.id} session={session} content={content} />) : null,
  });
}
