import React, { createContext, useContext, useEffect, useRef, useState } from "react";

import { alertFailure } from "../components/alerts/raiseError";
import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { useLeaveGuard } from "../components/dialog/UnsavedChanges";
import { focusAfterModalCloses } from "../components/focus/modalFocus";
import { useStableHandler } from "../components/hooks/useLatest";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import type { ScheduledTask, Session } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import type { TaskSave } from "./taskDraft";
import { taskName } from "./taskName";
import type { ListingFailure } from "./useSnapshotListing";

/** What the Scheduled tasks page's sidebar and content area share. */
export interface ScheduledTasksState {
  /** The daemon's tasks, oldest first; `undefined` until they have been listed. */
  tasks: ScheduledTask[] | undefined;
  /** Why they could not be listed, while that is so: drawn in the list's place, with `retry`. */
  failure: ListingFailure | undefined;
  /** A listing asked for is awaiting its answer. */
  retrying: boolean;
  retry: () => void;
  /** The task the content area shows, if any exists; none while a new one is being made. */
  selectedId: string | undefined;
  /** A new task's form is in the content area. */
  adding: boolean;
  /** A task starts its session in a console, so with none a new one cannot be made. */
  canAdd: boolean;
  select: (id: string) => void;
  startAdd: () => void;
  cancelAdd: () => void;
  /** Creates a task (`id` absent) or changes one; a new one is then the one shown. A change answers with the task the
   * daemon's last reply to it carried (its `update_scheduled_task`, or the `set_scheduled_task_enabled` sent after it),
   * which tells the form when the change has reached the store; a creation answers with none. */
  save: (id: string | undefined, save: TaskSave) => Promise<ScheduledTask | undefined>;
  setEnabled: (task: ScheduledTask, enabled: boolean) => void;
  askDelete: (task: ScheduledTask) => void;
  /** The tasks whose Run now is being asked for, which cannot be asked for again until the daemon answers. */
  running: ReadonlySet<string>;
  /** Runs a task once now, whatever its schedule and whether it is enabled. */
  runNow: (task: ScheduledTask) => void;
  /** Shows a session a task's run started, leaving the page. */
  openSession: (session: Session) => void;
}

const ScheduledTasksContext = createContext<ScheduledTasksState | undefined>(undefined);

export function useScheduledTasks(): ScheduledTasksState {
  const state = useContext(ScheduledTasksContext);
  if (!state) throw new Error("useScheduledTasks outside ScheduledTasksProvider");
  return state;
}

/**
 * What the Scheduled tasks page keeps beside the window's choice of task (`selectedId`, `onSelect`, `adding`, which the
 * window keeps so that a selection from outside the page, a notification or the navigation history, ends a new task's
 * form too): the Delete confirmation, and the requests that save, switch, run and delete a task, with which tasks' Run
 * now is awaiting its answer, and the question before another task or a new task's form replaces a form with changes not
 * saved (`useLeaveGuard`). The tasks themselves are the store's (`State.scheduledTasks`), kept up to date by the
 * daemon's broadcasts. A save answered after the form was cancelled, replaced or the page left selects nothing. The
 * sidebar (`ScheduledTasksSidebar`) and the content area (`ScheduledTaskPage`) read this through `useScheduledTasks`.
 * The handlers every row takes keep their identity.
 */
