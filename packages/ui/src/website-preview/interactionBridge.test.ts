// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { PREVIEW_MESSAGE } from "./messages";
import { installInteractionRelay, installPreviewInteractionBridge } from "./interactionBridge";

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

const key = (target: EventTarget, value: string) => {
  const event = new KeyboardEvent("keydown", {
    key: value,
    bubbles: true,
    composed: true,
    cancelable: true,
  });
  target.dispatchEvent(event);
  return event;
};
const pointer = (target: EventTarget) => {
  const event = new Event("pointerdown", { bubbles: true, composed: true, cancelable: true });
  target.dispatchEvent(event);
  return event;
};

describe("preview interaction relay", () => {
  it("observes pointer and keyboard use without moving focus, selection, or consuming input", () => {
    const send = vi.fn();
    const remove = installInteractionRelay(window, send);
    cleanups.push(remove);
    const host = document.createElement("div");
    document.body.appendChild(host);
    const input = document.createElement("input");
    host.attachShadow({ mode: "open" }).appendChild(input);
    input.value = "selected sample";
    input.focus();
    input.setSelectionRange(0, 8);
    const receive = vi.fn((event: Event) => event.stopPropagation());
    input.addEventListener("keydown", receive);
    input.addEventListener("pointerdown", receive);

    expect(pointer(input).defaultPrevented).toBe(false);
    expect(key(input, "F6").defaultPrevented).toBe(false);
    expect(key(input, "a").defaultPrevented).toBe(false);
    expect(pointer(input).defaultPrevented).toBe(false);
    expect(send.mock.calls).toEqual([["pointer"], ["keyboard"], ["keyboard"], ["pointer"]]);
    expect(receive).toHaveBeenCalledTimes(4);
    expect(host.shadowRoot!.activeElement).toBe(input);
    expect([input.selectionStart, input.selectionEnd]).toEqual([0, 8]);

    remove();
    send.mockClear();
    pointer(input);
    key(input, "Tab");
    expect(send).not.toHaveBeenCalled();
  });

  it("does not treat a modifier alone as keyboard navigation", () => {
    const send = vi.fn();
    cleanups.push(installInteractionRelay(window, send));
    for (const value of ["Alt", "Control", "Meta", "Shift"]) key(document.body, value);
    expect(send).not.toHaveBeenCalled();
    key(document.body, "Tab");
    expect(send).toHaveBeenCalledExactlyOnceWith("keyboard");
  });

  it("leaves a standalone preview alone", () => {
    const send = vi.spyOn(window, "postMessage").mockImplementation(() => {});
    cleanups.push(installPreviewInteractionBridge());
    pointer(document.body);
    key(document.body, "Tab");
    expect(send).not.toHaveBeenCalled();
  });

  it("forwards its own window's input to the website and stops after cleanup", () => {
    const preview = document.createElement("iframe");
    document.body.appendChild(preview);
    const child = preview.contentWindow!;
    const send = vi.spyOn(child.parent, "postMessage").mockImplementation(() => {});
    const remove = installPreviewInteractionBridge(child);
    cleanups.push(remove);

    pointer(child.document.body);
    key(child.document.body, "Tab");
    expect(send.mock.calls).toEqual(["pointer", "keyboard"].map((modality) => [
      { type: PREVIEW_MESSAGE.interaction, modality }, "/",
    ]));
    send.mockClear();

    remove();
    pointer(child.document.body);
    key(child.document.body, "Tab");
    expect(send).not.toHaveBeenCalled();
  });
});
