import { CircleAlert, Clock, ClockPlus, Play, Plus, Power, PowerOff, Trash2 } from "lucide-react";
import React, { useId, useRef } from "react";

import { useHandoffAfterRemoval } from "../components/focus/HandoffOnUnmount";
import { FOCUSABLE } from "../components/focus/selectors";
import { RowControls, RowIconGroup } from "../components/controls/RowControls";
import { ActionMenu } from "../components/menus/ActionMenu";
import { useListMotion } from "../components/motion/useListMotion";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { PANE_HEADER_FRAME_CLASS } from "../components/panel/paneHeaderFrame";
import { RowIconButton } from "../components/rows/RowIconButton";
import { RowLabel, TreeRow } from "../components/rows/TreeRow";
import { FadedScroller } from "../components/scroll/scrollFade";
import { useNow } from "../format/useNow";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import { retryCanHelp } from "../errors/errorRecovery";
import { Button, Tooltip } from "../primitives";
import type { ScheduledTask } from "../protocol";
import { useScheduledTasks } from "./ScheduledTasks";
import { countdownWords, formatInstant, scheduleShort, scheduleWords } from "./scheduleText";
import { taskName } from "./taskName";
import { useTarget } from "./useTarget";
import { useTaskZone } from "./useTaskZone";

/** The sidebar's content while the Scheduled tasks page shows: a header with the page's title and New scheduled task,
 * and below it the tasks, oldest first, one row each: the task's name, and under it one quiet line with its schedule in
 * a few words, then the name of the console or project it starts a session in (the part that fades where the row is
 * too narrow), and at the line's end how long until it next runs, or that it is off; a task that is off is drawn
 * marked by its muted name and the word Off. The countdown's tooltip is the time of the next run, and the line's own is its whole text where it is cut
 * short; the long wording, with that time, is the row's description for assistive technology.
 * The row's menu (Run now, unavailable while that task's own run is being asked for; Enable or Disable; Delete) opens
 * from its ⋮ and on a right-click. The selected row is marked as the current item. With none, one compact
 * empty row stands in the list's place, whose press starts New scheduled task; with no console to start a session in,
 * New scheduled task is unavailable with that as its reason, and the row is no button. Nothing is listed until the
 * tasks have been read, and a failure to read them is a compact row with Try again in the list's place. */
export function ScheduledTasksSidebar(): React.ReactElement {
  const t = useT();
  const { tasks, failure, retrying, retry, canAdd, selectedId, select, startAdd, setEnabled, askDelete, running, runNow } = useScheduledTasks();
  const listRef = useListMotion<HTMLDivElement>();
  // The failure goes, with the Try again that was pressed, when a retry succeeds: focus held on it goes to the first
  // control of the list that takes its place (the first task, or the empty row's New scheduled task), and with none (an
  // empty list and no console, whose row has no action) to the header's New scheduled task, which keeps focus while it is
  // unavailable; the ring follows the last input.
  const scroller = useRef<HTMLDivElement>(null);
  const header = useRef<HTMLDivElement>(null);
  const handoffFrom = useHandoffAfterRemoval(() => scroller.current?.querySelector<HTMLElement>(FOCUSABLE) ?? header.current?.querySelector<HTMLElement>("button"));
  const noConsole = t("scheduledTasks.noConsoles");
  return (
    <>
      <div ref={header} className={`${PANE_HEADER_FRAME_CLASS} gap-2 px-(--pane-inset)`}>
        <h2 className="min-w-0 flex-1 text-base font-semibold">
          <RowLabel title={t("scheduledTasks.label")} className="block">{t("scheduledTasks.label")}</RowLabel>
        </h2>
        <RowIconGroup>
          <RowIconButton icon={Plus} label={t("scheduledTasks.new")} unavailable={canAdd ? undefined : noConsole} onPress={startAdd} />
        </RowIconGroup>
      </div>
      <FadedScroller ref={scroller} className="min-h-0 flex-1 px-(--row-fill-inset) pt-2 pb-2">
        {!tasks && failure && (
          // The sentence is a plain compact row with the danger glyph, and Try again a button of its own beside it, each
          // with its own name: a row that is one button runs its sentence and its action together, and shows only the
          // sentence. A failure that asking again cannot mend has no button.
          <div data-listing-failure="" className="flex items-center gap-2">
            {handoffFrom("[data-listing-failure]")}
            <div className="min-w-0 flex-1">
              <EmptyPanel compact tone="danger" icon={CircleAlert} message={t("scheduledTasks.listFailedTitle")} />
            </div>
            {retryCanHelp(failure.message) && (
              <Button size="sm" variant="secondary" focusOnPress={false} pending={retrying} className="me-1 shrink-0" onPress={retry}>
                {t("error.tryAgain")}
              </Button>
            )}
          </div>
        )}
        {tasks && tasks.length === 0 && (
          // One compact row in the list's place, whose press starts what fills it; with no console there is nothing to
          // start, and the row is plain.
          <EmptyPanel compact icon={canAdd ? ClockPlus : Clock} message={t("scheduledTasks.empty")} action={canAdd ? { label: t("scheduledTasks.new"), onPress: startAdd } : undefined} />
        )}
        {tasks && tasks.length > 0 && (
          <div ref={listRef} className="relative flex flex-col gap-0.5">
            {tasks.map((task) => (
              <div key={task.id} data-list-motion={task.id}>
                <TaskRow task={task} selected={task.id === selectedId} running={running.has(task.id)} onSelect={select} onRunNow={runNow} onSetEnabled={setEnabled} onDelete={askDelete} />
              </div>
            ))}
          </div>
        )}
      </FadedScroller>
    </>
  );
}

