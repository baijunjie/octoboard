// Tests a page's `pattern` against a value in a Web Worker, with a time limit. The pattern is the agent's, so the window's
// own thread never runs it: a test that does not finish within `PATTERN_TIMEOUT_MS` counts as unchecked, the worker that
// is stuck on it is ended, and the next test gets a new one. A window that cannot start a worker leaves every test
// unchecked.
import type { PatternReply, PatternRequest } from "./patternWorker";
import type { PatternVerdict } from "./patternTest";

/** The result of a test: its verdict, or `unchecked` where it could not be finished. */
export type PatternResult = PatternVerdict | "unchecked";

/** How long a test may take once the worker is running. */
export const PATTERN_TIMEOUT_MS = 100;

/** How long a worker may take to load before the tests waiting for it are given up. */
const WORKER_START_MS = 5_000;

interface Waiting extends PatternRequest {
  settle: (result: PatternResult) => void;
}

// The tests wait here in turn: the worker runs one at a time, so a test's time limit starts when it is the one posted,
// not while it waits behind another.
const waiting = new Map<number, Waiting>();
let nextId = 0;
let worker: Worker | undefined;
let ready = false;
let running: Waiting | undefined;
let timer: ReturnType<typeof setTimeout> | undefined;
let startTimer: ReturnType<typeof setTimeout> | undefined;

function finish(request: Waiting, result: PatternResult): void {
  if (!waiting.delete(request.id)) return;
  if (running === request) {
    running = undefined;
    clearTimeout(timer);
  }
  request.settle(result);
}

/** Gives every waiting test up, where there is no worker to run them. */
function giveUp(): void {
  for (const request of [...waiting.values()]) finish(request, "unchecked");
}

/** Ends the worker, which a test that never finishes would otherwise keep busy for good. */
function drop(): void {
  worker?.terminate();
  worker = undefined;
  ready = false;
  clearTimeout(startTimer);
}

function start(): Worker | undefined {
  if (worker) return worker;
  if (typeof Worker === "undefined") return undefined;
  try {
    const started = new Worker(new URL("./patternWorker.ts", import.meta.url), { type: "module" });
    started.onmessage = (event: MessageEvent<PatternReply>) => {
      if (started !== worker) return;
      if ("ready" in event.data) {
        ready = true;
        clearTimeout(startTimer);
        next();
      } else if (running?.id === event.data.id) {
        finish(running, event.data.verdict);
        next();
      }
    };
    started.onerror = () => {
      if (started !== worker) return;
      drop();
      giveUp();
    };
    startTimer = setTimeout(() => {
      drop();
      giveUp();
    }, WORKER_START_MS);
    worker = started;
    return started;
  } catch {
    return undefined;
  }
}

/** Posts the next waiting test once the worker is free, and gives it `PATTERN_TIMEOUT_MS` from then. A test that
 * overruns is given up, its worker ended, and the tests behind it go to a new one. */
function next(): void {
  if (running) return;
  const request = waiting.values().next().value;
  if (!request) return;
  const target = start();
  if (!target) {
    giveUp();
    return;
  }
  if (!ready) return;
  running = request;
  timer = setTimeout(() => {
    drop();
    finish(request, "unchecked");
    next();
  }, PATTERN_TIMEOUT_MS);
  target.postMessage({ id: request.id, pattern: request.pattern, text: request.text } satisfies PatternRequest);
}

/** Whether `text` as a whole matches `pattern`, found in the worker within `PATTERN_TIMEOUT_MS`. */
export function checkPattern(pattern: string, text: string): Promise<PatternResult> {
  return new Promise((settle) => {
    const request: Waiting = { id: nextId++, pattern, text, settle };
    waiting.set(request.id, request);
    next();
  });
}
