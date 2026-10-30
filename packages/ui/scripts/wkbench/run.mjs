// Measures the UI's latency in the system WKWebView, the engine the desktop app runs in, against a
// throwaway daemon with stand-in agents, and prints a table. One command does everything and stops
// only what it started: it builds the production bundle, compiles host.swift, starts the daemon on a
// scratch HOME and TMPDIR, seeds the fixtures, serves the bundle, runs the scenarios and cleans up, on
// failure and on Ctrl-C too. See README.md in this directory.
//
//   pnpm bench:wkwebview [options]
//
// Needs macOS with `swiftc`, Rust (for the daemon, unless --daemon is given) and Python 3 (the stand-in).

import { execFileSync, spawn } from "node:child_process";
import { createRequire } from "node:module";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { seedFixtures } from "./fixtures.mjs";
import { profileSteps } from "./profile.mjs";
import { runScript } from "./playwright.mjs";
import { stallWatch } from "./stall.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const packageDir = path.resolve(HERE, "../..");
const repoRoot = path.resolve(packageDir, "../..");

const SCENARIOS = ["switch", "scroll", "restyle", "idle"];
const DEFAULT_SCENARIOS = ["switch", "scroll", "restyle"];
const SCRATCH_MARKER = ".wkbench-scratch";
// What one scenario may take before the run gives up on it: a fixed part and one for each --reps.
const SCENARIO_BASE_S = 150;
const SCENARIO_PER_REP_S = 30;
const EXIT_SIGNALS = { SIGINT: 2, SIGTERM: 15, SIGHUP: 1 };

const USAGE = `Usage: pnpm bench:wkwebview [options]

Builds the UI, starts a throwaway daemon with stand-in agents, runs the scenarios in the system WKWebView
and prints a table. Stops everything it started when it ends. A window opens and takes focus while it runs.

  --scenarios <list>    Comma-separated, from ${SCENARIOS.join(", ")}, or "none" (default: ${DEFAULT_SCENARIOS.join(",")}).
                        switch also covers the session attach; idle needs --extra-projects of at least 1.
  --reps <n>            Repetitions of the switch round (default: 3). The table shows the median. Each
                        scenario is limited to ${SCENARIO_BASE_S} s plus ${SCENARIO_PER_REP_S} s per repetition.
  --scratch <dir>       Where everything the run makes goes: the throwaway HOME and TMPDIR, the daemon
                        build, the bundles, the host binary (default: packages/ui/.bench). Created if
                        missing; an existing one must be a previous scratch directory of this tool, as its
                        home/ and tmp/ are deleted at the start of a run.
  --daemon <path>       An octoboardd binary to use instead of building one into the scratch directory.
  --port <n>            Use the daemon already listening on this port, which must hold the fixtures,
                        instead of starting and seeding one. The only way to measure a daemon this tool did
                        not start; it is not stopped, but its sessions are driven and, under idle, renamed
                        and restored. Needs --key.
  --key <key>           The key that daemon printed on its startup line, which it requires of every
                        connection. Only with --port.
  --skip-build          Reuse the bundle and the host binary a previous run left in the scratch directory.
  --extra-projects <n>  Projects per console besides the driven one, with sessions that stop drawing a few
                        seconds after they start, to give the sidebar a realistic size (default: 5).
  --extra-sessions <n>  Sessions in each of them (default: 3).
  --engine <name>       wkwebview (default), webkit or chromium: the Playwright engines run the same
                        scripts; read their numbers as ratios only (see the README).
  --chromium <path>     The Chrome executable for chromium and --profile (also OCTOBOARD_CHROMIUM).
  --throttle <n>        CPU throttling rate for --engine chromium.
  --probe               Attribute forced style and layout to the code that read it (builds an unminified
                        bundle); lists the top readers per step under the table.
  --profile <step>      After the scenarios, a Chrome CPU profile of "avatar" or "attach" steps by source
                        file and component (builds an unminified bundle with source maps).
  --json <file>         Also write every result line the scenarios posted to this file.
  -h, --help            This text.

Exit status: 0 on success, 1 when a scenario reported an error or the run failed, 2 on a usage error.`;

class UsageError extends Error {}