/** One task's row, which the feature views of the website show on their own. */
export function TaskRow({ task, selected, running, onSelect, onRunNow, onSetEnabled, onDelete }: {
  task: ScheduledTask;
  selected: boolean;
  running: boolean;
  onSelect: (id: string) => void;
  onRunNow: (task: ScheduledTask) => void;
  onSetEnabled: (task: ScheduledTask, enabled: boolean) => void;
  onDelete: (task: ScheduledTask) => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const rowRef = useRef<HTMLDivElement>(null);
  const detailId = useId();
  const now = useNow();
  const name = taskName(task);
  const zone = useTaskZone();
  const target = useTarget(task.console_id, task.project_id);
  const schedule = scheduleWords(t, language, task.schedule, zone);
  const next = task.next_run_at;
  // The full wording the row's description carries, one message for each form; the countdown is left out of it so that it
  // does not change under a screen reader as the minutes pass. A schedule of a kind a newer daemon sends has no words, so
  // its part is left out, and Off and the next run stay.
  const time = next !== null ? formatInstant(language, next, zone) : undefined;
  const full = !task.enabled
    ? schedule === undefined ? t("scheduledTasks.row.detailOffNoSchedule", { target: target.words }) : t("scheduledTasks.row.detailOff", { target: target.words, schedule })
    : time !== undefined
      ? schedule === undefined ? t("scheduledTasks.row.detailNextNoSchedule", { target: target.words, time }) : t("scheduledTasks.row.detailNext", { target: target.words, schedule, time })
      : schedule === undefined ? target.words : t("scheduledTasks.row.detail", { target: target.words, schedule });
  const short = scheduleShort(t, language, task.schedule, zone);
  const meta = short === undefined ? isolate(target.name) : t("scheduledTasks.row.meta", { schedule: short, target: isolate(target.name) });
  const when = !task.enabled ? t("scheduledTasks.row.off") : next !== null ? countdownWords(t, language, next, now, 1) : undefined;
  return (
    <TreeRow
      ref={rowRef}
      ariaLabel={name}
      describedBy={detailId}
      selected={selected}
      onActivate={() => onSelect(task.id)}
      className="flex-col px-(--row-content-inset) pb-2"
    >
      <div className="flex h-8 min-w-0 items-center gap-(--row-control-gap) [--row-controls-gap:var(--row-control-gap)]">
        <RowLabel title={name} className={`min-w-0 flex-1 font-medium ${task.enabled ? "" : "text-app-muted"}`}>{name}</RowLabel>
        <RowControls reserve>
          <ActionMenu
            label={t("scheduledTasks.actionsLabel", { name: isolate(name) })}
            items={[
              { label: t("scheduledTasks.runNow"), icon: Play, onClick: () => onRunNow(task), disabled: running },
              task.enabled
                ? { label: t("scheduledTasks.disable"), icon: PowerOff, onClick: () => onSetEnabled(task, false) }
                : { label: t("scheduledTasks.enable"), icon: Power, onClick: () => onSetEnabled(task, true) },
              "separator",
              { label: t("common.delete"), icon: Trash2, onClick: () => onDelete(task), destructive: true },
            ]}
            contextTargetRef={rowRef}
          />
        </RowControls>
      </div>
      <div className="-mt-1 flex min-w-0 items-baseline gap-3 text-xs text-app-muted">
        <span aria-hidden="true" data-task-meta className="flex min-w-0 flex-1">
          <RowLabel title={meta} className="min-w-0">{meta}</RowLabel>
        </span>
        {when !== undefined &&
          (task.enabled && next !== null ? (
            // The countdown rounds the wait to one unit, so the time itself is a hover away.
            <Tooltip trigger="mark" content={formatInstant(language, next, zone)} touchPopover={false} className="shrink-0">
              <span aria-hidden="true" data-task-when className="tabular-nums">{when}</span>
            </Tooltip>
          ) : (
            <span aria-hidden="true" data-task-when className="shrink-0">{when}</span>
          ))}
      </div>
      <span id={detailId} className="sr-only">{full}</span>
    </TreeRow>
  );
}
