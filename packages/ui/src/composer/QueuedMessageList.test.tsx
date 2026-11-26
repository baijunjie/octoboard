// @vitest-environment jsdom
import { act } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { queuedMessage } from "../fixtures/builders";
import { t } from "../i18n/language";
import type { ClientRequestBody, QueuedMessage } from "../protocol";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { daemonError } from "../test/daemonError";
import { flushPromises } from "../test/clock";
import { click } from "../test/dom";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { QueuedMessageList } from "./QueuedMessageList";

afterEach(() => acknowledgeAllAlerts());

const first = queuedMessage({ id: "m1", text: "first" });
const second = queuedMessage({ id: "m2", text: "second" });
const third = queuedMessage({ id: "m3", text: "third" });

function setUp(messages: QueuedMessage[], { full = true, editing, refuseMove = false }: { full?: boolean; editing?: string; refuseMove?: boolean } = {}) {
  const sent: ClientRequestBody[] = [];
  const daemon = testDaemon({
    request: async (body) => {
      sent.push(body);
      if (refuseMove && body.type === "move_queued_message") throw daemonError("queued_message_not_found", { session: "s", message: body.message });
      return { type: "ack", id: null };
    },
  });
  const context = { store: daemon.store, request: daemon.request, t };
  const fallback = document.createElement("button");
  const render = (shown: QueuedMessage[]) => (
    <QueuedMessageList session="s" messages={shown} context={context} full={full} editing={editing} onEdit={() => {}} fallback={() => fallback} label="Queued" />
  );
  const container = mountHost(render(messages), { daemon });
  const row = (id: string) => container.querySelector<HTMLElement>(`[data-queued="${id}"]`)!;
  const order = () => [...container.querySelectorAll("[data-queued]")].map((element) => element.getAttribute("data-queued"));
  const openMenu = async (id: string) => {
    await click(row(id).querySelector('button[aria-label="Message actions"]'));
    return [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"], [role="menuitemradio"]')];
  };
  return { sent, container, row, order, openMenu, rerender: (shown: QueuedMessage[]) => container.render(render(shown)) };
}

const isDisabled = (element: Element | null | undefined) => element?.getAttribute("aria-disabled") === "true" || (element as HTMLButtonElement | null | undefined)?.disabled === true;
const itemNamed = (items: HTMLElement[], name: string) => items.find((item) => item.textContent?.includes(name));

describe("a queued message's row", () => {
  it("shows its text on one line, its mode, a command mark, and how many files it carries", () => {
    const { row } = setUp([
      queuedMessage({ id: "m1", text: "first\nline", mode: "manual", images: ["/a.png"], files: ["/b.txt", "/c.txt"] }),
      queuedMessage({ id: "m2", text: "/compact", command: true }),
    ]);
    expect(row("m1").textContent).toContain("first line");
    expect(row("m1").textContent).toContain("Manual");
    expect(row("m1").textContent).toContain("3 files");
    expect(row("m2").textContent).toContain("When idle");
    expect(row("m2").textContent).toContain("Command");
    expect(row("m2").textContent).not.toContain("files");
  });

  it("reads a mode this client does not know as manual", () => {
    const { row } = setUp([queuedMessage({ id: "m1", text: "x", mode: "at_noon" as never })]);
    expect(row("m1").textContent).toContain("Manual");
  });

  it("names a refusal and a failed delivery apart, and gives the daemon's reason", () => {
    const { row } = setUp([
      queuedMessage({ id: "m1", text: "a", failure: { kind: "refused", code: "unknown_session", params: { session: "s" }, message: "unknown session" } }),
      queuedMessage({ id: "m2", text: "b", failure: { kind: "not_delivered", code: "message_not_taken", params: { agent: "claude" }, message: "The agent did not take the message" } }),
    ]);
    expect(row("m1").textContent).toContain("Refused");
    expect(row("m1").textContent).not.toContain("Not delivered");
    expect(row("m2").textContent).toContain("Not delivered");
    expect(row("m2").querySelector('[aria-label="Why it was not sent"]')).not.toBeNull();
  });

  it("is marked as the one being edited, and cannot be edited again meanwhile", async () => {
    const { row, openMenu } = setUp([first, second], { editing: "m1" });
    expect(row("m1").textContent).toContain("Editing");
    expect(row("m2").textContent).not.toContain("Editing");
    expect(isDisabled(itemNamed(await openMenu("m1"), "Edit"))).toBe(true);
  });

  it("shows and records the mode it is to wait in after its edit, sending nothing while it is edited", async () => {
    const sent: ClientRequestBody[] = [];
    const daemon = testDaemon({ request: async (body) => { sent.push(body); return { type: "ack", id: null }; } });
    const picked: string[] = [];
    const container = mountHost(
      <QueuedMessageList session="s" messages={[queuedMessage({ id: "m1", text: "x", mode: "manual" })]} context={{ store: daemon.store, request: daemon.request, t }} full editing="m1" editedMode="when_idle" onEditedMode={(mode) => picked.push(mode)} onEdit={() => {}} fallback={() => null} label="Queued" />,
      { daemon },
    );
    await click(container.querySelector('button[aria-label="Message actions"]'));
    const items = [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"], [role="menuitemradio"]')];
    expect(itemNamed(items, "When idle")?.getAttribute("aria-checked")).toBe("true");
    await act(async () => itemNamed(items, "Manual")!.click());
    expect(picked).toEqual(["manual"]);
    expect(sent).toEqual([]);
  });

  it("cannot be sent now while it is being edited, as what would go is not what the editor holds", () => {
    const { row } = setUp([first, second], { editing: "m1" });
    expect(isDisabled(row("m1").querySelector('button[aria-label="Send now"]'))).toBe(true);
    expect(isDisabled(row("m2").querySelector('button[aria-label="Send now"]'))).toBe(false);
  });

  it("offers nothing to edit, change or drag in the compact list, only Send now and Withdraw", () => {
    const { row } = setUp([first, second], { full: false });
    expect([...row("m1").querySelectorAll("button")].map((button) => button.getAttribute("aria-label"))).toEqual(["Send now", "Withdraw"]);
    expect(row("m1").querySelector(".cursor-grab")).toBeNull();
  });

  it("offers no drag handle for a single message", () => {
    const { row } = setUp([first]);
    expect(row("m1").querySelector(".cursor-grab")).toBeNull();
  });
});

describe("a message being handed over", () => {
  const sending = queuedMessage({ id: "m1", text: "going", state: "sending" });

  it("shows as sending, and can be neither sent again, edited nor given another mode", async () => {
    const { row, openMenu } = setUp([sending, second]);
    expect(row("m1").textContent).toContain("Sending");
    expect(isDisabled(row("m1").querySelector('button[aria-label="Send now"]'))).toBe(true);
    const items = await openMenu("m1");
    expect(isDisabled(itemNamed(items, "Edit"))).toBe(true);
    expect(isDisabled(itemNamed(items, "When idle"))).toBe(true);
    expect(isDisabled(itemNamed(items, "Manual"))).toBe(true);
    // It can still be withdrawn, which the daemon answers by whether it is too late.
    expect(isDisabled(row("m1").querySelector('button[aria-label="Withdraw"]'))).toBe(false);
  });
});

describe("moving a message from its menu", () => {
  it("cannot move the first earlier or the last later", async () => {
    const { openMenu } = setUp([first, second, third]);
    expect(isDisabled(itemNamed(await openMenu("m1"), "Move earlier"))).toBe(true);
    expect(isDisabled(itemNamed(await openMenu("m1"), "Move later"))).toBe(false);
    await act(async () => document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    expect(isDisabled(itemNamed(await openMenu("m3"), "Move later"))).toBe(true);
    expect(isDisabled(itemNamed(await openMenu("m3"), "Move earlier"))).toBe(false);
  });

  it("marks the mode the message waits in", async () => {
    const { openMenu } = setUp([queuedMessage({ id: "m1", text: "x", mode: "manual" })]);
    const items = await openMenu("m1");
    expect(itemNamed(items, "Manual")?.getAttribute("aria-checked")).toBe("true");
    expect(itemNamed(items, "When idle")?.getAttribute("aria-checked")).toBe("false");
  });
});

describe("dragging a message", () => {
  function drag(row: HTMLElement, from: number, to: number) {
    const handle = row.querySelector<HTMLElement>(".cursor-grab")!;
    return act(async () => {
      handle.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, clientY: from, pointerType: "mouse" }));
      document.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientY: to, pointerType: "mouse" }));
      document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientY: to, pointerType: "mouse" }));
    });
  }
  const place = (container: HTMLElement) => container.querySelectorAll<HTMLElement>("[data-queued]").forEach((row, i) => vi.spyOn(row, "getBoundingClientRect").mockReturnValue(new DOMRect(0, i * 40, 300, 40)));

  it("shows the order dropped until the daemon's queue changes, then follows the daemon's", async () => {
    const { container, sent, order, row, rerender } = setUp([first, second, third]);
    place(container);
    await drag(row("m1"), 20, 110);
    expect(sent).toEqual([{ type: "move_queued_message", session: "s", message: "m1", position: 2 }]);
    expect(order()).toEqual(["m2", "m3", "m1"]);
    rerender([second, first, third]);
    expect(order()).toEqual(["m2", "m1", "m3"]);
  });

  it("goes back to the daemon's order when the move is refused, and says so (the shown order is always the daemon's)", async () => {
    const { container, order, row } = setUp([first, second, third], { refuseMove: true });
    place(container);
    await drag(row("m1"), 20, 110);
    await flushPromises();
    expect(errorAlerts.getSnapshot()[0]?.title).toBe(t("queue.changeFailedTitle"));
    expect(order()).toEqual(["m1", "m2", "m3"]);
  });
});

