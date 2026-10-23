import { useState } from "react";

import { choiceKey, findEntry } from "../accountChoices";
import { AGENT_LABEL } from "../agents";
import { DaemonRequestError } from "../daemon-client";
import { CONVERSATION_ID } from "../conversationId";
import type { ErrorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Agent, Project, Session } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { useDaemon, useDaemonStore } from "../store";
import { useChoiceGroups } from "./useLaunchChoice";
import { useLocatedConversation } from "./useLocatedConversation";

/** How long the typed Session ID rests before the daemon is asked where its conversation is. */
const LOCATE_DELAY_MS = 300;

/** What the Resume tab found for the ID. A session of the project is classified by its state now,
 * which may have moved on since the daemon's reply: an archived or interrupted one is resumed, any
 * other is gone to. An agent's own record opens a new session, on the agent and account that hold
 * it (`accountName` is the account as the pickers name it). */
export type ResumeTarget =
  | { kind: "archived" | "interrupted" | "running"; session: Session }
  | { kind: "record"; agent: Agent; account: string | null; accountName: string };

const FOUND_SESSION_MESSAGE = {
  archived: "dialog.session.resumeFoundArchived",
  interrupted: "dialog.session.resumeFoundInterrupted",
  running: "dialog.session.resumeFoundRunning",
} as const;

/**
 * Says which session holds a conversation that is outside the project, and where: `undefined` for
 * a session the store does not have, or one whose project or console it does not have, which the
 * caller words generally instead. The names are isolated.
 */
function useHeldElsewhere(): (sessionId: string | undefined) => DaemonRequestError | undefined {
  const t = useT();
  const sessions = useDaemonStore((s) => s.sessions);
  const projects = useDaemonStore((s) => s.projects);
  const consoles = useDaemonStore((s) => s.consoles);
  return (sessionId) => {
    const session = sessionId === undefined ? undefined : sessions.get(sessionId);
    if (!session) return undefined;
    if (session.project_id) {
      const project = projects.get(session.project_id);
      return project && new DaemonRequestError(t("dialog.session.resumeHeldInProject", { session: isolate(sessionDisplayTitle(t, session)), project: isolate(project.name) }), "conversation_held_in_project", { session: session.id, project: project.id });
    }
    const owner = consoles.get(session.console_id);
    return owner && new DaemonRequestError(t("dialog.session.resumeHeldInConsole", { session: isolate(sessionDisplayTitle(t, session)), console: isolate(owner.name) }), "conversation_held_in_console", { session: session.id, console: owner.id });
  };
}

/**
 * The Resume tab's answer for `resumeId`: once the ID is valid the daemon is asked where the
 * conversation is held (`locate_conversation_to_resume`), which settles `target`, what the submit
 * does and on which agent and account. `idProblem` is what is wrong with the ID itself, by kind;
 * `problem` is what the answer says is wrong with the conversation (a session outside the project,
 * no conversation at all, a holder no longer offered, a failed lookup), shown under the ID field;
 * `status` is what the live region says. `looking` is true while the answer for the ID is not in. All
 * are empty unless `resuming`.
 *
 * A session the answer named that the store does not have (yet, or any more) makes the answer
 * doubtful: it is asked again once for that session, which then settles on the session once it is
 * there, or on what holds the conversation now that it is not. `reask` asks again for a caller that
 * learned the answer no longer holds.
 */
export function useResumeTarget(project: Project | undefined, resumeId: string, resuming: boolean, sessions: Session[]): {
  idProblem: "required" | "invalid" | undefined;
  target: ResumeTarget | undefined;
  looking: boolean;
  problem: string | ErrorPresentation | undefined;
  status: string | undefined;
  reask: () => void;
} {
  const t = useT();
  const { request } = useDaemon();
  const groups = useChoiceGroups();
  const heldElsewhere = useHeldElsewhere();
  const [checkedMissing, setCheckedMissing] = useState<string>();
  const [reasked, setReasked] = useState(0);

  const idProblem = !resuming ? undefined : resumeId === "" ? "required" : !CONVERSATION_ID.test(resumeId) ? "invalid" : undefined;
  const lookup = useLocatedConversation(request, project?.id ?? "", resuming && idProblem === undefined ? resumeId : undefined, LOCATE_DELAY_MS, JSON.stringify([checkedMissing, reasked]));
  const located = lookup.status === "ready" ? lookup.location : undefined;
  const heldId = located?.kind === "archived_session" || located?.kind === "live_session" ? located.session : undefined;
  const holder = useDaemonStore((s) => heldId === undefined ? undefined : s.sessions.get(heldId)) ?? sessions.find((s) => s.id === heldId);
  const missing = heldId !== undefined && holder === undefined;
  if (missing && checkedMissing !== heldId) setCheckedMissing(heldId);

  // The pair that holds the record may have stopped being offered since the reply; the pickers'
  // fallback is never opened in its place.
  const recordEntry = located?.kind === "record" ? findEntry(groups, choiceKey(located)) : undefined;
  let target: ResumeTarget | undefined;
  if (holder) {
    target = { kind: holder.status === "archived" || holder.status === "interrupted" ? holder.status : "running", session: holder };
  } else if (located?.kind === "record" && recordEntry?.selectable) {
    target = { kind: "record", agent: located.agent, account: located.account, accountName: recordEntry.name };
  }

  let problem: string | ErrorPresentation | undefined;
  if (located?.kind === "outside_project") problem = heldElsewhere(located.session) ?? t("daemon.conversation_outside_project");
  else if (located?.kind === "not_found") problem = t("dialog.session.resumeNotFound");
  else if (located?.kind === "record" && !target) problem = t("dialog.session.resumeUnavailable");
  else if (lookup.status === "failed") problem = lookup.error;

  // A session the lookup named that the store does not have yet counts as still looking.
  const looking = idProblem === undefined && resuming && (lookup.status === "pending" || missing);
  let status: string | undefined;
  if (looking) status = t("dialog.session.resumeLooking");
  else if (target?.kind === "record") status = t("dialog.session.resumeFound", { agent: AGENT_LABEL[target.agent], account: isolate(target.accountName) });
  else if (target) status = t(FOUND_SESSION_MESSAGE[target.kind], { title: isolate(sessionDisplayTitle(t, target.session)) });

  return { idProblem, target, looking, problem, status, reask: () => setReasked((count) => count + 1) };
}
