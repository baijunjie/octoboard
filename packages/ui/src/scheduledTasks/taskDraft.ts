import type { Schedule, ScheduledTask, ScheduledTaskLaunch } from "../protocol";
import { sameValue } from "../values/sameValue";
import { sameLaunchSettings, type TaskFields } from "./taskFields";

/** What a save of the form changes. A new task carries every part; a saved one only the parts that changed, so that a
 * change made elsewhere to another part stands. `target.project_id` follows the daemon's tri-state: a project, or
 * `null` for a console task, and left out only where the project did not change. */
export interface TaskSave {
  target?: { console_id?: string; project_id?: string | null };
  launch?: ScheduledTaskLaunch;
  prompt?: string;
  schedule?: Schedule;
  enabled?: boolean;
}

/** The fields each part of a save is made from; the launch's agent, account and settings are the pickers'
 * (`useTaskLaunch`). */
const PART_FIELDS: { [Part in keyof TaskSave]-?: (keyof TaskFields)[] } = {
  target: ["consoleId", "projectId"],
  launch: ["name", "boundTo"],
  prompt: ["prompt"],
  schedule: ["kind", "runAt", "minute", "day", "time", "days"],
  enabled: ["enabled"],
};

/** The values of `fields` that the parts `save` holds were made from. */
export function fieldsSaved(fields: TaskFields, save: TaskSave): Partial<TaskFields> {
  const parts = (Object.keys(PART_FIELDS) as (keyof TaskSave)[]).filter((part) => save[part] !== undefined);
  return Object.fromEntries(parts.flatMap((part) => PART_FIELDS[part].map((key) => [key, fields[key]])));
}

/** `task` as it is once the parts `save` holds are stored over it. */
export function withSave(task: ScheduledTask, save: TaskSave | undefined): ScheduledTask {
  if (!save) return task;
  const { target, launch, prompt, schedule, enabled } = save;
  return {
    ...task,
    ...(target?.console_id !== undefined && { console_id: target.console_id }),
    ...(target?.project_id !== undefined && { project_id: target.project_id }),
    ...(launch && { launch }),
    ...(prompt !== undefined && { prompt }),
    ...(schedule && { schedule }),
    ...(enabled !== undefined && { enabled }),
  };
}

/** A save of the form that has not reached the form's task yet: the parts it sent (`save`), the fields they were made
 * from (`fields`), and, once the daemon has answered every request of it, the task its last reply carried
 * (`answered`). */
export interface SentSave {
  save: TaskSave;
  fields: Partial<TaskFields>;
  answered: ScheduledTask | undefined;
}

/** Whether `task` holds the change the daemon answered with `answer`. Every stored change of a task's definition or of
 * whether it is on stamps `updated_at` past the task's previous one, and a request that changes nothing answers with the
 * task as it is (`apps/daemon/PROTOCOL.md`, "Scheduled tasks"); so of two versions of a task the one stamped later is
 * the later, and a task stamped at or past the answer's holds it. */
function holdsAnswer(task: ScheduledTask, answer: ScheduledTask): boolean {
  return task.updated_at >= answer.updated_at;
}

/**
 * The state of the task form's draft over the saved task (`useTaskDraft`). The form shows, where the user has no edit,
 * its base: the saved task overlaid with the parts of this form's save that have not reached it (`sent`). `edits` holds
 * the user's fields that differ from the base, and only those: a field set to its base value is no edit, whether the
 * user set it back or the base came to it. So a field the user has not changed, or changed back, follows the saved task
 * through every change made elsewhere, and one they changed keeps their value. The launch's agent, account and settings
 * are one more such part, held by the pickers (`followLaunch`).
 *
 * A save moves the fields of the parts it sends from `edits` into `sent`, so what is edited while it is out is an edit
 * against the value being saved, whatever the task held before. The save is settled, and `sent` dropped whole, once the
 * task holds the change the daemon's last reply carried (`holdsAnswer`): whatever the task holds then is the truth,
 * whichever of the reply and the broadcast came first and whatever else changed meanwhile. A refused save
 * puts the fields it sent back as edits wherever the user has not edited them since; a part an earlier request of it
 * stored stops being an edit once the task holds it.
 */
export interface TaskDraft {
  edits: Partial<TaskFields>;
  sent: SentSave | undefined;
}

