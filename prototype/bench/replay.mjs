// T3 replay exactness: the rigorous one. A "counter" fixture session (fixtures/bin/codex, mode
// "counter") free-runs `printf "LINE %d\n"` independently of any client, giving a verifiable
// byte stream with no dependency on what the bench sends. Each case attaches, detaches mid
// stream, waits, and reattaches.
//
// What "exactness" actually means here, worth spelling out because it is not quite the naive
// reading: the daemon has one ring buffer per *session*, not one per client (server.rs /
// state.rs — `attach_term` snapshots whatever is currently in that shared buffer and atomically
// subscribes to live output from that point on). It does not remember what any particular
// client already saw. So reattaching after a short detach can legitimately *replay content the
// client already saw* — that is not a bug, it is the same thing `tmux attach` does. The
// guarantee worth asserting is narrower and is exactly what server.rs's own doc comment claims:
//
//   1. Within a single attachment, the snapshot-then-live transition has no gap and no
//      duplicate, even while output is actively streaming at attach time.
//   2. Across the whole test, nothing that was ever produced is permanently lost *unless* it
//      fell outside the ring buffer (state.rs RING_CAPACITY) by the time of reattach —
//      that loss is expected by design, not a bug, and Case B exists to show what it looks like.
//
// Case A detaches briefly, kept (by throttling the fixture's counter — see fixtures/bin/codex)
// well under the ring buffer capacity for the whole test, so claim 2 reduces to zero loss. Case B detaches
// long enough to overrun RING_CAPACITY on purpose.

import WebSocket from "ws";
import { startDaemon, startFixtureSession, openTermSocket, assert, sleep } from "./common.mjs";

/**
 * Accumulates "LINE N" integers out of a raw byte stream, tolerating lines split across frames.
 * The "counter" fixture's PTY stays in the daemon's default canonical mode (unlike the "cat"
 * fixture, it never calls `stty raw`), so the line discipline's ONLCR rewrites its `\n` to
 * `\r\n` on the way out — the trailing `\r` has to be stripped or every line fails to match.
 */
function makeLineParser() {
  let leftover = "";
  const numbers = [];
  return {
    feed(buf) {
      leftover += buf.toString("utf8");
      const lines = leftover.split("\n");
      leftover = lines.pop() ?? "";
      for (const line of lines) {
        const match = line.replace(/\r$/, "").match(/^LINE (\d+)$/);
        if (match) numbers.push(Number(match[1]));
      }
    },
    numbers,
  };
}

/** Claim 1: no gap and no duplicate anywhere within one client's own received stream. */
function assertContiguous(numbers, label) {
  assert(numbers.length > 1, `${label}: too few counter lines captured to assert anything (${numbers.length})`);
  for (let i = 1; i < numbers.length; i++) {
    assert(
      numbers[i] === numbers[i - 1] + 1,
      `${label}: not contiguous — LINE ${numbers[i - 1]} followed by LINE ${numbers[i]} (gap or duplicate) at index ${i}`,
    );
  }
}

async function attachFor(daemon, session, durationMs) {
  const term = await openTermSocket(daemon.port, session);
  const parser = makeLineParser();
  term.on("message", (data) => parser.feed(data));
  await sleep(durationMs);
  term.close();
  return parser.numbers;
}

async function main() {
  const daemon = await startDaemon();
  try {
    await runCases(daemon);
  } finally {
    // The fixture's counter loop runs forever regardless of client state, so a thrown
    // assertion must still stop the daemon (and, with it, the counter process) rather than
    // leaking a background process that spins for the rest of this machine's uptime.
    daemon.stop();
  }
  console.log("PASS  T3 replay exactness");
}

async function runCases(daemon) {
  const { session, control } = await startFixtureSession(daemon.port, "counter");

  // --- Case A: short detach, held well under the ring buffer capacity ---
  const before = await attachFor(daemon, session, 500);
  assertContiguous(before, "case A, before disconnect");
  await sleep(200); // detached: the counter keeps running server-side with nobody attached
  const after = await attachFor(daemon, session, 500);
  assertContiguous(after, "case A, after reconnect (snapshot + live)"); // claim 1

  const lastBefore = before[before.length - 1];
  const firstAfter = after[0];
  const unseenGap = firstAfter - lastBefore - 1; // > 0 would mean a number neither client saw
  // unseenGap == 0: firstAfter is exactly lastBefore + 1, nothing re-shown, nothing skipped.
  // unseenGap < 0: the snapshot re-starts at or before lastBefore, so the re-displayed count is
  // the number of already-seen lines between firstAfter and lastBefore inclusive, i.e. -unseenGap.
  console.log(
    `Case A (short detach): client saw up to LINE ${lastBefore}; after reconnecting, the ` +
      `replay snapshot started at LINE ${firstAfter} (${
        unseenGap === 0
          ? "perfect resume, nothing re-shown, as expected"
          : unseenGap < 0
            ? `re-displayed ${-unseenGap} already-seen line(s), as expected`
            : `${unseenGap} line(s) unseen by either client`
      })`,
  );
  assert(unseenGap <= 0, `Case A expected zero loss (no number unseen by both clients), got a gap of ${unseenGap}`);
  console.log("PASS  Case A: snapshot+live replay is gap-free and loss-free across a short disconnect");

  // --- Case B: long detach, deliberately overruns the ring buffer capacity ---
  const beforeOverflow = await attachFor(daemon, session, 300);
  assertContiguous(beforeOverflow, "case B, before disconnect");
  const lastBeforeOverflow = beforeOverflow[beforeOverflow.length - 1];

  // Detach long enough to be confident the counter produced more than the ring buffer can hold
  // (state.rs RING_CAPACITY) while nobody was attached — see fixtures/bin/codex for the
  // throttled rate this is sized against.
  await sleep(8000);

  const afterOverflow = await attachFor(daemon, session, 500);
  assertContiguous(afterOverflow, "case B, after reconnect (snapshot + live)"); // claim 1 still holds
  const firstAfterOverflow = afterOverflow[0];
  const lostLines = firstAfterOverflow - lastBeforeOverflow - 1;

  console.log(
    `Case B (long detach, ring buffer exceeded): client saw up to LINE ${lastBeforeOverflow}, ` +
      `reconnected and the replay snapshot started at LINE ${firstAfterOverflow} (${lostLines} lines lost)`,
  );
  assert(
    lostLines > 0,
    "Case B expected the ring buffer to have been overrun (lostLines > 0) — the detach " +
      "window may need to be longer than this machine's counter rate needs; re-run before " +
      "trusting a 0 here",
  );
  console.log(
    `PASS  Case B: ring-buffer overflow behaves as documented — ${lostLines} lines permanently ` +
      "lost (expected: the ring buffer only holds the most recent bytes, see state.rs " +
      "RING_CAPACITY), still gap-free/duplicate-free on " +
      "both sides of that loss",
  );

  if (control.readyState === WebSocket.OPEN) {
    control.send(JSON.stringify({ type: "kill_session", session }));
  }
  control.close();
}

main().catch((err) => {
  console.error(`FAIL  ${err.message}`);
  process.exitCode = 1;
});
