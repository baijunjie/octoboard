// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { SAMPLE, snapshotState } from "./builders";
import { createFixtureDaemon } from "./fixtureDaemon";
import type { SendMode, Session } from "../protocol";

const SESSION = "s-web-2";

/** A daemon whose Website idle session has `send_mode` as its default. */
const daemonWith = (sendMode: SendMode) =>
  createFixtureDaemon({
    state: snapshotState({
      consoles: [SAMPLE.console],
      projects: [SAMPLE.web],
      sessions: SAMPLE.sessions.map((session): Session => (session.id === SESSION ? { ...session, send_mode: sendMode } : session)),
    }),
  });
const queueOf = (daemon: ReturnType<typeof daemonWith>) => daemon.store.getState().messageQueues?.get(SESSION) ?? [];

describe("the fixture daemon's composer queue", () => {
  it("queues a message created without a mode in the session's default mode", async () => {
    for (const [sendMode, queued] of [["when_idle", "when_idle"], ["manual", "manual"]] as const) {
      const daemon = daemonWith(sendMode);
      await daemon.request({ type: "create_queued_message", session: SESSION, text: "Run the tests" });
      expect(queueOf(daemon).map((message) => message.mode)).toEqual([queued]);
    }
  });

  it("sends at once, queueing nothing, a message created in the mode now, set or defaulted", async () => {
    const asked = daemonWith("when_idle");
    await asked.request({ type: "create_queued_message", session: SESSION, text: "Run the tests", mode: "now" });
    expect(queueOf(asked)).toEqual([]);
    const defaulted = daemonWith("now");
    await defaulted.request({ type: "create_queued_message", session: SESSION, text: "Run the tests" });
    expect(queueOf(defaulted)).toEqual([]);
  });

  it("queues a message created in an explicit mode whatever the session's default", async () => {
    const daemon = daemonWith("now");
    await daemon.request({ type: "create_queued_message", session: SESSION, text: "Run the tests", mode: "manual" });
    expect(queueOf(daemon).map((message) => message.mode)).toEqual(["manual"]);
  });
});
