import { useDaemon } from "../store";
import { type ListingFailure, useSnapshotListing } from "./useSnapshotListing";

/**
 * Lists the daemon's scheduled tasks whenever the window has the snapshot of its connection, and again after every
 * later snapshot (a reconnect or a lag recovery), since tasks are not part of one: the reply lands in the store
 * (`State.scheduledTasks`), which the change broadcasts keep up to date in between. A failure other than the connection
 * going is returned as `failure`, for the page to draw in the list's place with Try again (`retry`); it raises no alert,
 * being a load nobody asked for, and the lost connection lists again on its next snapshot.
 */
export function useScheduledTasksSync(enabled: boolean): { failure: ListingFailure | undefined; pending: boolean; retry: () => void } {
  const { request } = useDaemon();
  return useSnapshotListing(enabled, "", () => request({ type: "list_scheduled_tasks" }), ["connection_closed"]);
}
