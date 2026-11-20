import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import {
  endGroup, endLeftovers, groupMembers, listProcesses, parseCpuTime, runInGroup, signalGroup, startOf, STALL_CPU_SECONDS, STALL_MS, STALL_SETTING, STALLED_EXIT, stallMs, stallWatch, testThreads,
} from "./process-group.mjs";
import { alive, armedWhenReported, assertSharedServerSpared, fixtureArgs, gone, reported, scratch, startGroup, until, withoutPs } from "./processTestSupport.mjs";

// Runs the fixture in `mode` through `runInGroup`. The stall window only runs once the fixture has reported its
// processes, so that a slow start cannot end it; `console.error` is collected in `errors` rather than printed.
function run(t, area, mode, { fixture = {}, ...options } = {}) {
  const errors = [];
  t.mock.method(console, "error", (...lines) => errors.push(lines.join(" ")));
  const result = runInGroup(process.execPath, fixtureArgs(area, mode, { watchParent: true, ...fixture }), {
    onGroup: (group) => area.track(group.pid),
    armed: armedWhenReported(area),
    pollMs: 100,
    ...options,
  });
  return { result, errors };
}

test("a CPU time from ps is read in the formats macOS and Linux print", () => {
  const cases = [
    ["0:00.00", 0],
    ["0:01.23", 1.23],
    ["123:45.67", 123 * 60 + 45.67],
    ["00:00:01", 1],
    ["01:02:03", 3723],
    ["1-02:03:04", 86_400 + 2 * 3600 + 3 * 60 + 4],
  ];
  for (const [text, seconds] of cases) assert.ok(Math.abs(parseCpuTime(text) - seconds) < 1e-9, `${text} is ${seconds}s, got ${parseCpuTime(text)}`);
});

test("the stall window is the configured number of seconds when it is positive, else the default", () => {
  assert.equal(stallMs("30"), 30_000);
  assert.equal(stallMs("0.5"), 500);
  for (const value of [null, "", "0", "-3", "soon"]) assert.equal(stallMs(value), STALL_MS, String(value));
  assert.equal(STALL_MS, 10 * 60_000);
});

test("the stall window is read from the environment when no setting is given", () => {
  const before = process.env[STALL_SETTING];
  process.env[STALL_SETTING] = "7";
  try {
    assert.equal(stallMs(), 7000);
  } finally {
    if (before === undefined) delete process.env[STALL_SETTING];
    else process.env[STALL_SETTING] = before;
  }
});

function readFileOrNull(file) {
  try {
    return readFileSync(file, "utf8");
  } catch {
    return null;
  }
}

const pids = (members) => members.map((member) => member.pid);

test("a shared server and everything it runs are left out of a group's members", () => {
  const processes = [
    { pid: 10, ppid: 1, pgid: 10, name: "node" },
    { pid: 12, ppid: 10, pgid: 10, name: "rustc" },
    { pid: 13, ppid: 1, pgid: 99, name: "node" },
    { pid: 14, ppid: 10, pgid: 10, name: "sccache-wrapper" },
    // A server whose parent is gone, as the one a client started is, and what it runs.
    { pid: 11, ppid: 1, pgid: 10, name: "sccache" },
    { pid: 15, ppid: 11, pgid: 10, name: "rustc" },
    { pid: 16, ppid: 15, pgid: 10, name: "cc" },
  ];
  assert.deepEqual(pids(groupMembers(10, processes)), [10, 12, 14]);
  assert.deepEqual(groupMembers(5, processes), []);
});

test("a server started by an `sccache` client is a shared server, the client under cargo is not", () => {
  const processes = [
    { pid: 10, ppid: 1, pgid: 10, name: "cargo" },
    { pid: 20, ppid: 10, pgid: 10, name: "sccache" },
    { pid: 21, ppid: 20, pgid: 10, name: "sccache" },
    { pid: 22, ppid: 21, pgid: 10, name: "rustc" },
  ];
  assert.deepEqual(pids(groupMembers(10, processes)), [10, 20]);
});

