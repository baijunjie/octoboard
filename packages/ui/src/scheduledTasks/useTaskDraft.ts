import { useState } from "react";

import { choiceKey, findEntry } from "../accounts/choiceEntries";
import { useDialogAction } from "../components/dialog/dialogHooks";
import { useUnsavedChanges } from "../components/dialog/UnsavedChanges";
import { useValidationMarks } from "../components/form/useValidationMarks";
import { accountRefusalMessage } from "../dialogs/accountRefusalMessage";
import { useChoiceGroups } from "../dialogs/useLaunchChoice";
import { useNow } from "../format/useNow";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Console, Project, Schedule, ScheduledTask, ScheduledTaskLaunch } from "../protocol";
import { type State, useDaemonStore } from "../store";
import { sameValue } from "../values/sameValue";
import { isBlankPrompt } from "./blankPrompt";
import { draftAfter, draftBase, draftOver, fieldsSaved, NO_DRAFT, type TaskDraftEvent, type TaskSave, withSave } from "./taskDraft";
import { fieldsOf, NO_OWNER, NO_PROJECT, sameLaunch, scheduleOf, scheduleProblems, type TaskFields } from "./taskFields";
import { useTaskLaunch } from "./useTaskLaunch";
import { useTaskZone } from "./useTaskZone";

/** The console and the project, if any, the fields name, while they exist: a project that is gone, or that is no
 * longer the console's, is none, and a console that is gone is `fallback`. */
function resolveTarget(fields: Pick<TaskFields, "consoleId" | "projectId">, consoles: Console[], projects: Project[], fallback: string): { console: Console; project?: Project } {
  const found = consoles.find((one) => one.id === fields.consoleId) ?? consoles.find((one) => one.id === fallback) ?? consoles[0]!;
  return { console: found, project: projects.find((one) => one.id === fields.projectId && one.console_id === found.id) };
}

/** Whether `id` is a console session of `console` that the window holds, which a task can report to whatever its status. */
function ownerFits(s: State, id: string, console: string): boolean {
  const owner = s.sessions.get(id);
  return owner?.kind === "console" && owner.console_id === console;
}

/** Whether `task` names no console session to report to, or one the window holds. */
function ownerHeld(s: State, task: ScheduledTask | undefined): boolean {
  return task?.launch.bound_to == null || ownerFits(s, task.launch.bound_to, task.console_id);
}

/** `launch` without the console session it reports to, which is gone. */
function withoutOwner(launch: ScheduledTaskLaunch): ScheduledTaskLaunch {
  const { bound_to: _gone, ...rest } = launch;
  return rest;
}

/** Which parts of a saved task the form, showing `fields`, `prompt` (trimmed), `schedule` and `launch`, would change. */
interface Differences {
  console: boolean;
  project: boolean;
  launch: boolean;
  prompt: boolean;
  schedule: boolean;
  enabled: boolean;
}

/** The parts the form changes of `task`, whose launch the form compares with is `taskLaunch` and whose schedule it reads as
 * `taskSchedule`: the schedule through the fields' own reading (a one-off's time to the minute), and the target by the
 * ids the task names, so a console or project the window does not hold yet does not read as changed. A schedule of a
 * kind this client does not know is changed only once a kind is picked. */
function differences(
  shown: { fields: TaskFields; prompt: string; schedule: Schedule | undefined; launch: ScheduledTaskLaunch },
  task: ScheduledTask,
  taskLaunch: ScheduledTaskLaunch,
  taskSchedule: Schedule | undefined,
): Differences {
  const { fields } = shown;
  return {
    console: fields.consoleId !== task.console_id,
    project: (fields.projectId === NO_PROJECT ? null : fields.projectId) !== task.project_id,
    launch: !sameLaunch(shown.launch, taskLaunch),
    prompt: shown.prompt !== task.prompt,
    schedule: fields.kind !== undefined && (!shown.schedule || !sameValue(shown.schedule, taskSchedule)),
    enabled: fields.enabled !== task.enabled,
  };
}

/** The parts that differ in either. */
function eitherDiffers(left: Differences, right: Differences | undefined): Differences {
  if (!right) return left;
  return {
    console: left.console || right.console,
    project: left.project || right.project,
    launch: left.launch || right.launch,
    prompt: left.prompt || right.prompt,
    schedule: left.schedule || right.schedule,
    enabled: left.enabled || right.enabled,
  };
}

