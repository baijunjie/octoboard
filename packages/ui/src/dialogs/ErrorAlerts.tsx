import { AlertDialog } from "@heroui/react";
import React, { useId, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";

import { Button } from "../components/Button";
import { appConfig } from "../appConfig";
import { CodeBlock } from "../components/CodeBlock";
import { ExternalLink } from "../components/ExternalLink";
import { RecoveryGuidance, recoveryAction } from "../components/ErrorGuidance";
import { InlineChip } from "../components/InlineChip";
import { FadedScroller, useFadeRegion } from "../components/scrollFade";
import { errorAlerts } from "../errorAlerts";
import { diagnosticText } from "../errorText";
import { Message, useT } from "../i18n/react";
import { useOptionalPlatform } from "../platform/react";
import { openSettingsFromAnywhere } from "../settings/settingsLauncher";
import { DaemonProvider, type Daemon } from "../store";
import { dialogAround } from "./dialogElement";
import { Dialog } from "./Dialog";

export const issueUrl = appConfig.issueUrl;

/** The "Error details" disclosure around details a caller laid out itself. */
function DetailsDisclosure({ children }: { children: React.ReactNode }): React.ReactElement {
  const t = useT();
  return (
    <details className="min-w-0 max-w-full">
      <summary>{t("error.diagnostics")}</summary>
      <div className="mt-2 flex min-w-0 max-w-full flex-col gap-3">{children}</div>
    </details>
  );
}

function DiagnosticDetails({ text }: { text: string }): React.ReactElement {
  const t = useT();
  const summaryId = useId();
  const region = useFadeRegion({ labelledBy: summaryId });
  return (
    <details className="min-w-0 max-w-full">
      <summary id={summaryId}>{t("error.diagnostics")}</summary>
      <div className={`min-w-0 max-w-full rounded-md bg-default ${region.focused ? "ring-2 ring-app-focus" : ""}`}>
        <FadedScroller dir="ltr" orientation="horizontal" region={region} className="min-w-0 max-w-full overflow-x-auto overflow-y-hidden rounded-md">
          <CodeBlock dir="ltr" wrap={false} className="[&>.app-code]:rounded-none [&>.app-code]:bg-transparent">{text}</CodeBlock>
        </FadedScroller>
      </div>
    </details>
  );
}

/** Mounted outside the application boundary, so startup and render failures can also be acknowledged. */
export function ErrorAlerts({ daemon }: { daemon?: Daemon }): React.ReactElement | null {
  const t = useT();
  const platform = useOptionalPlatform();
  const [error] = useSyncExternalStore(errorAlerts.subscribe, errorAlerts.getSnapshot);
  const acknowledge = useRef<HTMLButtonElement>(null);
  const reporting = useRef(false);
  const [reportPending, setReportPending] = useState(false);
  useLayoutEffect(() => { dialogAround(acknowledge)?.focus(); }, [error?.id]);
  if (!error) return null;
  const unexpected = error.classification.kind === "unexpected";
  const diagnostics = [error.error, ...(error.failures?.map((failure) => failure.error) ?? [])].map(diagnosticText).filter((detail): detail is string => !!detail && detail !== error.message);
  const manualIssue = !platform?.links && platform?.kind === "tauri";
  // A button for the remedy when it is the one remedy of every failure the alert is about.
  const action = error.sharedRecovery ? recoveryAction(t, error.sharedRecovery) : undefined;
  const report = async () => {
    if (reporting.current) return;
    reporting.current = true;
    setReportPending(true);
    try {
      if (!platform?.links) throw new Error("External links are unavailable.");
      await platform.links.open(issueUrl);
    } catch (cause) {
      errorAlerts.report({
        message: t("error.issue.openFailed"),
        error: cause,
        recovery: "issueLink",
        content: <><p>{t("error.issue.openFailed")}</p><p><InlineChip size="inherit" monospace wrap>{issueUrl}</InlineChip></p></>,
      }, { immediate: true });
    } finally {
      reporting.current = false;
      setReportPending(false);
    }
  };
  const dialog = (
    <Dialog
      key={error.id}
      alert
      dismissable={false}
      headerLeading={<AlertDialog.Icon status="danger" />}
      title={error.title ?? t(unexpected ? "error.unexpected.title" : "error.title")}
      onClose={() => {}}
      resetKey={error.message}
      footer={<>
        {unexpected && !manualIssue && <ExternalLink href={issueUrl} appearance="button" onOpen={platform?.links ? report : undefined} isPending={reportPending}>{t("error.issue.report")}</ExternalLink>}
        {action && (
          <Button variant="secondary" onPress={() => {
            errorAlerts.acknowledge();
            openSettingsFromAnywhere("accounts", { addAgent: action.addAgent });
          }}>{action.label}</Button>
        )}
        <Button ref={acknowledge} variant="danger" onPress={() => errorAlerts.acknowledge()}>{t("error.acknowledge")}</Button>
      </>}
    >
      <div className="min-w-0 whitespace-pre-wrap [overflow-wrap:anywhere]">{error.content ?? error.message}</div>
      {error.classification.kind === "unexpected"
        ? <>
          <p>{manualIssue
            ? <Message id="error.unexpected.manualAddressGuidance" params={{ address: <InlineChip size="inherit" monospace wrap>{issueUrl}</InlineChip> }} />
            : <Message id="error.unexpected.reportGuidance" params={{ report: <InlineChip size="inherit">{t("error.issue.report")}</InlineChip> }} />}</p>
          {error.details === undefined && diagnostics.length > 0 && <DiagnosticDetails text={[...new Set(diagnostics)].join("\n\n")} />}
        </>
        : !error.failures?.length && error.classification.recovery !== "none" && <p><RecoveryGuidance recovery={error.classification.recovery} /></p>}
      {error.details ? <DetailsDisclosure>{error.details}</DetailsDisclosure> : null}
    </Dialog>
  );
  return daemon ? <DaemonProvider value={daemon}>{dialog}</DaemonProvider> : dialog;
}