function parse() {
  let parsed;
  try {
    parsed = parseArgs({
      options: {
        scenarios: { type: "string" }, reps: { type: "string" }, scratch: { type: "string" }, daemon: { type: "string" },
        port: { type: "string" }, key: { type: "string" }, "skip-build": { type: "boolean" }, "extra-projects": { type: "string" },
        "extra-sessions": { type: "string" }, engine: { type: "string" }, chromium: { type: "string" },
        throttle: { type: "string" }, probe: { type: "boolean" }, profile: { type: "string" }, json: { type: "string" },
        help: { type: "boolean", short: "h" },
      },
    });
  } catch (error) {
    throw new UsageError(`${error.message}\n\n${USAGE}`);
  }
  const v = parsed.values;
  if (v.help) {
    console.log(USAGE);
    process.exit(0);
  }
  const number = (name, value, fallback, min) => {
    if (value === undefined) return fallback;
    const n = Number(value);
    if (!Number.isInteger(n) || n < min) throw new UsageError(`--${name} is a whole number of at least ${min}, not "${value}"`);
    return n;
  };
  const scenarios = v.scenarios === undefined ? DEFAULT_SCENARIOS : v.scenarios === "none" ? [] : v.scenarios.split(",").map((s) => s.trim());
  for (const s of scenarios) if (!SCENARIOS.includes(s)) throw new UsageError(`unknown scenario "${s}"; known: ${SCENARIOS.join(", ")}`);
  if (v.daemon !== undefined) {
    try { fs.accessSync(v.daemon, fs.constants.X_OK); } catch { throw new UsageError(`--daemon "${v.daemon}" is not an executable file`); }
    if (!fs.statSync(v.daemon).isFile()) throw new UsageError(`--daemon "${v.daemon}" is not an executable file`);
  }
  const engine = v.engine ?? "wkwebview";
  if (!["wkwebview", "webkit", "chromium"].includes(engine)) throw new UsageError(`--engine is wkwebview, webkit or chromium, not "${engine}"`);
  if (v.profile !== undefined && !["avatar", "attach"].includes(v.profile)) throw new UsageError(`--profile is avatar or attach, not "${v.profile}"`);
  if ((v.port === undefined) !== (v.key === undefined)) throw new UsageError("--port and --key go together: the daemon refuses a connection without its key");
  const extraProjects = number("extra-projects", v["extra-projects"], 5, 0);
  if (scenarios.includes("idle") && extraProjects < 1) throw new UsageError("the idle scenario renames the extra projects' sessions: --extra-projects must be at least 1");
  return {
    scenarios, engine, extraProjects, profile: v.profile, probe: v.probe ?? false, skipBuild: v["skip-build"] ?? false,
    reps: number("reps", v.reps, 3, 1), extraSessions: number("extra-sessions", v["extra-sessions"], 3, 1),
    scratch: path.resolve(v.scratch ?? path.join(packageDir, ".bench")),
    daemon: v.daemon && path.resolve(v.daemon), port: v.port === undefined ? undefined : number("port", v.port, undefined, 1), key: v.key,
    chromium: v.chromium, throttle: v.throttle, json: v.json && path.resolve(v.json),
  };
}

// ---- what the run started, and stopping it ----

/** Every child the run started, stopped in reverse order by `cleanup`. Only these PIDs are ever signalled. */
const started = [];
const closers = [];
let stopping = false; // set once a signal or an exception starts the shutdown: nothing new is started

/** Spawns a child that `cleanup` will stop. `child.failed` rejects, with the cause, if it cannot be started. */
function start(command, args, options) {
  if (stopping) throw new Error("the run is being stopped");
  const child = spawn(command, args, options);
  started.push(child);
  child.failed = new Promise((_, reject) => child.once("error", (e) => reject(new Error(`cannot run ${command}: ${e.message}`))));
  child.failed.catch(() => {}); // a caller that does not race on it must not crash the run
  return child;
}

function waitExit(child, ms) {
  return new Promise((resolve) => {
    if (child.exitCode !== null || child.signalCode !== null) return resolve(true);
    const timer = setTimeout(() => resolve(false), ms);
    child.once("exit", () => { clearTimeout(timer); resolve(true); });
  });
}

async function stopChild(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill("SIGTERM");
  // A process can take seconds to exit on a machine whose endpoint security stalls process work.
  if (!(await waitExit(child, 8000))) {
    child.kill("SIGKILL");
    await waitExit(child, 3000);
  }
}

let cleaning;
function cleanup() {
  cleaning ??= (async () => {
    for (const close of closers.reverse()) await close().catch(() => {});
    for (const child of started.reverse()) await stopChild(child);
  })();
  return cleaning;
}