test("a server adopted by a subreaper, or still under the client that started it, is a shared server", () => {
  const processes = [
    { pid: 10, ppid: 1, pgid: 10, name: "cargo" },
    { pid: 12, ppid: 10, pgid: 10, name: "rustc" },
    { pid: 500, ppid: 1, pgid: 500, name: "systemd" },
    { pid: 40, ppid: 500, pgid: 10, name: "sccache" },
    { pid: 41, ppid: 40, pgid: 10, name: "rustc" },
  ];
  assert.deepEqual(pids(groupMembers(10, processes)), [10, 12]);
});

test("precomputed server pids are used in place of the ones the listing shows, and what they run comes from the listing", () => {
  const processes = [
    { pid: 10, ppid: 1, pgid: 10, name: "cargo" },
    { pid: 30, ppid: 10, pgid: 10, name: "sccache" },
    { pid: 31, ppid: 30, pgid: 10, name: "rustc" },
  ];
  // 30 named as a server: it is left out with its compiler, which only the listing says is its child.
  assert.deepEqual(pids(groupMembers(10, processes, new Set([30]))), [10]);
  assert.deepEqual(pids(groupMembers(10, processes, new Set())), [10, 30, 31]);
  // A server pid that is not in the listing any more names nothing.
  assert.deepEqual(pids(groupMembers(10, processes, new Set([77]))), [10, 30, 31]);
});

test("an `sccache` client under cargo is a member, and so is the compiler it runs", () => {
  const processes = [
    { pid: 10, ppid: 1, pgid: 10, name: "cargo" },
    { pid: 30, ppid: 10, pgid: 10, name: "sccache" },
    { pid: 31, ppid: 30, pgid: 10, name: "rustc" },
    { pid: 32, ppid: 31, pgid: 10, name: "cc" },
  ];
  assert.deepEqual(pids(groupMembers(10, processes)), [10, 30, 31, 32]);
});

test("a failed process listing is null, never a group with nothing left in it", () => {
  assert.equal(withoutPs(() => listProcesses()), null);
  assert.equal(groupMembers(10, null), null);
});

test("the start time of a process is read, and empty for one that does not exist", () => {
  assert.notEqual(startOf(process.pid), "");
  assert.equal(startOf(process.pid), startOf(process.pid));
  assert.equal(startOf(2 ** 22 + 12345), "");
});

test("the process list holds this process with its parent, group and CPU time", () => {
  const me = listProcesses().find((entry) => entry.pid === process.pid);
  assert.ok(me, "this process is listed");
  assert.equal(me.ppid, process.ppid);
  assert.ok(me.pgid > 0);
  assert.ok(Number.isFinite(me.cpu) && me.cpu >= 0);
  assert.equal(me.name, path.basename(process.execPath));
});

test("test threads are the sampled threads named after a Rust test, each once", () => {
  const report = [
    "Call graph:",
    "    875 Thread_39727407   DispatchQueue_1: com.apple.main-thread  (serial)",
    "    + 875 std::sys::thread::Thread::new::thread_start  (in daemon) + 12  [0x1]",
    "    875 Thread_39727412: DelayedTaskSchedulerWorker",
    "    875 Thread_39727414",
    "    2290 Thread_31578302: session_git_hooks::tests::route::with_no_route_at_all_nothing_i…",
    "    2290 Thread_31578303: coordinator::tests::restart_waits  ",
    "    2290 Thread_31578304: session_git_hooks::tests::route::with_no_route_at_all_nothing_i…",
    "",
  ].join("\n");
  assert.deepEqual(testThreads(report), ["session_git_hooks::tests::route::with_no_route_at_all_nothing_i…", "coordinator::tests::restart_waits"]);
  assert.deepEqual(testThreads(""), []);
});

// The stall rule on synthetic polls: a member is `{ pid, cpu }` (CPU seconds it has used in all), `now` is in ms.
const WINDOW = 1000;
const POLL = 100;
const newWatch = () => stallWatch({ stallMs: WINDOW, stallCpuSeconds: 2, pollMs: POLL, start: 0 });
// Polls every `POLL` ms from `from` (exclusive) on, `members(now)` answering what is seen; answers the time the
// watch first says stalled, or null if it has not by `until`.
function stalledAt(watch, members, { from = 0, until = 10 * WINDOW } = {}) {
  for (let now = from + POLL; now <= until; now += POLL) if (watch.observe(members(now), now)) return now;
  return null;
}

