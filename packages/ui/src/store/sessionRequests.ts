import type { Agent, Event, Schedule, ScheduledTaskLaunch, SessionKind, SessionRequestAction } from "../protocol";
import { isSessionKind } from "../sessions/sessionKind";
import { knownValues } from "../values/knownValue";
import { isKnownScheduleKind } from "../values/scheduleKind";

/** What approving a `session_request` does, every value this client knows. A request with a value it does not know is
 * not offered, and the daemon's timeout ends the call. */
const isRequestAction = knownValues<SessionRequestAction>({
  start: true, reopen: true, resume: true, resume_record: true, switch: true, handoff: true,
  create_scheduled_task: true, update_scheduled_task: true, set_scheduled_task_enabled: true, delete_scheduled_task: true, run_scheduled_task: true,
});

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
  /** The agent the new session would run, or the agent of the session to bring back. Without meaning for a
   * scheduled task request, whose own launch settings name the agent. */
  agent: Agent;
  /** The branches an approval would hand to the new session, as wire paths; empty for none. */
  branches: string[];
  /** The text the requesting agent wrote for the request, verbatim, which the dialog shows folded away: the brief of a
   * `start`, the note of a `handoff`, the prompt of a task created or changed. Absent where there is none. */
  prompt?: string;
  requestedAt: number;
}

/** A session's request for a session with no owner, to resume a conversation, to switch or hand
 * off a session it owns, or to create, change, turn on or off, delete or run a scheduled task, waiting for the user's answer. What it carries depends on what an approval
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
  | {
    action: "create_scheduled_task";
    /** The task's prompt, which a create always carries. */
    prompt: string;
    /** The whole task as it would be saved: its launch settings (`launch.title` is its name), its schedule and whether it is on. */
    launch: ScheduledTaskLaunch;
    schedule: Schedule;
    enabled: boolean;
  }
  | {
    action: "update_scheduled_task";
    task: string;
    /** The whole new launch settings, present only where the call changes them. */
    launch?: ScheduledTaskLaunch;
    /** The new schedule, present only where the call changes it. */
    schedule?: Schedule;
  }
  | { action: "set_scheduled_task_enabled"; task: string; enabled: boolean }
  | { action: "delete_scheduled_task" | "run_scheduled_task"; task: string }
);

/** The actions that act on a scheduled task. */
const TASK_ACTIONS = ["create_scheduled_task", "update_scheduled_task", "set_scheduled_task_enabled", "delete_scheduled_task", "run_scheduled_task"] as const satisfies readonly SessionRequestAction[];

/** The requests that act on a scheduled task, whose `kind`, `console` and `project` are the task's target after the change. */
export type ScheduledTaskRequest = Extract<SessionRequest, { action: (typeof TASK_ACTIONS)[number] }>;

/** Whether the request is to create, change, turn on or off, delete or run a scheduled task. */
export function isScheduledTaskRequest(request: SessionRequest): request is ScheduledTaskRequest {
  return (TASK_ACTIONS as readonly string[]).includes(request.action);
}

/** The request a `session_request` event describes, or undefined for one that does not carry what
 * its action needs (a reopen, resume, switch or handoff without a target, a record without its conversation, a task
 * request without its task or what it creates or changes, or a schedule of a kind this client does not word) or
 * has an action or a session kind this client does not know: the dialog is the one place an approval is enforced, so
 * a request it cannot describe is not offered at all, and the daemon's own timeout ends the call. */
export function sessionRequestOf(event: Extract<Event, { type: "session_request" }>): SessionRequest | undefined {
  // The wire carries what a newer daemon may add, which the type does not know.
  const { action, kind } = event;
  if (!isRequestAction(action) || !isSessionKind(kind)) return undefined;
  const base: SessionRequestBase = {
    requestId: event.request_id,
    session: event.session,
    kind,
    console: event.console,
    project: event.project,
    agent: event.agent,
    branches: event.branches,
    ...(event.prompt !== undefined && event.prompt !== "" && { prompt: event.prompt }),
    requestedAt: event.requested_at,
  };
  const asked = event.scheduled_task;
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
    case "create_scheduled_task":
      if (!asked?.launch || !asked.schedule || asked.enabled === undefined || !base.prompt || !isKnownScheduleKind(asked.schedule.kind)) return undefined;
      return { ...base, action, prompt: base.prompt, launch: asked.launch, schedule: asked.schedule, enabled: asked.enabled };
    case "update_scheduled_task":
      if (!asked?.task || (asked.schedule && !isKnownScheduleKind(asked.schedule.kind))) return undefined;
      return { ...base, action, task: asked.task, launch: asked.launch, schedule: asked.schedule };
    case "set_scheduled_task_enabled":
      if (!asked?.task || asked.enabled === undefined) return undefined;
      return { ...base, action, task: asked.task, enabled: asked.enabled };
    case "delete_scheduled_task":
    case "run_scheduled_task":
      if (!asked?.task) return undefined;
      return { ...base, action, task: asked.task };
    default:
      return { ...base, action };
  }
}

/** The requests with `request` added, kept in the order they were made whatever order the
 * broadcast and the repeat after a snapshot arrive in. */
export function withRequest(requests: SessionRequest[], request: SessionRequest): SessionRequest[] {
  return [...requests, request].sort((a, b) => a.requestedAt - b.requestedAt);
}
