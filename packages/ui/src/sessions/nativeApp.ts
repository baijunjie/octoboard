import type { Translate } from "../i18n/catalog";
import type { Session } from "../protocol";
import { hasNoProcess } from "./sessionStatus";

/** Why a session's conversation cannot be opened in its agent's desktop application, or `undefined` when nothing keeps
 * it: the session has a process (the application and Octoboard would both write to the conversation, and nothing is
 * stopped for it) or it has no conversation yet. Only a session that has no process at all (interrupted or archived)
 * is opened, which is stricter than a handoff's rest: a session idle at its prompt still has one. `detail` is the
 * whole sentence, for a menu item's description or an alert, and `state` the short reason drawn under a menu item. */
export function openInAppBlock(t: Translate, session: Pick<Session, "status" | "has_conversation" | "agent_session_id">): { detail: string; state: string } | undefined {
  if (!hasNoProcess(session.status)) return { detail: t("sidebar.session.openInApp.running"), state: t("sidebar.session.openInApp.running.reason") };
  if (!session.has_conversation || !session.agent_session_id) return { detail: t("sidebar.session.openInApp.noConversation"), state: t("sidebar.session.openInApp.noConversation.reason") };
  return undefined;
}

/** What to tell the user, without keeping them from opening, when the daemon found that the default directory, the only
 * one the agent's application reads, does not hold the conversation's latest record (`seen` false): the application
 * may show nothing or an older state. `note` is the short line shown with the action, `detail` the whole sentence. */
export function openInAppNote(t: Translate, seen: boolean): { detail: string; note: string } | undefined {
  return seen ? undefined : { detail: t("sidebar.session.openInApp.notSeen"), note: t("sidebar.session.openInApp.notSeen.note") };
}
