import type { ScheduledTask } from "../protocol";
import { useDaemonStore } from "../store";

/** How many of the listed scheduled tasks are enabled (`enabled`), whether or not one is running or has a run to come.
 * `undefined` while the tasks have not been listed (`State.scheduledTasks`). A number, so what reads it renders only
 * when the count changes. */
export function useEnabledTaskCount(): number | undefined {
  return useDaemonStore((state) => enabledTaskCount(state.scheduledTasks));
}

function enabledTaskCount(tasks: ReadonlyMap<string, ScheduledTask> | undefined): number | undefined {
  if (!tasks) return undefined;
  let enabled = 0;
  for (const task of tasks.values()) {
    if (task.enabled) enabled++;
  }
  return enabled;
}