test("a command whose processes use no CPU is stalled once the window has passed", () => {
  assert.equal(stalledAt(newWatch(), () => [{ pid: 1, cpu: 0.5 }]), WINDOW);
  assert.equal(STALL_CPU_SECONDS, 2);
});

test("CPU used in the window starts the window over, once it reaches the threshold", () => {
  // Under the threshold (1.5 of 2 s, in two steps) the window runs on.
  const small = (now) => [{ pid: 1, cpu: now >= 300 ? 0.5 : 0 }, { pid: 2, cpu: now >= 500 ? 1 : 0 }];
  assert.equal(stalledAt(newWatch(), small), WINDOW);
  // Two seconds used at 500 ms: the window starts over from there.
  assert.equal(stalledAt(newWatch(), (now) => [{ pid: 1, cpu: now >= 500 ? 2 : 0 }]), 500 + WINDOW);
  // Two seconds in small steps across processes, the way a build's compilers use it.
  assert.equal(stalledAt(newWatch(), (now) => [{ pid: 1, cpu: now >= 200 ? 1 : 0 }, { pid: 2, cpu: now >= 400 ? 1 : 0 }]), 400 + WINDOW);
});

test("a process that appears counts the CPU it has used so far, but its appearing alone is no progress", () => {
  // A new process with no CPU each poll, as a loop that starts short processes: still stalled at the window.
  assert.equal(stalledAt(newWatch(), (now) => [{ pid: 1, cpu: 0.5 }, { pid: 1000 + now, cpu: 0 }]), WINDOW);
  // One that shows up having used 3 s is progress.
  assert.equal(stalledAt(newWatch(), (now) => (now >= 600 ? [{ pid: 1, cpu: 0.5 }, { pid: 2, cpu: 3 }] : [{ pid: 1, cpu: 0.5 }])), 600 + WINDOW);
});

test("a process that ends does not take CPU back", () => {
  const members = (now) => (now < 300 ? [{ pid: 1, cpu: 1.5 }] : [{ pid: 2, cpu: 0 }]);
  // 1.5 s seen first, below the threshold; the process going away must not read as negative or as progress.
  assert.equal(stalledAt(newWatch(), members), WINDOW);
});

test("a gap between polls counts at most two poll intervals", () => {
  const watch = newWatch();
  const members = [{ pid: 1, cpu: 0 }];
  assert.equal(watch.observe(members, POLL), false);
  // The machine slept for ten seconds: it adds two intervals, not ten seconds.
  assert.equal(watch.observe(members, POLL + 10_000), false);
  // Quiet so far: POLL + 2 * POLL. The rest of the window comes from ordinary polls.
  let now = POLL + 10_000;
  let polls = 0;
  while (!watch.observe(members, (now += POLL))) polls++;
  assert.equal(polls * POLL, WINDOW - 3 * POLL - POLL);
});

test("polls that cannot see the processes add nothing to the window", () => {
  const watch = newWatch();
  const members = [{ pid: 1, cpu: 0 }];
  assert.equal(watch.observe(members, POLL), false);
  for (let now = 2 * POLL; now < WINDOW; now += POLL) assert.equal(watch.observe(null, now), false);
  // Time that passed unseen is not counted: this poll adds one interval, far from the window.
  assert.equal(watch.observe(members, WINDOW), false);
  let polls = 0;
  let now = WINDOW;
  while (!watch.observe(members, (now += POLL))) polls++;
  assert.equal(polls * POLL, WINDOW - 2 * POLL - POLL);
});

test("signalling a group whose members cannot be listed reaches the leader alone", async (t) => {
  const area = scratch(t);
  const group = startGroup(area, "idle");
  const info = await reported(area);
  assert.equal(signalGroup(group.pid, "SIGTERM", null), null);
  assert.equal((await group.exited).signal, "SIGTERM");
  assert.ok(alive(info.sleep), "what the leader started was not signaled");
});

