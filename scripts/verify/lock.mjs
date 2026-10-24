// A machine-wide cap on concurrent heavy steps (UI tests, Rust builds and tests), shared by every
// worktree. Several worktrees running vitest, cargo and Playwright at once drove a 10-core machine
// to a load average several times the core count and slowed every process on it; a cap on each run's
// own workers cannot prevent that, so the runs queue for a slot here instead.
//
// A slot is a directory made with `mkdir`, which is atomic, holding an `owner.json` with the
// holder's pid and a token that marks it as the holder's. A slot whose holder is gone (killed
// without releasing) is taken over on the next poll.
//
// Waiters are served first come, first served: each takes a numbered ticket in `queue/` before it
// looks at the slots, and claims one only while fewer waiters are ahead of it than slots are free.
// Otherwise whoever polled first after a release would win, and a waiter could keep losing to runs
// that arrived after it. A ticket whose waiter is gone is dropped by the next waiter behind it.
import { spawnSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { linkSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, statSync, unlinkSync, writeFileSync } from "node:fs";
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
  // An empty start means `ps` itself failed, which a loaded machine can cause; the process exists, so
  // it is taken as alive rather than have its slot or ticket taken from it.
  const now = started && startOf(pid);
  return !now || now === started;
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

// Removes `dir` if its holder is gone; checked again right before the move, since a waiter that got
// there first may already have replaced it.
function clearOrphan(dir) {
  const owner = readOwner(dir);
  if (!isOrphaned(dir, owner) || JSON.stringify(readOwner(dir)) !== JSON.stringify(owner)) return false;
  const stale = `${dir}.stale-${process.pid}`;
  try {
    renameSync(dir, stale);
  } catch {
    // Another waiter took it over first, and may already hold it.
    return false;
  }
  rmSync(stale, { recursive: true, force: true });
  return true;
}

// The free slots' directories and the labels of the runs holding the others.
function inspectSlots(root, slots) {
  const free = [];
  const holders = [];
  for (let index = 0; index < slots; index++) {
    const dir = path.join(root, `slot-${index}`);
    if (!statOrNull(dir) || clearOrphan(dir)) free.push(dir);
    else holders.push(readOwner(dir)?.label ?? "another run");
  }
  return { free, holders };
}

function statOrNull(file) {
  try {
    return statSync(file);
  } catch {
    return null;
  }
}

function claim(dir, owner) {
  try {
    mkdirSync(dir);
  } catch (error) {
    if (error.code === "EEXIST") return false;
    throw error;
  }
  writeFileSync(path.join(dir, "owner.json"), JSON.stringify(owner));
  return true;
}

// The queue's tickets in order, as `{ number, file }`; names that are not ticket numbers are skipped.
function tickets(queue) {
  return readdirSync(queue)
    .filter((name) => /^\d+$/.test(name))
    .map((name) => ({ number: Number(name), file: path.join(queue, name) }))
    .sort((a, b) => a.number - b.number);
}

// Takes the number after the highest ticket. The ticket is written under a temporary name and linked
// into place, so it is never seen without its owner, and the link fails if another waiter took the
// number first. A waiter that arrives while another is taking its ticket may get the lower number:
// the two arrived at the same moment, so either order is first come, first served.
function takeTicket(queue, owner) {
  const temporary = path.join(queue, `.ticket-${process.pid}-${Date.now()}`);
  writeFileSync(temporary, JSON.stringify(owner));
  try {
    let number = (tickets(queue).at(-1)?.number ?? 0) + 1;
    for (;;) {
      const file = path.join(queue, String(number));
      try {
        linkSync(temporary, file);
        return file;
      } catch (error) {
        if (error.code !== "EEXIST") throw error;
        number++;
      }
    }
  } finally {
    unlinkSync(temporary);
  }
}

// The labels of the live waiters ahead of `mine`, or null when `mine` is no longer in the queue; the
// tickets of waiters that are gone are removed.
function waitersAhead(queue, mine) {
  const ahead = [];
  for (const ticket of tickets(queue)) {
    if (ticket.file === mine.file) return ahead;
    const owner = readTicket(ticket.file);
    if (owner && !isAlive(owner)) rmSync(ticket.file, { force: true });
    else if (owner) ahead.push(owner.label ?? "another run");
  }
  return null;
}

function holdsTicket(file, owner) {
  return readTicket(file)?.token === owner.token;
}

function readTicket(file) {
  try {
    return JSON.parse(readFileSync(file, "utf8"));
  } catch {
    return null;
  }
}

// A temporary ticket left by a waiter killed between writing and linking it.
function clearTemporaryTickets(queue) {
  for (const name of readdirSync(queue)) {
    if (!name.startsWith(".ticket-")) continue;
    const file = path.join(queue, name);
    const stat = statOrNull(file);
    if (stat && Date.now() - stat.mtimeMs > NO_OWNER_GRACE_MS) rmSync(file, { force: true });
  }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// How a waiter tells what it is waiting for: "running now: <holders>", and the waiters ahead of it.
export function describeWait(holders, ahead) {
  const running = `running now: ${holders.length > 0 ? holders.join("; ") : "nothing (a slot is being handed over)"}`;
  return ahead.length > 0 ? `${running}; queued ahead (${ahead.length}): ${ahead.join("; ")}` : running;
}

// Resolves with a `release` function once a slot is held. `onWait(holders, ahead)` gets the labels of
// the runs holding the slots and of the waiters queued ahead; it is called when the first attempt
// fails, whenever the number ahead changes, and otherwise every `reminderMs`. Releasing twice is harmless.
export async function acquireSlot(label, { root = defaultRoot, slots = slotCount(), pollMs = 2000, reminderMs = 60_000, onWait = () => {} } = {}) {
  const queue = path.join(root, "queue");
  mkdirSync(queue, { recursive: true });
  // The token tells this run's ticket and slot from another's that reused the number or took the slot over.
  const owner = { pid: process.pid, started: startOf(process.pid), label, token: randomUUID() };
  const ticket = { file: takeTicket(queue, owner) };
  const leaveQueue = () => {
    if (holdsTicket(ticket.file, owner)) rmSync(ticket.file, { force: true });
  };
  process.once("exit", leaveQueue);
  try {
    let nextReminder = 0;
    let lastAhead = -1;
    for (;;) {
      let ahead = waitersAhead(queue, ticket);
      // This waiter's ticket was removed, or its number reused: it queues again, at the back.
      if (!ahead || !holdsTicket(ticket.file, owner)) {
        ticket.file = takeTicket(queue, owner);
        ahead = waitersAhead(queue, ticket);
      }
      const { free, holders } = inspectSlots(root, slots);
      // Only as many waiters as there are free slots may claim one, the earliest first; a waiter whose
      // place cannot be read this poll claims nothing.
      const dir = ahead && ahead.length < free.length ? free.find((candidate) => claim(candidate, owner)) : undefined;
      ahead ??= [];
      if (dir) {
        // Only the holder's own slot is removed: one that was taken over meanwhile belongs to someone else.
        const release = () => {
          if (readOwner(dir)?.token === owner.token) rmSync(dir, { recursive: true, force: true });
        };
        process.once("exit", release);
        return () => {
          process.off("exit", release);
          release();
        };
      }
      if (Date.now() >= nextReminder || ahead.length !== lastAhead) {
        onWait(holders, ahead);
        nextReminder = Date.now() + reminderMs;
        lastAhead = ahead.length;
      }
      clearTemporaryTickets(queue);
      await sleep(pollMs);
    }
  } finally {
    process.off("exit", leaveQueue);
    leaveQueue();
  }
}
