// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { composePageDocument } from "../report/pageDocument";
import {
  installBoundaryWheelRelay,
  installPreviewScrollBridge,
  PREVIEW_SCROLL_MESSAGE,
  withReportScrollBridge,
} from "./scrollBridge";

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
  document.body.replaceChildren();
  vi.restoreAllMocks();
});
function scroller(parent: Node = document.body, top = 0) {
  const element = document.createElement("div");
  element.style.overflowY = "auto";
  Object.defineProperties(element, {
    scrollHeight: { value: 300 },
    clientHeight: { value: 100 },
  });
  element.scrollTop = top;
  parent.appendChild(element);
  return element;
}
function wheel(
  target: EventTarget,
  deltaY: number,
  options: WheelEventInit = {},
) {
  const event = new WheelEvent("wheel", {
    bubbles: true,
    composed: true,
    cancelable: true,
    deltaY,
    ...options,
  });
  target.dispatchEvent(event);
  return event;
}
function relay() {
  const send = vi.fn();
  cleanups.push(installBoundaryWheelRelay(window, send));
  return send;
}
const reportDocument = () =>
  composePageDocument(
    "<body data-website-preview-report><p>Sample report</p></body>",
    false,
    "abcdef012345",
  );

describe("preview wheel boundary handoff", () => {
  it("keeps inner scrolling in both directions and hands off only at the boundary, including small deltas", () => {
    const send = relay();
    const list = scroller();
    expect(wheel(list, 0.1).defaultPrevented).toBe(false);
    expect(wheel(list, -0.1).defaultPrevented).toBe(true);
    list.scrollTop = 100;
    wheel(list, 30);
    wheel(list, -30);
    expect(send.mock.calls).toEqual([[-0.1]]);
    list.scrollTop = 199.8;
    expect(wheel(list, 0.1).defaultPrevented).toBe(true);
    expect(wheel(list, -1).defaultPrevented).toBe(false);
    expect(send.mock.calls).toEqual([[-0.1], [0.1]]);
  });
  it("checks ancestor scrollers and the diff renderer's shadow tree", () => {
    const send = relay();
    const outer = scroller(document.body, 50);
    const host = document.createElement("div");
    outer.appendChild(host);
    const inner = scroller(host.attachShadow({ mode: "open" }), 200);
    expect(wheel(inner, 40).defaultPrevented).toBe(false);
    outer.scrollTop = 200;
    expect(wheel(inner, 40).defaultPrevented).toBe(true);
    inner.scrollTop = 100;
    expect(wheel(inner, 40).defaultPrevented).toBe(false);
    expect(send).toHaveBeenCalledExactlyOnceWith(40);
  });
  it("preserves horizontal, zoom, already handled and non-cancelable gestures", () => {
    const send = relay();
    for (const options of [
      { deltaX: 30 },
      { shiftKey: true },
      { ctrlKey: true },
      { metaKey: true },
      { cancelable: false },
    ]) {
      wheel(document.body, 20, options);
    }
    const handled = new WheelEvent("wheel", {
      deltaY: 20,
      cancelable: true,
      bubbles: true,
    });
    handled.preventDefault();
    document.body.dispatchEvent(handled);
    expect(send).not.toHaveBeenCalled();
  });
  it("normalizes line and page deltas to CSS pixels without changing tiny pixel deltas", () => {
    const send = relay();
    document.body.style.lineHeight = "20px";
    wheel(document.body, 2, { deltaMode: 1 });
    wheel(document.body, -1, { deltaMode: 2 });
    wheel(document.body, 0.05);
    expect(send.mock.calls).toEqual([[40], [-window.innerHeight], [0.05]]);
    document.body.style.lineHeight = "";
  });
  it("reads xterm's virtual scrollback and captures boundary gestures before its consuming handler", () => {
    const send = relay();
    document.body.innerHTML =
      '<div class="xterm-scrollable-element"><div class="scrollbar vertical" style="height:300px"><div class="slider" style="height:100px;top:100px"></div></div><div id="screen"></div></div>';
    const screen = document.getElementById("screen")!;
    const thumb = document.querySelector<HTMLElement>(".slider")!;
    const xterm = vi.fn((event: Event) => {
      event.preventDefault();
      event.stopPropagation();
    });
    screen.addEventListener("wheel", xterm);
    wheel(screen, 40);
    wheel(screen, -40);
    expect(xterm).toHaveBeenCalledTimes(2);
    expect(send).not.toHaveBeenCalled();
    thumb.style.top = "200px";
    wheel(screen, 40);
    thumb.style.top = "0px";
    wheel(screen, -40);
    expect(send.mock.calls).toEqual([[40], [-40]]);
    expect(xterm).toHaveBeenCalledTimes(2);
  });
  it("removes the capture listener on cleanup", () => {
    const send = vi.fn();
    const cleanup = installBoundaryWheelRelay(window, send);
    cleanup();
    wheel(document.body, 40);
    expect(send).not.toHaveBeenCalled();
  });
});

