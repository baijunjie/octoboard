// T2 latency: round-trip time from writing input on /ws/term/:session to the echo arriving
// back, against a plain `cat` session (fixtures/bin/codex, mode "cat") so this measures the
// daemon/WebSocket path only — not an agent TUI's own render latency, which needs the real app
// (prototype/ui + prototype/tauri) and is explicitly out of scope here.

import { startDaemon, startFixtureSession, openTermSocket, waitForReady, percentile, assert } from "./common.mjs";

const SAMPLES = 300;
const MARKER_TIMEOUT_MS = 5000;

async function main() {
  const daemon = await startDaemon();
  const { session, control } = await startFixtureSession(daemon.port, "cat");
  const term = await openTermSocket(daemon.port, session);
  await waitForReady(term);

  let buffered = Buffer.alloc(0);
  term.on("message", (data) => {
    buffered = Buffer.concat([buffered, data]);
  });

  const samples = [];
  try {
    for (let i = 0; i < SAMPLES; i++) {
      // A unique marker per round trip so a late byte from a previous round can never be
      // mistaken for this round's echo. The fixture session puts its PTY in raw mode
      // (fixtures/bin/codex), so what comes back is `cat`'s own write of exactly these bytes,
      // nothing added by the line discipline — the trailing newline is still stripped from the
      // search needle only so the same code would keep working if that ever changes.
      const markerText = `m${i.toString().padStart(6, "0")}`;
      const markerBuf = Buffer.from(markerText, "utf8");
      const start = process.hrtime.bigint();
      term.send(Buffer.from(`${markerText}\n`, "utf8"));
      await waitForMarker(markerBuf);
      const end = process.hrtime.bigint();
      samples.push(Number(end - start) / 1e6); // ms
    }
  } finally {
    // A rejected waitForMarker (the deadline above) must still stop the daemon rather than
    // leaking it for the rest of this machine's uptime.
    term.close();
    control.close();
    daemon.stop();
  }

  function waitForMarker(markerBuf) {
    return new Promise((resolve, reject) => {
      // A measurement harness must fail rather than hang: without this deadline, one lost byte
      // turns into an infinite 100%-CPU `setImmediate` spin that never stops the daemon.
      const timer = setTimeout(
        () => reject(new Error(`timed out waiting for echo of marker ${markerBuf.toString("utf8")}`)),
        MARKER_TIMEOUT_MS,
      );
      const check = () => {
        const idx = buffered.indexOf(markerBuf);
        if (idx !== -1) {
          buffered = buffered.subarray(idx + markerBuf.length);
          clearTimeout(timer);
          resolve();
        } else {
          setImmediate(check);
        }
      };
      check();
    });
  }

  samples.sort((a, b) => a - b);
  const median = percentile(samples, 50);
  const p95 = percentile(samples, 95);
  console.log(`samples=${SAMPLES} median=${median.toFixed(3)}ms p95=${p95.toFixed(3)}ms`);
  console.log(
    "NOTE: measures daemon<->WebSocket only (plain `cat` session); excludes the xterm.js render step.",
  );

  assert(p95 < 50, `p95 latency ${p95.toFixed(3)}ms exceeds 50ms budget`);
  console.log("PASS  T2 latency");
}

main().catch((err) => {
  console.error(`FAIL  ${err.message}`);
  process.exitCode = 1;
});
