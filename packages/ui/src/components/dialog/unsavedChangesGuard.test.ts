import { describe, expect, it, vi } from "vitest";

import { createUnsavedChangesGuard, type HeldForm } from "./unsavedChangesGuard";

const form = (overrides: Partial<HeldForm> = {}): HeldForm => ({ saving: false, discard: vi.fn(), ...overrides });

describe("run", () => {
  it("runs the leave at once while nothing is held", () => {
    const guard = createUnsavedChangesGuard();
    const leave = vi.fn();
    expect(guard.run(leave)).toBe("left");
    expect(leave).toHaveBeenCalledTimes(1);
    expect(guard.getSnapshot().asking).toBe(false);
  });

  it("asks first while a form is held, and keeps the leave for the answer", () => {
    const guard = createUnsavedChangesGuard();
    guard.hold({}, form());
    const leave = vi.fn();
    expect(guard.run(leave)).toBe("asked");
    expect(leave).not.toHaveBeenCalled();
    expect(guard.getSnapshot()).toEqual({ dirty: true, asking: true });
  });

  it("drops the press while a held form is saving", () => {
    const guard = createUnsavedChangesGuard();
    guard.hold({}, form());
    guard.hold({}, form({ saving: true }));
    const leave = vi.fn();
    expect(guard.run(leave)).toBe("held");
    expect(leave).not.toHaveBeenCalled();
    expect(guard.getSnapshot().asking).toBe(false);
    expect(guard.answer(true)).toBeUndefined();
  });

  it("lets a second leave replace the one waiting on the open question", () => {
    const guard = createUnsavedChangesGuard();
    guard.hold({}, form());
    const first = vi.fn();
    const second = vi.fn();
    guard.run(first);
    expect(guard.run(second)).toBe("asked");
    guard.answer(true)?.();
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });
});

describe("answer", () => {
  it("discards every held form, empties the registry and hands back the leave", () => {
    const guard = createUnsavedChangesGuard();
    const first = form();
    const second = form();
    guard.hold({}, first);
    guard.hold({}, second);
    guard.run(vi.fn());
    const leave = guard.answer(true);
    expect(first.discard).toHaveBeenCalledTimes(1);
    expect(second.discard).toHaveBeenCalledTimes(1);
    expect(guard.getSnapshot()).toEqual({ dirty: false, asking: false });
    expect(leave).toBeTypeOf("function");
    // What the leave runs goes through at once, as nothing is held.
    const inner = vi.fn();
    expect(guard.run(inner)).toBe("left");
  });

  it("only closes the question when the changes are kept", () => {
    const guard = createUnsavedChangesGuard();
    const held = form();
    guard.hold({}, held);
    const leave = vi.fn();
    guard.run(leave);
    expect(guard.answer(false)).toBeUndefined();
    expect(held.discard).not.toHaveBeenCalled();
    expect(leave).not.toHaveBeenCalled();
    expect(guard.getSnapshot()).toEqual({ dirty: true, asking: false });
  });

  it("does nothing while no question is open", () => {
    const guard = createUnsavedChangesGuard();
    const held = form();
    guard.hold({}, held);
    expect(guard.answer(true)).toBeUndefined();
    expect(held.discard).not.toHaveBeenCalled();
    expect(guard.isDirty()).toBe(true);
  });
});

describe("hold and release", () => {
  it("replaces what a key registered rather than adding a second form", () => {
    const guard = createUnsavedChangesGuard();
    const key = {};
    const stale = form();
    const current = form();
    guard.hold(key, stale);
    guard.hold(key, current);
    guard.run(vi.fn());
    guard.answer(true);
    expect(stale.discard).not.toHaveBeenCalled();
    expect(current.discard).toHaveBeenCalledTimes(1);
  });

  it("stays dirty until every key is released", () => {
    const guard = createUnsavedChangesGuard();
    const first = {};
    const second = {};
    guard.hold(first, form());
    guard.hold(second, form());
    guard.release(first);
    expect(guard.isDirty()).toBe(true);
    guard.release(second);
    expect(guard.isDirty()).toBe(false);
  });

  it("closes the open question and drops its leave when the last form is released", () => {
    const guard = createUnsavedChangesGuard();
    const key = {};
    guard.hold(key, form());
    const leave = vi.fn();
    guard.run(leave);
    guard.release(key);
    expect(guard.getSnapshot()).toEqual({ dirty: false, asking: false });
    expect(guard.answer(true)).toBeUndefined();
    expect(leave).not.toHaveBeenCalled();
  });

  it("keeps the question open while another form is still held", () => {
    const guard = createUnsavedChangesGuard();
    const first = {};
    guard.hold(first, form());
    guard.hold({}, form());
    guard.run(vi.fn());
    guard.release(first);
    expect(guard.getSnapshot().asking).toBe(true);
  });
});

describe("setBeforeAsk", () => {
  it("runs as the question opens, once for the leaves that replace each other", () => {
    const guard = createUnsavedChangesGuard();
    guard.hold({}, form());
    const hook = vi.fn();
    guard.setBeforeAsk(hook);
    guard.run(vi.fn());
    guard.run(vi.fn());
    expect(hook).toHaveBeenCalledTimes(1);
  });

  it("does not run for a leave that is not asked about, and stops once removed", () => {
    const guard = createUnsavedChangesGuard();
    const hook = vi.fn();
    const remove = guard.setBeforeAsk(hook);
    guard.run(vi.fn());
    expect(hook).not.toHaveBeenCalled();
    remove();
    guard.hold({}, form());
    guard.run(vi.fn());
    expect(hook).not.toHaveBeenCalled();
  });
});

describe("getSnapshot and subscribe", () => {
  it("is the same object until dirty or asking changes", () => {
    const guard = createUnsavedChangesGuard();
    const key = {};
    const listener = vi.fn();
    guard.subscribe(listener);
    const idle = guard.getSnapshot();
    guard.hold(key, form());
    const dirty = guard.getSnapshot();
    expect(dirty).not.toBe(idle);
    // A change of the registry that changes neither flag.
    guard.hold(key, form({ saving: true }));
    expect(guard.getSnapshot()).toBe(dirty);
    expect(listener).toHaveBeenCalledTimes(1);
    guard.release({});
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("stops telling a listener that unsubscribed", () => {
    const guard = createUnsavedChangesGuard();
    const listener = vi.fn();
    guard.subscribe(listener)();
    guard.hold({}, form());
    expect(listener).not.toHaveBeenCalled();
  });
});
