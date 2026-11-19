import type { Location } from "../navigation/history";
import { scheduledTaskPreference } from "../scheduledTasks/scheduledTaskPreference";
import { useScheduledTasksSync } from "../scheduledTasks/useScheduledTasksSync";
import { useDaemonStore } from "../store";
import { useAnchoredPage } from "./useAnchoredPage";

/**
 * Whether the Scheduled tasks page shows, and the task it shows. Every window of a daemon has the page, since the
 * daemon holds the tasks, but only while that daemon can be used (`available`): the tasks are listed from it once it
 * is (`useScheduledTasksSync`), and the page is gone while it cannot be.
 *
 * The page is a page of its own beside a console's sessions, as the Remote devices page is. It shows over the console
 * and the session that were current when it was opened, and is gone as soon as either changes, in the very render that
 * changes it, by whatever way (a console's avatar, a session chosen from a notification, Back), except that while a form
 * on it holds changes that are not saved (`held`) a change nobody asked for (the session under it deleted) re-anchors it
 * instead, so that such a change never takes the form away. A page the user leaves asks first, and a confirmed leave
 * calls `leave` or `follow` itself, so `held` never brings a left page back. The task shown is the one last chosen while
 * it still exists, else the first; choosing one (`select`) is remembered per daemon.
 *
 * The anchoring, the choice kept per daemon and whether a new task's form is open (`adding`) are `useAnchoredPage`'s.
 */
export function useScheduledTasksPage({ available, consoleId, sessionId, held }: {
  available: boolean;
  /** The current console and the selected session, which the page shows over. */
  consoleId: string | undefined;
  sessionId: string | undefined;
  /** A form on the page holds changes that are not saved. */
  held: boolean;
}) {
  const listing = useScheduledTasksSync(available);
  const stored = useDaemonStore((s) => s.scheduledTasks);
  const tasks = available && stored ? [...stored.values()] : undefined;
  const page = useAnchoredPage({ consoleId, sessionId, held, available, preference: scheduledTaskPreference });
  const task = (tasks?.find((one) => one.id === page.chosen) ?? tasks?.[0])?.id;
  return {
    tasks,
    listing,
    shown: page.shown,
    task,
    adding: page.adding,
    startAdd: page.startAdd,
    cancelAdd: page.cancelAdd,
    select: page.select,
    show: page.show,
    leave: page.leave,
    follow: (target: Location) => page.follow(target, target.tasks),
  };
}
