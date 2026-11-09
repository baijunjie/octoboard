import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readdirSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { mock, test } from "node:test";
import { NO_OWNER_GRACE_MS, acquireSlot, describeWait, slotCount } from "./lock.mjs";

const GIB = 1024 ** 3;
const POLL_MS = 20;
const options = (root, slots) => ({ root, slots, pollMs: POLL_MS, reminderMs: 1 });
// `acquireSlot` polls on `setTimeout` and reads `Date.now()`, so the tests run it on node:test's fake clock rather
// than wait out real poll intervals: nothing here depends on how fast the machine is. The clock starts at the real
// time because ticket and slot ages are compared with file times.
function fakeClock(t) {
  mock.timers.enable({ apis: ["setTimeout", "Date"], now: Date.now() });
  // node:test's fake clock has no count of pending timers, so leaving none behind cannot be asserted: runAll ticks only
  // to the furthest pending timer, and reset clears any armed beyond it.
  t.after(() => {
    mock.timers.runAll();
    mock.timers.reset();
  });
}
// Lets every promise continuation run. Between two polls a waiter only does synchronous file work, so after this
// each waiter has armed its next timer; it is a yield, not a wait on any duration.
const settle = () => new Promise((resolve) => setImmediate(resolve));
// Advances the fake clock one millisecond at a time, so that waiters with different poll intervals interleave as
// they would in real time, until `condition()` holds. The cap is in fake milliseconds, so it never depends on speed.
async function advanceUntil(condition, limitMs = 60_000) {
  for (let elapsed = 0; !condition(); elapsed++) {
    if (elapsed >= limitMs) throw new Error("the condition did not hold within the fake time allowed");
    mock.timers.tick(1);
    await settle();
  }
}
const advance = async (ms) => {
  let elapsed = 0;
  await advanceUntil(() => elapsed++ >= ms);
};
// Resolves with the promise's value once the fake clock has run it to completion.
async function resolved(promise) {
  let done = false;
  promise.then(() => (done = true), () => (done = true));
  await advanceUntil(() => done);
  return promise;
}
// A pid no process has: above the largest pid macOS and Linux hand out by default.
const DEAD_PID = 2 ** 22 + 12345;

// Starts a waiter that appends its label to `order` once it holds a slot. A later waiter is given a
// shorter poll, so that only the queue, not who polls first after a release, can keep the order.
function wait(label, root, slots, order, pollMs = POLL_MS) {
  return acquireSlot(label, { ...options(root, slots), pollMs }).then((release) => {
    order.push(label);
    return release;
  });
}

test("the slot count follows cores and memory, between one and two", () => {
  assert.equal(slotCount({ cores: 10, memory: 16 * GIB, configured: "" }), 1);
  assert.equal(slotCount({ cores: 4, memory: 8 * GIB, configured: "" }), 1);
  assert.equal(slotCount({ cores: 16, memory: 32 * GIB, configured: "" }), 2);
  assert.equal(slotCount({ cores: 64, memory: 256 * GIB, configured: "" }), 2);
  assert.equal(slotCount({ cores: 16, memory: 16 * GIB, configured: "" }), 1);
  assert.equal(slotCount({ cores: 10, memory: 16 * GIB, configured: "3" }), 3);
});

