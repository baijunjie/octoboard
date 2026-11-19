import type { ScheduledTask, Session } from "../protocol";
import { isKnownSessionStatus } from "../sessions/sessionStatus";
import { useDaemonStore } from "../store";

/** Whether `session`, a session the window holds, is starting or working: the statuses a run's session is going through
 * (read through the normaliser, so a status this client does not know is not one). */
function atWork(session: Session | undefined): boolean {
  return !!session && isKnownSessionStatus(session.status) && (session.status === "starting" || session.status === "working");
}

/** How many of the listed scheduled tasks are running: those with a session of their runs (`run_sessions`) that the
 * window holds and that is starting or working, whether or not the task is enabled. A session the window does not hold
 * counts for nothing, and a session two tasks share counts for both. `undefined` while the tasks have not been listed
 * (`State.scheduledTasks`). A number, so what reads it renders only when the count changes: a session's status change
 * that leaves it as it was renders nothing. */
export function useRunningTaskCount(): number | undefined {
  return useDaemonStore((state) => runningTaskCount(state.scheduledTasks, state.sessions));
}

function runningTaskCount(tasks: ReadonlyMap<string, ScheduledTask> | undefined, sessions: ReadonlyMap<string, Session>): number | undefined {
  if (!tasks) return undefined;
  let running = 0;
  for (const task of tasks.values()) {
    if (task.run_sessions.some((id) => atWork(sessions.get(id)))) running++;
  }
  return running;
}