describe("a message leaving the list", () => {
  const focused = () => ({ row: document.activeElement?.closest("[data-queued]")?.getAttribute("data-queued"), label: document.activeElement?.getAttribute("aria-label") });

  it("hands focus on from the message that held it to the next message's Withdraw", async () => {
    const { row, rerender } = setUp([first, second, third]);
    act(() => row("m1").querySelector<HTMLElement>('button[aria-label="Withdraw"]')!.focus());
    expect(focused().row).toBe("m1");
    rerender([second, third]);
    await act(async () => {});
    expect(focused()).toEqual({ row: "m2", label: "Withdraw" });
  });

  it("lands on the message now in a middle message's place, not the first, and never on Send now", async () => {
    const { row, rerender } = setUp([first, second, third]);
    act(() => row("m2").querySelector<HTMLElement>('button[aria-label="Send now"]')!.focus());
    rerender([first, third]);
    await act(async () => {});
    expect(focused()).toEqual({ row: "m3", label: "Withdraw" });
  });

  it("lands on the message before the last one when the last goes, even one being handed over", async () => {
    const { row, rerender } = setUp([queuedMessage({ id: "m1", text: "going", state: "sending" }), second]);
    act(() => row("m2").querySelector<HTMLElement>('button[aria-label="Withdraw"]')!.focus());
    rerender([queuedMessage({ id: "m1", text: "going", state: "sending" })]);
    await act(async () => {});
    expect(focused()).toEqual({ row: "m1", label: "Withdraw" });
  });
});