/**
 * The draft of the Scheduled tasks page's form, which creates a task or edits one (`task`) and is held above the page's
 * tabs, so that what was changed and not saved survives a look at Run history (the page gives another task another
 * draft by keying what holds it). Its state is a `TaskDraft` (`taskDraft.ts`, where its rules are): the form shows the
 * base, the saved task overlaid with what a save of the form sent that has not reached it, and over it the user's edits,
 * each kept only while it differs from the base. A part the user has not changed, or changed back, follows the saved
 * task through every change made elsewhere; one they changed keeps their value; an edit made while a save is out stays an
 * edit once the save lands, and a refused save leaves its values on screen as edits. The agent, account and launch
 * settings are one part of it, held by the pickers (`useTaskLaunch`). The draft is brought up to the saved task during
 * render, so that no render shows a task beside a draft that has not taken it in.
 *
 * Whether each part (the target, the launch, the prompt, the schedule, being on) differs from the saved task is
 * `changed`, and the form is `dirty` while any does (always, for a new task); a part left as it is is not sent. While a
 * save of the form has not landed, a part also counts as changed where it differs from what that save sent, so a second
 * save pressed before the first landed sends it. The schedule and the launch are compared with the saved task run
 * through the same reading as the fields (a one-off's time is read to the minute), and the target by the ids the task
 * names, so that a form nothing was changed in is never dirty for what the window cannot show of it. What this client
 * does not know of the saved task (a schedule kind a newer daemon sends, a console session that is not among the
 * console's sessions it holds as the one to report to) is shown as none and does not make the form dirty; a console
 * session of the console that is archived or still starting is shown as that session.
 *
 * A field says what is wrong with it (`shownError`) once it was edited and left, never merely because another control
 * changed; a Save pressed with something wrong saves nothing, says it of every such field and counts an
 * `invalidAttempt`, which the view moves focus to the first of. A Save that goes ahead drops every mark, all it sends
 * being valid, so a field edited and left while it is out keeps the mark it gets then. Only what the daemon refuses at
 * save blocks Save: an account
 * that is turned off, when the save names one; one that is signed out or used up is left to the run. `submit` sends the
 * change through `onSave`, which answers with the task the daemon's last reply carried; a rejection is shown in an error
 * alert. While the form holds changes that are not saved (for a new task, once anything was changed) it is held by the
 * window's guard (`useUnsavedChanges`), which asks before the page is left; `revert` is what its Discard calls, as the
 * form's own Revert does.
 */
