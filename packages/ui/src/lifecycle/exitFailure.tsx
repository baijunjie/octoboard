import { DiagnosticBlock } from "../components/alerts/DiagnosticBlock";
import { DaemonErrorMessage } from "../components/alerts/DaemonErrorMessage";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { raiseError } from "../components/alerts/raiseError";
import type { CodedError } from "../errors/codedError";
import { InlineReference } from "../i18n/InlineReference";
import { t } from "../i18n/language";
import { Message } from "../i18n/react";

/** Reports a failure of the application's own lifecycle in its words, titled for what failed, with the shell's text
 * behind the alert's Details as raw text always is. It carries no classified error, so the alert treats it as
 * unexpected and offers Report issue: nothing the user did explains a daemon that went away or a quit that did not
 * happen. */
function report(key: "exit.daemonExited" | "exit.restartFailed" | "exit.failed", detail: string): void {
  errorAlerts.report({
    message: t(key),
    title: t(`${key}.title`),
    content: <Message id={key} />,
    details: detail ? <DiagnosticBlock>{detail}</DiagnosticBlock> : null,
    diagnostic: detail || undefined,
  });
}

/** Tells the user that the daemon went away without being asked to. */
export function reportDaemonExit(detail: string): void {
  report("exit.daemonExited", detail);
}

/** Tells the user that the quit or restart they asked for did not happen. */
export function reportExitFailure(restart: boolean, detail: string): void {
  report(restart ? "exit.restartFailed" : "exit.failed", detail);
}

/** Tells the user that the daemon refused the restart a browser tab asked for. The alert is the refusal's own, worded by
 * its code's copy with the raw text behind Error details. A daemon that does not restart itself this way
 * (`daemon_update_unavailable`) is also told the command to run on the server, since there is no application to quit and
 * open again; the other refusals (a download under way, `daemon_update_running`, and the daemon already stopping,
 * `daemon_shutting_down`) have their own sentence only. */
export function reportDaemonRestartFailure(refusal: CodedError): void {
  raiseError(refusal, {
    title: t("exit.daemonRestartFailed.title"),
    recovery: "none",
    content: (
      <div className="flex flex-col gap-4">
        <DaemonErrorMessage error={refusal} />
        {refusal.code === "daemon_update_unavailable" && (
          <p><Message id="exit.daemonRestartFailed.remedy" params={{ command: <InlineReference monospace>octoboardd restart</InlineReference> }} /></p>
        )}
      </div>
    ),
  });
}