describe("sample report relay", () => {
  it("only injects once into the known nonce-protected fixture", () => {
    const source = reportDocument();
    const injected = withReportScrollBridge(source)!;
    expect(injected).toContain(
      '<script nonce="abcdef012345" data-website-preview-scroll-bridge>',
    );
    expect(withReportScrollBridge(injected)).toBe(injected);
    expect(
      withReportScrollBridge(
        source.replaceAll("data-website-preview-report", "other-report"),
      ),
    ).toBe(source.replaceAll("data-website-preview-report", "other-report"));
    expect(
      withReportScrollBridge("<body data-website-preview-report>"),
    ).toBe("<body data-website-preview-report>");
  });
  it("serializes executable code with no module closure dependencies", () => {
    const injected = withReportScrollBridge(reportDocument())!;
    const script = injected
      .slice(
        injected.lastIndexOf(
          '<script nonce="abcdef012345" data-website-preview-scroll-bridge>',
        ),
      )
      .replace(/^<script[^>]+>/, "")
      .replace(/<\/script>$/, "");
    const send = vi.spyOn(window, "postMessage").mockImplementation(() => {});
    // The actual injected script installs a window handler. Track and remove it after exercising it.
    const add = vi.spyOn(window, "addEventListener");
    window.eval(script);
    const listener = add.mock.calls.find(([type]) => type === "wheel")![1];
    cleanups.push(() => window.removeEventListener("wheel", listener, true));
    wheel(document.body, -27);
    expect(send).toHaveBeenCalledWith(
      { type: "octoboard-preview-report-scroll", deltaY: -27 },
      "*",
    );
  });
  it("leaves independently opened previews and their reports untouched", () => {
    const report = document.createElement("iframe");
    report.srcdoc = reportDocument();
    document.body.innerHTML = '<div class="website-preview-app"></div>';
    document.body.firstElementChild!.appendChild(report);
    cleanups.push(installPreviewScrollBridge(window));
    expect(wheel(document.body, 40).defaultPrevented).toBe(false);
    expect(report.srcdoc).not.toContain("data-website-preview-scroll-bridge");
  });
  it("validates live report sources without rewriting their documents and cleans up", async () => {
    const preview = document.createElement("iframe");
    document.body.appendChild(preview);
    const child = preview.contentWindow!;
    child.document.body.innerHTML = '<div class="website-preview-app"></div>';
    const wrapper = child.document.body.firstElementChild!;
    const send = vi
      .spyOn(child.parent, "postMessage")
      .mockImplementation(() => {});
    const cleanup = installPreviewScrollBridge(child);
    cleanups.push(cleanup);
    const frame = child.document.createElement("iframe");
    frame.srcdoc = withReportScrollBridge(reportDocument());
    wrapper.appendChild(frame);
    await new Promise((resolve) => setTimeout(resolve, 0));
    const injected = frame.srcdoc;
    expect(injected).toContain("data-website-preview-scroll-bridge");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(frame.srcdoc).toBe(injected);
    const message = (source: Window, deltaY: unknown = 10) =>
      child.dispatchEvent(
        new MessageEvent("message", {
          source,
          data: { type: "octoboard-preview-report-scroll", deltaY },
        }),
      );
    message(window);
    message(frame.contentWindow!, Infinity);
    message(frame.contentWindow!, "20");
    expect(send).not.toHaveBeenCalled();
    message(frame.contentWindow!);
    expect(send).toHaveBeenCalledExactlyOnceWith(
      { type: PREVIEW_SCROLL_MESSAGE, deltaY: 10 },
      "*",
    );
    frame.srcdoc = reportDocument();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(frame.srcdoc).not.toContain("data-website-preview-scroll-bridge");
    message(frame.contentWindow!);
    expect(send).toHaveBeenCalledTimes(1);
    const reportWindow = frame.contentWindow!;
    frame.remove();
    message(reportWindow);
    expect(send).toHaveBeenCalledTimes(1);
    cleanup();
    const replacement = child.document.createElement("iframe");
    replacement.srcdoc = reportDocument();
    wrapper.appendChild(replacement);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(replacement.srcdoc).not.toContain(
      "data-website-preview-scroll-bridge",
    );
    message(replacement.contentWindow!);
    expect(send).toHaveBeenCalledTimes(1);
  });
});