export function useTaskDraft({ task, consoleId, consoles, projects, onSave }: {
  task: ScheduledTask | undefined;
  /** The console a new task's target starts on. */
  consoleId: string;
  consoles: Console[];
  projects: Project[];
  /** Sends the save; for a saved task, answers with the task the daemon's last reply to it carried. */
  onSave: (save: TaskSave) => Promise<ScheduledTask | undefined>;
}) {
  const t = useT();
  const now = useNow();
  // The form's starting values are taken once, so a new one-off's time does not move while the form is open.
  const [opened] = useState(now);
  const zone = useTaskZone();
  const fieldsFor = (one: ScheduledTask | undefined) => fieldsOf(one, consoleId, opened, zone.zone);
  const [state, setState] = useState(NO_DRAFT);
  // Brought up to the saved task as it is now: set during render, behind the check that it changes something.
  const draft = draftOver(state, task, fieldsFor);
  if (draft !== state) setState(draft);
  const apply = (event: TaskDraftEvent) => setState((current) => draftAfter(current, event));
  const base = task && withSave(task, draft.sent?.save);
  const marks = useValidationMarks();
  const { busy, run } = useDialogAction(undefined, t("scheduledTasks.form.saveFailedTitle"));
  const held: TaskFields = { ...draftBase(draft, task, fieldsFor), ...draft.edits };
  // A console session to report to must be one of the task's console that the window holds (the daemon takes it whatever
  // its status), or it is none, shown and saved as none; so is the saved task's own, and the base's.
  const ownerExists = useDaemonStore((s) => held.boundTo === NO_OWNER || ownerFits(s, held.boundTo, held.consoleId));
  const savedOwnerExists = useDaemonStore((s) => ownerHeld(s, task));
  const baseOwnerExists = useDaemonStore((s) => ownerHeld(s, base));
  const fields: TaskFields = ownerExists ? held : { ...held, boundTo: NO_OWNER };
  // A project that is gone (removed while the form is open) is none, and a console that is gone falls back to the one the
  // form opened on.
  const target = resolveTarget(fields, consoles, projects, consoleId);
  const projectTask = target.project !== undefined;
  // A saved task's own project, which the window may not hold yet, keeps the console session it reports to.
  const keepsOwner = projectTask || (task?.project_id != null && fields.projectId === task.project_id);

  const taskLaunch = useTaskLaunch({ base: base?.launch, console: target.console, project: target.project, fields, keepsOwner });
  const builtLaunch = taskLaunch.launch;
  const groups = useChoiceGroups();

  const change = (patch: Partial<TaskFields>) => {
    apply({ type: "edit", patch });
    marks.edit(Object.keys(patch));
  };

  const schedule = scheduleOf(fields, zone.zone);
  const prompt = fields.prompt.trim();
  const shown = { fields, prompt, schedule, launch: builtLaunch };
  const fromTask = task && differences(shown, task, savedOwnerExists ? task.launch : withoutOwner(task.launch), scheduleOf(fieldsFor(task), zone.zone));
  const fromSent = base && draft.sent && differences(shown, base, baseOwnerExists ? base.launch : withoutOwner(base.launch), scheduleOf(fieldsFor(base), zone.zone));
  const moved = fromTask && eitherDiffers(fromTask, fromSent);
  const consoleChanged = !moved || moved.console;
  const changed = {
    target: consoleChanged || !moved || moved.project,
    launch: !moved || moved.launch,
    prompt: !moved || moved.prompt,
    schedule: !moved || moved.schedule,
    enabled: moved !== undefined && moved.enabled,
  };
  const dirty = changed.target || changed.launch || changed.prompt || changed.schedule || changed.enabled;
  const problems = scheduleProblems(fields, now, changed.schedule, zone.zone);

  // What the daemon refuses at save: an account that is turned off, where the save names one (a launch that leaves it to
  // its target names none). A signed-out or used-up one, or a sharing change under way, is decided when the task runs.
  const sentLaunch = changed.launch ? builtLaunch : undefined;
  const named = sentLaunch?.agent !== undefined && sentLaunch.account !== undefined ? findEntry(groups, choiceKey({ agent: sentLaunch.agent, account: sentLaunch.account })) : undefined;
  const accountError = named?.disabled ? accountRefusalMessage("accountChoice.disabled.description", named.name) : undefined;

  const errors = {
    prompt: isBlankPrompt(prompt) ? t("scheduledTasks.form.prompt.required") : undefined,
    runAt: problems.runAt === "missing" ? t("scheduledTasks.form.runAt.required") : problems.runAt === "past" ? t("scheduledTasks.form.runAt.past") : undefined,
    time: problems.time ? t("scheduledTasks.form.time.required") : undefined,
    days: problems.days ? t("scheduledTasks.form.days.required") : undefined,
  };
  const valid = !errors.prompt && !errors.runAt && !errors.time && !errors.days && accountError === undefined;
  const shownError = (field: keyof typeof errors) => marks.shown(field, errors[field]);

  const revert = () => {
    apply({ type: "revert" });
    taskLaunch.revert();
    marks.forget();
  };
  // A new task is dirty from the start, so the form is only held once something was changed: a picker was used, or a
  // field differs from what it opened with. A saved task is held while it differs from the task.
  const edited = taskLaunch.edited || Object.keys(draft.edits).length > 0;

  const submit = () => {
    if (!dirty || busy) return;
    if (!valid || (changed.schedule && !schedule)) {
      marks.attempt();
      return;
    }
    const save: TaskSave = {
      ...(changed.target && { target: { ...(consoleChanged && { console_id: target.console.id }), project_id: target.project?.id ?? null } }),
      ...(changed.launch && { launch: builtLaunch }),
      ...(changed.prompt && { prompt }),
      ...(changed.schedule && schedule && { schedule }),
      ...((task ? changed.enabled : !fields.enabled) && { enabled: fields.enabled }),
    };
    // The marks of what is sent go with it; a field edited and left while the save is out is marked afresh.
    marks.forget();
    // A new task has no saved task to follow: its draft stays as it is until the task it creates is shown.
    if (task) apply({ type: "sent", save, fields: fieldsSaved(fields, save) });
    void run(async () => {
      const stored = await onSave(save).catch((error: unknown) => {
        if (task) {
          apply({ type: "refused" });
          taskLaunch.keep();
        }
        throw error;
      });
      if (task) apply({ type: "answered", task: stored });
    });
  };
  // Held by the window's guard while it has changes that are not saved, so that leaving the page asks first; Revert, which
  // the form's own button and the guard's Discard both call, is the discard.
  useUnsavedChanges({ unsaved: task ? dirty : edited, saving: busy, discard: revert });

  return {
    fields,
    change,
    leave: marks.leave,
    shownError,
    /** The turned-off account the save names, which refuses it. */
    accountError,
    /** The console and the project, if any, the fields name. */
    target,
    projectTask,
    zone,
    /** Where times are entered, named where the daemon's zone differs from the window's. */
    zoneDescription: zone.named ? t("scheduledTasks.form.zoneDescription", { zone: isolate(zone.zone!) }) : undefined,
    /** The agent, the account and the launch settings, as the pickers show and change them. */
    launchPickers: taskLaunch.pickers,
    changed,
    dirty,
    busy,
    invalidAttempt: marks.attempts,
    submit,
    revert,
    /** A day's checkbox was changed: the field is left at once, as it has no blur of its own. */
    leaveDays: () => marks.markLeft("days"),
  };
}