test("the command's exit code is passed on, a signal that ended it is 128 plus its number", async (t) => {
  const area = scratch(t);
  const options = { onGroup: (group) => area.track(group.pid) };
  assert.deepEqual(await runInGroup(process.execPath, ["-e", "process.exit(3)"], options), { code: 3, signaled: null, stalled: false });
  assert.equal((await runInGroup(process.execPath, ["-e", ""], options)).code, 0);
  assert.equal((await run(t, area, "suicide").result).code, 128 + 9);
});

test("a command that cannot start fails with code 1 and says so", async (t) => {
  const errors = [];
  t.mock.method(console, "error", (...lines) => errors.push(lines.join(" ")));
  let announced = false;
  const result = await runInGroup("octoboard-no-such-command", [], { name: "verify", onGroup: () => (announced = true) });
  assert.equal(result.code, 1);
  assert.equal(result.stalled, false);
  assert.equal(announced, false, "there is no group to announce");
  assert.match(errors.join("\n"), /verify: could not run octoboard-no-such-command/);
});

test("the group is announced once, with the leader's pid and start time, as a group of its own", async (t) => {
  const area = scratch(t);
  const groups = [];
  const { result } = run(t, area, "idle", { onGroup: (group) => (area.track(group.pid), groups.push(group)), startTime: (pid) => `started ${pid}`, stallMs: STALL_MS });
  const info = await reported(area);
  assert.equal(groups.length, 1);
  assert.deepEqual(groups[0], { pid: info.pid, started: `started ${info.pid}` });
  const members = groupMembers(info.pid);
  assert.ok(members.some((member) => member.pid === info.pid), "the leader is in the group");
  assert.ok(members.some((member) => member.pid === info.sleep), "what the leader started joined its group");
  assert.notEqual(listProcesses().find((entry) => entry.pid === process.pid).pgid, info.pid);
  process.kill(info.pid, "SIGKILL");
  assert.equal((await result).code, 128 + 9);
  await gone(info.sleep, "the process the killed leader left");
});

test("the real start time of the leader is announced by default", async (t) => {
  const area = scratch(t);
  const groups = [];
  const { result } = run(t, area, "idle", { onGroup: (group) => (area.track(group.pid), groups.push(group)), stallMs: STALL_MS });
  const info = await reported(area);
  assert.equal(groups[0].started, startOf(info.pid));
  assert.notEqual(groups[0].started, "");
  process.kill(info.pid, "SIGKILL");
  await result;
});

test("a command that makes no progress is reported and ended with its group, sparing the shared server and what it runs", async (t) => {
  const area = scratch(t);
  const { result, errors } = run(t, area, "idle", { fixture: { sccache: "server" }, stallMs: 1000, label: "rust tests", name: "heavy" });
  const outcome = await result;
  assert.deepEqual(outcome, { code: STALLED_EXIT, signaled: null, stalled: true });
  const info = await reported(area);
  await gone(info.pid, "the stalled command");
  await gone(info.sleep, "the process it started");
  const text = errors.join("\n");
  assert.match(text, /heavy: rust tests made no progress for 1s/);
  assert.match(text, new RegExp(`pid ${info.pid}: `));
  assert.match(text, new RegExp(`pid ${info.sleep}: sleep`));
  assert.match(text, /was ended for making no progress \(exit 124\)/);
  for (const pid of [info.sccache, info.sccacheChild]) assert.doesNotMatch(text, new RegExp(`pid ${pid}: `), "the shared server and its child are not listed as the command's processes");
  await assertSharedServerSpared(area, assert);
});

test("a command that only starts short processes, using no CPU, is ended", async (t) => {
  const area = scratch(t);
  const { result } = run(t, area, "churn", { stallMs: 1000 });
  assert.deepEqual(await result, { code: STALLED_EXIT, signaled: null, stalled: true });
  const info = await reported(area);
  await gone(info.pid, "the command");
});

test("what the command leaves running in its group is ended once it has closed, sparing the shared server and what it runs", async (t) => {
  const area = scratch(t);
  const { result } = run(t, area, "orphan", { fixture: { sccache: "server", stubborn: true }, stallMs: STALL_MS });
  assert.deepEqual(await result, { code: 0, signaled: null, stalled: false });
  const info = await reported(area);
  // The runner has ended the orphans by the time it answers, one of which ignores SIGTERM.
  await gone(info.sleep, "the orphaned sleep");
  await gone(info.stubborn, "the orphan that ignores SIGTERM");
  await assertSharedServerSpared(area, assert);
});

