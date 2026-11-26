// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { queuedMessage, sessionOf, snapshotState } from "../fixtures/builders";
import type { ClientRequestBody, Event, QueuedMessage, Session } from "../protocol";
import { applyDaemonEvent, type Daemon } from "../store";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { click, type } from "../test/dom";
import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { appendToComposer, composerDraft, setComposerDraft } from "./composerDrafts";
import { sessionComposerEnabled } from "./composerSetting";
import { SessionComposer, useComposerShown } from "./SessionComposer";
import { daemonError } from "../test/daemonError";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { windowAttention } from "../components/announce/windowTestSupport";
import { fakeTimers } from "../test/clock";
import { format } from "../i18n/catalog";
import { setLanguage } from "../i18n/language";

const MAC = { kind: "browser" as const, host: MAC_HOST };

function sessionWith(extra: Partial<Session> = {}): Session {
  return sessionOf("s", "c", "p", "Work", "idle", extra);
}

function setUp({ session = sessionWith(), queue = [], reply }: { session?: Session; queue?: QueuedMessage[]; reply?: (body: ClientRequestBody) => Event | Promise<Event> } = {}) {
  const sent: ClientRequestBody[] = [];
  const daemon: Daemon = testDaemon({
    state: snapshotState({ sessions: [session], messageQueues: { s: queue } }),
    request: async (body) => {
      sent.push(body);
      if (reply) return reply(body);
      if (body.type === "create_queued_message") return { type: "queued_message_created", id: null, message: "new" };
      return { type: "ack", id: null };
    },
  });
  const render = (current: Session) => <SessionComposer session={current} />;
  const container = mountHost(render(session), { daemon, platform: MAC });
  const field = () => container.querySelector<HTMLTextAreaElement>("textarea")!;
  const button = (name: string) => [...container.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === name || b.getAttribute("aria-label") === name);
  return { daemon, sent, container, field, button, rerender: (current: Session) => container.render(render(current)) };
}

function press(element: HTMLElement, init: KeyboardEventInit) {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  act(() => {
    element.dispatchEvent(event);
  });
  return event;
}

beforeEach(() => sessionComposerEnabled.set(true));
afterEach(() => {
  setLanguage("en");
  sessionComposerEnabled.set(false);
  acknowledgeAllAlerts();
});