/** Runs a command to completion with its output on `logFile`; rejects with the log's tail on failure. */
function runLogged(command, args, { cwd, env, logFile }) {
  return new Promise((resolve, reject) => {
    const fd = fs.openSync(logFile, "w");
    const child = start(command, args, { cwd, env, stdio: ["ignore", fd, fd] });
    child.failed.catch((e) => { fs.closeSync(fd); reject(e); });
    child.once("exit", (code) => {
      fs.closeSync(fd);
      if (code === 0) return resolve();
      const tail = fs.readFileSync(logFile, "utf8").split("\n").slice(-25).join("\n");
      reject(new Error(`${command} ${args.join(" ")} failed (exit ${code}); log ${logFile}:\n${tail}`));
    });
  });
}

// ---- preparing the scratch directory ----

function prepareScratch(scratch, withHome) {
  if (fs.existsSync(scratch)) {
    if (!fs.statSync(scratch).isDirectory()) throw new Error(`${scratch} is not a directory`);
    if (!fs.existsSync(path.join(scratch, SCRATCH_MARKER)) && fs.readdirSync(scratch).length > 0) {
      throw new Error(`${scratch} is not empty and not a previous scratch directory of this tool; it will not be touched`);
    }
  }
  fs.mkdirSync(scratch, { recursive: true });
  fs.writeFileSync(path.join(scratch, SCRATCH_MARKER), "Scratch directory of pnpm bench:wkwebview; home/ and tmp/ are recreated on every run.\n");
  const dirs = { home: path.join(scratch, "home"), tmp: path.join(scratch, "tmp") };
  // Under --port the daemon is someone else's and may run on this very `home/`: leave it alone.
  if (!withHome) return dirs;
  // The daemon leaves read-only directories (its git hooks) that `rm` cannot enter until they are writable.
  for (const dir of Object.values(dirs)) {
    if (fs.existsSync(dir)) execFileSync("chmod", ["-R", "u+rwx", dir]);
    fs.rmSync(dir, { recursive: true, force: true });
  }
  for (const dir of Object.values(dirs)) fs.mkdirSync(dir, { recursive: true });
  // The daemon launches agents through the user's shell; the throwaway HOME's .zshrc puts the stand-in
  // first on PATH under the name `claude`.
  const fakebin = path.join(dirs.home, "fakebin");
  fs.mkdirSync(fakebin);
  fs.copyFileSync(path.join(HERE, "standin-claude.py"), path.join(fakebin, "claude"));
  fs.chmodSync(path.join(fakebin, "claude"), 0o755);
  fs.writeFileSync(path.join(dirs.home, ".zshrc"), `export PATH=${JSON.stringify(fakebin)}:$PATH\n`);
  fs.mkdirSync(path.join(dirs.home, "code"));
  return dirs;
}

// ---- building ----

const withCargoOnPath = (env) => ({ ...env, PATH: `${path.join(os.homedir(), ".cargo", "bin")}:${env.PATH}` });

async function buildDaemon(scratch) {
  const target = path.join(scratch, "target");
  console.error("building octoboardd (release) ...");
  await runLogged("cargo", ["build", "--release", "-p", "octoboardd"], {
    cwd: repoRoot, env: withCargoOnPath({ ...process.env, CARGO_TARGET_DIR: target }), logFile: path.join(scratch, "cargo-build.log"),
  });
  return path.join(target, "release", "octoboardd");
}

async function buildBundle(scratch, name, extraArgs = []) {
  const outDir = path.join(scratch, name);
  console.error(`building the ${name} bundle ...`);
  // Vite's own entry run by node, not through `pnpm exec`: stopping the child then stops the build.
  const vite = path.join(path.dirname(createRequire(path.join(packageDir, "package.json")).resolve("vite/package.json")), "bin", "vite.js");
  await runLogged(process.execPath, [vite, "build", "--outDir", outDir, "--emptyOutDir", ...extraArgs], {
    cwd: packageDir, env: process.env, logFile: path.join(scratch, `vite-build-${name}.log`),
  });
  return outDir;
}

async function buildHost(scratch) {
  const binary = path.join(scratch, "wkbench-host");
  const source = path.join(HERE, "host.swift");
  if (fs.existsSync(binary) && fs.statSync(binary).mtimeMs > fs.statSync(source).mtimeMs) return binary;
  console.error("compiling host.swift ...");
  await runLogged("swiftc", ["-O", source, "-o", binary], { cwd: scratch, env: process.env, logFile: path.join(scratch, "swiftc.log") });
  return binary;
}