test("a second claim waits until the first is released", async (t) => {
  fakeClock(t);
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    const releaseFirst = await acquireSlot("first", options(root, 1));
    let waited = null;
    let acquired = false;
    const second = acquireSlot("second", { ...options(root, 1), onWait: (holders) => (waited ??= holders) }).then((release) => {
      acquired = true;
      return release;
    });
    await advance(POLL_MS * 7);
    assert.equal(acquired, false);
    assert.deepEqual(waited, ["first"]);
    assert.equal(readdirSync(path.join(root, "queue")).length, 1);
    releaseFirst();
    (await resolved(second))();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("two slots admit two holders and queue the third", async (t) => {
  fakeClock(t);
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    const releases = [await acquireSlot("a", options(root, 2)), await acquireSlot("b", options(root, 2))];
    let acquired = false;
    const third = acquireSlot("c", options(root, 2)).then((release) => {
      acquired = true;
      return release;
    });
    await advance(POLL_MS * 5);
    assert.equal(acquired, false);
    releases[0]();
    (await resolved(third))();
    releases[1]();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a slot whose holder is gone is taken over", async () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    mkdirSync(path.join(root, "slot-0"));
    writeFileSync(path.join(root, "slot-0", "owner.json"), JSON.stringify({ pid: DEAD_PID, label: "dead" }));
    (await acquireSlot("live", options(root, 1)))();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("waiters get the slot in the order they arrived", async (t) => {
  fakeClock(t);
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    let release = await acquireSlot("holder", options(root, 1));
    const order = [];
    const waiters = [];
    for (const [label, pollMs] of [["a", 80], ["b", 40], ["c", 5]]) {
      waiters.push(wait(label, root, 1, order, pollMs));
      await advance(40);
    }
    for (const waiter of waiters) {
      release();
      release = await resolved(waiter);
    }
    release();
    assert.deepEqual(order, ["a", "b", "c"]);
    assert.deepEqual(readdirSync(path.join(root, "queue")), []);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a run that releases and claims again queues behind the earlier waiter", async (t) => {
  fakeClock(t);
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    const order = [];
    let sweep = await acquireSlot("sweep", options(root, 1));
    const verify = wait("verify", root, 1, order, 80);
    await advance(40);
    // A sweep that loads again right after its release takes a ticket behind the waiting run.
    sweep();
    sweep = wait("sweep", root, 1, order, 5);
    (await resolved(verify))();
    (await resolved(sweep))();
    assert.deepEqual(order, ["verify", "sweep"]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a waiter only claims while fewer are ahead of it than slots are free", async (t) => {
  fakeClock(t);
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    const held = [await acquireSlot("x", options(root, 2)), await acquireSlot("y", options(root, 2))];
    const order = [];
    const first = wait("first", root, 2, order, 300);
    await advance(40);
    let aheadOfSecond = null;
    const second = acquireSlot("second", { ...options(root, 2), pollMs: 5, onWait: (_, ahead) => (aheadOfSecond ??= ahead) }).then((release) => {
      order.push("second");
      return release;
    });
    await advance(40);
    assert.deepEqual(aheadOfSecond, ["first"]);
    // One slot is free and one waiter is ahead: the second, polling far more often, must leave it to the first.
    held[0]();
    const releaseFirst = await resolved(first);
    assert.deepEqual(order, ["first"]);
    held[1]();
    (await resolved(second))();
    releaseFirst();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a waiter that is gone does not hold up the queue", async () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    const queue = path.join(root, "queue");
    mkdirSync(queue, { recursive: true });
    writeFileSync(path.join(queue, "1"), JSON.stringify({ pid: DEAD_PID, label: "dead waiter" }));
    (await acquireSlot("live", options(root, 1)))();
    assert.deepEqual(readdirSync(queue), []);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a ticket left half-taken by a killed waiter is cleared", async (t) => {
  fakeClock(t);
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    const release = await acquireSlot("holder", options(root, 1));
    const queue = path.join(root, "queue");
    const halfTaken = path.join(queue, ".ticket-1-1");
    writeFileSync(halfTaken, "{}");
    const old = new Date(Date.now() - 2 * NO_OWNER_GRACE_MS);
    utimesSync(halfTaken, old, old);
    const waiter = acquireSlot("waiter", options(root, 1));
    // A waiter clears such tickets in its first poll, which runs inside the call.
    assert.deepEqual(readdirSync(queue).filter((name) => name.startsWith(".")), []);
    release();
    (await resolved(waiter))();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the wait message names what runs and what is queued ahead", () => {
  assert.equal(describeWait(["a: ui tests"], []), "running now: a: ui tests");
  assert.equal(describeWait(["a: ui tests"], ["b: gallery sweep", "c: rust tests"]), "running now: a: ui tests; queued ahead (2): b: gallery sweep; c: rust tests");
  assert.equal(describeWait([], ["b: gallery sweep"]), "running now: nothing (a slot is being handed over); queued ahead (1): b: gallery sweep");
});

test("a waiter whose ticket was taken from it queues again and leaves others' tickets alone", async (t) => {
  fakeClock(t);
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    const release = await acquireSlot("holder", options(root, 1));
    const order = [];
    const robbed = wait("robbed", root, 1, order);
    const queue = path.join(root, "queue");
    rmSync(path.join(queue, readdirSync(queue)[0]));
    const later = wait("later", root, 1, order);
    // The robbed waiter notices at its next poll and takes a second ticket, behind the later one.
    await advanceUntil(() => readdirSync(queue).length === 2);
    release();
    (await resolved(later))();
    (await resolved(robbed))();
    assert.deepEqual(order, ["later", "robbed"]);
    assert.deepEqual(readdirSync(queue), []);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a waiter whose ticket was removed takes a new one behind those queued meanwhile", async (t) => {
  fakeClock(t);
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    const release = await acquireSlot("holder", options(root, 1));
    const order = [];
    const queue = path.join(root, "queue");
    const robbed = wait("robbed", root, 1, order);
    const middle = wait("middle", root, 1, order);
    rmSync(path.join(queue, readdirSync(queue).sort((a, b) => a - b)[0]));
    await advanceUntil(() => readdirSync(queue).length === 2);
    release();
    (await resolved(middle))();
    (await resolved(robbed))();
    assert.deepEqual(order, ["middle", "robbed"]);
    assert.deepEqual(readdirSync(queue), []);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
