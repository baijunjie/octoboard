// A machine-wide cap on concurrent heavy steps (UI tests, Rust builds and tests), shared by every
// worktree. Several worktrees running vitest, cargo and Playwright at once drove a 10-core machine
// to a load average several times the core count and slowed every process on it; a cap on each run's
// own workers cannot prevent that, so the runs queue for a slot here instead.
//
// A slot is a directory made with `mkdir`, which is atomic, holding an `owner.json` with the
// holder's pid. A slot whose holder is gone (killed without releasing) is taken over on the next poll.
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";

const GIB = 1024 ** 3;
// A slot directory is made before its owner file is written; one without an owner for this long is orphaned.
const NO_OWNER_GRACE_MS = 10_000;

// A fixed path rather than `os.tmpdir()`, which follows `$TMPDIR` and would give a run with another
// `$TMPDIR` slots of its own; the user id keeps one user's slots from blocking another's.
export const defaultRoot = process.platform === "win32"
  ? path.join(os.tmpdir(), "octoboard-verify-slots")
  : `/tmp/octoboard-verify-slots-${process.getuid()}`;

// One heavy step per 8 cores and per 12 GiB of memory, between one and two, so a 10-core 16 GiB
// machine gets one. `OCTOBOARD_VERIFY_SLOTS` overrides it.
export function slotCount({ cores = os.cpus().length, memory = os.totalmem(), env = process.env } = {}) {
  const override = Number.parseInt(env.OCTOBOARD_VERIFY_SLOTS ?? "", 10);
  if (override > 0) return override;
  return Math.min(2, Math.max(1, Math.min(Math.floor(cores / 8), Math.floor(memory / (12 * GIB)))));
}

// When the process started, to tell a holder from an unrelated process that reused its pid.
function startOf(pid) {
  // `ps` prints the time in the caller's locale and time zone; pin both so every run reads the same text.
  const result = spawnSync("ps", ["-o", "lstart=", "-p", String(pid)], { encoding: "utf8", env: { ...process.env, LC_ALL: "C", TZ: "UTC" } });
  return result.status === 0 ? result.stdout.trim() : "";
}

function isAlive({ pid, started }) {
  try {
    process.kill(pid, 0);
  } catch (error) {
    if (error.code !== "EPERM") return false;
  }
  return !started || startOf(pid) === started;
}

function readOwner(dir) {
  try {
    return JSON.parse(readFileSync(path.join(dir, "owner.json"), "utf8"));
  } catch {
    return null;
  }
}

function isOrphaned(dir, owner) {
  if (owner) return !isAlive(owner);
  try {
    return Date.now() - statSync(dir).mtimeMs > NO_OWNER_GRACE_MS;
  } catch {
    return false;
  }
}

// Tries each slot once: the claimed directory, or the labels of the runs holding them all.
function tryClaim(root, slots, label) {
  const holders = [];
  for (let index = 0; index < slots; index++) {
    const dir = path.join(root, `slot-${index}`);
    try {
      mkdirSync(dir);
      writeFileSync(path.join(dir, "owner.json"), JSON.stringify({ pid: process.pid, started: startOf(process.pid), label }));
      return { dir };
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
    }
    const owner = readOwner(dir);
    // Checked again right before the move: a waiter that got there first may already have replaced the slot.
    if (isOrphaned(dir, owner) && JSON.stringify(readOwner(dir)) === JSON.stringify(owner)) {
      const stale = `${dir}.stale-${process.pid}`;
      try {
        renameSync(dir, stale);
        rmSync(stale, { recursive: true, force: true });
      } catch {
        // Another waiter took it over first.
      }
    }
    holders.push(owner?.label ?? "another run");
  }
  return { holders };
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Resolves with a `release` function once a slot is held; `onWait(holders)` is called when the
// first attempt fails and then every `reminderMs`. Releasing twice is harmless.
export async function acquireSlot(label, { root = defaultRoot, slots = slotCount(), pollMs = 2000, reminderMs = 60_000, onWait = () => {} } = {}) {
  mkdirSync(root, { recursive: true });
  let nextReminder = 0;
  for (;;) {
    const claim = tryClaim(root, slots, label);
    if (claim.dir) {
      // Only the holder's own slot is removed: one that was taken over meanwhile belongs to someone else.
      const release = () => {
        if (readOwner(claim.dir)?.pid === process.pid) rmSync(claim.dir, { recursive: true, force: true });
      };
      process.once("exit", release);
      return () => {
        process.off("exit", release);
        release();
      };
    }
    if (Date.now() >= nextReminder) {
      onWait(claim.holders);
      nextReminder = Date.now() + reminderMs;
    }
    await sleep(pollMs);
  }
}
