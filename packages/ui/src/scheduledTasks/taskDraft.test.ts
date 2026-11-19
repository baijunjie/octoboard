import { expect, it } from "vitest";

import type { ScheduledTask, ScheduledTaskLaunch } from "../protocol";
import { draftAfter, draftBase, draftOver, fieldsSaved, followLaunch, type LaunchFollow, NO_DRAFT, type TaskDraftEvent, type TaskSave, withSave } from "./taskDraft";
import { fieldsOf, sameLaunchSettings, type TaskFields } from "./taskFields";

// A Thursday, 10:20 UTC.
const NOW = Date.UTC(2026, 10, 19, 10, 20);

/** The saved task the draft tests start from, as the daemon stores it at `updated_at` 1. */
const SAVED: ScheduledTask = {
  id: "t-1",
  console_id: "c-1",
  project_id: null,
  launch: { title: "Report", agent: "claude", account: null, compatibility: {} },
  prompt: "Do the thing",
  schedule: { kind: "daily", time: { hour: 7, minute: 5 } },
  enabled: true,
  created_at: 1,
  updated_at: 1,
  next_run_at: null,
  last_run_at: null,
  run_sessions: [],
};
const CLAUDE = SAVED.launch;
const CODEX: ScheduledTaskLaunch = { title: "Report", agent: "codex", account: null, compatibility: {} };
const OPUS: ScheduledTaskLaunch = { ...CLAUDE, model: "opus" };

/** `SAVED` as the daemon stores it after a change at `at`. */
function storedAt(at: number, extra: Partial<ScheduledTask>): ScheduledTask {
  return { ...SAVED, ...extra, updated_at: at };
}

/**
 * The form over a saved task, driven as `useTaskDraft` and `useTaskLaunch` drive it: after every event the draft is
 * brought up to the store's task, and the pickers follow the base launch. The pickers are stood for by the launch they
 * build. `send` builds the save of the named parts from what the form shows, as Save does.
 */
function formOver(initial: ScheduledTask) {
  const fieldsFor = (task: ScheduledTask | undefined) => fieldsOf(task, "c-1", NOW, "UTC");
  let task = initial;
  let draft = NO_DRAFT;
  let built = initial.launch;
  let follow: LaunchFollow = { base: initial.launch, keep: false };
  const baseLaunch = () => withSave(task, draft.sent?.save).launch;
  const settle = () => {
    draft = draftOver(draft, task, fieldsFor);
    const followed = followLaunch(follow, baseLaunch(), built);
    if (followed) {
      follow = followed.follow;
      if (followed.reset) built = baseLaunch();
    }
  };
  const after = (event: TaskDraftEvent) => {
    draft = draftAfter(draft, event);
    settle();
  };
  const shown = (): TaskFields => ({ ...draftBase(draft, task, fieldsFor), ...draft.edits });
  return {
    shown,
    /** The launch the pickers build. */
    launch: () => built,
    /** What the user holds as theirs: the fields, and the launch while it differs from the base. */
    edits: () => ({ ...draft.edits, ...(!sameLaunchSettings(built, baseLaunch()) && { launch: built }) }),
    /** Whether a save has not landed. */
    unsettled: () => draft.sent !== undefined,
    edit: (patch: Partial<TaskFields>) => after({ type: "edit", patch }),
    pick: (launch: ScheduledTaskLaunch) => {
      built = launch;
      settle();
    },
    send: (...parts: ("prompt" | "enabled" | "launch")[]) => {
      const fields = shown();
      const save: TaskSave = {
        ...(parts.includes("prompt") && { prompt: fields.prompt.trim() }),
        ...(parts.includes("enabled") && { enabled: fields.enabled }),
        ...(parts.includes("launch") && { launch: { ...built, title: fields.name } }),
      };
      after({ type: "sent", save, fields: fieldsSaved(fields, save) });
    },
    /** The daemon's last reply to the save, carrying the task as it stored it. */
    answer: (stored: ScheduledTask) => after({ type: "answered", task: stored }),
    // The hook asks the pickers to keep what they hold in the same update as the refusal.
    refuse: () => {
      follow = { ...follow, keep: true };
      after({ type: "refused" });
    },
    /** The store takes `next` (a broadcast). */
    land: (next: ScheduledTask) => {
      task = next;
      settle();
    },
    revert: () => {
      after({ type: "revert" });
      built = baseLaunch();
      follow = { base: built, keep: false };
    },
  };
}

