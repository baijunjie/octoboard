import { nativeAppLabel } from "../../agents/providers";
import { useT } from "../../i18n/react";
import type { Session } from "../../protocol";
import { useDaemon } from "../../store";
import { alertFailure } from "../alerts/raiseError";

/** Opens a session's conversation in its agent's own desktop application, on the daemon's host
 * (`open_session_in_app`); a failure is raised as an error alert. An archived session is opened at once. Any other is
 * archived by the daemon first, with the sessions bound to it, so `confirm` is called instead, with the session and the
 * provider's label of the action, to ask first (the choice of keeping the process trees that ends running is the
 * confirmation's); it is never sent without the question, so nothing happens for an agent with no such label, which
 * offers no action. */
export function useOpenSessionInApp(confirm: (session: Session, label: string) => void): (session: Session) => void {
  const t = useT();
  const { request, store } = useDaemon();
  return (session) => {
    if (session.status !== "archived") {
      const label = nativeAppLabel(t, store.getState().providers, session.agent);
      if (label) confirm(session, label);
      return;
    }
    request({ type: "open_session_in_app", session: session.id }).catch(alertFailure({ title: t("sidebar.session.openInAppFailedTitle"), session: session.id }));
  };
}