// ---- the daemon, the static server and the host ----

async function startDaemon(binary, dirs, scratch) {
  // The daemon reads its environment from here only, so nothing of the developer's own session (agent
  // markers, credentials, an installed app's settings) reaches it.
  const env = {
    HOME: dirs.home, TMPDIR: `${dirs.tmp}/`, USER: os.userInfo().username, SHELL: "/bin/zsh", LANG: "en_US.UTF-8",
    PATH: "/usr/bin:/bin:/usr/sbin:/sbin",
  };
  const stdoutPath = path.join(scratch, "daemon.stdout");
  const out = fs.openSync(stdoutPath, "w");
  const err = fs.openSync(path.join(scratch, "daemon.stderr"), "w");
  const child = start(binary, [], { env, stdio: ["ignore", out, err] });
  fs.closeSync(out);
  fs.closeSync(err);
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    const match = /listening on 127\.0\.0\.1:(\d+) with key ([0-9a-f]+)/.exec(fs.readFileSync(stdoutPath, "utf8"));
    if (match) return { port: Number(match[1]), key: match[2] };
    if (child.exitCode !== null || child.signalCode !== null) throw new Error(`the daemon exited (${child.exitCode ?? child.signalCode}); see ${path.join(scratch, "daemon.stderr")}`);
    await Promise.race([new Promise((resolve) => setTimeout(resolve, 100)), child.failed]);
  }
  throw new Error("the daemon did not report its port and key within 60 s");
}

const MIME = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".css": "text/css", ".json": "application/json", ".svg": "image/svg+xml", ".png": "image/png", ".woff2": "font/woff2", ".woff": "font/woff", ".wasm": "application/wasm", ".map": "application/json" };

