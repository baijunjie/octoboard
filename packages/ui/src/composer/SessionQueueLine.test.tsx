// @vitest-environment jsdom
// First, as `windowAttention` asks: it recognises the window's own focus events.
import "../input/focusGuard";
import { act } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { queuedMessage, sessionOf, snapshotState } from "../fixtures/builders";
import type { ClientRequestBody, QueuedMessage } from "../protocol";
import { applyDaemonEvent } from "../store";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { windowAttention } from "../components/announce/windowTestSupport";
import { fakeTimers } from "../test/clock";
import { click } from "../test/dom";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { sessionComposerEnabled } from "./composerSetting";
import { SessionQueueLine } from "./SessionQueueLine";

const held = { onFocus: () => {}, onBlur: () => {} };

function setUp(queue: QueuedMessage[], { offered = true } = {}) {
  const sent: ClientRequestBody[] = [];
  const daemon = testDaemon({
    state: snapshotState({ sessions: [sessionOf("s", "c", "p", "Work", "idle")], messageQueues: offered ? { s: queue } : null }),
    request: async (body) => {
      sent.push(body);
      return { type: "ack", id: null };
    },
  });
  const container = mountHost(<SessionQueueLine session="s" held={held} />, { daemon });
  return { sent, container, daemon };
}

afterEach(() => sessionComposerEnabled.set(false));

describe("SessionQueueLine", () => {
  const two = [queuedMessage({ id: "m1", text: "first" }), queuedMessage({ id: "m2", text: "second" })];

  it("says how many messages are queued while the composer is off", () => {
    const { container } = setUp(two);
    expect(container.textContent).toContain("2 messages queued");
    expect(container.querySelector('[data-region="queue"]')).not.toBeNull();
    expect(container.querySelector('button[aria-label="View 2 queued messages"]')?.textContent).toBe("View");
  });

  it("shows nothing with nothing queued, with the composer on, or for a daemon that keeps no queue", () => {
    expect(setUp([]).container.textContent).toBe("");
    expect(setUp(two, { offered: false }).container.textContent).toBe("");
    sessionComposerEnabled.set(true);
    expect(setUp(two).container.textContent).toBe("");
  });

  it("follows the queue the daemon sends", () => {
    const { container, daemon } = setUp(two);
    act(() => applyDaemonEvent(daemon.store, { type: "session_message_queue", session: "s", messages: [two[0]!] }));
    expect(container.textContent).toContain("1 message queued");
    act(() => applyDaemonEvent(daemon.store, { type: "session_message_queue", session: "s", messages: [] }));
    expect(container.textContent).toBe("");
  });

  it("lists the messages with Send now and Withdraw, and nothing else", async () => {
    const { container, sent } = setUp(two);
    await click(container.querySelector('button[aria-label="View 2 queued messages"]'));
    const row = (id: string) => document.body.querySelector(`[data-queued="${id}"]`)!;
    expect(row("m1").textContent).toContain("first");
    expect(row("m1").querySelector('button[aria-label="Message actions"]')).toBeNull();
    expect(row("m1").querySelector(".cursor-grab")).toBeNull();
    await click(row("m1").querySelector('button[aria-label="Send now"]'));
    await click(row("m2").querySelector('button[aria-label="Withdraw"]'));
    expect(sent).toEqual([
      { type: "send_queued_message", session: "s", message: "m1" },
      { type: "withdraw_queued_message", session: "s", message: "m2" },
    ]);
  });
});

describe("the queue line's announcement", () => {
  const two = [queuedMessage({ id: "m1", text: "first" }), queuedMessage({ id: "m2", text: "second" })];
  const spoken = () => document.body.querySelector('[role="status"]')?.textContent ?? "";
  const wait = () => act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS + 500));

  function announcing(queue: QueuedMessage[], startDeferred = false, attended = true) {
    (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    const setAttended = windowAttention();
    setAttended(attended);
    fakeTimers();
    const daemon = testDaemon({ state: snapshotState({ sessions: [sessionOf("s", "c", "p", "Work", "idle"), sessionOf("t", "c", "p", "Other", "idle")], messageQueues: { s: queue, t: two } }) });
    const container = mountHost(<SessionQueueLine session="s" held={held} deferred={startDeferred} />, { daemon });
    return { daemon, container, setAttended, render: (session: string, deferred = startDeferred) => container.render(<SessionQueueLine session={session} held={held} deferred={deferred} />) };
  }

  it("says nothing of a queue already there when the session opens", () => {
    announcing(two);
    wait();
    expect(spoken()).toBe("");
  });

  it("says the line coming up under the open session, once an attended wait has passed", () => {
    const { daemon } = announcing([]);
    act(() => applyDaemonEvent(daemon.store, { type: "session_message_queue", session: "s", messages: two }));
    expect(spoken()).toBe("");
    wait();
    expect(spoken()).toBe("2 messages queued");
  });

  it("holds it while the terminal's announcements are held, and says it once they are not", () => {
    const { daemon, render } = announcing([], true);
    act(() => applyDaemonEvent(daemon.store, { type: "session_message_queue", session: "s", messages: two }));
    wait();
    expect(spoken()).toBe("");
    render("s", false);
    wait();
    expect(spoken()).toBe("2 messages queued");
  });

  it("says nothing more when the count changes after the line came up", () => {
    const { daemon } = announcing([]);
    act(() => applyDaemonEvent(daemon.store, { type: "session_message_queue", session: "s", messages: two }));
    wait();
    expect(spoken()).toBe("2 messages queued");
    act(() => applyDaemonEvent(daemon.store, { type: "session_message_queue", session: "s", messages: [...two, queuedMessage({ id: "m3", text: "third" })] }));
    expect(spoken()).not.toContain("3");
    wait();
    expect(spoken()).not.toContain("3");
  });

  it("says the count as it stands when the sentence comes due, not as the line came up, and nothing for a later change", () => {
    const { daemon, setAttended } = announcing([], false, false);
    const queue = (count: number) => act(() => applyDaemonEvent(daemon.store, { type: "session_message_queue", session: "s", messages: Array.from({ length: count }, (_, i) => queuedMessage({ id: `m${i}`, text: `${i}` })) }));
    queue(1);
    queue(2);
    wait();
    expect(spoken()).toBe("");
    setAttended(true);
    wait();
    expect(spoken()).toBe("2 messages queued");
    queue(3);
    wait();
    expect(spoken()).not.toContain("3");
  });

  it("sits in a live announcer, which a modal's hiding of the page leaves exposed", () => {
    const { container } = announcing([]);
    expect(container.querySelector('[role="status"]')!.closest("[data-live-announcer=true]")).not.toBeNull();
  });

  // The other session has messages queued too, so only the change of session drops the sentence.
  it("drops it unsaid when the session changes first", () => {
    const { daemon, render } = announcing([]);
    act(() => applyDaemonEvent(daemon.store, { type: "session_message_queue", session: "s", messages: two }));
    render("t");
    wait();
    expect(spoken()).toBe("");
  });
});