test("an `sccache` that is the command's own child, with the compiler it runs, is ended with the group", async (t) => {
  const area = scratch(t);
  // Ended while the command runs, as a stall does.
  const { result } = run(t, area, "idle", { fixture: { sccache: "client" }, stallMs: 1000 });
  assert.deepEqual(await result, { code: STALLED_EXIT, signaled: null, stalled: true });
  const info = await reported(area);
  await gone(info.sccache, "the client");
  await gone(info.sccacheChild, "the compiler the client ran");
});

test("the leftovers of a group whose leader is gone are ended", async (t) => {
  const area = scratch(t);
  const group = startGroup(area, "orphan");
  const started = startOf(group.pid);
  await group.exited;
  const info = await reported(area);
  assert.ok(alive(info.sleep));
  assert.ok(endLeftovers({ pid: group.pid, started }) >= 1);
  await gone(info.sleep, "the orphaned sleep");
});

test("leftovers are ended with their leader still running when its start time matches", async (t) => {
  const area = scratch(t);
  const group = startGroup(area, "idle");
  const info = await reported(area);
  assert.ok(endLeftovers({ pid: group.pid, started: startOf(group.pid) }) >= 2);
  assert.equal((await group.exited).signal, "SIGKILL");
  await gone(info.sleep, "the process the leader started");
});

test("a client adopted once its parent has ended is still ended, by SIGKILL after the grace", async (t) => {
  const area = scratch(t);
  // The leader stands in for cargo: it ends on SIGTERM, and the client it started, which ignores SIGTERM, is then
  // adopted and no longer looks like a client.
  const group = startGroup(area, "idle", { sccache: "client", sccacheTrap: true });
  const info = await reported(area);
  await endGroup(group.pid, 500);
  assert.equal((await group.exited).signal, "SIGTERM");
  await gone(info.sccache, "the client that ignores SIGTERM");
  await gone(info.sccacheChild, "the compiler it ran");
  await gone(info.sleep, "the process the leader started");
});

test("a process a shared server starts while the group is being ended is spared, through the SIGKILL phase", async (t) => {
  const area = scratch(t);
  const group = startGroup(area, "idle", { sccache: "server", stubborn: true, sccacheLate: true });
  const info = await reported(area);
  const ending = endGroup(group.pid, 3000);
  // The leader is gone once it has been signaled, which is after the sweep listed the processes.
  await group.exited;
  writeFileSync(path.join(area.dir, "spawn-late"), "");
  const late = Number(await until(() => readFileOrNull(path.join(area.dir, "late.pid")), "the server to start a process"));
  await ending;
  await gone(info.stubborn, "the member that ignores SIGTERM");
  assert.ok(alive(late), "what the server started meanwhile is left running");
  assert.ok(alive(info.sccache), "and so is the server");
});

test("nothing is ended when a live process has the leader's pid with another start time", async (t) => {
  const area = scratch(t);
  const group = startGroup(area, "idle");
  const info = await reported(area);
  assert.equal(endLeftovers({ pid: group.pid, started: "Thu Jan  1 00:00:00 1970" }), 0);
  // It was sent no SIGKILL: the first signal it gets is this SIGTERM, which a SIGKILL sent earlier would have preceded.
  process.kill(group.pid, "SIGTERM");
  assert.equal((await group.exited).signal, "SIGTERM");
  assert.ok(alive(info.sleep), "what it started is untouched");
});

test("nothing is ended, and the caller is told, when the processes cannot be listed", async (t) => {
  const area = scratch(t);
  const group = startGroup(area, "idle");
  await reported(area);
  const started = startOf(group.pid);
  assert.equal(withoutPs(() => endLeftovers({ pid: group.pid, started })), null);
  process.kill(group.pid, "SIGTERM");
  assert.equal((await group.exited).signal, "SIGTERM");
});

test("no recorded group means nothing to end", () => {
  assert.equal(endLeftovers(undefined), 0);
  assert.equal(endLeftovers(null), 0);
  assert.equal(endLeftovers({}), 0);
});
