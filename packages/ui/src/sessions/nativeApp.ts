import type { Translate } from "../i18n/catalog";
import type { Session } from "../protocol";
import { sessionIsBusy } from "./sessionStatus";

/** Why a session's conversation cannot be opened in its agent's desktop application, or `undefined` when nothing keeps
 * it: it is locked and not archived yet (opening archives it, which a lock refuses), it is at work (busy as
 * `sessionIsBusy` has it, which is the daemon's judgement of a session that is not at rest), an account switch holds it
 * (`switching`), it is still starting, or it has no conversation yet. Any other session is opened: an archived one as it
 * is, the rest after the daemon has archived them with the sessions bound to them. `detail` is the whole sentence, for a
 * menu item's description or an alert, and `state` the short reason drawn under a menu item, absent for the short-lived
 * blocks (a switch, a start). */
export function openInAppBlock(
  t: Translate,
  session: Pick<Session, "status" | "has_conversation" | "agent_session_id" | "background_tasks" | "at_usage_limit" | "locked">,
  { switching = false }: { switching?: boolean } = {},
): { detail: string; state?: string } | undefined {
  if (session.locked && session.status !== "archived") return { detail: t("sidebar.session.openInApp.locked"), state: t("accountChoice.sessionLocked.reason") };
  if (sessionIsBusy(session)) return { detail: t("sidebar.session.openInApp.working"), state: t("accountChoice.handoff.working.reason") };
  if (switching) return { detail: t("accountChoice.switching.detail") };
  if (session.status === "starting") return { detail: t("sidebar.session.openInApp.starting") };
  if (!session.has_conversation || !session.agent_session_id) return { detail: t("sidebar.session.openInApp.noConversation"), state: t("sidebar.session.openInApp.noConversation.reason") };
  return undefined;
}

/** What to tell the user, without keeping them from opening, when the daemon found that the default directory, the only
 * one the agent's application reads, does not hold the conversation's latest record (`seen` false): the application
 * may show nothing or an older state. `note` is the short line shown with the action, `detail` the whole sentence. */
export function openInAppNote(t: Translate, seen: boolean): { detail: string; note: string } | undefined {
  return seen ? undefined : { detail: t("sidebar.session.openInApp.notSeen"), note: t("sidebar.session.openInApp.notSeen.note") };
}