it("follows a change made elsewhere to a field and a launch the user has not changed", () => {
  const form = formOver(SAVED);
  form.land(storedAt(2, { prompt: "Theirs", enabled: false, launch: CODEX }));
  expect(form.shown()).toMatchObject({ prompt: "Theirs", enabled: false });
  expect(form.launch()).toEqual(CODEX);
  expect(form.edits()).toEqual({});
});

it("counts a field and a launch the user changed back as unchanged, following a later change made elsewhere", () => {
  const form = formOver(SAVED);
  form.edit({ enabled: false });
  form.edit({ enabled: true });
  form.pick(CODEX);
  form.pick(CLAUDE);
  expect(form.edits()).toEqual({});
  form.land(storedAt(2, { enabled: false, launch: OPUS }));
  expect(form.shown().enabled).toBe(false);
  expect(form.launch()).toEqual(OPUS);
  expect(form.edits()).toEqual({});
});

it("keeps what the user changed when the task changes elsewhere, in that part and in another, and drops it once the task holds it", () => {
  const form = formOver(SAVED);
  form.edit({ prompt: "Mine" });
  form.pick(CODEX);
  form.land(storedAt(2, { prompt: "Theirs", launch: OPUS }));
  form.land(storedAt(3, { prompt: "Theirs", launch: { ...OPUS, title: "Renamed" }, enabled: false }));
  expect(form.shown()).toMatchObject({ prompt: "Mine", name: "Renamed", enabled: false });
  expect(form.launch()).toEqual(CODEX);
  // The saved task catching up with an edit leaves nothing of the user's to keep: the field follows again.
  form.land(storedAt(4, { prompt: "Mine" }));
  form.land(storedAt(5, { prompt: "Later" }));
  expect(form.shown().prompt).toBe("Later");
});

it.each([
  ["the reply before the broadcast", true],
  ["the broadcast before the reply", false],
])("keeps the prompt typed back to its old value and the launch picked back while the save was out, with %s", (_, replyFirst) => {
  const form = formOver(SAVED);
  form.edit({ prompt: "New" });
  form.pick(CODEX);
  form.send("prompt", "launch");
  form.edit({ prompt: SAVED.prompt });
  form.pick(CLAUDE);
  const landed = storedAt(2, { prompt: "New", launch: CODEX });
  if (replyFirst) {
    form.answer(landed);
    // Answered, but the store does not hold the change yet: what the user did still stands against what was sent.
    expect(form.unsettled()).toBe(true);
    expect(form.shown().prompt).toBe(SAVED.prompt);
    form.land(landed);
  } else {
    form.land(landed);
    expect(form.unsettled()).toBe(true);
    form.answer(landed);
  }
  expect(form.unsettled()).toBe(false);
  expect(form.shown().prompt).toBe(SAVED.prompt);
  expect(form.launch()).toEqual(CLAUDE);
  // Both now differ from the saved task, which holds the save: the form is dirty with them.
  expect(form.edits()).toEqual({ prompt: SAVED.prompt, launch: CLAUDE });
});

it("counts an edit made while the save is out that equals what is being saved as none, following the task after", () => {
  const form = formOver(SAVED);
  form.edit({ prompt: "New" });
  form.send("prompt");
  form.edit({ prompt: "New, almost" });
  form.edit({ prompt: "New" });
  expect(form.edits()).toEqual({});
  form.answer(storedAt(2, { prompt: "New" }));
  form.land(storedAt(2, { prompt: "New" }));
  form.land(storedAt(3, { prompt: "Theirs" }));
  expect(form.shown().prompt).toBe("Theirs");
  expect(form.edits()).toEqual({});
});

it("leaves what a refused save sent as the user's edits, and what they changed since as they changed it", () => {
  const form = formOver(SAVED);
  form.edit({ prompt: "New", enabled: false });
  form.pick(CODEX);
  form.send("prompt", "enabled", "launch");
  form.edit({ prompt: "Newer" });
  form.refuse();
  expect(form.unsettled()).toBe(false);
  expect(form.shown()).toMatchObject({ prompt: "Newer", enabled: false });
  expect(form.edits()).toEqual({ prompt: "Newer", enabled: false, launch: CODEX });
});

