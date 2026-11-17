import { useT } from "../../i18n/react";
import type { Session } from "../../protocol";
import { useDaemon } from "../../store";
import { alertFailure } from "../alerts/raiseError";

/** Opens a session's conversation in its agent's own desktop application, on the daemon's host
 * (`open_session_in_app`); a failure is raised as an error alert. */
export function useOpenSessionInApp(): (session: Session) => void {
  const t = useT();
  const { request } = useDaemon();
  return (session) => {
    request({ type: "open_session_in_app", session: session.id }).catch(alertFailure({ title: t("sidebar.session.openInAppFailedTitle"), session: session.id }));
  };
}
