import { DaemonErrorMessage } from "../components/alerts/DaemonErrorMessage";
import { raiseError } from "../components/alerts/raiseError";
import type { CodedError } from "../errors/codedError";
import { InlineReference } from "../i18n/InlineReference";
import { t } from "../i18n/language";
import { Message } from "../i18n/react";

/** Tells the user that the daemon refused the restart a browser tab asked for. The alert is the refusal's own, worded by
 * its code's copy with the raw text behind Error details. A daemon that does not restart itself this way
 * (`daemon_update_unavailable`) is also told the command to run on the server, since there is no application to quit and
 * open again; the other refusals (a download under way, `daemon_update_running`; the daemon already stopping,
 * `daemon_shutting_down`; holders inside the daemon's systemd unit, `holders_in_service`, whose own copy names the
 * command; an agent's CLI being updated, `agent_update_running`) have their own sentence only. The window foresees the
 * last (`UpdateState.restartBlocked` leaves Restart now unavailable, whose press raises this alert with that code), so
 * from the daemon it arrives only when the update began after the press. */
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
