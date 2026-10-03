// T2 throughput: flood a plain `cat` session (fixtures/bin/codex, mode "cat") with heavy input
// and measure the echo coming back, plus whether the daemon ever falls behind its broadcast
// channel and drops the client.
//
// Two separate measurements, because they answer different questions:
//
//   - "sustained": input paced low enough that the client's receive loop never falls behind.
//     What this actually measures is echo round-trip throughput (client send -> daemon -> PTY
//     -> daemon -> client receive), not one-directional daemon output throughput — there is no
//     way to measure the latter without a source that writes independently of what the client
//     sends (see replay.mjs's "counter" fixture for that shape). A dropped or stalled client
//     here would be a genuine daemon-side bug, so this is the one whose failure fails the
//     script.
//   - "flood": input sent as fast as Node can push it onto the socket, to characterize whether
//     the daemon falls behind under real pressure. It does (see the printed finding below);
//     that is expected, documented prototype behavior (server.rs's `RecvError::Lagged` path),
//     not treated as a failure. Because the daemon cuts the connection mid-stream, "received /
//     elapsed" for this run is not a throughput figure at all — it is however much got through
//     before the cut, over however long that took — so it is reported without a MiB/s figure.
//
// Runs both at two frame sizes to answer whether batching into larger frames matters.

import WebSocket from "ws";
import { startDaemon, startFixtureSession, openTermSocket, waitForReady, sleep } from "./common.mjs";

const SUSTAINED_PAYLOAD_BYTES = 1 * 1024 * 1024; // 1 MiB, backpressure-paced
const FLOOD_PAYLOAD_BYTES = 20 * 1024 * 1024; // 20 MiB, unthrottled
const COMPLETION_TIMEOUT_MS = 30_000;
const DRAIN_TIMEOUT_MS = 10_000;
const VARIANTS = [
  { name: "large frames (64 KiB)", chunk: 64 * 1024 },
  { name: "small frames (4 KiB)", chunk: 4 * 1024 },
];
// The daemon's broadcast channel (state.rs TERM_BROADCAST_CAPACITY) is sized in *messages*, not
// bytes — each PTY write becomes one broadcast message, and a client that falls more than that
// many messages behind gets `RecvError::Lagged` and disconnected (server.rs). So the pacing gate
// below counts outstanding WebSocket messages (one per `cat` echo, which here is one per input
// chunk), not bytes, and stays a comfortable fraction under that capacity.
const SUSTAINED_MAX_OUTSTANDING_MESSAGES = 64;
// Discard the first/last slice of the sustained run before computing a rate: the first chunk or
// two is still filling the pipe (process scheduling, TCP slow start) and the final chunk waits
// out whatever is still in flight, neither of which reflects steady-state throughput.
const STEADY_STATE_LOW_FRACTION = 0.1;
const STEADY_STATE_HIGH_FRACTION = 0.9;

/**
 * Backpressure gate sized in broadcast-channel messages (see the constant above), woken by the
 * "message" handler as soon as room opens up rather than polling on a timer — a `sleep(1)` poll
 * is clamped by Node to at least ~1ms, which on a fast loopback connection is itself slower than
 * the thing it is supposed to be timing.
 */
class MessageBackpressureGate {
  #maxOutstanding;
  #sent = 0;
  #received = 0;
  #waiters = [];

  constructor(maxOutstanding) {
    this.#maxOutstanding = maxOutstanding;
  }

  recordSent() {
    this.#sent++;
  }

  recordReceived() {
    this.#received++;
    if (this.#sent - this.#received <= this.#maxOutstanding) {
      const waiters = this.#waiters;
      this.#waiters = [];
      for (const wake of waiters) wake();
    }
  }

  /** Resolves once outstanding messages are back under the limit, or rejects on a deadline — a
   * measurement harness must fail rather than hang if the echo stream stalls for good. */
  async waitForRoom(deadlineMs) {
    if (this.#sent - this.#received <= this.#maxOutstanding) return;
    await new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error("timed out waiting for the broadcast channel to drain")),
        deadlineMs,
      );
      this.#waiters.push(() => {
        clearTimeout(timer);
        resolve();
      });
    });
  }
}

function send(term, payload) {
  return new Promise((resolve) => {
    // Deliberately doesn't reject on error: in the "flood" run, the daemon closing the socket
    // out from under an in-flight send (once it falls behind, see the FINDING below) races with
    // this send's own readyState check, and failing the whole script on that expected outcome
    // would defeat the point of exercising it. The caller's readyState check on the next loop
    // iteration is what actually stops the flood.
    term.send(payload, () => resolve());
  });
}

async function sendPayload(term, totalBytes, chunkSize, gate, paced) {
  const payload = Buffer.alloc(chunkSize, 0x41); // content is irrelevant, only volume is
  const chunks = Math.ceil(totalBytes / chunkSize);
  for (let i = 0; i < chunks && term.readyState === WebSocket.OPEN; i++) {
    if (paced) await gate.waitForRoom(DRAIN_TIMEOUT_MS);
    await send(term, payload);
    gate.recordSent();
  }
}

async function waitForCompletion(term, getReceived, targetBytes, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (getReceived() < targetBytes && term.readyState === WebSocket.OPEN && Date.now() < deadline) {
    await sleep(5);
  }
}

