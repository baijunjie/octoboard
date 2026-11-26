// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";

import { sessionOf, snapshotState } from "../fixtures/builders";
import { testDaemon } from "../test/testDaemon";
import { composerOffered, composerTakesInput } from "./composerInput";
import { sessionComposerEnabled } from "./composerSetting";

afterEach(() => sessionComposerEnabled.set(false));

describe("composerOffered", () => {
  const live = { status: "idle" } as const;

  it("is offered only with the switch on, a daemon that keeps a queue, and a session that is not archived", () => {
    expect(composerOffered(true, true, live)).toBe(true);
    expect(composerOffered(false, true, live)).toBe(false);
    expect(composerOffered(true, false, live)).toBe(false);
    expect(composerOffered(true, true, { status: "archived" })).toBe(false);
    expect(composerOffered(true, true, undefined)).toBe(false);
  });

  it("is offered for a session with no process, where messages queue and wait", () => {
    expect(composerOffered(true, true, { status: "interrupted" })).toBe(true);
    expect(composerOffered(true, true, { status: "waiting_user" })).toBe(true);
  });
});

describe("composerTakesInput", () => {
  function storeOf(session = sessionOf("s", "c", "p", "Work", "idle"), queues: Record<string, never[]> | null = {}) {
    return testDaemon({ state: snapshotState({ sessions: [session], messageQueues: queues }) }).store;
  }

  it("is off by default, since the composer is", () => {
    expect(composerTakesInput(storeOf(), "s")).toBe(false);
  });

  it("follows the switch, read when asked", () => {
    const store = storeOf();
    sessionComposerEnabled.set(true);
    expect(composerTakesInput(store, "s")).toBe(true);
    sessionComposerEnabled.set(false);
    expect(composerTakesInput(store, "s")).toBe(false);
  });

  it("is false for an archived session, an unknown one, and a daemon that keeps no queue", () => {
    sessionComposerEnabled.set(true);
    expect(composerTakesInput(storeOf(sessionOf("s", "c", "p", "Work", "archived")), "s")).toBe(false);
    expect(composerTakesInput(storeOf(), "other")).toBe(false);
    expect(composerTakesInput(storeOf(undefined, null), "s")).toBe(false);
  });
});