describe("SessionComposer sending", () => {
  it("sends the message in one piece with Command-Enter, by the session's default mode, and clears the editor", async () => {
    const { sent, field, daemon } = setUp({ session: sessionWith({ send_mode: "when_idle" }) });
    type(field(), "first line\nsecond line");
    await act(async () => {
      press(field(), { key: "Enter", metaKey: true });
    });
    expect(sent).toEqual([{ type: "create_queued_message", session: "s", text: "first line\nsecond line", mode: "when_idle" }]);
    expect(composerDraft(daemon.store, "s").text).toBe("");
  });

  it("sends with Ctrl-Enter outside a Mac", async () => {
    const sent: ClientRequestBody[] = [];
    const daemon = testDaemon({
      state: snapshotState({ sessions: [sessionWith()], messageQueues: {} }),
      request: async (body) => {
        sent.push(body);
        return { type: "queued_message_created", id: null, message: "new" };
      },
    });
    const container = mountHost(<SessionComposer session={sessionWith()} />, { daemon, platform: { kind: "browser", host: WINDOWS_HOST } });
    const field = container.querySelector<HTMLTextAreaElement>("textarea")!;
    type(field, "hello");
    await act(async () => {
      press(field, { key: "Enter", metaKey: true });
    });
    expect(sent).toEqual([]);
    await act(async () => {
      press(field, { key: "Enter", ctrlKey: true });
    });
    expect(sent).toEqual([{ type: "create_queued_message", session: "s", text: "hello", mode: "now" }]);
  });

  it("never sends on a plain Enter or on a key an input method is composing with", async () => {
    const { sent, field } = setUp();
    type(field(), "こんにちは");
    expect(press(field(), { key: "Enter" }).defaultPrevented).toBe(false);
    expect(press(field(), { key: "Enter", metaKey: true, isComposing: true }).defaultPrevented).toBe(false);
    expect(press(field(), { key: "Enter", metaKey: true, keyCode: 229 } as KeyboardEventInit).defaultPrevented).toBe(false);
    await act(async () => {});
    expect(sent).toEqual([]);
  });

  it("names the action by the mode: Send for now, Add to queue otherwise, and offers nothing to send while empty", () => {
    const { button, rerender } = setUp();
    expect(button("Send")?.getAttribute("aria-disabled") ?? (button("Send")?.disabled ? "true" : null)).toBe("true");
    rerender(sessionWith({ send_mode: "manual" }));
    expect(button("Add to queue")).toBeDefined();
  });

  it("sends a lone slash command as a command", async () => {
    const { sent, field, button } = setUp();
    type(field(), "  /compact keep the plan  ");
    await click(button("Send"));
    expect(sent).toEqual([{ type: "create_queued_message", session: "s", text: "/compact keep the plan", command: true, mode: "now" }]);
  });

  it("sends text with a slash on several lines as a message", async () => {
    const { sent, field, button } = setUp();
    type(field(), "/compact\nand more");
    await click(button("Send"));
    expect(sent[0]).toEqual({ type: "create_queued_message", session: "s", text: "/compact\nand more", mode: "now" });
  });

  it("keeps the text and raises the refusal when the daemon refuses the message", async () => {
    const { field, button, daemon } = setUp({
      reply: (body) => {
        if (body.type === "create_queued_message") throw daemonError("limit_exceeded", { limit: "queued_messages", max: "100" });
        return { type: "ack", id: null };
      },
    });
    type(field(), "keep me");
    await click(button("Send"));
    expect(composerDraft(daemon.store, "s").text).toBe("keep me");
    expect(errorAlerts.getSnapshot()[0]?.title).toBe("Could not send the message");
  });

  it("sets the session's default mode from its selector", async () => {
    const { sent, container } = setUp();
    await act(async () => container.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
    const option = [...document.body.querySelectorAll<HTMLElement>('[role="option"]')].find((item) => item.textContent?.startsWith("Manual"))!;
    await act(async () => option.click());
    expect(sent).toEqual([{ type: "set_session_send_mode", session: "s", mode: "manual" }]);
  });
});

describe("SessionComposer state", () => {
  it("keeps each session's draft while the composer is away", () => {
    const { field, daemon, container } = setUp();
    type(field(), "half written");
    container.render(null);
    expect(composerDraft(daemon.store, "s").text).toBe("half written");
    container.render(<SessionComposer session={sessionWith()} />);
    expect(container.querySelector("textarea")!.value).toBe("half written");
  });

  it("says when the session has no process and when the agent waits on the user", () => {
    const { container, rerender } = setUp({ session: sessionWith({ status: "interrupted" }) });
    expect(container.textContent).toContain("The session is not running.");
    rerender(sessionWith({ status: "waiting_user" }));
    expect(container.textContent).toContain("The agent is waiting for your answer in the terminal.");
  });

  it("shows text appended from elsewhere in the editor on screen, and holds it while none is", () => {
    const { field, daemon, container } = setUp();
    type(field(), "see");
    act(() => appendToComposer(daemon.store, "s", "this"));
    expect(field().value).toBe("see\nthis");
    container.render(null);
    appendToComposer(daemon.store, "s", "src/app.ts");
    expect(composerDraft(daemon.store, "s").text).toBe("see\nthis\nsrc/app.ts");
  });

  it("names the text area by a label of its own, not by its placeholder", () => {
    const { field } = setUp();
    const labelledBy = field().getAttribute("aria-labelledby");
    const name = field().getAttribute("aria-label") ?? labelledBy?.split(" ").map((id) => document.getElementById(id)?.textContent).join(" ");
    expect(name).toBe("Message");
    expect(field().placeholder).toBe("Write a message for the agent");
  });

  it("puts focus in the text area once the message it sent leaves the button with nothing to send", async () => {
    const { field, button } = setUp();
    type(field(), "hello");
    act(() => button("Send")!.focus());
    await click(button("Send"));
    expect(document.activeElement).toBe(field());
  });

  it("holds back a message with more images than one message takes, saying why on the button", () => {
    const images = Array.from({ length: 11 }, (_, i) => ({ id: `i${i}`, name: `${i}.png`, image: true, path: `/p/${i}.png` }));
    const { daemon } = setUp();
    act(() => setComposerDraft(daemon.store, "s", { text: "look", attachments: images }));
    // Its name is still Send; the reason is its description, inside it.
    const send = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.endsWith("Send"))!;
    expect(send.getAttribute("aria-disabled")).toBe("true");
    const description = (send.getAttribute("aria-describedby") ?? "").split(" ").map((id) => document.getElementById(id)?.textContent ?? "").join(" ");
    expect(description).toContain("at most 10 images");
  });

  it("leaves the mode to the daemon where the session's is one this client does not know", async () => {
    const { sent, field, button, container } = setUp({ session: sessionWith({ send_mode: "at_noon" as never }) });
    expect(container.querySelector('[data-slot="select-trigger"]')!.textContent).toContain("When to send");
    type(field(), "hello");
    await click(button("Send"));
    expect(sent).toEqual([{ type: "create_queued_message", session: "s", text: "hello" }]);
  });

  it("sends a lone slash command as text to an agent that takes no slash commands", async () => {
    const { sent, field, button, daemon } = setUp();
    act(() => daemon.store.setState((state) => ({ providers: state.providers.map((provider) => ({ ...provider, capabilities: { ...provider.capabilities, slash_commands: false } })) })));
    type(field(), "/compact");
    await click(button("Send"));
    expect(sent).toEqual([{ type: "create_queued_message", session: "s", text: "/compact", mode: "now" }]);
  });
});

