import React from "react";

import { joinValues } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { scheduleWords } from "../scheduledTasks/scheduleText";
import { taskName } from "../scheduledTasks/taskName";
import { useTaskZone } from "../scheduledTasks/useTaskZone";
import { useDaemonStore, type ScheduledTaskRequest } from "../store";
import { hasLaunchRows, ScheduledTaskLaunchRows } from "./ScheduledTaskLaunchRows";
import { recordReference, requesterOf } from "./sessionRequestReferences";

/** The sentence of each request for a scheduled task, which names the requesting session, the task and where it starts
 * its sessions. */
const QUESTIONS = {
  create_scheduled_task: "dialog.sessionRequest.question.createTask",
  update_scheduled_task: "dialog.sessionRequest.question.updateTask",
  delete_scheduled_task: "dialog.sessionRequest.question.deleteTask",
  run_scheduled_task: "dialog.sessionRequest.question.runTask",
} as const;

/** What a request for a scheduled task says: the sentence (the requesting session as the `start` dialog names it, the
 * action, the task, and the project and console, or the console alone, it starts its sessions in), under it, for a
 * delete, what is not affected, for an update, which parts change, and, for a create and for an update for what it
 * changes, the new values as a list: the schedule in the page's own words (in the daemon's time zone), and the launch
 * settings the task names, each as the page and the session dialogs show it, a setting left open left out, and each
 * compatibility switch its launch holds (`ScheduledTaskLaunchRows`). A name is the page's (`taskName`): of an existing task
 * the window holds, from its list, else its id; of a task to create, from its launch settings and prompt. The task's
 * name (a create's, an update's Name row) is the one thing here the requesting session wrote, a short title or the first
 * line of its prompt, shown as the page shows a name; the prompt itself is the dialog's disclosure (`RequestPrompt`). The
 * task's own target is the request's console and project, and the request's agent means nothing. */
export function ScheduledTaskRequestMessage({ request }: { request: ScheduledTaskRequest }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const zone = useTaskZone();
  const requesting = useDaemonStore((s) => s.sessions.get(request.session));
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const existing = useDaemonStore((s) => ("task" in request ? s.scheduledTasks?.get(request.task) : undefined));
  const launch = request.action === "create_scheduled_task" || request.action === "update_scheduled_task" ? request.launch : undefined;
  const requester = requesterOf(t, request, requesting, consoles, projects);
  const console_ = recordReference(consoles.get(request.console)?.name, request.console);
  const target = request.project
    ? <Message id="dialog.sessionRequest.task.target.project" params={{ project: recordReference(projects.get(request.project)?.name, request.project), console: console_ }} />
    : <Message id="dialog.sessionRequest.task.target.console" params={{ console: console_ }} />;
  const name = request.action === "create_scheduled_task"
    ? taskName({ launch: request.launch, prompt: request.prompt })
    : existing && taskName(existing);
  const task = "task" in request ? recordReference(name, request.task) : recordReference(name, "");
  const params = { requester, task, target };

  let question: React.ReactNode;
  if (request.action === "set_scheduled_task_enabled") {
    question = <Message id={request.enabled ? "dialog.sessionRequest.question.enableTask" : "dialog.sessionRequest.question.disableTask"} params={params} />;
  } else {
    question = <Message id={QUESTIONS[request.action]} params={params} />;
  }

  // The parts of an existing task an update changes. Its target is told only where the window knows the task it is
  // now, to compare with.
  let changes: string | undefined;
  if (request.action === "update_scheduled_task") {
    const parts = [
      request.prompt !== undefined && t("dialog.sessionRequest.task.change.prompt"),
      request.schedule !== undefined && t("dialog.sessionRequest.task.change.schedule"),
      request.launch !== undefined && t("dialog.sessionRequest.task.change.launch"),
      existing !== undefined && (existing.console_id !== request.console || existing.project_id !== request.project) && t("dialog.sessionRequest.task.change.target"),
    ].filter((part) => part !== false);
    if (parts.length > 0) changes = t("dialog.sessionRequest.task.changes", { changes: joinValues(language, parts) });
  }

  const schedule = request.action === "create_scheduled_task" || request.action === "update_scheduled_task" ? request.schedule : undefined;
  const rows: { label: string; value: React.ReactNode }[] = [];
  if (request.action === "update_scheduled_task" && launch?.title?.trim()) rows.push({ label: t("common.name"), value: <span dir="auto">{launch.title.trim()}</span> });
  const when = schedule && scheduleWords(t, language, schedule, zone);
  if (when) rows.push({ label: t("scheduledTasks.form.schedule"), value: when });
  if (request.action === "create_scheduled_task") rows.push({ label: t("scheduledTasks.form.enabled"), value: t(request.enabled ? "dialog.sessionRequest.task.enabled.on" : "dialog.sessionRequest.task.enabled.off") });
  const projectTask = request.project !== null;
  return (
    <div className="flex flex-col gap-3 [overflow-wrap:anywhere]">
      <p>{question}</p>
      {request.action === "delete_scheduled_task" && <p>{t("scheduledTasks.deleteMessage")}</p>}
      {changes && <p>{changes}</p>}
      {(rows.length > 0 || (launch && hasLaunchRows(launch, projectTask))) && (
        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1">
          {rows.map(({ label, value }) => (
            <React.Fragment key={label}>
              <dt className="text-app-muted">{label}</dt>
              <dd className="min-w-0">{value}</dd>
            </React.Fragment>
          ))}
          {launch && <ScheduledTaskLaunchRows launch={launch} projectTask={projectTask} />}
        </dl>
      )}
    </div>
  );
}

