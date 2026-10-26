import { AlertDialog } from "@heroui/react";
import React, { useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";

import { Button } from "../components/Button";
import { appConfig } from "../appConfig";
import { ExternalLink } from "../components/ExternalLink";
import { recoveryAction } from "../components/ErrorGuidance";
import { InlineReference } from "../components/InlineReference";
import { DiagnosticBlock } from "../components/DiagnosticBlock";
import { errorAlerts } from "../errorAlerts";
import { diagnosticText } from "../errorText";
import { Message, useT } from "../i18n/react";
import { useOptionalPlatform } from "../platform/react";
import { openSettingsFromAnywhere } from "../settings/settingsLauncher";
import { DaemonProvider, type Daemon } from "../store";
import { dialogAround } from "./dialogElement";
import { Dialog } from "./Dialog";
import { ErrorAlertBody, alertTitle } from "./ErrorAlertBody";

export const issueUrl = appConfig.issueUrl;

/** What an alert about an unexpected failure offers for reporting it: the sentence asking for the report (or, where
 * the platform cannot open links, naming the address to report at) and the Report issue link for the footer, which is
 * absent where the address is only named. A link that cannot be opened is itself reported. */
export function useIssueReport(): { guidance: React.ReactElement; link: React.ReactElement | null } {
  const t = useT();
  const platform = useOptionalPlatform();
  const reporting = useRef(false);
  const [reportPending, setReportPending] = useState(false);
  const manualIssue = !platform?.links && platform?.kind === "tauri";
  const report = async () => {
    if (reporting.current) return;
    reporting.current = true;
    setReportPending(true);
    try {
      if (!platform?.links) throw new Error("External links are unavailable.");
      await platform.links.open(issueUrl);
    } catch (cause) {
      // Not unexpected, so no diagnostics are worked out for it: what the system said is its "Error details",
      // the message alone, since a stack of the UI's own code says nothing about why the browser did not open.
      const reason = cause instanceof Error ? cause.message : diagnosticText(cause);
      errorAlerts.report({
        message: t("error.issue.openFailed"),
        error: cause,
        recovery: "issueLink",
        content: <><p>{t("error.issue.openFailed")}</p><p dir="ltr" className="font-mono [overflow-wrap:anywhere]">{issueUrl}</p></>,
        details: reason ? <DiagnosticBlock>{reason}</DiagnosticBlock> : undefined,
      }, { immediate: true });
    } finally {
      reporting.current = false;
      setReportPending(false);
    }
  };
  return {
    guidance: manualIssue
      ? <Message id="error.unexpected.manualAddressGuidance" params={{ address: <InlineReference monospace wrap>{issueUrl}</InlineReference> }} />
      : <Message id="error.unexpected.reportGuidance" params={{ report: <InlineReference>{t("error.issue.report")}</InlineReference> }} />,
    link: manualIssue ? null : <ExternalLink href={issueUrl} appearance="button" onOpen={platform?.links ? report : undefined} isPending={reportPending}>{t("error.issue.report")}</ExternalLink>,
  };
}

/** Mounted outside the application boundary, so startup and render failures can also be acknowledged. */
export function ErrorAlerts({ daemon }: { daemon?: Daemon }): React.ReactElement | null {
  const t = useT();
  const [error] = useSyncExternalStore(errorAlerts.subscribe, errorAlerts.getSnapshot);
  const acknowledge = useRef<HTMLButtonElement>(null);
  const issue = useIssueReport();
  useLayoutEffect(() => { dialogAround(acknowledge)?.focus(); }, [error?.id]);
  if (!error) return null;
  const unexpected = error.classification.kind === "unexpected";
  // A button for the remedy when it is the one remedy of every failure the alert is about.
  const action = error.sharedRecovery ? recoveryAction(t, error.sharedRecovery) : undefined;
  const dialog = (
    <Dialog
      key={error.id}
      alert
      dismissable={false}
      headerLeading={<AlertDialog.Icon status="danger" />}
      title={alertTitle(t, error)}
      onClose={() => {}}
      resetKey={error.message}
      footer={<>
        {unexpected && issue.link}
        {action && (
          <Button variant="secondary" onPress={() => {
            errorAlerts.acknowledge();
            openSettingsFromAnywhere("accounts", { addAgent: action.addAgent });
          }}>{action.label}</Button>
        )}
        <Button ref={acknowledge} variant="danger" onPress={() => errorAlerts.acknowledge()}>{t("error.acknowledge")}</Button>
      </>}
    >
      <ErrorAlertBody alert={error} issueGuidance={issue.guidance} />
    </Dialog>
  );
  return daemon ? <DaemonProvider value={daemon}>{dialog}</DaemonProvider> : dialog;
}
