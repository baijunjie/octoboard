import { useDaemonStore } from "../store";
import { taskZone, type TaskZone } from "./scheduleText";

/** The time zone the Scheduled tasks page shows and enters times in: the one the daemon reads schedules in, as its task
 * listing names it (`State.scheduledTasksZone`), named where the window's own differs; the window's own until the tasks
 * are listed, or where the daemon names none. */
export function useTaskZone(): TaskZone {
  const zone = useDaemonStore((s) => s.scheduledTasksZone);
  return taskZone(zone);
}
