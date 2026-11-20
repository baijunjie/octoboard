// Runs a check's command so that it can neither outlive its run nor, once stalled, hold the machine-wide
// heavy-step slot (`lock.mjs`) for ever. `heavy.mjs` runs the command it takes the slot for through it, and
// `pnpm verify` every step.
//
// The command runs as the leader of a process group of its own (a session of its own: `detached`), which every
// process it starts joins and keeps when its parent exits: a test binary whose `cargo` was killed is still found
// by the group. A signal sent to the runner goes to the whole group, and whatever of the group is left once the
// command has closed is ended, so no orphaned test binary goes on running after the run that started it. The
// group is recorded with the slot, so a waiter that takes over the slot of a runner killed outright (SIGKILL,
// which no handler sees, also when sent to the runner's own group, which the command is no longer in) ends what
// that runner left behind first. Being in a session of its own, the command has no controlling terminal: Ctrl-C
// reaches it through the runner, and Ctrl-Z stops the runner alone.
//
// A command whose processes together use almost no CPU time for `STALL_MS` is taken to be stalled: a build or a
// test suite that makes progress always uses CPU, and the longest wait a test makes on purpose is far shorter. A
// stalled command has the threads of its processes reported (a Rust test binary names each test's thread after
// the test, so the hung test is named), is ended with its group, and fails with `STALLED_EXIT`.
//
// Unix only: Windows has no process groups to find leftovers by; there the runner signals its child alone.
import { spawn, spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { localValue } from "../lib/env.mjs";

// How long a command may go without using `STALL_CPU_SECONDS` of CPU in all before it counts as stalled.
// `OCTOBOARD_HEAVY_STALL_SECONDS` overrides it, from the environment or else from `.env.local`
// (`.env.local.example`).
export const STALL_MS = 10 * 60_000;
export const STALL_SETTING = "OCTOBOARD_HEAVY_STALL_SECONDS";
// Below this much CPU time over the window, the command made no progress: an idle test binary's runtime
// threads and timers still use a little.
export const STALL_CPU_SECONDS = 2;
// The exit status of a command ended for stalling, the one `timeout(1)` uses.
export const STALLED_EXIT = 124;
// How long a group has to end after SIGTERM before it is sent SIGKILL.
export const KILL_GRACE_MS = 10_000;
// Servers a command can start that serve every run on the machine, not this one, and stay in its group: the Rust
// compiler cache's server. Neither they nor the compilers they run for whichever build asks are counted,
// signaled or ended with a group. The same program also runs as each compile's client, a child of the build
// (cargo), which runs some compiles itself; those are the run's own. A server is told from a client by its
// parent: a client's is another program of its own group (cargo), while a server's is the client that started it,
// or, once that has exited, whichever process adopted it (init, or a subreaper on Linux). A client whose cargo
// was ended is adopted too and would then look like a server, so a group being ended keeps the servers it saw
// before it signaled anything (`endGroup`), and takes what they run afresh at every look.
export const SHARED_SERVERS = new Set(["sccache"]);

// The stall window `configured` (seconds) gives, or `STALL_MS`.
export function stallMs(configured = localValue(STALL_SETTING)) {
  const seconds = Number.parseFloat(configured ?? "");
  return seconds > 0 ? seconds * 1000 : STALL_MS;
}

// When the process started, to tell a process from an unrelated one that reused its pid; empty when `ps` cannot
// tell.
export function startOf(pid) {
  // `ps` prints the time in the caller's locale and time zone; pin both so every run reads the same text.
  const result = spawnSync("ps", ["-o", "lstart=", "-p", String(pid)], { encoding: "utf8", env: { ...process.env, LC_ALL: "C", TZ: "UTC" } });
  return result.status === 0 ? result.stdout.trim() : "";
}

// Seconds of CPU time from `ps`'s `time` column: `[dd-][hh:]mm:ss[.cc]`.
export function parseCpuTime(text) {
  const [days, clock] = text.includes("-") ? text.split("-") : ["0", text];
  const parts = clock.split(":").map(Number);
  const seconds = parts.reduce((total, part) => total * 60 + part, 0);
  return Number(days) * 86_400 + seconds;
}

// Every process on the machine, as `{ pid, ppid, pgid, cpu, name }`; null when `ps` fails, which a loaded machine
// can cause, so that a failure is never read as a group with nothing left in it.
export function listProcesses() {
  const result = spawnSync("ps", ["-ax", "-o", "pid=,ppid=,pgid=,time=,comm="], { encoding: "utf8", env: { ...process.env, LC_ALL: "C" } });
  if (result.status !== 0) return null;
  const processes = [];
  for (const line of result.stdout.split("\n")) {
    const match = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(\S+)\s+(.*)$/.exec(line);
    if (!match) continue;
    const [, pid, ppid, pgid, time, command] = match;
    processes.push({ pid: Number(pid), ppid: Number(ppid), pgid: Number(pgid), cpu: parseCpuTime(time), name: path.basename(command.trim()) });
  }
  return processes;
}

// The pids of the shared servers among `processes`.
function serverPids(processes) {
  const byPid = new Map(processes.map((entry) => [entry.pid, entry]));
  const isServer = (entry) => {
    const parent = byPid.get(entry.ppid);
    return SHARED_SERVERS.has(entry.name) && !(parent && parent.pgid === entry.pgid && parent.name !== entry.name);
  };
  return new Set(processes.filter(isServer).map((entry) => entry.pid));
}

// The pids of `servers` (by default the shared servers among `processes`) and of everything they run among
// `processes`.
function sharedPids(processes, servers = serverPids(processes)) {
  const shared = new Set([...servers].filter((pid) => processes.some((entry) => entry.pid === pid)));
  for (let grew = shared.size > 0; grew;) {
    grew = false;
    for (const entry of processes) {
      if (!shared.has(entry.pid) && shared.has(entry.ppid)) {
        shared.add(entry.pid);
        grew = true;
      }
    }
  }
  return shared;
}

// The processes of group `pgid` other than the shared servers (`servers`, by default the ones among `processes`)
// and what they run; null when the processes could not be listed.
export function groupMembers(pgid, processes = listProcesses(), servers = undefined) {
  if (!processes) return null;
  const shared = sharedPids(processes, servers);
  return processes.filter((entry) => entry.pgid === pgid && !shared.has(entry.pid));
}

// Sends `signal` to every member of group `pgid` (not to the group as a whole, which would reach its shared
// servers too); answers how many there were. When the processes cannot be listed the leader alone is signaled,
// so that an interrupt still reaches the command, and the answer is null.
export function signalGroup(pgid, signal, processes = listProcesses(), servers = undefined) {
  const members = groupMembers(pgid, processes, servers);
  for (const pid of members ? members.map((member) => member.pid) : [pgid]) {
    try {
      process.kill(pid, signal);
    } catch {
      // Gone meanwhile.
    }
  }
  return members ? members.length : null;
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Ends group `pgid`: SIGTERM, then SIGKILL for whatever is left after `graceMs`.
// The shared servers are the ones seen before anything is signaled: a client adopted once its cargo has ended
// stays a member, while a compiler a server starts meanwhile, for whichever build, is spared.
export async function endGroup(pgid, graceMs = KILL_GRACE_MS) {
  const first = listProcesses();
  const servers = first ? serverPids(first) : undefined;
  if (signalGroup(pgid, "SIGTERM", first, servers) === 0) return;
  const until = Date.now() + graceMs;
  while (Date.now() < until) {
    await sleep(Math.min(200, graceMs));
    if (groupMembers(pgid, listProcesses(), servers)?.length === 0) return;
  }
  signalGroup(pgid, "SIGKILL", listProcesses(), servers);
}

// Ends at once what is left of the group a slot's holder recorded (`{ pid, started }`, its leader and the
// leader's start time), once the holder is gone. A group id is not reused while any process of the group is
// left, and a new group takes the id of its own leader, so a live process with the leader's pid and another
// start time means the group is gone and the id now names someone else's: nothing is ended then. Answers how many
// were ended, or null when the processes could not be listed, for the caller to try again.
export function endLeftovers(group, startTime = startOf) {
  if (!group?.pid) return 0;
  const processes = listProcesses();
  if (!processes) return null;
  const leader = processes.find((entry) => entry.pid === group.pid);
  if (leader && group.started && startTime(group.pid) !== group.started) return 0;
  return signalGroup(group.pid, "SIGKILL", processes);
}

// Whether a command has stalled, from what its group's processes are seen using: `observe(members, now)` takes
// one poll (`groupMembers`'s answer, null when unknown) and answers true once the processes have used under
// `stallCpuSeconds` of CPU for `stallMs`. Only the time between polls counts, at most two poll intervals each,
// so the machine sleeping, or a poll held up, is not taken for the command doing nothing. A process that started
// and ended between two polls is not seen: CPU is what counts, not processes coming and going, so a loop that
// starts a process now and then does not keep a hung command alive.
export function stallWatch({ stallMs: window = STALL_MS, stallCpuSeconds = STALL_CPU_SECONDS, pollMs, start = Date.now() }) {
  let seen = new Map();
  let used = 0;
  let quiet = 0;
  let last = start;
  return {
    observe(members, now) {
      const elapsed = Math.min(Math.max(0, now - last), 2 * pollMs);
      last = now;
      if (!members) return false;
      const cpu = new Map(members.map((member) => [member.pid, member.cpu]));
      for (const [pid, time] of cpu) used += seen.has(pid) ? Math.max(0, time - seen.get(pid)) : time;
      seen = cpu;
      if (used >= stallCpuSeconds) {
        used = 0;
        quiet = 0;
        return false;
      }
      quiet += elapsed;
      return quiet >= window;
    },
  };
}

// The names of the threads in `sample`'s report (macOS) that are a Rust test's, named after the test by
// libtest: the ones holding a path (`module::test`).
export function testThreads(report) {
  const names = new Set();
  for (const line of report.split("\n")) {
    const match = /Thread_\d+(?:\s+DispatchQueue_\d+)?:\s+(.+?)\s*$/.exec(line);
    if (match && match[1].includes("::")) names.add(match[1]);
  }
  return [...names];
}

// `sample`'s report of `pid` over one second, or "" when it fails or takes too long; asynchronous, so that the
// runner still handles an interrupt meanwhile.
function sampleOf(pid) {
  return new Promise((resolve) => {
    const child = spawn("sample", [String(pid), "1"], { stdio: ["ignore", "pipe", "ignore"], timeout: 30_000 });
    let report = "";
    child.stdout.on("data", (chunk) => (report += chunk));
    child.on("error", () => resolve(""));
    child.on("close", (code) => resolve(code === 0 ? report : ""));
  });
}

// What the members of a stalled group are, with the tests their threads run where they can be read. All are
// sampled at once, the leaves (members that run no other member) first where there are more than the cap: a
// test binary is usually one, but not when it has a stand-in of its own running.
async function describeGroup(pgid) {
  const members = groupMembers(pgid) ?? [];
  const parents = new Set(members.map((member) => member.ppid));
  const sampled = [...members].sort((a, b) => Number(parents.has(a.pid)) - Number(parents.has(b.pid))).slice(0, 12);
  const tests = process.platform === "darwin" ? await Promise.all(sampled.map((member) => sampleOf(member.pid).then(testThreads))) : [];
  const lines = members.slice(0, 20).map((member) => `  pid ${member.pid}: ${member.name}`);
  sampled.forEach((member, index) => {
    for (const test of tests[index] ?? []) lines.push(`  pid ${member.pid} is running test: ${test}`);
  });
  return lines;
}

// Runs `command` with `args` as described above; resolves with `{ code, signaled, stalled }`, `code` being the
// command's exit status (`128 + n` for a signal, 1 when it could not start, `STALLED_EXIT` when it was ended for
// stalling) and `signaled` the signal this process was sent and passed on, if any. `onGroup(group)` is called
// with `{ pid, started }` once the group exists; `name` prefixes what is reported. A poll counts towards the
// stall window only once `armed()` holds, for a test that must not have its command ended while it starts.
export function runInGroup(command, args, { env = process.env, name = "heavy", label = command, stallMs: window = STALL_MS, stallCpuSeconds = STALL_CPU_SECONDS, pollMs = Math.min(30_000, window / 4), onGroup = () => {}, startTime = startOf, armed = () => true } = {}) {
  return new Promise((resolve) => {
    const child = spawn(command, args, { stdio: "inherit", env, detached: true });
    const pgid = child.pid;
    let signaled = null;
    let stalled = false;
    let watch = null;
    const forward = (signal) => {
      signaled = signal;
      signalGroup(pgid, signal);
    };
    const signals = ["SIGINT", "SIGTERM", "SIGHUP"];
    const stop = () => {
      clearInterval(watch);
      for (const signal of signals) process.off(signal, forward);
    };
    child.on("error", (error) => {
      stop();
      console.error(`${name}: could not run ${command}: ${error.message}`);
      resolve({ code: 1, signaled, stalled });
    });
    if (!pgid) return;
    for (const signal of signals) process.on(signal, forward);
    onGroup({ pid: pgid, started: startTime(pgid) });

    let stall = null;
    watch = setInterval(() => {
      if (stalled) return;
      if (!armed()) return;
      stall ??= stallWatch({ stallMs: window, stallCpuSeconds, pollMs });
      if (!stall.observe(groupMembers(pgid), Date.now())) return;
      stalled = true;
      clearInterval(watch);
      const shown = window >= 60_000 ? `${Math.round(window / 6000) / 10} min` : `${Math.round(window / 1000)}s`;
      console.error(`${name}: ${label} made no progress for ${shown} (its processes used under ${stallCpuSeconds}s of CPU); ending it. Its processes:`);
      void describeGroup(pgid).then((lines) => {
        for (const line of lines) console.error(line);
        return endGroup(pgid);
      });
    }, pollMs);
    watch.unref();

    child.on("close", async (code, signal) => {
      clearInterval(watch);
      // Whatever the command left running in its group: an orphan nothing would end otherwise. The signal
      // handlers stay until then, so a second interrupt goes to what is left rather than ending the runner
      // before it has released the slot.
      await endGroup(pgid, 2000);
      stop();
      if (stalled) {
        console.error(`${name}: ${label} was ended for making no progress (exit ${STALLED_EXIT})`);
        resolve({ code: STALLED_EXIT, signaled, stalled });
        return;
      }
      const ended = signal ?? signaled;
      resolve({ code: code ?? (ended ? 128 + (os.constants.signals[ended] ?? 0) : 1), signaled, stalled });
    });
  });
}
