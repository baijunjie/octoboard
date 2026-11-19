// What the fixture daemon answers for the scheduled tasks: listing, creating, changing, turning on and off, deleting and
// running them, and listing their runs.
import { CodedError } from "../errors/codedError";
import { type Schedule, type ScheduledRunOutcome, type ScheduledTask, type ScheduledTaskLaunch, type ScheduledTaskRun } from "../protocol";
import { applyDaemonEvent } from "../store";
import { refuseUnfillable } from "./banner";
import type { FixtureContext, Handler, Handlers } from "./fixtureContext";
import { nextRun, WEEK_ORDER } from "./scheduledTaskNextRun";

/** The most runs of a task the daemon keeps (`SCHEDULED_TASK_RUNS_KEPT`). */
const RUNS_KEPT = 50;

/** The run listeners the fixture daemon keeps, to which a run it records is delivered as `scheduled_task_run_added`
 * reaches the real client's. */
export function createScheduledTaskRunListeners() {
  const listeners = new Set<(run: ScheduledTaskRun) => void>();
  return {
    deliver: (run: ScheduledTaskRun) => listeners.forEach((listener) => listener(run)),
    subscribe: (listener: (run: ScheduledTaskRun) => void) => {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
  };
}

/** A value written with its object fields in key order, so that two equal schedules read the same whatever order their
 * fields were written in. */
const canonical = (value: unknown): string =>
  JSON.stringify(value, (_, field: unknown) => (field && typeof field === "object" && !Array.isArray(field) ? Object.fromEntries(Object.entries(field).sort(([x], [y]) => x.localeCompare(y))) : field));

/** Whether two schedules are the same one, as the daemon compares them to keep a task's next run. */
const sameSchedule = (a: Schedule, b: Schedule): boolean => canonical(a) === canonical(b);

/** The `updated_at` a change of `task`'s definition or of whether it is on is stored with at `now`: past the task's
 * previous one even within one millisecond, as the daemon stamps it (`apps/daemon/PROTOCOL.md`, "Scheduled tasks"). */
const stampAfter = (task: ScheduledTask, now: number): number => Math.max(now, task.updated_at + 1);

/** The replies about scheduled tasks. The daemon's tasks and runs are the scenario's `scheduledTasks`, kept here and
 * changed by the requests; the window learns of them from the replies and the broadcasts, as it does from a daemon.
 * `openSession` is the reply of `open_session`, by which a run starts its session; `deliverRun` hands a recorded run to
 * the window's run listeners. */
export function createScheduledTaskReplies({ scenario, store, fail }: FixtureContext, openSession: Handler<"open_session">, deliverRun: (run: ScheduledTaskRun) => void): Handlers {
  const zoneName = scenario.scheduledTasks?.zone ?? null;
  // The machine's own zone is the window's where the daemon names none.
  const zone = zoneName ?? undefined;
  const tasks = new Map<string, ScheduledTask>((scenario.scheduledTasks?.tasks ?? []).map((task) => [task.id, task]));
  const runs = new Map<string, ScheduledTaskRun[]>(Object.entries(scenario.scheduledTasks?.runs ?? {}));
  let created = 0;
  let recorded = 0;

  // Deleting an account clears it from the tasks that named it, which then inherit their target's account again, and tells
  // every window of each (`scheduled_task_upserted`), as the daemon does.
  store.subscribe((state, before) => {
    if (state.settings.accounts === before.settings.accounts) return;
    const kept = new Set(state.settings.accounts.map((account) => account.id));
    const removed = new Set(before.settings.accounts.map((account) => account.id).filter((id) => !kept.has(id)));
    for (const task of tasks.values()) {
      const { account, ...launch } = task.launch;
      if (account != null && removed.has(account)) saved({ ...task, launch, updated_at: stampAfter(task, Date.now()) });
    }
  });

  const require = (id: string): ScheduledTask => {
    const task = tasks.get(id);
    if (!task) return fail({ code: "unknown_scheduled_task", params: { task: id }, message: `unknown scheduled task ${id}` });
    return task;
  };

  /** `task` as the daemon sends it: its `run_sessions` are the sessions its kept runs started, most recent run first, each
   * once, leaving out one the daemon no longer holds or that is archived. Derived on every send, so a session that was
   * archived since stays listed until the task is next sent, as there. */
  const sent = (task: ScheduledTask): ScheduledTask => {
    const held = store.getState().sessions;
    const started = (runs.get(task.id) ?? []).flatMap((run) => (run.outcome.kind === "started" ? [run.outcome.session] : []));
    const run_sessions = [...new Set(started)].filter((id) => {
      const session = held.get(id);
      return session !== undefined && session.status !== "archived";
    });
    return { ...task, run_sessions };
  };

  /** Tells every window of `task`, and answers the request with it. */
  const saved = (task: ScheduledTask) => {
    const stored = sent(task);
    tasks.set(task.id, stored);
    applyDaemonEvent(store, { type: "scheduled_task_upserted", task: stored });
    return { type: "scheduled_task_saved" as const, id: null, task: stored };
  };

  /** The definition as the daemon stores it, or the refusal, in its order: a blank prompt, a schedule that is not well
   * formed, a project of another console, a console session to report to that is none. The launch settings are not
   * checked as `open_session` checks them; the fixture takes them as given. */
  const checked = (definition: { console_id: string; project_id: string | null; launch: ScheduledTaskLaunch; prompt: string; schedule: Schedule }, checkOnce: boolean) => {
    const prompt = definition.prompt.trim();
    if (!prompt) return fail({ code: "field_required", params: { field: "prompt" }, message: "prompt is required" });
    const { schedule } = definition;
    if (schedule.kind === "once" && checkOnce && schedule.run_at <= Date.now()) {
      return fail({ code: "scheduled_time_passed", params: { run_at: String(schedule.run_at) }, message: "the one-off time is not in the future" });
    }
    if (schedule.kind === "weekly" && schedule.days.length === 0) return fail({ code: "field_required", params: { field: "days" }, message: "days is required" });
    if (schedule.kind === "monthly" && !(Number.isInteger(schedule.day) && schedule.day >= 1 && schedule.day <= 31)) {
      return fail({ code: "unreadable_request", params: {}, message: "the schedule's day is out of range" });
    }
    const state = store.getState();
    if (!state.consoles.has(definition.console_id)) return fail({ code: "unknown_console", params: { console: definition.console_id }, message: `unknown console ${definition.console_id}` });
    if (definition.project_id !== null && state.projects.get(definition.project_id)?.console_id !== definition.console_id) {
      return fail({ code: "unknown_project", params: { project: definition.project_id }, message: `unknown project ${definition.project_id}` });
    }
    const { title, bound_to, ...rest } = definition.launch;
    const bound = definition.project_id === null ? undefined : bound_to;
    // Any console session of the task's console, whatever its status: whether it can take a session is decided when the task runs.
    const owner = bound === undefined ? undefined : state.sessions.get(bound);
    if (bound !== undefined && !(owner?.kind === "console" && owner.console_id === definition.console_id)) return fail({ code: "unknown_session", params: { session: bound }, message: `unknown session ${bound}` });
    const named = title?.trim();
    return {
      ...definition,
      prompt,
      launch: { ...rest, ...(named && { title: named }), ...(bound !== undefined && { bound_to: bound }) },
      // A weekly schedule's days are stored in week order, each once.
      schedule: schedule.kind === "weekly" ? { ...schedule, days: WEEK_ORDER.filter((day) => schedule.days.includes(day)) } : schedule,
    };
  };

  /** The tasks the daemon lists: those whose console and project still exist, as it deletes a console's and a project's
   * tasks with them. A task the scenario gives a project of another console is one the daemon never holds. */
  const listed = (): ScheduledTask[] => {
    const state = store.getState();
    const present = [...tasks.values()].filter((task) => state.consoles.has(task.console_id) && (task.project_id === null || state.projects.has(task.project_id)));
    for (const task of present) {
      if (task.project_id !== null && state.projects.get(task.project_id)?.console_id !== task.console_id) refuseUnfillable(`scheduled task ${task.id} names project ${task.project_id}, which is not a project of its console`);
    }
    for (const id of runs.keys()) {
      if (!tasks.has(id)) refuseUnfillable(`scheduled task runs are given for ${id}, which is no task of the scenario`);
    }
    return present.map(sent);
  };

  /** Starts the session of `task` as `open_session` opens one and records the run, a refusal or launch failure being a
   * failed run, never an `error`. */
  const run = async (task: ScheduledTask): Promise<void> => {
    // A console session to report to that cannot take sessions now (archived, gone, still starting, or of another console)
    // leaves the session independent.
    const owner = task.launch.bound_to;
    const held = owner === undefined ? undefined : store.getState().sessions.get(owner);
    const reportsTo = held?.kind === "console" && held.status !== "archived" && held.status !== "starting" && held.console_id === task.console_id ? held.id : undefined;
    const { title, agent, account, model, effort, compatibility } = task.launch;
    let outcome: ScheduledRunOutcome;
    try {
      const { session } = await openSession({
        type: "open_session",
        console_id: task.console_id,
        ...(task.project_id !== null && { project_id: task.project_id }),
        ...(agent !== undefined && { agent }),
        ...(account !== undefined && { account }),
        ...(model !== undefined && { model }),
        ...(effort !== undefined && { effort }),
        compatibility,
        task: task.prompt,
        ...(title !== undefined && { title }),
        ...(reportsTo !== undefined && { bound_to: reportsTo }),
      });
      outcome = { kind: "started", session: session.id, started_independent: owner !== undefined && reportsTo === undefined };
    } catch (error) {
      if (!(error instanceof CodedError)) throw error;
      outcome = { kind: "failed", code: error.code, params: error.params, message: error.message };
    }
    const record: ScheduledTaskRun = { id: `r-gallery-${++recorded}`, task_id: task.id, ran_at: Date.now(), due_at: null, trigger: "run_now", outcome };
    // A task deleted while its session was starting leaves no record.
    const current = tasks.get(task.id);
    if (!current) return;
    runs.set(task.id, [record, ...(runs.get(task.id) ?? [])].slice(0, RUNS_KEPT));
    // Never moved back: another run that settled later in real time may carry an earlier time than one recorded already.
    saved({ ...current, last_run_at: Math.max(current.last_run_at ?? 0, record.ran_at) });
    applyDaemonEvent(store, { type: "scheduled_task_run_added", run: record });
    deliverRun(record);
  };

  return {
    list_scheduled_tasks: () => {
      if (scenario.scheduledTasks?.listError) return fail(scenario.scheduledTasks.listError);
      const reply = { type: "scheduled_tasks_listed" as const, id: null, tasks: listed(), time_zone: zoneName };
      // As the real client's reply reaches the reducer, so a window with no task ends up listed as empty.
      applyDaemonEvent(store, reply);
      return reply;
    },
    create_scheduled_task: (body) => {
      const now = Date.now();
      const definition = checked({ console_id: body.console_id, project_id: body.project_id ?? null, launch: body.launch ?? { compatibility: {} }, prompt: body.prompt, schedule: body.schedule }, true);
      // A task created off has no next run until it is turned on.
      const enabled = body.enabled ?? true;
      return saved({
        id: `t-gallery-created-${++created}`,
        ...definition,
        enabled,
        created_at: now,
        updated_at: now,
        next_run_at: enabled ? nextRun(definition.schedule, now, zone) : null,
        last_run_at: null,
        run_sessions: [],
      });
    },
    update_scheduled_task: (body) => {
      const task = require(body.task);
      const now = Date.now();
      // The project is its own choice, whatever the console: absent keeps it, `null` makes a console task, a value must be
      // a project of the console the task ends up in (which `checked` holds it to).
      const definition = checked(
        {
          console_id: body.console_id ?? task.console_id,
          project_id: body.project_id === undefined ? task.project_id : body.project_id,
          launch: body.launch ?? task.launch,
          prompt: body.prompt ?? task.prompt,
          schedule: body.schedule ?? task.schedule,
        },
        body.schedule !== undefined,
      );
      // An edit never turns the task on or off; a schedule left as it was keeps the next run it had, a changed one
      // computes it anew, and an off task has none.
      const nextRunAt = !task.enabled ? null : sameSchedule(definition.schedule, task.schedule) ? task.next_run_at : nextRun(definition.schedule, now, zone);
      return saved({ ...task, ...definition, updated_at: stampAfter(task, now), next_run_at: nextRunAt });
    },
    set_scheduled_task_enabled: (body) => {
      const task = require(body.task);
      // A task already in the state asked for is left as it is, its next run and time of change included.
      if (task.enabled === body.enabled) return saved(task);
      const now = Date.now();
      return saved({ ...task, enabled: body.enabled, updated_at: stampAfter(task, now), next_run_at: body.enabled ? nextRun(task.schedule, now, zone) : null });
    },
    delete_scheduled_task: (body) => {
      require(body.task);
      tasks.delete(body.task);
      runs.delete(body.task);
      applyDaemonEvent(store, { type: "scheduled_task_deleted", task: body.task });
      return { type: "ack", id: null };
    },
    run_scheduled_task: (body) => {
      const task = require(body.task);
      // Acknowledged at once: the run is recorded once its session has started or failed to, and the window learns of it
      // from the broadcast. A failure that is not the daemon's own refusal surfaces as an unhandled rejection.
      void run(task);
      return { type: "ack", id: null };
    },
    list_scheduled_task_runs: (body) => {
      require(body.task);
      if (scenario.scheduledTasks?.runsError) return fail(scenario.scheduledTasks.runsError);
      const reply = { type: "scheduled_task_runs_listed" as const, id: null, task: body.task, runs: runs.get(body.task) ?? [] };
      applyDaemonEvent(store, reply);
      return reply;
    },
  };
}