describe("SessionComposer queue", () => {
  const two = [queuedMessage({ id: "m1", text: "first" }), queuedMessage({ id: "m2", text: "second", mode: "manual" })];

  it("lists the queued messages above the editor with their modes", () => {
    const { container } = setUp({ queue: two });
    const rows = [...container.querySelectorAll("[data-queued]")];
    expect(rows.map((row) => row.getAttribute("data-queued"))).toEqual(["m1", "m2"]);
    expect(rows[0]!.textContent).toContain("When idle");
    expect(rows[1]!.textContent).toContain("Manual");
  });

  it("sends a message now and withdraws one", async () => {
    const { sent, container } = setUp({ queue: two });
    const rowButton = (id: string, name: string) => container.querySelector(`[data-queued="${id}"]`)!.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`);
    await click(rowButton("m1", "Send now"));
    await click(rowButton("m2", "Withdraw"));
    expect(sent).toEqual([
      { type: "send_queued_message", session: "s", message: "m1" },
      { type: "withdraw_queued_message", session: "s", message: "m2" },
    ]);
  });

  it("changes a message's mode and moves it from its menu", async () => {
    const { sent, container } = setUp({ queue: two });
    const item = async (id: string, name: string) => {
      await click(container.querySelector(`[data-queued="${id}"]`)!.querySelector('button[aria-label="Message actions"]'));
      const found = [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"], [role="menuitemradio"]')].find((el) => el.textContent?.includes(name));
      await act(async () => found!.click());
    };
    await item("m1", "Manual");
    await item("m2", "Move earlier");
    expect(sent).toEqual([
      { type: "set_queued_message_mode", session: "s", message: "m1", mode: "manual" },
      { type: "move_queued_message", session: "s", message: "m2", position: 0 },
    ]);
  });

  it("moves a message dragged by its handle to its new place", async () => {
    const { sent, container } = setUp({ queue: two });
    const rows = [...container.querySelectorAll<HTMLElement>("[data-queued]")];
    rows.forEach((row, i) => vi.spyOn(row, "getBoundingClientRect").mockReturnValue(new DOMRect(0, i * 40, 300, 40)));
    const handle = rows[0]!.querySelector<HTMLElement>(".cursor-grab")!;
    await act(async () => {
      handle.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, clientY: 20, pointerType: "mouse" }));
      document.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientY: 50, pointerType: "mouse" }));
      document.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientY: 70, pointerType: "mouse" }));
      document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientY: 70, pointerType: "mouse" }));
    });
    expect(sent).toEqual([{ type: "move_queued_message", session: "s", message: "m1", position: 1 }]);
    // Shown in the order dropped until the daemon's queue changes.
    expect([...container.querySelectorAll("[data-queued]")].map((row) => row.getAttribute("data-queued"))).toEqual(["m2", "m1"]);
  });

  it("edits a queued message in the editor and saves only what changed, bringing the new message back", async () => {
    const { sent, container, field, button, daemon } = setUp({ queue: [queuedMessage({ id: "m1", text: "draft text", mode: "manual", files: ["/home/u/a.txt"] })] });
    type(field(), "my new message");
    await click(container.querySelector('[data-queued="m1"] button[aria-label="Message actions"]'));
    const edit = [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((el) => el.textContent?.includes("Edit"))!;
    await act(async () => edit.click());
    expect(field().value).toBe("draft text");
    expect(container.textContent).toContain("a.txt");
    type(field(), "better text");
    await click(button("Save"));
    expect(sent).toEqual([{ type: "update_queued_message", session: "s", message: "m1", text: "better text", command: false }]);
    expect(composerDraft(daemon.store, "s").text).toBe("my new message");
  });

  it("keeps an edit as a new message when its queued message leaves the queue", async () => {
    const { container, field, daemon } = setUp({ queue: [queuedMessage({ id: "m1", text: "queued" })] });
    await click(container.querySelector('[data-queued="m1"] button[aria-label="Message actions"]'));
    const edit = [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((el) => el.textContent?.includes("Edit"))!;
    await act(async () => edit.click());
    type(field(), "queued, edited");
    act(() => applyDaemonEvent(daemon.store, { type: "session_message_queue", session: "s", messages: [] }));
    const draft = composerDraft(daemon.store, "s");
    expect(draft.editing).toBeUndefined();
    expect(draft.text).toBe("queued, edited");
  });

  it("shows a message being handed over as sending, and one that came back as not delivered or refused", () => {
    const { container } = setUp({
      queue: [
        queuedMessage({ id: "m1", text: "going", state: "sending" }),
        queuedMessage({ id: "m2", text: "back", mode: "manual", failure: { kind: "not_delivered", code: "message_not_taken", params: { agent: "claude" }, message: "The agent did not take the message" } }),
        queuedMessage({ id: "m3", text: "no", failure: { kind: "refused", code: "unknown_session", params: { session: "s" }, message: "unknown session" } }),
      ],
    });
    const row = (id: string) => container.querySelector(`[data-queued="${id}"]`)!;
    expect(row("m1").textContent).toContain("Sending");
    expect(row("m1").querySelector<HTMLButtonElement>('button[aria-label="Send now"]')!.getAttribute("aria-disabled") ?? String(row("m1").querySelector<HTMLButtonElement>('button[aria-label="Send now"]')!.disabled)).toBe("true");
    expect(row("m2").textContent).toContain("Not delivered");
    expect(row("m2").querySelector('[aria-label="Why it was not sent"]')).not.toBeNull();
    expect(row("m3").textContent).toContain("Refused");
  });
});

describe("SessionComposer sending by mode", () => {
  it.each(["now", "when_idle", "manual"] as const)("sends in the session's default mode, %s, with the message as it is", async (mode) => {
    const { sent, field, button } = setUp({ session: sessionWith({ send_mode: mode }) });
    type(field(), "do it");
    await click(button(mode === "now" ? "Send" : "Add to queue"));
    expect(sent).toEqual([{ type: "create_queued_message", session: "s", text: "do it", mode }]);
  });

  it("shows the session's default mode in its selector", () => {
    const { container, rerender } = setUp({ session: sessionWith({ send_mode: "manual" }) });
    const trigger = () => container.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.textContent;
    expect(trigger()).toContain("Manual");
    rerender(sessionWith({ send_mode: "when_idle" }));
    expect(trigger()).toContain("When idle");
  });

  it("sends once where the shortcut is pressed again while the message is still out", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    const { sent, field, daemon } = setUp({
      reply: async (body) => {
        if (body.type === "create_queued_message") {
          await gate;
          return { type: "queued_message_created", id: null, message: "new" };
        }
        return { type: "ack", id: null };
      },
    });
    type(field(), "once");
    await act(async () => {
      press(field(), { key: "Enter", metaKey: true });
    });
    await act(async () => {
      press(field(), { key: "Enter", metaKey: true });
    });
    await act(async () => release());
    expect(sent.filter((body) => body.type === "create_queued_message")).toHaveLength(1);
    expect(composerDraft(daemon.store, "s").text).toBe("");
  });

  it("sends nothing for blank text", async () => {
    const { sent, field } = setUp();
    type(field(), "  \n ");
    await act(async () => {
      press(field(), { key: "Enter", metaKey: true });
    });
    expect(sent).toEqual([]);
  });

  it("saves an edit that is now a lone slash command as a command, and ends the edit", async () => {
    const { sent, container, field, button, daemon } = setUp({ queue: [queuedMessage({ id: "m1", text: "plain", mode: "manual" })] });
    await click(container.querySelector('[data-queued="m1"] button[aria-label="Message actions"]'));
    const edit = [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((el) => el.textContent?.includes("Edit"))!;
    await act(async () => edit.click());
    type(field(), "/compact");
    await click(button("Save"));
    expect(sent).toEqual([{ type: "update_queued_message", session: "s", message: "m1", text: "/compact", command: true }]);
    expect(composerDraft(daemon.store, "s").editing).toBeUndefined();
  });

  it("puts focus in the text area once a saved edit leaves the button with nothing to send", async () => {
    const { container, field, button, daemon } = setUp({ queue: [queuedMessage({ id: "m1", text: "queued", mode: "manual" })] });
    await click(container.querySelector('[data-queued="m1"] button[aria-label="Message actions"]'));
    const edit = [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((el) => el.textContent?.includes("Edit"))!;
    await act(async () => edit.click());
    type(field(), "queued, changed");
    act(() => button("Save")!.focus());
    await click(button("Save"));
    expect(composerDraft(daemon.store, "s").editing).toBeUndefined();
    expect(document.activeElement).toBe(field());
  });

  it("words the refusal of a message sent with the shortcut in the language the window has now, not the one it had when it opened", async () => {
    setLanguage("de");
    const { field, rerender } = setUp({
      reply: (body) => {
        if (body.type === "create_queued_message") throw daemonError("limit_exceeded", { limit: "queued_messages", max: "100" });
        return { type: "ack", id: null };
      },
    });
    setLanguage("en");
    rerender(sessionWith());
    type(field(), "hello");
    await act(async () => {
      press(field(), { key: "Enter", metaKey: true });
    });
    expect(errorAlerts.getSnapshot()[0]?.title).toBe(format("en", "composer.sendFailedTitle"));
  });

  it("cancels an edit with the new message that was being written brought back", async () => {
    const { container, field, button, daemon } = setUp({ queue: [queuedMessage({ id: "m1", text: "queued" })] });
    type(field(), "my new message");
    await click(container.querySelector('[data-queued="m1"] button[aria-label="Message actions"]'));
    const edit = [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((el) => el.textContent?.includes("Edit"))!;
    await act(async () => edit.click());
    expect(field().value).toBe("queued");
    act(() => button("Cancel")!.focus());
    await click(button("Cancel"));
    expect(field().value).toBe("my new message");
    // The button went with the edit: focus is in the text area, not on the page.
    expect(document.activeElement).toBe(field());
    expect(composerDraft(daemon.store, "s").editing).toBeUndefined();
  });
});

describe("SessionComposer drafts", () => {
  it("keeps each session's own draft while the window moves between sessions", () => {
    const first = sessionWith();
    const second = sessionOf("t", "c", "p", "Other", "idle");
    const daemon = testDaemon({ state: snapshotState({ sessions: [first, second], messageQueues: {} }) });
    const render = (session: Session) => <SessionComposer key={session.id} session={session} />;
    const container = mountHost(render(first), { daemon, platform: MAC });
    const field = () => container.querySelector<HTMLTextAreaElement>("textarea")!;
    type(field(), "for the first");
    container.render(render(second));
    expect(field().value).toBe("");
    type(field(), "for the second");
    container.render(render(first));
    expect(field().value).toBe("for the first");
    container.render(render(second));
    expect(field().value).toBe("for the second");
  });

  it("keeps the draft where the composer is turned off and on again", () => {
    const { daemon, field, container } = setUp();
    type(field(), "survives");
    act(() => sessionComposerEnabled.set(false));
    container.render(null);
    act(() => sessionComposerEnabled.set(true));
    container.render(<SessionComposer session={sessionWith()} />);
    expect(container.querySelector("textarea")!.value).toBe("survives");
    expect(composerDraft(daemon.store, "s").text).toBe("survives");
  });
});

describe("whether the composer shows", () => {
  function Probe({ session }: { session: Session | undefined }) {
    return <span data-shown={String(useComposerShown(session))} />;
  }
  function shown(session: Session | undefined, { queue = true }: { queue?: boolean } = {}) {
    const daemon = testDaemon({ state: snapshotState({ sessions: session ? [session] : [], messageQueues: queue ? {} : null }) });
    return mountHost(<Probe session={session} />, { daemon }).querySelector("span")!.getAttribute("data-shown");
  }

  it("is off until the switch is turned on, for any session", () => {
    sessionComposerEnabled.set(false);
    expect(shown(sessionWith())).toBe("false");
  });

  it("is on for a live session and for one with no process", () => {
    expect(shown(sessionWith())).toBe("true");
    expect(shown(sessionWith({ status: "interrupted" }))).toBe("true");
  });

  it("is off for an archived session, with no session, and for a daemon that keeps no queue", () => {
    expect(shown(sessionWith({ status: "archived" }))).toBe("false");
    expect(shown(undefined)).toBe("false");
    expect(shown(sessionWith(), { queue: false })).toBe("false");
  });

  it("follows the switch while mounted", () => {
    const session = sessionWith();
    const daemon = testDaemon({ state: snapshotState({ sessions: [session], messageQueues: {} }) });
    const container = mountHost(<Probe session={session} />, { daemon });
    expect(container.querySelector("span")!.getAttribute("data-shown")).toBe("true");
    act(() => sessionComposerEnabled.set(false));
    expect(container.querySelector("span")!.getAttribute("data-shown")).toBe("false");
  });
});

describe("the waiting-for-you announcement", () => {
  const spoken = (container: HTMLElement) => [...container.querySelectorAll('[role="status"]')].map((region) => region.textContent).join("");
  const wait = () => act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS + 500));

  function announcing(status: Session["status"], exposed = true) {
    (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    windowAttention()(true);
    fakeTimers();
    const daemon = testDaemon({ state: snapshotState({ sessions: [sessionWith({ status })], messageQueues: {} }) });
    const render = (current: Session, shown = exposed) => <SessionComposer session={current} exposed={shown} />;
    const container = mountHost(render(sessionWith({ status })), { daemon, platform: MAC });
    return { container, rerender: (current: Session, shown?: boolean) => container.render(render(current, shown)) };
  }

  it("sits in a live announcer, which a modal's hiding of the page leaves exposed", () => {
    const { container } = announcing("working");
    expect(container.querySelector('[role="status"]')!.closest("[data-live-announcer=true]")).not.toBeNull();
  });

  it("says nothing of a session opened already waiting", () => {
    const { container } = announcing("waiting_user");
    wait();
    expect(spoken(container)).toBe("");
  });

  it("says the agent turning to wait under the open composer, once an attended wait has passed", () => {
    const { container, rerender } = announcing("working");
    rerender(sessionWith({ status: "waiting_user" }));
    expect(spoken(container)).toBe("");
    wait();
    expect(spoken(container)).toBe("The agent is waiting for your answer in the terminal.");
  });

  it("holds it while the composer is out of reach, and says it once it is exposed", () => {
    const { container, rerender } = announcing("working", false);
    rerender(sessionWith({ status: "waiting_user" }), false);
    wait();
    expect(spoken(container)).toBe("");
    rerender(sessionWith({ status: "waiting_user" }), true);
    wait();
    expect(spoken(container)).toBe("The agent is waiting for your answer in the terminal.");
  });
});
