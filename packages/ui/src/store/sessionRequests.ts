import type { Agent, Event, SessionKind, SessionRequestAction } from "../protocol";

/** What approving a `session_request` does, every value this client knows. A request with a value it does not know is
 * not offered, and the daemon's timeout ends the call. Written as a record's keys so the type check fails while one is
 * missing. */
const SESSION_REQUEST_ACTIONS: readonly string[] = Object.keys({
  start: true, reopen: true, resume: true, resume_record: true, switch: true, handoff: true,
} satisfies Record<SessionRequestAction, true>);

/** What every session request says, whatever an approval does. */
interface SessionRequestBase {
  requestId: string;
  /** The requesting session. */
  session: string;
  /** Whether the session asked for would be a console session or a project session. */
  kind: SessionKind;
  /** The console the new session would be in, or the console of the session to bring back. */
  console: string;
  /** The project the new session would be in, or the project of the session to bring back; null
   * for a console session. */
  project: string | null;
  /** The agent the new session would run, or the agent of the session to bring back. */
  agent: Agent;
  /** The branches an approval would hand to the new session, as wire paths; empty for none. */
  branches: string[];
  requestedAt: number;
}

/** A session's request for a session with no owner, to resume a conversation, or to switch or hand
 * off a session it owns, waiting for the user's answer. What it carries depends on what an approval
 * does, which `action` tells. */
export type SessionRequest = SessionRequestBase & (
  | { action: "start" }
  | {
    action: "reopen" | "resume";
    /** The session an approval brings back. */
    target: string;
    /** The session the target reports to; absent when it has none. */
    owner?: string;
    /** The archived owners that come back with the target, topmost first. */
    reopenedOwners: string[];
  }
  | {
    action: "resume_record";
    /** The account that holds the record; null for the agent's default account. */
    account: string | null;
    /** The conversation's id. */
    conversation: string;
  }
  | {
    action: "switch";
    /** The session an approval moves. */
    target: string;
    /** The account the call asked for; null for the agent's default account. */
    account: string | null;
  }
  | {
    action: "handoff";
    /** The session an approval hands off. */
    target: string;
    /** The account the receiving session would run on; null for the agent's default account. */
    account: string | null;
    /** The model and effort the call asked for, absent where it left them out. */
    model?: string | null;
    effort?: string | null;
    compatibility?: Record<string, boolean | null>;
  }
);

/** The request a `session_request` event describes, or undefined for one that does not carry what
 * its action needs (a reopen, resume, switch or handoff without a target, a record without its conversation) or
 * has an action this client does not know: the dialog is the one place an approval is enforced, so
 * a request it cannot describe is not offered at all, and the daemon's own timeout ends the call. */
export function sessionRequestOf(event: Extract<Event, { type: "session_request" }>): SessionRequest | undefined {
  const base: SessionRequestBase = {
    requestId: event.request_id,
    session: event.session,
    kind: event.kind,
    console: event.console,
    project: event.project,
    agent: event.agent,
    branches: event.branches,
    requestedAt: event.requested_at,
  };
  // The wire carries what a newer daemon may add, which the type does not know.
  if (!SESSION_REQUEST_ACTIONS.includes(event.action)) return undefined;
  const { action } = event;
  switch (action) {
    case "reopen":
    case "resume":
      if (event.target === null) return undefined;
      return { ...base, action, target: event.target, owner: event.owner ?? undefined, reopenedOwners: event.reopened_owners };
    case "resume_record":
      if (event.conversation === null) return undefined;
      return { ...base, action, account: event.account, conversation: event.conversation };
    case "switch":
      if (event.target === null) return undefined;
      return { ...base, action, target: event.target, account: event.account };
    case "handoff":
      if (event.target === null) return undefined;
      return { ...base, action, target: event.target, account: event.account, model: event.model, effort: event.effort, compatibility: event.compatibility };
    default:
      return { ...base, action };
  }
}

/** The requests with `request` added, kept in the order they were made whatever order the
 * broadcast and the repeat after a snapshot arrive in. */
export function withRequest(requests: SessionRequest[], request: SessionRequest): SessionRequest[] {
  return [...requests, request].sort((a, b) => a.requestedAt - b.requestedAt);
}
