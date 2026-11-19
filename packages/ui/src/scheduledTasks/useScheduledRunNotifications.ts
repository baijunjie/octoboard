import { useEffect } from "react";

import { useStableHandler } from "../components/hooks/useLatest";
import { daemonMessage } from "../errors/daemonMessage";
import { isolate } from "../i18n/isolate";
import { currentLanguage, t } from "../i18n/language";
import type { NotificationCapability } from "../platform";
import { usePlatform } from "../platform/react";
import type { ScheduledRunOutcome } from "../protocol";
import { type Daemon, useDaemon } from "../store";
import { taskName } from "./taskName";

/** The target a failed run's notification carries, by which choosing it opens the page on that task. */
const TARGET_PREFIX = "scheduled-task:";

/**
 * Raises a system notification for each run of a scheduled task that fails to start its session, as the daemon
 * records it (`scheduled_task_run_added` with a failed outcome); a started run raises none, since its session shows in
 * the sidebar. The notification names the task and the failure worded from its code, in the language and with the
 * names of the moment it arrives. Choosing it brings the window forward and opens the Scheduled tasks page on that task
 * (`onOpen`), where the platform hears the choice.
 *
 * Best-effort, as the waiting notifications are (`useWaitingNotifications`): with no notifications on the platform, the
 * permission declined or a call failing, nothing is shown and nothing is reported, since the run is recorded on the page.
 */
export function useScheduledRunNotifications(onOpen: (taskId: string) => void): void {
  const { notifications } = usePlatform();
  const daemon = useDaemon();
  const open = useStableHandler(onOpen);

  useEffect(() => {
    if (!notifications) return;
    return daemon.onScheduledTaskRun((run) => {
      if (run.outcome.kind === "failed") void notifyFailedRun(notifications, daemon, run.task_id, run.outcome);
    });
  }, [notifications, daemon]);

  useEffect(() => {
    if (!notifications?.onChosen) return;
    return notifications.onChosen((target) => {
      if (target.startsWith(TARGET_PREFIX)) open(target.slice(TARGET_PREFIX.length));
    });
  }, [notifications]);
}

/** Raises one failed run's notification; outside the hook, where the compiler does not have to follow its `try`. */
async function notifyFailedRun(notifications: NotificationCapability, daemon: Daemon, taskId: string, failure: Extract<ScheduledRunOutcome, { kind: "failed" }>): Promise<void> {
  try {
    if (!(await notifications.ensurePermission())) return;
    const state = daemon.store.getState();
    const task = state.scheduledTasks?.get(taskId);
    const reason = daemonMessage(currentLanguage(), failure.code, failure.params, failure.message, state);
    await notifications.notify({
      title: t("scheduledTasks.notification.failedTitle"),
      body: task ? t("scheduledTasks.notification.failedBody", { name: isolate(taskName(task)), reason }) : reason,
      target: `${TARGET_PREFIX}${taskId}`,
    });
  } catch {
    // Best-effort — see the hook's doc.
  }
}
