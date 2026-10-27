import { AlertDialog } from "@heroui/react";
import React, { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";
import { useShallow } from "zustand/react/shallow";

import { Button } from "../components/Button";
import { appConfig } from "../appConfig";
import { ExternalLink } from "../components/ExternalLink";
import { recoveryAction } from "../components/ErrorGuidance";
import { InlineReference } from "../components/InlineReference";
import { DiagnosticBlock } from "../components/DiagnosticBlock";
import { alertFailures, errorAlerts, type ReportableFailure } from "../errorAlerts";
import { diagnosticText } from "../errorText";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { issueReportUrl, recordNames } from "../issueReport";
import { useOptionalPlatform } from "../platform/react";
import { openSettingsFromAnywhere } from "../settings/settingsLauncher";
import { DaemonProvider, useErrorRecords, type Daemon } from "../store";
import { dialogAround } from "./dialogElement";
import { Dialog } from "./Dialog";
import { ErrorAlertBody, alertTitle } from "./ErrorAlertBody";

export const issueUrl = appConfig.issueUrl;

const NO_NAMES: string[] = [];

/** What an alert about an unexpected failure offers for reporting it: the sentence asking for the report (or, where
 * the platform cannot open links, naming the address to report at) and the Report issue link for the footer, which is
 * absent where the address is only named. `failures` are those of an unexpected failure to report, or `undefined`
 * where it is not one. The link opens the Issue page prefilled with a description of them from which the user's
 * personal information is removed (see `issueReportUrl`), or the bare address with no `failures`. The address named
 * to report at, and shown for a link that cannot be opened, is always the bare one, which can be read and typed. A link
 * that cannot be opened is itself reported. */
export function useIssueReport(failures: readonly ReportableFailure[] | undefined): { guidance: React.ReactElement; link: React.ReactElement | null } {
  const t = useT();
  const platform = useOptionalPlatform();
  const language = useCurrentLanguage();
  const reporting = useRef(false);
  const [reportPending, setReportPending] = useState(false);
  const [version, setVersion] = useState<string>();
  const unexpected = failures !== undefined;
  useEffect(() => {
    if (!unexpected) return;
    let current = true;
    platform?.updater?.currentVersion().then((found) => { if (current) setVersion(found); }, () => {});
    return () => { current = false; };
  }, [unexpected, platform]);
  // Walking every record is only worth it for a failure that is going to be reported.
  const names = useErrorRecords(useShallow((state) => (unexpected ? recordNames(state) : NO_NAMES)));
  const homeDir = useErrorRecords((state) => state.homeDir);
  const reportUrl = failures
    ? issueReportUrl(issueUrl, {
      failures,
      environment: { version, platform: platform?.kind, userAgent: typeof navigator === "undefined" ? undefined : navigator.userAgent, language },
      context: { names, homeDir },
    })
    : issueUrl;
  const manualIssue = !platform?.links && platform?.kind === "tauri";
  const report = async () => {
    if (reporting.current) return;
    reporting.current = true;
    setReportPending(true);
    try {
      if (!platform?.links) throw new Error("External links are unavailable.");
      await platform.links.open(reportUrl);
    } catch (cause) {
      // Not unexpected, so no diagnostics are worked out for it: what the system said is its "Error details",
      // the message alone, since a stack of the UI's own code says nothing about why the browser did not open.
      const reason = cause instanceof Error ? cause.message : diagnosticText(cause);
      errorAlerts.report({
        message: t("error.issue.openFailed"),
        title: t("error.issue.openFailedTitle"),
        error: cause,
        recovery: "issueAddress",
        content: <div className="flex flex-col gap-4"><p>{t("error.issue.openFailed")}</p><p dir="ltr" className="font-mono [overflow-wrap:anywhere]">{issueUrl}</p></div>,
        details: reason ? <DiagnosticBlock>{reason}</DiagnosticBlock> : undefined,
      }, { immediate: true });
    } finally {
      reporting.current = false;
      setReportPending(false);
    }
  };
  return {
    guidance: manualIssue
      ? <Message id="error.unexpected.manualReportGuidance" params={{ address: <InlineReference monospace wrap>{issueUrl}</InlineReference> }} />
      : <Message id="error.unexpected.prefilledReportGuidance" params={{ report: <InlineReference>{t("error.issue.report")}</InlineReference> }} />,
    link: manualIssue ? null : <ExternalLink href={reportUrl} appearance="button" onOpen={platform?.links ? report : undefined} isPending={reportPending}>{t("error.issue.report")}</ExternalLink>,
  };
}

/** Mounted outside the application boundary, so startup and render failures can also be acknowledged. */
export function ErrorAlerts({ daemon }: { daemon?: Daemon }): React.ReactElement {
  const host = <ErrorAlertHost />;
  // The report reads the daemon's records, so it is inside the provider too.
  return daemon ? <DaemonProvider value={daemon}>{host}</DaemonProvider> : host;
}

function ErrorAlertHost(): React.ReactElement | null {
  const t = useT();
  const [error] = useSyncExternalStore(errorAlerts.subscribe, errorAlerts.getSnapshot);
  const acknowledge = useRef<HTMLButtonElement>(null);
  const issue = useIssueReport(error?.classification.kind === "unexpected" ? alertFailures(error, "report") : undefined);
  // Focus the alert in the render after the one that mounted it, before the task that raised it ends, and nowhere
  // else: its `Dialog` never puts focus on it by itself, not even when the alert it replaces leaves focus on `<body>`.
  // react-aria turns a new modal's focus containment on from a layout effect at mount, which forces a render; the state
  // set here from a layout effect of the same commit is set after it and lands in that render or a later one, and a
  // layout effect of that render runs after the containment's own, which sit below it. Focus must land after that:
  // a dialog under the alert that has no focused element of its own yet (one opened by a press without a pointer,
  // whose own opening focus react-aria's `focusSafely` defers to the next frame or past the open transition) treats
  // focus on the not-yet-containing alert as escaping its containment and queues `focusSafely` on the alert's first
  // control, where Enter would then press it (for Manage accounts…, opening Settings). It must also land before the
  // next frame, because that dialog's deferred opening focus fires then unless focus has moved: once it lands on the
  // dialog, the alert is outside the containment it joins and focus is taken back from it.
  const [mountedId, setMountedId] = useState<number>();
  useLayoutEffect(() => { setMountedId(error?.id); }, [error?.id]);
  useLayoutEffect(() => {
    if (mountedId !== undefined) dialogAround(acknowledge)?.focus();
  }, [mountedId]);
  if (!error) return null;
  const unexpected = error.classification.kind === "unexpected";
  // A button for the remedy when it is the one remedy of every failure the alert is about.
  const action = error.sharedRecovery ? recoveryAction(t, error.sharedRecovery) : undefined;
  return (
    <Dialog
      key={error.id}
      alert
      dismissable={false}
      headerLeading={<AlertDialog.Icon status="danger" />}
      title={alertTitle(t, error)}
      onClose={() => {}}
      focusesItselfOnOpen
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
}
