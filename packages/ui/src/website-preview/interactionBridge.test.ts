// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { composePageDocument } from "../report/pageDocument";
import {
  installInteractionRelay,
  installPreviewInteractionBridge,
  PREVIEW_INTERACTION_MESSAGE,
  withReportInteractionBridge,
} from "./interactionBridge";
import { withReportScrollBridge } from "./scrollBridge";

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

const reportDocument = () => composePageDocument(
  "<body data-website-preview-report><p>Sample report</p></body>",
  false,
  "abcdef012345",
);
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

  it("forwards only its own live report's valid messages and cleans up the embedded relay", () => {
    const preview = document.createElement("iframe");
    document.body.appendChild(preview);
    const child = preview.contentWindow!;
    child.document.body.innerHTML = '<div class="website-preview-app"></div>';
    const report = child.document.createElement("iframe");
    report.srcdoc = withReportInteractionBridge(reportDocument());
    child.document.body.firstElementChild!.appendChild(report);
    const send = vi.spyOn(child.parent, "postMessage").mockImplementation(() => {});
    const remove = installPreviewInteractionBridge(child);
    cleanups.push(remove);
    const message = (source: Window, modality: unknown) => child.dispatchEvent(new MessageEvent("message", {
      source,
      data: { type: "octoboard-preview-report-interaction", modality },
    }));

    pointer(child.document.body);
    key(child.document.body, "Tab");
    message(report.contentWindow!, "pointer");
    message(report.contentWindow!, "keyboard");
    expect(send.mock.calls).toEqual(["pointer", "keyboard", "pointer", "keyboard"].map((modality) => [
      { type: PREVIEW_INTERACTION_MESSAGE, modality }, "*",
    ]));
    send.mockClear();
    message(window, "keyboard");
    message(report.contentWindow!, "mouse");
    message(report.contentWindow!, true);
    report.srcdoc = reportDocument();
    message(report.contentWindow!, "keyboard");
    report.srcdoc = withReportInteractionBridge(reportDocument());
    const oldReport = report.contentWindow!;
    report.remove();
    message(oldReport, "keyboard");
    expect(send).not.toHaveBeenCalled();

    remove();
    pointer(child.document.body);
    key(child.document.body, "Tab");
    expect(send).not.toHaveBeenCalled();
  });
});

describe("report interaction script", () => {
  it("only augments the known nonce-protected fixture once and preserves its scroll bridge", () => {
    const source = withReportScrollBridge(reportDocument());
    const injected = withReportInteractionBridge(source);
    expect(injected).toContain(source);
    expect(injected).toContain('<script nonce="abcdef012345" data-website-preview-interaction-bridge>');
    expect(withReportInteractionBridge(injected)).toBe(injected);
    for (const other of [
      source.replaceAll("data-website-preview-report", "other-report"),
      source.replaceAll('nonce="abcdef012345"', ""),
      "<body data-website-preview-report>",
    ]) expect(withReportInteractionBridge(other)).toBe(other);
  });

  it("runs the serialized relay without module dependencies", () => {
    const injected = withReportInteractionBridge(reportDocument());
    const script = injected.slice(injected.lastIndexOf('<script nonce="abcdef012345" data-website-preview-interaction-bridge>'))
      .replace(/^<script[^>]+>/, "").replace(/<\/script>$/, "");
    const send = vi.spyOn(window, "postMessage").mockImplementation(() => {});
    const add = vi.spyOn(window, "addEventListener");
    window.eval(script);
    for (const type of ["keydown", "pointerdown"] as const) {
      const listener = add.mock.calls.find(([name]) => name === type)![1];
      cleanups.push(() => window.removeEventListener(type, listener, true));
    }
    pointer(document.body);
    key(document.body, "Tab");
    expect(send.mock.calls).toEqual(["pointer", "keyboard"].map((modality) => [
      { type: "octoboard-preview-report-interaction", modality }, "*",
    ]));
  });
});
