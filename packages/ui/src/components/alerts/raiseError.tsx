import { InlineReference } from "../../i18n/InlineReference";
import { t } from "../../i18n/language";
import { Message } from "../../i18n/react";
import { appDataRecovery } from "../../errors/errorRecovery";
import { sessionDisplayTitle } from "../../sessions/sessionLabel";
import { errorNotification, type AlertOptions, type State, type NotificationRequest } from "../../store";
import { errorAlertParts } from "./errorAlertParts";
import { errorAlerts, type ErrorAlertRequest } from "./errorAlerts";

const NO_RECORDS = { sessions: new Map(), consoles: new Map(), projects: new Map() };

/** The error alert for an error request: its title, body and "Error details" from `errorAlertParts`, and, for a request
 * about a session, the lines that name it ahead of the body. The daemon words its message as "this session" without
 * naming it, so which session the alert is about comes from the session's record or, when it is gone, from the name the
 * request carries. */
export function errorAlertOf(request: NotificationRequest, { sessions, consoles, projects }: Pick<State, "sessions" | "consoles" | "projects">): ErrorAlertRequest {
  const { title, body, details, remedyInBody } = errorAlertParts(request);
  const session = request.session ? sessions.get(request.session) : undefined;
  const sessionName = session ? sessionDisplayTitle(t, session) : request.daemonTitle?.params.title;
  const sessionLine = sessionName ? <InlineReference dir="auto" wrap>{sessionName}</InlineReference> : undefined;
  const place = session && (session.project_id ? projects.get(session.project_id) : consoles.get(session.console_id));
  const placeName = place ? <InlineReference dir="auto" wrap>{place.name}</InlineReference> : t("error.alertLocationUnknown");
  return {
    message: request.message,
    session: request.session,
    error: request.error ?? request.daemonError ?? request.message,
    recovery: request.recovery,
    title,
    content: (
      <>
        {sessionLine && <div className="mb-2 flex flex-col gap-1">
          <p dir="auto"><Message id="error.alertSession" params={{ session: sessionLine }} /></p>
          {session && <p dir="auto">{session.project_id
            ? <Message id="error.alertProject" params={{ project: placeName }} />
            : <Message id="error.alertConsole" params={{ console: placeName }} />}</p>}
        </div>}
        {body}
      </>
    ),
    details,
    remedyInBody,
  };
}

/** Raises an error alert for `error`, for code that has no daemon to ask. It cannot name a session, which needs the
 * store's records: `Daemon.raiseError` is the route for code that has a daemon and may name one. */
export function raiseError(error: unknown, alert: AlertOptions & { session?: undefined }): void {
  errorAlerts.report(errorAlertOf(errorNotification(error, alert), NO_RECORDS));
}

/** The catch handler of a request that raises its failure as an error alert, with no session to name:
 * `request(body).catch(alertFailure({ title: t("…") }))`. */
export function alertFailure(alert: AlertOptions & { session?: undefined }): (error: unknown) => void {
  return (error) => raiseError(error, alert);
}

/** `alertFailure` for a failed write to the application's own data (a setting, the order of the agents or accounts): a denied access or a full disk is
 * worded for the folder holding that data, not for a folder to choose (see `appDataRecovery`). */
export function alertAppDataFailure(title: string): (error: unknown) => void {
  return (error) => raiseError(error, { title, recovery: appDataRecovery(error) });
}