export function ScheduledTasksProvider({ tasks, listing, selectedId, adding, onSelect, onAdd, onCancelAdd, onOpenSession, children }: {
  tasks: ScheduledTask[] | undefined;
  /** The failure of listing the tasks, if any, and its retry. */
  listing: { failure: ListingFailure | undefined; pending: boolean; retry: () => void };
  selectedId: string | undefined;
  adding: boolean;
  /** A task was chosen: shows it, which ends a new task's form. */
  onSelect: (id: string) => void;
  /** A new task's form was asked for. */
  onAdd: () => void;
  onCancelAdd: () => void;
  /** A session a run started was chosen: shows it as selecting it from elsewhere in the window does, leaving the page. */
  onOpenSession: (session: Session) => void;
  children: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const canAdd = useDaemonStore((s) => s.consoles.size > 0);
  // Counts what ends or replaces a new task's form, however it ends, so that a save answered after is selects nothing.
  const addEpoch = useRef(0);
  useEffect(() => {
    addEpoch.current++;
  }, [adding]);
  const [deleting, setDeleting] = useState<ScheduledTask>();
  const [running, setRunning] = useState<ReadonlySet<string>>(new Set());

  const leaveForm = useLeaveGuard();
  // Choosing the task already shown, or asking for the form already open, replaces nothing and asks nothing; any other
  // choice asks first while the form holds changes that are not saved.
  const select = useStableHandler((id: string) => {
    if (!adding && id === selectedId) onSelect(id);
    else leaveForm(() => onSelect(id));
  });
  const startAdd = useStableHandler(() => {
    if (canAdd && !adding) leaveForm(onAdd);
  });
  const setEnabled = useStableHandler((task: ScheduledTask, enabled: boolean) => {
    request({ type: "set_scheduled_task_enabled", task: task.id, enabled }).catch(
      alertFailure({ title: t(enabled ? "scheduledTasks.enableFailedTitle" : "scheduledTasks.disableFailedTitle") }),
    );
  });
  const runNow = useStableHandler((task: ScheduledTask) => {
    if (running.has(task.id)) return;
    setRunning((current) => new Set([...current, task.id]));
    request({ type: "run_scheduled_task", task: task.id })
      .catch(alertFailure({ title: t("scheduledTasks.runFailedTitle") }))
      .finally(() =>
        setRunning((current) => {
          const next = new Set(current);
          next.delete(task.id);
          return next;
        }),
      );
  });

  const state: ScheduledTasksState = {
    tasks,
    failure: listing.failure,
    retrying: listing.pending,
    retry: listing.retry,
    selectedId: adding ? undefined : selectedId,
    adding,
    canAdd,
    select,
    startAdd,
    cancelAdd: onCancelAdd,
    save: async (id, save) => {
      if (id !== undefined) {
        const { target, launch, prompt, schedule, enabled } = save;
        let last: ScheduledTask | undefined;
        if (target || launch || prompt !== undefined || schedule) {
          last = (await request({ type: "update_scheduled_task", task: id, ...target, ...(launch && { launch }), ...(prompt !== undefined && { prompt }), ...(schedule && { schedule }) })).task;
        }
        if (enabled !== undefined) last = (await request({ type: "set_scheduled_task_enabled", task: id, enabled })).task;
        return last;
      }
      const epoch = addEpoch.current;
      const { task } = await request({
        type: "create_scheduled_task",
        console_id: save.target!.console_id!,
        ...(save.target!.project_id && { project_id: save.target!.project_id }),
        launch: save.launch,
        prompt: save.prompt!,
        schedule: save.schedule!,
        ...(save.enabled === false && { enabled: false }),
      });
      if (epoch === addEpoch.current) onSelect(task.id);
      return undefined;
    },
    setEnabled,
    askDelete: setDeleting,
    running,
    runNow,
    openSession: onOpenSession,
  };

  return (
    <ScheduledTasksContext.Provider value={state}>
      {children}
      {deleting && (
        <ConfirmDialog
          title={t("scheduledTasks.deleteTitle")}
          errorTitle={t("scheduledTasks.deleteFailedTitle")}
          subject={<Message id="scheduledTasks.deleteSubject" params={{ name: <InlineReference dir="auto" wrap>{taskName(deleting)}</InlineReference> }} />}
          message={t("scheduledTasks.deleteMessage")}
          confirmLabel={t("common.delete")}
          destructive
          resetKey={deleting.id}
          onConfirm={async () => {
            await request({ type: "delete_scheduled_task", task: deleting.id });
            setDeleting(undefined);
            // The control that opened the confirmation goes with the task: focus lands on the task now shown, or the
            // empty state's action, where the modal's own return finds nothing.
            focusAfterModalCloses();
          }}
          onCancel={() => setDeleting(undefined)}
        />
      )}
    </ScheduledTasksContext.Provider>
  );
}
