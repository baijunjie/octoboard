// @vitest-environment jsdom
// What the Scheduled tasks page's hook says is shown: when the page is there, over what, and the task it shows.
import { act } from "react";
import { afterEach, expect, it } from "vitest";

import type { Location } from "../navigation/history";
import type { ScheduledTask } from "../protocol";
import { scheduledTaskPreference } from "../scheduledTasks/scheduledTaskPreference";
import { applyDaemonEvent, type Daemon } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { useScheduledTasksPage } from "./useScheduledTasksPage";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// A choice remembered is held in module memory, which nothing else resets.
afterEach(() => scheduledTaskPreference.set(undefined));

function taskOf(id: string): ScheduledTask {
  return { id, console_id: "c-1", project_id: null, launch: { compatibility: {} }, prompt: "Do it", schedule: { kind: "manual" }, enabled: true, created_at: 1, updated_at: 1, next_run_at: null, last_run_at: null, run_sessions: [] };
}

type Page = ReturnType<typeof useScheduledTasksPage>;

/** Mounts the hook over `daemon` and returns what it last said, and a way to re-render it over another console and session. */
function mount(options: { available?: boolean; consoleId?: string; sessionId?: string; held?: boolean; tasks?: ScheduledTask[] } = {}) {
  const { available = true, tasks = [taskOf("t-a"), taskOf("t-b")] } = options;
  const daemon: Daemon = testDaemon({
    state: { connectionState: "open", snapshotEpoch: 1, scheduledTasks: new Map(tasks.map((task) => [task.id, task])) },
    request: async () => ({ type: "scheduled_tasks_listed", id: null, tasks, time_zone: null }),
  });
  let page!: Page;
  function Probe(props: { available: boolean; consoleId: string | undefined; sessionId: string | undefined; held: boolean }): null {
    page = useScheduledTasksPage(props);
    return null;
  }
  const initial = { available, consoleId: options.consoleId ?? "c-1", sessionId: options.sessionId, held: options.held ?? false };
  const container = mountHost(<Probe {...initial} />, { daemon });
  return {
    daemon,
    now: () => page,
    over: (patch: Partial<typeof initial>) => act(async () => container.render(<Probe {...initial} {...patch} />)),
    act: (action: (page: Page) => void) => act(async () => action(page)),
  };
}

it("holds the tasks in the order the store does, none while the daemon cannot be used", async () => {
  const usable = mount();
  await act(async () => {});
  expect(usable.now().tasks?.map((task) => task.id)).toEqual(["t-a", "t-b"]);
  const unusable = mount({ available: false });
  await act(async () => {});
  expect(unusable.now().tasks).toBeUndefined();
  expect(unusable.now().task).toBeUndefined();
});

it("shows the page once it is asked for, over the console and session then current, and not while the daemon cannot be used", async () => {
  const { now, act: run } = mount({ sessionId: "s-1" });
  expect(now().shown).toBe(false);
  await run((page) => page.show());
  expect(now().shown).toBe(true);
  // Asking again while it shows changes nothing.
  await run((page) => page.show());
  expect(now().shown).toBe(true);
  await run((page) => page.leave());
  expect(now().shown).toBe(false);

  const unusable = mount({ available: false });
  await unusable.act((page) => page.show());
  expect(unusable.now().shown).toBe(false);
});

it("is gone as soon as the console or the session it was shown over changes, and stays gone", async () => {
  for (const change of [{ consoleId: "c-2" }, { sessionId: "s-2" }, { sessionId: undefined }]) {
    const { now, over, act: run } = mount({ sessionId: "s-1" });
    await run((page) => page.show());
    expect(now().shown).toBe(true);
    await over(change);
    expect(now().shown).toBe(false);
    // Back to what it was shown over: it does not come back by itself.
    await over({ consoleId: "c-1", sessionId: "s-1" });
    expect(now().shown).toBe(false);
  }
});

it("stays, over what is current then, when what it was shown over changes while a form holds changes not saved", async () => {
  const { now, over, act: run } = mount({ sessionId: "s-1", held: true });
  await run((page) => page.show());
  await over({ sessionId: "s-2" });
  expect(now().shown).toBe(true);
  // Anchored to the new session: once nothing is held, a further change leaves it.
  await over({ sessionId: "s-2", held: false });
  expect(now().shown).toBe(true);
  await over({ sessionId: "s-3", held: false });
  expect(now().shown).toBe(false);
});

it("does not come back when a leave is confirmed: leaving sets the page away whatever is held", async () => {
  const { now, over, act: run } = mount({ sessionId: "s-1", held: true });
  await run((page) => page.show());
  await run((page) => page.leave());
  expect(now().shown).toBe(false);
  await over({ sessionId: "s-2" });
  expect(now().shown).toBe(false);
});

it("shows the task last chosen while it exists, else the first, and remembers a choice for the next window", async () => {
  scheduledTaskPreference.set("t-b");
  const { daemon, now, act: run } = mount();
  expect(now().task).toBe("t-b");
  await run((page) => page.select("t-a"));
  expect(now().task).toBe("t-a");
  expect(scheduledTaskPreference.get()).toBe("t-a");

  await act(async () => applyDaemonEvent(daemon.store, { type: "scheduled_task_deleted", task: "t-a" }));
  expect(now().task).toBe("t-b");
  await act(async () => applyDaemonEvent(daemon.store, { type: "scheduled_task_deleted", task: "t-b" }));
  expect(now().task).toBeUndefined();
});

it("follows a history entry: shows the page and its task, or leaves it", async () => {
  const { now, act: run } = mount({ sessionId: "s-1" });
  const entry = (tasks: string | undefined): Location => ({ console: "c-1", session: "s-1", tasks });
  await run((page) => page.follow(entry("t-b")));
  expect(now().shown).toBe(true);
  expect(now().task).toBe("t-b");
  expect(scheduledTaskPreference.get()).toBe("t-b");

  // An entry for the page with no task shows it without changing the task last chosen.
  await run((page) => page.follow(entry("")));
  expect(now().shown).toBe(true);
  expect(now().task).toBe("t-b");

  await run((page) => page.follow(entry(undefined)));
  expect(now().shown).toBe(false);
});