async function runOnce(daemon, { totalBytes, chunkSize, paced }) {
  const { session, control } = await startFixtureSession(daemon.port, "cat");
  const term = await openTermSocket(daemon.port, session);
  await waitForReady(term);

  const gate = new MessageBackpressureGate(SUSTAINED_MAX_OUTSTANDING_MESSAGES);
  let received = 0;
  const lowWatermark = totalBytes * STEADY_STATE_LOW_FRACTION;
  const highWatermark = totalBytes * STEADY_STATE_HIGH_FRACTION;
  let steadyStart;
  let steadyStartBytes;
  let steadyEnd;
  let steadyEndBytes;
  term.on("message", (data) => {
    const before = received;
    received += data.length;
    gate.recordReceived();
    // The watermark crossing lands mid-chunk, not exactly on it — a message that straddles
    // `lowWatermark`/`highWatermark` moves `received` past it by up to one chunk. Stamp the
    // timestamp against the `received` value actually reached at that moment, not the nominal
    // watermark, so the rate below isn't off by up to one chunk at each end.
    if (before < lowWatermark && received >= lowWatermark) {
      steadyStart = process.hrtime.bigint();
      steadyStartBytes = received;
    }
    if (before < highWatermark && received >= highWatermark) {
      steadyEnd = process.hrtime.bigint();
      steadyEndBytes = received;
    }
  });

  const start = process.hrtime.bigint();
  await sendPayload(term, totalBytes, chunkSize, gate, paced);
  await waitForCompletion(term, () => received, totalBytes, COMPLETION_TIMEOUT_MS);
  const end = process.hrtime.bigint();
  const elapsedSec = Number(end - start) / 1e9;
  const steadyStateSec = steadyStart !== undefined && steadyEnd !== undefined ? Number(steadyEnd - steadyStart) / 1e9 : undefined;
  const steadyStateBytes = steadyStartBytes !== undefined && steadyEndBytes !== undefined ? steadyEndBytes - steadyStartBytes : undefined;

  const droppedByDaemon = term.readyState !== WebSocket.OPEN && received < totalBytes;

  term.close();
  if (control.readyState === WebSocket.OPEN) {
    control.send(JSON.stringify({ type: "kill_session", session }));
  }
  control.close();

  return { received, totalBytes, elapsedSec, steadyStateSec, steadyStateBytes, droppedByDaemon };
}

function report(label, result, { suppressRate }) {
  const base =
    `${label}: received=${result.received}/${result.totalBytes}B in ${result.elapsedSec.toFixed(3)}s, ` +
    `droppedByDaemon=${result.droppedByDaemon}`;
  if (suppressRate) {
    // See the file header: a run the daemon cut short mid-stream has no steady rate to report.
    console.log(`${base} (no throughput figure — truncated by the daemon's Lagged disconnect)`);
    return;
  }
  if (result.steadyStateSec !== undefined) {
    const steadyMbPerSec = result.steadyStateBytes / result.steadyStateSec / (1024 * 1024);
    console.log(`${base}, echo round-trip throughput (steady-state window) = ${steadyMbPerSec.toFixed(1)} MiB/s`);
  } else {
    // Payload too small (relative to the watermarks) to ever reach steady state; fall back to
    // the whole-run average rather than printing nothing.
    const mbPerSec = result.received / result.elapsedSec / (1024 * 1024);
    console.log(`${base}, echo round-trip throughput (whole run, no steady-state window reached) = ${mbPerSec.toFixed(1)} MiB/s`);
  }
}

async function main() {
  const daemon = await startDaemon();
  let sustainedOk = true;

  try {
    for (const variant of VARIANTS) {
      const sustained = await runOnce(daemon, {
        totalBytes: SUSTAINED_PAYLOAD_BYTES,
        chunkSize: variant.chunk,
        paced: true,
      });
      report(`sustained, ${variant.name}`, sustained, { suppressRate: false });
      if (sustained.droppedByDaemon || sustained.received < sustained.totalBytes) sustainedOk = false;

      const flood = await runOnce(daemon, {
        totalBytes: FLOOD_PAYLOAD_BYTES,
        chunkSize: variant.chunk,
        paced: false,
      });
      report(`flood, ${variant.name}`, flood, { suppressRate: flood.droppedByDaemon });
    }
  } finally {
    daemon.stop();
  }

  console.log(
    "FINDING: an unthrottled flood overruns the daemon's fixed-size broadcast channel " +
      "(state.rs TERM_BROADCAST_CAPACITY) almost immediately and the daemon disconnects the " +
      "term socket (server.rs's Lagged handling) rather than ever catching up or applying " +
      "backpressure to the PTY reader thread. This is documented prototype behavior, not a bug " +
      "introduced by this bench — a real client has to treat that as 'must reconnect', matching " +
      "T3's reconnect path.",
  );

  if (!sustainedOk) {
    console.error("FAIL  T2 throughput: the paced ('sustained') run still dropped or stalled");
    process.exitCode = 1;
    return;
  }
  console.log("PASS  T2 throughput");
}

main().catch((err) => {
  console.error(`FAIL  ${err.message}`);
  process.exitCode = 1;
});
