import { appDataRecovery } from "../../errors/errorRecovery";
import { errorNotification, type AlertOptions, type NotificationRequest } from "../../store";
import { errorAlertParts } from "./errorAlertParts";
import { errorAlerts, type ErrorAlertRequest } from "./errorAlerts";

/** The error alert for an error request: its title, body and "Error details" from `errorAlertParts`, and, for a request
 * about a session, the session's id, with the name its request carries for a session that may be gone. The alert names
 * the session from the store when it shows (`AlertSessionLines`), not here. */
export function errorAlertOf(request: NotificationRequest): ErrorAlertRequest {
  const { title, body, details, remedyInBody } = errorAlertParts(request);
  return {
    message: request.message,
    session: request.session,
    sessionName: request.daemonTitle?.params.title,
    error: request.error ?? request.daemonError ?? request.message,
    recovery: request.recovery,
    title,
    content: body,
    details,
    remedyInBody,
  };
}

/** Raises an error alert for `error`, which may be anything a request rejected with; see `AlertOptions` for what the
 * call site says about it, the session it is about included. The one route every action raising an alert takes. */
export function raiseError(error: unknown, alert: AlertOptions): void {
  errorAlerts.report(errorAlertOf(errorNotification(error, alert)));
}

/** The catch handler of a request that raises its failure as an error alert:
 * `request(body).catch(alertFailure({ title: t("…") }))`. */
export function alertFailure(alert: AlertOptions): (error: unknown) => void {
  return (error) => raiseError(error, alert);
}

/** `alertFailure` for a failed write to the application's own data (a setting, the order of the agents or accounts): a denied access or a full disk is
 * worded for the folder holding that data, not for a folder to choose (see `appDataRecovery`). */
export function alertAppDataFailure(title: string): (error: unknown) => void {
  return (error) => raiseError(error, { title, recovery: appDataRecovery(error) });
}