/** Serves `dir` statically on a free port of 127.0.0.1; closed by `cleanup`. */
async function serve(dir) {
  const server = http.createServer((req, res) => {
    try {
      const pathname = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
      const file = path.join(dir, pathname === "/" ? "index.html" : pathname);
      if (!file.startsWith(dir + path.sep) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
        res.writeHead(404).end();
        return;
      }
      res.writeHead(200, { "content-type": MIME[path.extname(file)] ?? "application/octet-stream" });
      fs.createReadStream(file).on("error", () => res.destroy()).pipe(res);
    } catch {
      // A request this server cannot make sense of is the requester's problem, not the run's.
      if (res.headersSent) res.destroy();
      else res.writeHead(400).end();
    }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  closers.push(() => new Promise((resolve) => { server.close(resolve); server.closeAllConnections(); }));
  return server.address().port;
}

/** Runs the host (or a Playwright engine) on `url`; returns the JSON lines the driver posted, in order. */
async function runScenario({ options, hostBinary, url, script, scenario }) {
  const timeoutS = SCENARIO_BASE_S + SCENARIO_PER_REP_S * options.reps;
  if (options.engine !== "wkwebview") {
    const lines = await runScript({ engine: options.engine, url, script, chromium: options.chromium, throttle: options.throttle, timeoutMs: timeoutS * 1000 });
    return lines.map((l) => JSON.parse(l));
  }
  // The host ends itself on the driver's last line or after `timeoutS`.
  const child = start(hostBinary, [url, path.join(HERE, "instrument.js"), path.join(HERE, script), String(timeoutS)], { stdio: ["ignore", "pipe", "inherit"] });
  let buffer = "";
  const results = [];
  // A page that posts nothing but heartbeats for this long is stalled (a window nobody sees gets no frames).
  const watch = stallWatch(scenario, (message) => {
    console.error(message);
    child.kill("SIGTERM");
  });
  child.stdout.on("data", (chunk) => {
    buffer += chunk;
    const lines = buffer.split("\n");
    buffer = lines.pop();
    for (const line of lines) {
      if (watch.seen(line)) continue;
      try { results.push(JSON.parse(line)); } catch { console.error(`host: ${line}`); }
    }
  });
  const closed = new Promise((resolve) => child.stdout.once("close", resolve));
  try {
    // The backstop, should the host not end itself.
    await Promise.race([(async () => {
      if (!(await waitExit(child, (timeoutS + 60) * 1000))) await stopChild(child);
      await closed;
    })(), child.failed]);
  } finally {
    watch.stop();
  }
  return results;
}

// ---- the table ----

const median = (xs) => {
  const s = xs.filter((x) => typeof x === "number").sort((a, b) => a - b);
  if (!s.length) return null;
  const mid = Math.floor(s.length / 2);
  return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
};
const cell = (v) => (v === null || v === undefined ? "-" : typeof v === "number" ? String(+v.toFixed(1)) : String(v));

function printTable(title, header, rows) {
  const widths = header.map((h, i) => Math.max(h.length, ...rows.map((r) => cell(r[i]).length)));
  const line = (r) => r.map((c, i) => (i === 0 ? cell(c).padEnd(widths[i]) : cell(c).padStart(widths[i]))).join("  ");
  console.log(`\n${title}`);
  console.log(line(header));
  console.log(widths.map((w) => "-".repeat(w)).join("  "));
  for (const r of rows) console.log(line(r));
}

function byStep(results) {
  const groups = new Map();
  for (const r of results) if (r.step) groups.set(r.step, [...(groups.get(r.step) ?? []), r]);
  return groups;
}

const unreliable = (r) => r.attachTimedOut || r.idleTimedOut;

function printResults(scenario, results) {
  const errors = results.filter((r) => r.error);
  for (const e of errors) console.error(`${scenario}: ${e.error}`);
  const steps = byStep(results);
  if (scenario === "switch" || scenario === "scroll" || scenario === "idle") {
    const timed = [...steps].filter(([, rs]) => rs[0].pressJs !== undefined);
    if (timed.length) {
      printTable(`${scenario}: from the press to the main thread idle (ms; median of ${Math.max(...timed.map(([, rs]) => rs.length))} round(s))`,
        ["step", "toIdle", "min", "max", "press JS", "forced style+layout", "next frame", "commits", "worst frame gap", "gaps >50 ms"],
        timed.map(([name, rs]) => {
          const ok = rs.filter((r) => !unreliable(r)); // a round that timed out measures nothing
          const idle = ok.map((r) => r.toIdle);
          return [name + (ok.length < rs.length ? " (!)" : ""), median(idle), idle.length ? Math.min(...idle) : null, idle.length ? Math.max(...idle) : null, median(ok.map((r) => r.pressJs)), median(ok.map((r) => r.forcedStyleLayout)), median(ok.map((r) => r.toNextFrame)), median(ok.map((r) => r.commits)), median(ok.map((r) => r.gapMax)), median(ok.map((r) => r.longGaps.length))];
        }));
      if (timed.some(([, rs]) => rs.some((r) => r.attachTimedOut))) console.log("(!) the terminal did not change to the target session within 30 s; its toIdle is not an attach and is left out of the statistics.");
      if (timed.some(([, rs]) => rs.some((r) => r.idleTimedOut))) console.log("(!) the main thread was not idle within 20 s of the press; its toIdle is a cut-off and is left out of the statistics.");
    }
    const other = [...steps].filter(([, rs]) => rs[0].pressJs === undefined);
    if (other.length) {
      printTable(`${scenario}: frame pacing while the page is driven or left alone (ms)`,
        ["step", "frames", "gap p50", "gap p95", "gap max", "gaps >50 ms", "dropped ms", "commits"],
        other.map(([name, [r]]) => [name, r.frames, r.gapP50, r.gapP95, r.gapMax, r.longGaps.length, r.droppedMs, r.commits]));
    }
  } else if (scenario === "restyle") {
    const r = steps.get("restyle")?.[0];
    if (!r) return;
    printTable("restyle: one forced style recalculation and layout after a custom property and the padding change on the region (ms; median of 7)",
      ["region", "elements", "ms"], Object.keys(r.restyleLayoutMs).map((k) => [k, r.counts[k], r.restyleLayoutMs[k]]));
    const hidden = Object.keys(r.restyleLayoutMs).filter((k) => r.restyleLayoutMs[k] === null);
    if (hidden.length) console.log(`(-) not shown on the page, so not measured: ${hidden.join(", ")}`);
    console.log(`document without .xterm-screen: ${r.documentWithoutXtermScreenMs} ms; ${r.cssRules} CSS rules; devicePixelRatio ${r.dpr}`);
  }
}

function printForced(results) {
  const last = new Map();
  for (const r of results) if (r.forced) last.set(r.step, r.forced);
  console.log("\nforced style and layout by reader (last round of each step)");
  for (const [step, f] of last) {
    console.log(`${step}: ${f.forcedMs} ms in ${f.slowReads} reads of 1 ms or more`);
    for (const t of f.top.slice(0, 4)) console.log(`    ${t}`);
  }
}

// ---- main ----

async function main() {
  const options = parse();
  if (process.platform !== "darwin" && options.engine === "wkwebview") throw new Error("the system WKWebView exists on macOS only; --engine webkit or chromium runs elsewhere");
  const dirs = prepareScratch(options.scratch, options.port === undefined);

  const hostBinary = options.engine === "wkwebview" ? await buildHost(options.scratch) : undefined;
  const bundle = options.skipBuild && fs.existsSync(path.join(options.scratch, "dist")) ? path.join(options.scratch, "dist") : await buildBundle(options.scratch, "dist");
  const needsDebug = options.probe || options.profile;
  const debugBundle = needsDebug ? (options.skipBuild && fs.existsSync(path.join(options.scratch, "dist-debug")) ? path.join(options.scratch, "dist-debug") : await buildBundle(options.scratch, "dist-debug", ["--minify", "false", "--sourcemap"])) : undefined;

  let { port, key } = options;
  if (port === undefined) {
    const daemon = options.daemon ?? (await buildDaemon(options.scratch));
    ({ port, key } = await startDaemon(daemon, dirs, options.scratch));
    console.error(`throwaway daemon on port ${port}`);
    console.error("seeding the fixtures ...");
    await seedFixtures({ port, key, home: dirs.home, extraProjects: options.extraProjects, extraSessions: options.extraSessions, log: (m) => console.error(m) });
  } else {
    console.error(`using the daemon on port ${port} as given; it is not started, seeded or stopped`);
  }

  const uiPort = await serve(options.probe ? debugBundle : bundle);
  const all = [];
  let failed = false;
  for (const scenario of options.scenarios) {
    if (stopping) break;
    console.error(`running ${scenario} ...`);
    // The UI picks its language from `languages`, else the system's; the lookups need the English labels.
    const query = new URLSearchParams({ port: String(port), key, scenario, reps: String(options.reps), languages: "en" });
    if (options.probe) query.set("probe", "1");
    let pump;
    if (scenario === "idle") {
      pump = start(process.execPath, [path.join(HERE, "event-pump.mjs"), String(port), key, "Console 2", "33", "30"], { stdio: ["ignore", "ignore", "inherit"] });
    }
    const results = await runScenario({ options, hostBinary, scenario, url: `http://127.0.0.1:${uiPort}/?${query}`, script: scenario === "restyle" ? "domcost.js" : "driver.js" });
    if (pump) {
      await stopChild(pump); // SIGTERM makes it restore the titles it renamed, then exit
      if (pump.exitCode) failed = true; // a restore the daemon refused, which the pump has logged
    }
    if (results.some((r) => r.error) || !results.some((r) => r.done)) failed = true;
    if (!results.some((r) => r.done)) console.error(`${scenario}: the run ended without finishing`);
    all.push(...results.map((r) => ({ scenario, ...r })));
    printResults(scenario, results);
  }
  if (options.probe) printForced(all.filter((r) => r.scenario === "switch" || r.scenario === "scroll"));
  if (stopping) return 1;
  if (options.profile) {
    const profilePort = await serve(debugBundle);
    console.error(`profiling ${options.profile} steps in Chrome ...`);
    await profileSteps({ url: `http://127.0.0.1:${profilePort}/?port=${port}&key=${key}&languages=en`, dist: debugBundle, step: options.profile, throttle: options.throttle, chromium: options.chromium });
  }
  if (options.json) fs.writeFileSync(options.json, `${all.map((r) => JSON.stringify(r)).join("\n")}\n`);
  return failed ? 1 : 0;
}

for (const [signal, code] of Object.entries(EXIT_SIGNALS)) {
  process.on(signal, () => {
    stopping = true;
    cleanup().finally(() => process.exit(128 + code));
  });
}
for (const event of ["uncaughtException", "unhandledRejection"]) {
  process.on(event, (error) => {
    console.error(`bench:wkwebview: ${error?.message ?? error}`);
    stopping = true;
    cleanup().finally(() => process.exit(1));
  });
}
let exitCode;
try {
  exitCode = await main();
} catch (error) {
  console.error(error instanceof UsageError ? error.message : `bench:wkwebview: ${error.message}`);
  exitCode = error instanceof UsageError ? 2 : 1;
} finally {
  await cleanup();
}
process.exit(exitCode);
