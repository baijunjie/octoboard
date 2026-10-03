// Shared daemon lifecycle + WebSocket helpers for the T2/T3 bench scripts.
//
// Every script here starts its own obd-proto instance (release build) rather than reusing one
// the user left running, so a bench run is reproducible and self-contained. See
// fixtures/bin/codex for why the session is launched as agent "codex" with SHELL=/bin/sh: the
// real protocol has no "plain shell" session type, and that is the narrowest way to get one
// without touching prototype/daemon/ or any real agent config.

import { spawn, execFile } from "node:child_process";
import { mkdtempSync, existsSync, realpathSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import WebSocket from "ws";

const __dirname = dirname(fileURLToPath(import.meta.url));
const DAEMON_BIN = join(__dirname, "..", "daemon", "target", "release", "obd-proto");
const SHIM_DIR = join(__dirname, "fixtures", "bin");
const SESSION_ENV = { ...process.env, SHELL: "/bin/sh", PATH: `${SHIM_DIR}:${process.env.PATH}` };

export function assert(cond, message) {
  if (!cond) {
    console.error(`FAIL  ${message}`);
    process.exitCode = 1;
    throw new Error(message);
  }
}

/**
 * Confirms the bench's `codex` shim (fixtures/bin/codex) actually wins the PATH lookup under
 * the exact shell the daemon will launch sessions with, instead of trusting that as a property
 * of this one machine's dotfiles. A real `codex`/`claude`/`grok` earlier on some other machine's
 * PATH would otherwise get launched against a user's real agent config — the one thing this
 * prototype must never do.
 */
async function assertShimWinsPathLookup() {
  const resolved = await new Promise((resolve, reject) => {
    execFile("/bin/sh", ["-l", "-i", "-c", "command -v codex"], { env: SESSION_ENV }, (err, stdout) => {
      if (err) reject(new Error(`could not resolve \`codex\` on PATH at all: ${err.message}`));
      else resolve(stdout.trim());
    });
  });
  assert(resolved.length > 0, "`codex` did not resolve to anything on PATH");
  const resolvedDir = dirname(realpathSync(resolved));
  assert(
    resolvedDir === realpathSync(SHIM_DIR),
    `\`codex\` resolved to ${resolved}, outside fixtures/bin (${SHIM_DIR}) — some other codex is ` +
      "earlier on this machine's PATH and would be launched against the user's real ~/.codex; " +
      "refusing to run the bench rather than risk that",
  );
}

/**
 * Spawns a fresh obd-proto, waits for its port, and returns a handle with a `stop()`.
 * `extraArgs` is how output_throughput.mjs sweeps `--pty-read-buf-kib`; every other bench
 * script leaves it at the daemon's default.
 */
export async function startDaemon(extraArgs = []) {
  if (!existsSync(DAEMON_BIN)) {
    throw new Error(`daemon binary not found at ${DAEMON_BIN} — run \`cargo build --release\` in prototype/daemon first`);
  }
  await assertShimWinsPathLookup();

  const child = spawn(DAEMON_BIN, ["--parent-pid", String(process.pid), ...extraArgs], {
    env: SESSION_ENV,
    stdio: ["ignore", "pipe", "inherit"],
  });

  const port = await new Promise((resolve, reject) => {
    let buf = "";
    const timer = setTimeout(() => reject(new Error("timed out waiting for daemon to print its port")), 10_000);
    const onData = (chunk) => {
      buf += chunk.toString("utf8");
      const match = buf.match(/listening on 127\.0\.0\.1:(\d+)/);
      if (match) {
        child.stdout.off("data", onData);
        clearTimeout(timer);
        resolve(Number(match[1]));
      }
    };
    child.stdout.on("data", onData);
    child.once("exit", (code) => reject(new Error(`daemon exited early (code ${code}) before printing its port`)));
  });

  return {
    port,
    stop() {
      child.kill("SIGTERM");
    },
  };
}

export function scratchDir(prefix) {
  return mkdtempSync(join(tmpdir(), `obd-proto-bench-${prefix}-`));
}

function controlUrl(port) {
  return `ws://127.0.0.1:${port}/ws/control`;
}

function termUrl(port, session) {
  return `ws://127.0.0.1:${port}/ws/term/${session}`;
}

/**
 * Starts a bench-fixture session (agent "codex", see fixtures/bin/codex) and returns its
 * session id. `mode` is passed as the session's `task`, which is this fixture's own tiny
 * control protocol ("cat" | "counter"); see the fixture script for what each does.
 */
export async function startFixtureSession(port, mode) {
  const control = new WebSocket(controlUrl(port));
  await new Promise((resolve, reject) => {
    control.once("open", resolve);
    control.once("error", reject);
  });

  const started = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("timed out waiting for session_started")), 10_000);
    control.on("message", (data) => {
      const event = JSON.parse(data.toString("utf8"));
      if (event.type === "session_started") {
        clearTimeout(timer);
        resolve(event.session);
      }
      if (event.type === "error") {
        clearTimeout(timer);
        reject(new Error(event.message));
      }
    });
  });

  control.send(
    JSON.stringify({
      type: "start_session",
      agent: "codex",
      cwd: scratchDir("session-cwd"),
      role: "worker",
      task: mode,
    }),
  );

  const session = await started;
  return { session, control };
}

/** Opens `/ws/term/:session` and reports the terminal size immediately on open, per T1's finding
 * that a PTY left at its default size can make a real agent TUI exit — the bench fixture does
 * not care, but every term-socket client in this prototype follows the same discipline. */
export async function openTermSocket(port, session, { cols = 80, rows = 24 } = {}) {
  const socket = new WebSocket(termUrl(port, session));
  socket.binaryType = "nodebuffer";
  await new Promise((resolve, reject) => {
    socket.once("open", () => {
      socket.send(JSON.stringify({ type: "resize", cols, rows }));
      resolve();
    });
    socket.once("error", reject);
  });
  return socket;
}

/**
 * Waits for the fixture's "READY\n" line (see fixtures/bin/codex's "cat" mode) before letting a
 * caller send real payload. Needed only for that mode: sending unbroken binary data (no
 * newline) can arrive while the PTY is still in the daemon's default canonical mode, where the
 * line discipline holds it forever waiting for a line terminator that will never come — once
 * `stty raw` actually takes effect that can't happen, and READY is the signal that it has.
 * Discards everything up to and including the frame containing "READY\n" — any payload bytes
 * that happened to share that frame are gone too — so it must run before the caller attaches
 * its own "message" listener for the real payload.
 */
export async function waitForReady(socket, timeoutMs = 5000) {
  await new Promise((resolve, reject) => {
    let buffered = Buffer.alloc(0);
    const timer = setTimeout(() => reject(new Error("timed out waiting for fixture READY")), timeoutMs);
    const onMessage = (data) => {
      buffered = Buffer.concat([buffered, data]);
      if (buffered.includes("READY\n")) {
        socket.off("message", onMessage);
        clearTimeout(timer);
        resolve();
      }
    };
    socket.on("message", onMessage);
  });
}

export function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export function percentile(sortedValues, p) {
  const idx = Math.min(sortedValues.length - 1, Math.floor((p / 100) * sortedValues.length));
  return sortedValues[idx];
}