export const NO_DRAFT: TaskDraft = { edits: {}, sent: undefined };

/** What happens to a draft: the user changed fields (`edit`), a save was sent, every request of it was answered or one
 * was refused, or Revert dropped every edit. A change of the saved task is `draftOver`'s. */
export type TaskDraftEvent =
  | { type: "edit"; patch: Partial<TaskFields> }
  | { type: "sent"; save: TaskSave; fields: Partial<TaskFields> }
  | { type: "answered"; task: ScheduledTask | undefined }
  | { type: "refused" }
  | { type: "revert" };

/** `draft` after `event`, before it is brought up to the saved task (`draftOver`). */
export function draftAfter(draft: TaskDraft, event: TaskDraftEvent): TaskDraft {
  switch (event.type) {
    case "edit":
      return { ...draft, edits: { ...draft.edits, ...event.patch } };
    case "sent": {
      // A save sent while an answered one has not landed sends its parts again, so the two are one save until the later
      // lands.
      const edits = { ...draft.edits };
      for (const key of Object.keys(event.fields)) delete edits[key as keyof TaskFields];
      const sent = { save: { ...draft.sent?.save, ...event.save }, fields: { ...draft.sent?.fields, ...event.fields }, answered: undefined };
      return { edits, sent };
    }
    case "answered":
      if (!draft.sent) return draft;
      // A save that sent nothing has nothing to wait for.
      return { ...draft, sent: event.task === undefined ? undefined : { ...draft.sent, answered: event.task } };
    case "refused":
      if (!draft.sent) return draft;
      return { edits: { ...draft.sent.fields, ...draft.edits }, sent: undefined };
    case "revert":
      return Object.keys(draft.edits).length === 0 ? draft : { ...draft, edits: {} };
  }
}

/** The fields the form shows where the user has no edit: `task` (none for a new task, whose base is its opening values)
 * overlaid with what the draft's unsettled save sent, read as the form reads them (`fieldsFor`). */
export function draftBase(draft: TaskDraft, task: ScheduledTask | undefined, fieldsFor: (task: ScheduledTask | undefined) => TaskFields): TaskFields {
  return fieldsFor(task && withSave(task, draft.sent?.save));
}

/** `draft` over the saved task as it is now: its save settled once `task` holds what the daemon answered, and its edits
 * that equal the base dropped. The same draft, by identity, when neither changes anything. */
export function draftOver(draft: TaskDraft, task: ScheduledTask | undefined, fieldsFor: (task: ScheduledTask | undefined) => TaskFields): TaskDraft {
  const answered = draft.sent?.answered;
  const settled = answered !== undefined && task !== undefined && holdsAnswer(task, answered);
  const sent = settled ? undefined : draft.sent;
  const base = draftBase({ ...draft, sent }, task, fieldsFor);
  const keys = Object.keys(draft.edits) as (keyof TaskFields)[];
  const kept = keys.filter((key) => !sameValue(draft.edits[key], base[key]));
  if (!settled && kept.length === keys.length) return draft;
  return { edits: kept.length === keys.length ? draft.edits : Object.fromEntries(kept.map((key) => [key, draft.edits[key]])), sent };
}

/** Where the launch pickers stand against the base launch: the one they were last reconciled with (`undefined` for a new
 * task, which has no base and so never moves), and whether the next one is taken without moving them (after a refused
 * save, whose launch stays the user's). */
export interface LaunchFollow {
  base: ScheduledTaskLaunch | undefined;
  keep: boolean;
}

/**
 * The launch pickers over a base launch that may have moved (`base`), `built` being the launch they build now. The
 * launch is the user's edit while what the pickers build differs from the base; so when the base moves, pickers that
 * built the base they were last reconciled with held no edit, and go back to the new one (`reset`), while pickers
 * holding an edit keep it. `undefined` when nothing changes.
 */
export function followLaunch(follow: LaunchFollow, base: ScheduledTaskLaunch | undefined, built: ScheduledTaskLaunch): { follow: LaunchFollow; reset: boolean } | undefined {
  const moved = !sameLaunchSettings(follow.base, base);
  if (!moved && !follow.keep) return undefined;
  return { follow: { base, keep: false }, reset: moved && !follow.keep && sameLaunchSettings(built, follow.base) };
}
