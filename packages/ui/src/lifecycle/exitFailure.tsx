import { DiagnosticBlock } from "../components/DiagnosticBlock";
import { errorAlerts } from "../errorAlerts";
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
