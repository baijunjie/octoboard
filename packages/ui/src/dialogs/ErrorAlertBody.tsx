import type React from "react";

import { DiagnosticBlock } from "../components/DiagnosticBlock";
import { DetailsDisclosure } from "../components/ErrorDetailsDisclosure";
import { RecoveryGuidance } from "../components/ErrorGuidance";
import type { ErrorAlertView } from "../errorAlerts";
import { diagnosticText } from "../errorText";
import type { useT } from "../i18n/react";

/** The title an error alert carries: the one its request names, else the one for its kind of failure. */
export function alertTitle(t: ReturnType<typeof useT>, alert: ErrorAlertView): string | React.ReactElement {
  return alert.title ?? t(alert.classification.kind === "unexpected" ? "error.unexpected.title" : "error.title");
}

/** What an error alert says in its body: the explanation, the remedy or the report guidance, and the raw text behind
 * "Error details". The error alert dialog and a dialog that turns into a failure modal share it. `issueGuidance` is the
 * sentence asking for a report of an unexpected failure (see `useIssueReport`), which a failure modal, never about an
 * unexpected failure, has no need of. */
export function ErrorAlertBody({ alert, issueGuidance }: { alert: ErrorAlertView; issueGuidance?: React.ReactNode }): React.ReactElement {
  const diagnostics = [alert.error, ...(alert.failures?.map((failure) => failure.error) ?? [])].map(diagnosticText).filter((detail): detail is string => !!detail && detail !== alert.message);
  return (
    <>
      <div className="min-w-0 whitespace-pre-wrap [overflow-wrap:anywhere]">{alert.content ?? alert.message}</div>
      {alert.classification.kind === "unexpected"
        ? <>
          {issueGuidance && <p>{issueGuidance}</p>}
          {alert.details === undefined && diagnostics.length > 0 && <DetailsDisclosure><DiagnosticBlock>{[...new Set(diagnostics)].join("\n\n")}</DiagnosticBlock></DetailsDisclosure>}
        </>
        : !alert.failures?.length && !alert.remedyInBody && alert.classification.recovery !== "none" && <p><RecoveryGuidance recovery={alert.classification.recovery} /></p>}
      {alert.details ? <DetailsDisclosure>{alert.details}</DetailsDisclosure> : null}
    </>
  );
}
