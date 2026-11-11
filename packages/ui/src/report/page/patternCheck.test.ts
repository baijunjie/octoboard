import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { PATTERN_TIMEOUT_MS } from "./patternCheck";
import { testPattern } from "./patternTest";
import type { PatternReply, PatternRequest } from "./patternWorker";

/** A stand-in worker that answers every request except those `hangs` is true for, once it has said it is ready. */
const workers: FakeWorker[] = [];
let hangs: (request: PatternRequest) => boolean;
class FakeWorker {
  onmessage: ((event: MessageEvent<PatternReply>) => void) | null = null;
  onerror: (() => void) | null = null;
  terminated = false;
  posted: PatternRequest[] = [];
  constructor() {
    workers.push(this);
    queueMicrotask(() => this.onmessage?.({ data: { ready: true } } as MessageEvent<PatternReply>));
  }
  postMessage(request: PatternRequest) {
    this.posted.push(request);
    if (hangs(request)) return;
    queueMicrotask(() => this.onmessage?.({ data: { id: request.id, verdict: testPattern(request.pattern, request.text) } } as MessageEvent<PatternReply>));
  }
  terminate() {
    this.terminated = true;
  }
}

// The client keeps its worker between tests, so each test loads it anew.
let checkPattern: typeof import("./patternCheck").checkPattern;

beforeEach(async () => {
  vi.resetModules();
  ({ checkPattern } = await import("./patternCheck"));
  vi.useFakeTimers();
  vi.stubGlobal("Worker", FakeWorker);
  hangs = () => false;
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  workers.length = 0;
});

it("answers from the worker", async () => {
  await expect(checkPattern("\\d+", "12")).resolves.toBe("match");
  await expect(checkPattern("\\d+", "ab")).resolves.toBe("mismatch");
});

it("counts a test that does not finish in time as unchecked, ends the stuck worker and tests on a new one", async () => {
  hangs = (request) => request.text === "stuck";
  const stuck = checkPattern("(a+)+$", "stuck");
  await vi.advanceTimersByTimeAsync(PATTERN_TIMEOUT_MS + 1);
  await expect(stuck).resolves.toBe("unchecked");
  const first = workers[workers.length - 1];
  expect(first.terminated).toBe(true);
  await expect(checkPattern("\\d+", "12")).resolves.toBe("match");
  expect(workers[workers.length - 1]).not.toBe(first);
});

it("sends the tests that waited behind a stuck one to a new worker", async () => {
  hangs = (request) => request.text === "stuck";
  const stuck = checkPattern("(a+)+$", "stuck");
  const behind = checkPattern("\\d+", "12");
  await vi.advanceTimersByTimeAsync(PATTERN_TIMEOUT_MS + 1);
  await expect(stuck).resolves.toBe("unchecked");
  await expect(behind).resolves.toBe("match");
});

it("times a test from when the worker starts it, not from when it joined the queue", async () => {
  hangs = (request) => request.text === "slow";
  const slow = checkPattern("\\d+", "slow");
  const queued = checkPattern("\\d+", "12");
  await vi.advanceTimersByTimeAsync(PATTERN_TIMEOUT_MS - 10);
  expect(workers[0].posted.map((request) => request.text)).toEqual(["slow"]);
  await vi.advanceTimersByTimeAsync(20);
  await expect(slow).resolves.toBe("unchecked");
  // The test behind it starts on the new worker with its full limit.
  hangs = () => false;
  await expect(queued).resolves.toBe("match");
});

it("leaves the test unchecked where the window has no worker", async () => {
  vi.stubGlobal("Worker", undefined);
  await expect(checkPattern("\\d+", "12")).resolves.toBe("unchecked");
});

it("hands the worker one test at a time, the next only once the one before has been answered", async () => {
  hangs = () => true;
  const first = checkPattern("\\d+", "1");
  const second = checkPattern("\\d+", "2");
  await vi.advanceTimersByTimeAsync(0);
  const worker = workers[0];
  expect(worker.posted.map((request) => request.text)).toEqual(["1"]);
  worker.onmessage?.({ data: { id: worker.posted[0].id, verdict: "match" } } as MessageEvent<PatternReply>);
  await expect(first).resolves.toBe("match");
  expect(worker.posted.map((request) => request.text)).toEqual(["1", "2"]);
  expect(worker.terminated).toBe(false);
  worker.onmessage?.({ data: { id: worker.posted[1].id, verdict: "mismatch" } } as MessageEvent<PatternReply>);
  await expect(second).resolves.toBe("mismatch");
});

it("runs a test that overran on no thread of the window: it is unchecked, and the pattern is never tried on the main thread", async () => {
  const run = vi.spyOn(RegExp.prototype, "test");
  hangs = () => true;
  const stuck = checkPattern("(a+)+$", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa!");
  await vi.advanceTimersByTimeAsync(PATTERN_TIMEOUT_MS + 1);
  await expect(stuck).resolves.toBe("unchecked");
  expect(run).not.toHaveBeenCalled();
  run.mockRestore();
});

it("gives the waiting tests up when the worker fails, and tests on a new one afterwards", async () => {
  hangs = () => true;
  const waiting = checkPattern("\\d+", "1");
  await vi.advanceTimersByTimeAsync(0);
  const broken = workers[0];
  broken.onerror?.();
  await expect(waiting).resolves.toBe("unchecked");
  expect(broken.terminated).toBe(true);
  hangs = () => false;
  await expect(checkPattern("\\d+", "12")).resolves.toBe("match");
  expect(workers).toHaveLength(2);
});

it("gives the waiting tests up when the worker never says it is ready", async () => {
  const posted: PatternRequest[] = [];
  vi.stubGlobal("Worker", class {
    onmessage: unknown = null;
    onerror: unknown = null;
    postMessage(request: PatternRequest) {
      posted.push(request);
    }
    terminate() {}
  });
  const waiting = checkPattern("\\d+", "1");
  await vi.advanceTimersByTimeAsync(0);
  // Nothing is posted to a worker that has not said it is ready.
  expect(posted).toEqual([]);
  await vi.runOnlyPendingTimersAsync();
  await expect(waiting).resolves.toBe("unchecked");
});
