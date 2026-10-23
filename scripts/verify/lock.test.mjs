import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { acquireSlot, slotCount } from "./lock.mjs";

const GIB = 1024 ** 3;
const options = (root, slots) => ({ root, slots, pollMs: 20, reminderMs: 1 });

test("the slot count follows cores and memory, between one and two", () => {
  assert.equal(slotCount({ cores: 10, memory: 16 * GIB, env: {} }), 1);
  assert.equal(slotCount({ cores: 4, memory: 8 * GIB, env: {} }), 1);
  assert.equal(slotCount({ cores: 16, memory: 32 * GIB, env: {} }), 2);
  assert.equal(slotCount({ cores: 64, memory: 256 * GIB, env: {} }), 2);
  assert.equal(slotCount({ cores: 16, memory: 16 * GIB, env: {} }), 1);
  assert.equal(slotCount({ cores: 10, memory: 16 * GIB, env: { OCTOBOARD_VERIFY_SLOTS: "3" } }), 3);
});

test("a second claim waits until the first is released", async () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    const releaseFirst = await acquireSlot("first", options(root, 1));
    let waited = null;
    let acquired = false;
    const second = acquireSlot("second", { ...options(root, 1), onWait: (holders) => (waited ??= holders) }).then((release) => {
      acquired = true;
      return release;
    });
    await new Promise((resolve) => setTimeout(resolve, 150));
    assert.equal(acquired, false);
    assert.deepEqual(waited, ["first"]);
    releaseFirst();
    (await second)();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("two slots admit two holders and queue the third", async () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    const releases = [await acquireSlot("a", options(root, 2)), await acquireSlot("b", options(root, 2))];
    let acquired = false;
    const third = acquireSlot("c", options(root, 2)).then((release) => {
      acquired = true;
      return release;
    });
    await new Promise((resolve) => setTimeout(resolve, 100));
    assert.equal(acquired, false);
    releases[0]();
    (await third)();
    releases[1]();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a slot whose holder is gone is taken over", async () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "slots-"));
  try {
    mkdirSync(path.join(root, "slot-0"));
    writeFileSync(path.join(root, "slot-0", "owner.json"), JSON.stringify({ pid: 2 ** 22 + 12345, label: "dead" }));
    (await acquireSlot("live", options(root, 1)))();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
