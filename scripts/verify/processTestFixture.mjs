// The stand-in command the process-group tests run (`processTestSupport.mjs` starts it): a program that
// starts the helper processes a hung or leaky test run would have, writes their pids to a file once every one
// is ready, and then behaves as `mode` says. Run as `node processTestFixture.mjs '<json options>'`.
//
// Modes: `idle` waits forever (a deadlocked test binary), `orphan` exits at once leaving its helpers running
// (a `cargo` that ended before its test binary), `churn` starts short processes one after another and never
// finishes (a hung test run that still spawns), `suicide` kills itself with SIGKILL.
// Helper roles, started by the modes above: `stubborn` ignores SIGTERM, SIGINT and SIGHUP; `logger` appends the
// name of each of them it receives to a file (the stand-in for a shared server that must not be signaled), and
// can start a logger of its own (what it runs); `starter` runs a `logger` under the name `sccache` and exits.
//
// The `sccache` option is the shape the fake `sccache` has: `"client"` is started by the fixture itself and
// stays its child, as cargo's per-compile client is, and ends on SIGTERM unless `sccacheTrap` is set; `"server"` is
// started by a `starter` that exits once it is up, so that it is adopted by whatever the machine has for orphans
// (pid 1 or a subreaper), as the server a client starts is; with `sccacheLate` the server starts one more process
// when `spawn-late` appears in `dir`, writing its pid to `late.pid`.
//
// None of it outlives a test that was killed: every process ends by itself after `LIFETIME_MS`, and with
// `watchParent` the leader also ends once its parent is gone. These processes ignore signals or live in
// sessions of their own, so nothing else would end them.
import { spawn, spawnSync } from "node:child_process";
import { appendFileSync, existsSync, readFileSync, renameSync, symlinkSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { LIFETIME_MS, PATIENCE_MS } from "./processTestSupport.mjs";

const options = JSON.parse(process.argv[2]);
const SIGNALS = ["SIGTERM", "SIGINT", "SIGHUP"];
const parent = process.ppid;
const stayAlive = () => {
  setTimeout(() => process.exit(0), LIFETIME_MS);
  if (options.watchParent) setInterval(() => process.ppid !== parent && process.exit(0), 500);
  setInterval(() => {}, 1_000_000);
};
const pause = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);

// Writes a file in one step, so a reader never sees a prefix.
function publish(file, text) {
  writeFileSync(`${file}.partial`, text);
  renameSync(`${file}.partial`, file);
}

if (options.role === "stubborn") {
  for (const signal of SIGNALS) process.on(signal, () => {});
  publish(options.ready, JSON.stringify({ pid: process.pid }));
  stayAlive();
} else if (options.role === "logger") {
  // A logger that does not trap (`trap: false`) ends on SIGTERM, as a client or a compiler would.
  if (options.trap !== false) for (const signal of SIGNALS) process.on(signal, () => appendFileSync(options.log, `${signal}\n`));
  const ready = { pid: process.pid };
  if (options.child) ready.child = startHelper(process.execPath, "logger", { log: options.childLog, trap: options.trap }).pid;
  // A logger given `trigger` starts one more process once that file exists, and records its pid in `late`: what
  // a shared server does for a build that asks it for a compile while a group is being ended.
  if (options.trigger) {
    const watch = setInterval(() => {
      if (!existsSync(options.trigger)) return;
      clearInterval(watch);
      publish(options.late, String(spawn("sleep", [String(LIFETIME_MS / 1000)], { stdio: "ignore" }).pid));
    }, 20);
  }
  publish(options.ready, JSON.stringify(ready));
  stayAlive();
} else if (options.role === "starter") {
  startSccache(undefined, options.late);
  process.exit(0);
} else {
  runMode();
}

// Starts this file again in `role`, in this process's group, and waits until it reports ready; answers what it
// reported. `command` names the executable, so a helper can run under another name.
function startHelper(command, role, extra) {
  const ready = path.join(options.dir, `${path.basename(extra?.log ?? role)}.ready`);
  spawn(command, [fileURLToPath(import.meta.url), JSON.stringify({ role, ready, dir: options.dir, ...extra })], { stdio: "ignore" });
  for (let waited = 0; !existsSync(ready); waited += 20) {
    if (waited > PATIENCE_MS) process.exit(3);
    pause(20);
  }
  return JSON.parse(readFileSync(ready, "utf8"));
}

// Starts the node under the name `sccache` (and its own child) and waits until it is ready.
function startSccache(trap, late) {
  const link = path.join(options.dir, "sccache");
  if (!existsSync(link)) symlinkSync(process.execPath, link);
  return startHelper(link, "logger", { log: path.join(options.dir, "sccache.log"), child: true, childLog: path.join(options.dir, "sccache-child.log"), trap, ...late });
}

// Whether `pid` has a parent other than `parentPid` by now, which is so once that parent has exited.
function reparented(pid, parentPid) {
  const ppid = spawnSync("ps", ["-o", "ppid=", "-p", String(pid)], { encoding: "utf8" }).stdout.trim();
  return ppid !== "" && Number(ppid) !== parentPid;
}

// Starts the fake `sccache` in the shape `options.sccache` names; answers `{ pid, child }`.
function startFakeSccache() {
  if (options.sccache === "client") return startSccache(Boolean(options.sccacheTrap));
  const late = options.sccacheLate ? { trigger: path.join(options.dir, "spawn-late"), late: path.join(options.dir, "late.pid") } : {};
  const starter = spawn(process.execPath, [fileURLToPath(import.meta.url), JSON.stringify({ role: "starter", dir: options.dir, late })], { stdio: "ignore" });
  const ready = path.join(options.dir, "sccache.log.ready");
  for (let waited = 0; !existsSync(ready); waited += 20) {
    if (waited > PATIENCE_MS) process.exit(3);
    pause(20);
  }
  const server = JSON.parse(readFileSync(ready, "utf8"));
  for (let waited = 0; !reparented(server.pid, starter.pid); waited += 20) {
    if (waited > PATIENCE_MS) process.exit(3);
    pause(20);
  }
  return server;
}

function runMode() {
  const pids = { pid: process.pid };
  if (options.mode === "idle" || options.mode === "orphan") {
    pids.sleep = spawn("sleep", [String(LIFETIME_MS / 1000)], { stdio: "ignore" }).pid;
    if (options.stubborn) pids.stubborn = startHelper(process.execPath, "stubborn").pid;
    if (options.sccache) {
      const server = startFakeSccache();
      pids.sccache = server.pid;
      pids.sccacheChild = server.child;
    }
    publish(options.info, JSON.stringify(pids));
    if (options.mode === "idle") stayAlive();
    else process.exit(0);
  } else if (options.mode === "churn") {
    publish(options.info, JSON.stringify(pids));
    const until = Date.now() + LIFETIME_MS;
    while (Date.now() < until && (!options.watchParent || process.ppid === parent)) spawnSync("sleep", ["0.2"]);
    process.exit(0);
  } else if (options.mode === "suicide") {
    process.kill(process.pid, "SIGKILL");
  }
}
