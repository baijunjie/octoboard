import { DiagnosticBlock } from "../components/alerts/DiagnosticBlock";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { t } from "../i18n/language";
import { Message } from "../i18n/react";

/** Reports a failure of the application's own lifecycle in its words, titled for what failed, with the shell's text
 * behind the alert's Details as raw text always is. It carries no classified error, so the alert treats it as
 * unexpected and offers Report issue: nothing the user did explains a daemon that went away or a quit that did not
 * happen. */
function report(key: "exit.daemonExited" | "exit.restartFailed" | "exit.failed", detail: string, action?: { label: string; onPress: () => void }): void {
  errorAlerts.report({
    action,
    message: t(key),
    title: t(`${key}.title`),
    content: <Message id={key} />,
    details: detail ? <DiagnosticBlock>{detail}</DiagnosticBlock> : null,
    diagnostic: detail || undefined,
  });
}

/** Tells the user that the daemon went away without being asked to, offering `restart`, the way to bring it back. */
export function reportDaemonExit(detail: string, restart: () => void): void {
  report("exit.daemonExited", detail, { label: t("common.restart"), onPress: restart });
}

/** Tells the user that the quit or restart they asked for did not happen, with `error`, the shell's rejection (an IPC
 * call may reject with a plain string), as its details. */
export function reportExitFailure(restart: boolean, error: unknown): void {
  report(restart ? "exit.restartFailed" : "exit.failed", error instanceof Error ? error.message : String(error));
}
