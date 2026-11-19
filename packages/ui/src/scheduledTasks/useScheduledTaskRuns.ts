import type { ScheduledTaskRun } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { type ListingFailure, useSnapshotListing } from "./useSnapshotListing";

/**
 * The kept runs of `taskId`, most recent first, or `undefined` until they have been listed, with the failure of
 * listing them. They are listed whenever the task is shown and again after every snapshot of the connection (a
 * reconnect or a lag recovery), since runs are not part of one: the reply lands in the store (`State.scheduledTaskRuns`),
 * which `scheduled_task_run_added` keeps up to date in between. A failure other than the connection going is
 * `failure`, for the page to draw in the history's place with Try again (`retry`), and raises no alert; the lost
 * connection lists again on its next snapshot. A task deleted meanwhile (`unknown_scheduled_task`) is gone from the page
 * with it, and says nothing.
 */
export function useScheduledTaskRuns(taskId: string): { runs: ScheduledTaskRun[] | undefined; failure: ListingFailure | undefined; pending: boolean; retry: () => void } {
  const { request } = useDaemon();
  const runs = useDaemonStore((s) => s.scheduledTaskRuns.get(taskId));
  const listing = useSnapshotListing(true, taskId, () => request({ type: "list_scheduled_task_runs", task: taskId }), ["connection_closed", "unknown_scheduled_task"]);
  return { runs, ...listing };
}