it.each([
  ["before the refusal", true],
  ["after the refusal", false],
])("keeps what the first request of a save stored as saved, and the refused rest as edits, with its broadcast %s", (_, broadcastFirst) => {
  const form = formOver(SAVED);
  form.edit({ prompt: "New", enabled: false });
  form.pick(CODEX);
  form.send("prompt", "launch", "enabled");
  // The update is stored; the set_scheduled_task_enabled that follows it is refused.
  const first = storedAt(2, { prompt: "New", launch: CODEX });
  if (broadcastFirst) form.land(first);
  form.refuse();
  if (!broadcastFirst) form.land(first);
  expect(form.shown()).toMatchObject({ prompt: "New", enabled: false });
  expect(form.launch()).toEqual(CODEX);
  expect(form.edits()).toEqual({ enabled: false });
});

it("keeps Enabled switched back on while a save that turns it off is out, the first request's broadcast not settling it and the second answered before its own", () => {
  const form = formOver(SAVED);
  form.edit({ prompt: "New", enabled: false });
  form.send("prompt", "enabled");
  form.edit({ enabled: true });
  // Each stored change is stamped past the one before, so the second request's task is stamped past the first's.
  form.land(storedAt(2, { prompt: "New" }));
  const second = storedAt(3, { prompt: "New", enabled: false });
  form.answer(second);
  expect(form.unsettled()).toBe(true);
  expect(form.shown().enabled).toBe(true);
  form.land(second);
  expect(form.unsettled()).toBe(false);
  expect(form.shown()).toMatchObject({ prompt: "New", enabled: true });
  expect(form.edits()).toEqual({ enabled: true });
});

it("sends a field typed back to the saved task's value with a second save pressed before the first landed, and ends on it", () => {
  const form = formOver(SAVED);
  form.edit({ prompt: "New" });
  form.send("prompt");
  form.answer(storedAt(2, { prompt: "New" }));
  form.edit({ prompt: SAVED.prompt });
  form.send("prompt");
  expect(form.edits()).toEqual({});
  // The first save's broadcast does not settle the second, which is still out.
  form.land(storedAt(2, { prompt: "New" }));
  expect(form.unsettled()).toBe(true);
  expect(form.shown().prompt).toBe(SAVED.prompt);
  const second = storedAt(3, { prompt: SAVED.prompt });
  form.answer(second);
  form.land(second);
  expect(form.unsettled()).toBe(false);
  expect(form.shown().prompt).toBe(SAVED.prompt);
  expect(form.edits()).toEqual({});
});

it("keeps a field typed back while the save is out through a change made elsewhere before the save's own broadcast", () => {
  const form = formOver(SAVED);
  form.edit({ prompt: "New" });
  form.send("prompt");
  form.edit({ prompt: SAVED.prompt });
  // Another window renames the task, stored before this save; then this save's reply, then its broadcast.
  form.land(storedAt(2, { launch: { ...CLAUDE, title: "Renamed" } }));
  const saved = storedAt(3, { prompt: "New", launch: { ...CLAUDE, title: "Renamed" } });
  form.answer(saved);
  expect(form.shown().prompt).toBe(SAVED.prompt);
  form.land(saved);
  expect(form.shown()).toMatchObject({ prompt: SAVED.prompt, name: "Renamed" });
  expect(form.edits()).toEqual({ prompt: SAVED.prompt });
});

it("settles a save the task already held at its answer, whose task the daemon did not change again", () => {
  const form = formOver(SAVED);
  form.edit({ enabled: false });
  form.send("enabled");
  // Turned off elsewhere first: the daemon leaves a task in the state asked for as it is, its updated_at included.
  form.land(storedAt(2, { enabled: false }));
  form.answer(storedAt(2, { enabled: false }));
  expect(form.unsettled()).toBe(false);
  form.land(storedAt(3, { enabled: true }));
  expect(form.shown().enabled).toBe(true);
});

it("drops every edit on Revert, but not a save that has yet to land", () => {
  const form = formOver(SAVED);
  form.edit({ prompt: "New" });
  form.send("prompt");
  form.answer(storedAt(2, { prompt: "New" }));
  form.edit({ enabled: false });
  form.pick(CODEX);
  form.revert();
  expect(form.edits()).toEqual({});
  expect(form.shown()).toMatchObject({ prompt: "New", enabled: true });
  expect(form.launch()).toEqual(CLAUDE);
  form.land(storedAt(2, { prompt: "New" }));
  expect(form.unsettled()).toBe(false);
});
