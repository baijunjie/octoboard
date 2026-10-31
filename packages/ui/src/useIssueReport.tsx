import React, { useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { appConfig } from "./appConfig";
import { DiagnosticBlock } from "./components/DiagnosticBlock";
import { ExternalLink } from "./components/ExternalLink";
import { InlineReference } from "./components/InlineReference";
import { errorAlerts, type ReportableFailure } from "./errorAlerts";
import { diagnosticText } from "./errorText";
import { Message, useCurrentLanguage, useT } from "./i18n/react";
import { issueReportUrl, recordNames } from "./issueReport";
import { useOptionalPlatform } from "./platform/react";
import { useErrorRecords } from "./store";

export const issueUrl = appConfig.issueUrl;

const NO_NAMES: string[] = [];

/** What reporting a failure offers wherever one is shown: the sentence asking for the report (or, where the platform
 * cannot open links, naming the address to report at) and the Report issue link, which is absent where the address is
 * only named. `failures` are those of a failure to report, or `undefined` where there is none; their text goes into the
 * report as it is, so it is raw or fixed English text, never the UI's translated wording. The link opens the Issue
 * page prefilled with a description of them from which the user's personal information is removed (see
 * `issueReportUrl`; the record names it removes are those of the daemon in context, so a caller outside every daemon
 * provider reports without them), or the bare address with no `failures`. The address named to report at, and shown
 * for a link that cannot be opened, is always the bare one, which can be read and typed. A link that cannot be opened
 * is itself reported.
 *
 * By default the failure is a possible bug, said so in the sentence and in the Issue's title. A `known` failure is one
 * the application can name and say what to do about (`title` is then the Issue's own title, in English so it can be
 * searched for), and its sentence only asks for a report if the steps given do not resolve it. `size` is the link
 * button's, for a row of small buttons. */
export function useIssueReport(failures: readonly ReportableFailure[] | undefined, options: { size?: "sm"; known?: { title: string } } = {}): { guidance: React.ReactElement; link: React.ReactElement | null } {
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
      known: options.known,
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
      ? <Message id={options.known ? "startup.report.manualGuidance" : "error.unexpected.manualReportGuidance"} params={{ address: <InlineReference monospace wrap>{issueUrl}</InlineReference> }} />
      : <Message id={options.known ? "startup.report.prefilledGuidance" : "error.unexpected.prefilledReportGuidance"} params={{ report: <InlineReference>{t("error.issue.report")}</InlineReference> }} />,
    link: manualIssue ? null : <ExternalLink href={reportUrl} appearance="button" size={options.size} onOpen={platform?.links ? report : undefined} isPending={reportPending}>{t("error.issue.report")}</ExternalLink>,
  };
}
