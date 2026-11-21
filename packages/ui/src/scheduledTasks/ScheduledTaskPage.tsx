import { CircleAlert, CircleCheck, Clock, Folder, Play, Plus, SquareTerminal, type LucideIcon } from "lucide-react";
import React, { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { PaneFailure } from "../components/alerts/PaneFailure";
import { FormCard } from "../components/card/FormCard";
import { focusFellToPage, focusLanding, TASKS_REGION } from "../components/focus/regionFocus";
import { MODAL_OPEN } from "../components/focus/selectors";
import { useHasPainted } from "../components/motion/useHasPainted";
import { ContentMain } from "../components/panel/ContentMain";
import { CenteredEmptyPanel } from "../components/panel/EmptyPanel";
import { FadedScroller, useFadeRegion } from "../components/scroll/scrollFade";
import { StatusBadge } from "../components/status/StatusBadge";
import { TextTabStrip } from "../components/tabs/TextTabStrip";
import { formatRelativeTime } from "../format/relativeTime";
import { useNow } from "../format/useNow";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import { ringForAutomaticFocus } from "../input/lastInput";
import { Button, TabPanel, Tabs, Tooltip } from "../primitives";
import { SettingRow } from "../settings/SettingRow";
import type { Console, Project, ScheduledTask, ScheduledTaskRun, Session } from "../protocol";
import { useDaemonStore } from "../store";
import { ScheduledTaskForm } from "./ScheduledTaskForm";
import { ScheduledTaskRuns } from "./ScheduledTaskRuns";
import { useScheduledTasks } from "./ScheduledTasks";
import { countdownWords, formatInstant, scheduleWords } from "./scheduleText";
import { taskName } from "./taskName";
import { useTarget } from "./useTarget";
import { useScheduledTaskRuns } from "./useScheduledTaskRuns";
import { useTaskDraft } from "./useTaskDraft";
import { useTaskZone } from "./useTaskZone";

/** The two views of a saved task under its header, keyed as the tabs and their panels are. */
type TaskViewKey = "settings" | "runs";

/** Space the page's body leaves under the last control, so the pinned action bar never covers it. */
const BODY_CLASS = "mx-auto flex w-full min-w-0 max-w-3xl flex-col gap-6 px-4 pt-10 pb-10 sm:px-8 sm:pt-12";

/**
 * The Scheduled tasks page's content area: the selected task under a header with its name, whether it is off, what it
 * starts, its schedule, Run now, and a card with its next and last run, then two views switched under it, Settings
 * (the task as a form, `ScheduledTaskForm`, and Delete, the one destructive action, in a row of its own) and Run history
 * (`ScheduledTaskRuns`); the chosen view is kept as another task is chosen. Settings' Save and Revert are pinned in an
 * action bar at the bottom of the area, below the scroll, while that view shows; a new task's form, titled New scheduled
 * task, has the same bar with Save and Cancel and no views; with no tasks, the empty state alone, centred, which with no
 * console to start a session in offers New console (`onNewConsole`) instead. The form's draft is held above the tabs, so
 * switching views keeps what was changed and not saved, while switching to another task asks first when the form holds
 * changes (`useUnsavedChanges`), and drops them once the user discards.
 * Nothing but the frame is drawn until the tasks have been read, and a failure to read them takes the area as the
 * failure of a whole subject, with Try again. Opening the page moves focus into it, once the tasks are read and no
 * dialog or drawer is in front; a form or failure that goes away under focus (Cancel, a retry that succeeds) hands it to
 * the page's landing.
 */
export function ScheduledTaskPage({ consoleId, drawerOpen = false, onNewConsole }: {
  /** The current console, where a new task's target starts. */
  consoleId: string | undefined;
  drawerOpen?: boolean;
  /** Opens the dialog that makes a console, which a task needs to start a session in. */
  onNewConsole: () => void;
}): React.ReactElement {
  const t = useT();
  const titleId = useId();
  const { tasks, failure, retrying, retry, selectedId, adding, startAdd, askDelete, openSession } = useScheduledTasks();
  const consoles = useDaemonStore(useShallow((s) => [...s.consoles.values()]));
  const projects = useDaemonStore(useShallow((s) => [...s.projects.values()]));
  const read = tasks !== undefined;
  const failed = !read && failure !== undefined;
  const painted = useHasPainted();
  const region = useFadeRegion({ labelledBy: read ? titleId : undefined });
  const pageRef = useRef<HTMLElement>(null);
  const [view, setView] = useState<TaskViewKey>("settings");
  const [actionBar, setActionBar] = useState<HTMLDivElement | null>(null);
  const task = tasks?.find((one) => one.id === selectedId);
  const startConsole = consoles.find((one) => one.id === consoleId)?.id ?? consoles[0]?.id;

  // Opening the page moves focus into it, once the tasks have been read (or could not be) and so there is a control to
  // land on; not over a dialog or a control of the page that already has it. A mouse open shows no ring
  // (`ringForAutomaticFocus`).
  const landed = useRef(false);
  const ready = read || failed;
  useEffect(() => {
    if (!ready || landed.current || !pageRef.current) return;
    // Not while the narrow window's drawer is open over the page, which has the focus; it lands once the drawer closes.
    if (drawerOpen) return;
    landed.current = true;
    if (document.querySelector(MODAL_OPEN) || pageRef.current.contains(document.activeElement)) return;
    ringForAutomaticFocus();
    // A new task's form lands on its first text field, which its autofocus could not take under the drawer.
    const opening = adding ? pageRef.current.querySelector<HTMLElement>("form input") : null;
    if (opening) opening.focus();
    else focusLanding(pageRef.current);
  }, [ready, drawerOpen]);

  // A new task's form (cancelled) or the failure (read again) goes away under the control that held focus, which falls
  // to `<body>`: it goes to the landing of what is shown now, as the page's opening does.
  const mode = adding ? "new" : failed ? "failed" : "shown";
  const lastMode = useRef(mode);
  useLayoutEffect(() => {
    const before = lastMode.current;
    lastMode.current = mode;
    if (before === "shown" || before === mode || !pageRef.current || !focusFellToPage()) return;
    ringForAutomaticFocus();
    focusLanding(pageRef.current);
  }, [mode]);

  const emptyPage = (content: React.ReactNode) => (
    <ContentMain ref={pageRef} data-region={TASKS_REGION} className={`min-h-0 flex-col ${region.insetRing}`}>
      <FadedScroller region={region} className="flex min-h-0 flex-1 flex-col p-8">
        {content}
      </FadedScroller>
    </ContentMain>
  );
  if (failed) {
    return (
      <ContentMain ref={pageRef} data-region={TASKS_REGION} className="min-h-0 flex-col">
        <PaneFailure title={t("scheduledTasks.listFailedTitle")} message={failure.message} announce={painted} attempt={failure.attempt} pending={retrying} landing onRetry={retry} />
      </ContentMain>
    );
  }

  let body: React.ReactNode = null;
  if (read) {
    if (startConsole === undefined) {
      // A task starts its session in a console, so there is nothing to make until there is one.
      return emptyPage(
        <CenteredEmptyPanel icon={Clock} headingLevel={1} headingId={titleId} message={t("welcome.heading")} description={t("scheduledTasks.noConsoles.description")} action={{ label: t("rail.newConsole"), icon: Plus, onPress: onNewConsole }} />,
      );
    } else if (adding) {
      body = <NewTaskView titleId={titleId} consoleId={startConsole} consoles={consoles} projects={projects} actionBar={actionBar} />;
    } else if (tasks.length === 0) {
      // None: the empty state alone, centred in the area, with the line that says what a scheduled task does. Its
      // heading is the page's.
      return emptyPage(
        <CenteredEmptyPanel icon={Clock} headingLevel={1} headingId={titleId} message={t("scheduledTasks.empty")} description={t("scheduledTasks.empty.description")} action={{ label: t("scheduledTasks.new"), icon: Plus, onPress: startAdd }} />,
      );
    } else if (task) {
      body = (
        <TaskView key={task.id} titleId={titleId} task={task} view={view} onViewChange={setView} consoleId={startConsole} consoles={consoles} projects={projects} actionBar={actionBar} onOpenSession={openSession} onDelete={askDelete} />
      );
    }
  }

  return (
    <ContentMain ref={pageRef} data-region={TASKS_REGION} className={`min-h-0 flex-col ${region.insetRing}`}>
      <FadedScroller region={region} className="flex min-h-0 flex-1 flex-col">
        <div className={BODY_CLASS}>{body}</div>
      </FadedScroller>
      {/* Save and Revert, which the form draws here, stay in view while the form scrolls above. */}
      <div ref={setActionBar} className="empty:hidden [&:not(:empty)]:groove-t [&:not(:empty)]:py-3 [&>div]:mx-auto [&>div]:max-w-3xl [&>div]:px-4 sm:[&>div]:px-8" />
    </ContentMain>
  );
}

/** A new task's form under its heading, with the draft it holds. */
function NewTaskView({ titleId, consoleId, consoles, projects, actionBar }: {
  titleId: string;
  consoleId: string;
  consoles: Console[];
  projects: Project[];
  actionBar: HTMLElement | null;
}): React.ReactElement {
  const t = useT();
  const { save, cancelAdd } = useScheduledTasks();
  const draft = useTaskDraft({ task: undefined, consoleId, consoles, projects, onSave: (change) => save(undefined, change) });
  return (
    <>
      <h1 id={titleId} className="text-2xl font-semibold">{t("scheduledTasks.new")}</h1>
      <ScheduledTaskForm draft={draft} consoles={consoles} projects={projects} actionBar={actionBar} autoFocus onCancel={cancelAdd} />
    </>
  );
}

/** A saved task under its header: the views Settings and Run history, each mounted while it shows. The draft of the
 * form is held here, above the tabs, so that what was changed survives a look at Run history, and the runs are listed
 * here, for the header's last run and the history. */
function TaskView({ titleId, task, view, onViewChange, consoleId, consoles, projects, actionBar, onOpenSession, onDelete }: {
  titleId: string;
  task: ScheduledTask;
  view: TaskViewKey;
  onViewChange: (view: TaskViewKey) => void;
  consoleId: string;
  consoles: Console[];
  projects: Project[];
  actionBar: HTMLElement | null;
  onOpenSession: (session: Session) => void;
  onDelete: (task: ScheduledTask) => void;
}): React.ReactElement {
  const t = useT();
  const { save } = useScheduledTasks();
  const runs = useScheduledTaskRuns(task.id);
  const draft = useTaskDraft({ task, consoleId, consoles, projects, onSave: (change) => save(task.id, change) });
  return (
    <>
      <TaskHeader titleId={titleId} task={task} runs={runs.runs} />
      <Tabs variant="underline" selectedKey={view} onSelectionChange={(key) => onViewChange(key as TaskViewKey)} className="gap-6">
        <TextTabStrip underline="groove" label={t("scheduledTasks.view.label")} tabs={[{ id: "settings", label: t("scheduledTasks.view.settings") }, { id: "runs", label: t("scheduledTasks.view.runs") }]} />
        <TabPanel id="settings" className="mt-0 flex flex-col gap-6 p-0">
          <ScheduledTaskForm draft={draft} consoles={consoles} projects={projects} actionBar={actionBar} />
          <FormCard>
            {/* The row's own padding is the card's. */}
            <div className="-my-4">
              <SettingRow label={t("scheduledTasks.deleteTitle")} compact>
                <Button size="sm" variant="danger-soft" focusOnPress={false} describedBy="" label={t("scheduledTasks.deleteLabel", { name: isolate(taskName(task)) })} onPress={() => onDelete(task)}>
                  {t("common.delete")}
                </Button>
              </SettingRow>
            </div>
          </FormCard>
        </TabPanel>
        <TabPanel id="runs" className="mt-0 flex flex-col gap-6 p-0">
          <ScheduledTaskRuns runs={runs} onOpenSession={onOpenSession} />
        </TabPanel>
      </Tabs>
    </>
  );
}

/** A saved task's header: its name as the page's heading, an Off badge while it does not run on its schedule, and Run
 * now, which is there whatever the schedule and whether the task is on, and unavailable while its own request is
 * awaiting its answer; under the name one quiet line with what it starts, as a glyph and a name, and its schedule in
 * words; then one card with two figures side by side, stacked where the area is narrow: when it next runs (the
 * countdown, with the time under it) and when it last ran (how long ago, a mark of what came of it where the runs say,
 * and the time under it). Times are in the daemon's zone, which a line names where the window's own differs. The
 * countdown and the time ago, which change as the minutes pass, are hidden from assistive technology, which reads the
 * times under them. */
function TaskHeader({ titleId, task, runs }: { titleId: string; task: ScheduledTask; runs: ScheduledTaskRun[] | undefined }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const zone = useTaskZone();
  const now = useNow();
  const { running, runNow } = useScheduledTasks();
  const name = taskName(task);
  const target = useTarget(task.console_id, task.project_id);
  const when = scheduleWords(t, language, task.schedule, zone);
  const TargetIcon = task.project_id ? Folder : SquareTerminal;
  const next = task.next_run_at;
  const last = task.last_run_at;
  // The latest kept run is the last one, once the runs have been listed.
  const lastOutcome = last !== null ? runs?.[0]?.outcome.kind : undefined;
  return (
    <header className="@container flex min-w-0 flex-col gap-5">
      <div className="flex min-w-0 items-start justify-between gap-4">
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <h1 id={titleId} dir="auto" className="min-w-0 text-2xl font-semibold [overflow-wrap:anywhere]">{name}</h1>
            {!task.enabled && <StatusBadge variant="soft">{t("scheduledTasks.row.off")}</StatusBadge>}
          </div>
          <p className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 text-sm text-app-muted">
            <span className="flex min-w-0 items-center gap-1.5">
              <TargetIcon className="size-4 shrink-0" aria-hidden />
              <span aria-hidden="true" dir="auto" className="min-w-0 [overflow-wrap:anywhere]">{target.name}</span>
              <span className="sr-only">{target.words}</span>
            </span>
            {/* A schedule of a kind a newer daemon sends has no words, and no separator. */}
            {when !== undefined && (
              <>
                <span aria-hidden="true">·</span>
                <span className="min-w-0 [overflow-wrap:anywhere]">{when}</span>
              </>
            )}
          </p>
        </div>
        <Button size="sm" className="shrink-0" variant="secondary" focusOnPress={false} pending={running.has(task.id)} label={t("scheduledTasks.runNowLabel", { name: isolate(name) })} onPress={() => runNow(task)}>
          <Play className="size-4" />
          {t("scheduledTasks.runNow")}
        </Button>
      </div>
      <dl className="card-fills grid grid-cols-1 rounded-lg border border-transparent bg-(--panel-row) shadow-(--panel-shadow) @md:grid-cols-2">
        <Figure label={t("scheduledTasks.header.nextRun")}>
          {!task.enabled ? (
            <FigureValue muted>{t("scheduledTasks.row.off")}</FigureValue>
          ) : next === null ? (
            <FigureValue muted>{t("scheduledTasks.header.noNextRun")}</FigureValue>
          ) : (
            <>
              <FigureValue hidden>{countdownWords(t, language, next, now)}</FigureValue>
              <FigureNote>{formatInstant(language, next, zone)}</FigureNote>
            </>
          )}
        </Figure>
        <Figure label={t("scheduledTasks.header.lastRun")} className="hairline-t @md:border-t-0 @md:hairline-s">
          {last === null ? (
            <FigureValue muted>{t("scheduledTasks.header.neverRun")}</FigureValue>
          ) : (
            <>
              <span className="flex min-w-0 items-center gap-2">
                <FigureValue hidden>{formatRelativeTime(language, last, now)}</FigureValue>
                {lastOutcome === "started" && <OutcomeMark icon={CircleCheck} label={t("scheduledTasks.header.lastRun.started")} className="text-success-soft-foreground" />}
                {lastOutcome === "failed" && <OutcomeMark icon={CircleAlert} label={t("scheduledTasks.runs.failed")} className="text-danger-soft-foreground" />}
              </span>
              <FigureNote>{formatInstant(language, last, zone)}</FigureNote>
            </>
          )}
        </Figure>
      </dl>
      {zone.named && <p className="-mt-2 text-xs text-app-muted">{t("scheduledTasks.header.zone", { zone: isolate(zone.zone!) })}</p>}
    </header>
  );
}

/** One figure of the header's card: a quiet label, then its value and the note under it. */
function Figure({ label, className = "", children }: { label: string; className?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div className={`flex min-w-0 flex-col gap-1 px-4 py-3.5 ${className}`}>
      <dt className="text-xs text-app-muted">{label}</dt>
      <dd className="flex min-w-0 flex-col gap-0.5">{children}</dd>
    </div>
  );
}

/** A figure's value, the line the eye lands on; `hidden` keeps one that changes as the minutes pass from assistive
 * technology, which reads the note under it. */
function FigureValue({ muted = false, hidden = false, children }: { muted?: boolean; hidden?: boolean; children: React.ReactNode }): React.ReactElement {
  return <span aria-hidden={hidden || undefined} className={`min-w-0 text-lg leading-6 font-semibold tabular-nums [overflow-wrap:anywhere] ${muted ? "text-app-muted" : ""}`}>{children}</span>;
}

function FigureNote({ children }: { children: React.ReactNode }): React.ReactElement {
  return <span className="min-w-0 text-xs text-app-muted [overflow-wrap:anywhere]">{children}</span>;
}

/** A small mark of what came of the last run, named for assistive technology and in a tooltip. */
function OutcomeMark({ icon: Icon, label, className }: { icon: LucideIcon; label: string; className: string }): React.ReactElement {
  return (
    <Tooltip trigger="mark" content={label} label={label} className="inline-flex shrink-0">
      <Icon className={`size-4 ${className}`} aria-hidden />
    </Tooltip>
  );
}
