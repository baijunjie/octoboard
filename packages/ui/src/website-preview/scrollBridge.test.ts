// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { composePageDocument } from "../report/pageDocument";
import {
  installBoundaryScrollRelay,
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
function touch(
  type: string,
  target: EventTarget,
  points: [number, number][],
  options: EventInit = {},
  time?: number,
) {
  const event = new Event(type, {
    bubbles: true,
    composed: true,
    cancelable: true,
    ...options,
  });
  Object.defineProperty(event, "touches", {
    value: points.map(([screenX, screenY]) => ({
      clientX: screenX,
      clientY: screenY,
      screenX,
      screenY,
    })),
  });
  if (time !== undefined)
    Object.defineProperty(event, "timeStamp", { value: time });
  target.dispatchEvent(event);
  return event;
}
function relay() {
  const send = vi.fn();
  cleanups.push(installBoundaryScrollRelay(window, send));
  return send;
}
// Touch tests look at scroll deltas and velocity only; stop signals have their own test.
function touchRelay() {
  const send = vi.fn();
  cleanups.push(
    installBoundaryScrollRelay(window, (deltaY, velocity, stop) => {
      if (stop) return;
      if (velocity === undefined) send(deltaY);
      else send(deltaY, velocity);
    }),
  );
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
    const cleanup = installBoundaryScrollRelay(window, send);
    cleanup();
    wheel(document.body, 40);
    expect(send).not.toHaveBeenCalled();
  });
});

describe("preview touch drag handoff", () => {
  it("relays a boundary drag from its start, preventing native handling", () => {
    const send = touchRelay();
    touch("touchstart", document.body, [[10, 300]]);
    expect(
      touch("touchmove", document.body, [[10, 296]]).defaultPrevented,
    ).toBe(false);
    const moved = touch("touchmove", document.body, [[10, 280]]);
    expect(moved.defaultPrevented).toBe(true);
    touch("touchmove", document.body, [[10, 290]]);
    touch("touchend", document.body, []);
    expect(send.mock.calls.filter(([deltaY]) => deltaY !== 0)).toEqual([
      [20],
      [-10],
    ]);
  });
  it("leaves a drag to the inner scroller and keeps that verdict after it reaches a boundary", () => {
    const send = touchRelay();
    const list = scroller(document.body, 100);
    touch("touchstart", list, [[10, 300]]);
    expect(touch("touchmove", list, [[10, 280]]).defaultPrevented).toBe(false);
    list.scrollTop = 200;
    expect(touch("touchmove", list, [[10, 250]]).defaultPrevented).toBe(false);
    touch("touchend", list, []);
    expect(send).not.toHaveBeenCalled();
    // The reverse direction has room in the same scroller, so it is native again.
    touch("touchstart", list, [[10, 300]]);
    expect(touch("touchmove", list, [[10, 320]]).defaultPrevented).toBe(false);
  });
  it("keeps relaying after an inner scroller gains room mid-gesture", () => {
    const send = touchRelay();
    const list = scroller(document.body, 0);
    touch("touchstart", list, [[10, 300]]);
    touch("touchmove", list, [[10, 320]]);
    list.scrollTop = 100;
    touch("touchmove", list, [[10, 340]]);
    expect(send.mock.calls).toEqual([[-20], [-20]]);
  });
  it("ignores mostly horizontal drags, multi-touch and non-cancelable moves", () => {
    const send = touchRelay();
    touch("touchstart", document.body, [[10, 300]]);
    touch("touchmove", document.body, [[40, 290]]);
    touch("touchmove", document.body, [[40, 200]]);
    touch("touchend", document.body, []);
    touch("touchstart", document.body, [[10, 300]]);
    touch("touchstart", document.body, [
      [10, 300],
      [50, 300],
    ]);
    touch("touchmove", document.body, [[10, 200]]);
    touch("touchend", document.body, []);
    touch("touchstart", document.body, [[10, 300]]);
    touch("touchmove", document.body, [[10, 200]], { cancelable: false });
    touch("touchmove", document.body, [[10, 100]]);
    expect(send).not.toHaveBeenCalled();
  });
  it("measures drags in screen coordinates, which do not move with the frame", () => {
    const send = touchRelay();
    touch("touchstart", document.body, [[10, 300]]);
    touch("touchmove", document.body, [[10, 290]]);
    // The frame scrolls under the finger: clientY jumps back while the finger steadily advances.
    for (const screenY of [280, 270, 260]) {
      const event = new Event("touchmove", { bubbles: true, cancelable: true });
      Object.defineProperty(event, "touches", {
        value: [{ clientX: 10, clientY: 500, screenX: 10, screenY }],
      });
      document.body.dispatchEvent(event);
    }
    expect(send.mock.calls).toEqual([[10], [10], [10], [10]]);
  });
  it("posts the release velocity from the last 100ms of a relayed drag", () => {
    const send = touchRelay();
    touch("touchstart", document.body, [[10, 300]], {}, 0);
    touch("touchmove", document.body, [[10, 280]], {}, 0);
    touch("touchmove", document.body, [[10, 260]], {}, 50);
    touch("touchmove", document.body, [[10, 240]], {}, 100);
    touch("touchend", document.body, [], {}, 110);
    expect(send.mock.calls.at(-1)).toEqual([0, 0.4]);
    // Older samples fall out of the window.
    send.mockClear();
    touch("touchstart", document.body, [[10, 300]], {}, 0);
    touch("touchmove", document.body, [[10, 200]], {}, 0);
    touch("touchmove", document.body, [[10, 190]], {}, 150);
    touch("touchmove", document.body, [[10, 180]], {}, 200);
    touch("touchend", document.body, [], {}, 210);
    expect(send.mock.calls.at(-1)).toEqual([0, 0.2]);
  });
  it("posts no velocity after a pause, a cancel or a native drag", () => {
    const send = touchRelay();
    touch("touchstart", document.body, [[10, 300]], {}, 0);
    touch("touchmove", document.body, [[10, 280]], {}, 0);
    touch("touchmove", document.body, [[10, 260]], {}, 50);
    send.mockClear();
    touch("touchend", document.body, [], {}, 400);
    touch("touchstart", document.body, [[10, 300]], {}, 0);
    touch("touchmove", document.body, [[10, 280]], {}, 0);
    touch("touchmove", document.body, [[10, 260]], {}, 50);
    touch("touchcancel", document.body, [], {}, 60);
    const list = scroller(document.body, 100);
    touch("touchstart", list, [[10, 300]], {}, 0);
    touch("touchmove", list, [[10, 280]], {}, 0);
    touch("touchmove", list, [[10, 260]], {}, 50);
    touch("touchend", list, [], {}, 60);
    expect(send.mock.calls.filter(([deltaY]) => deltaY === 0)).toEqual([]);
  });
  it("leaves drags on elements that opt out of vertical panning", () => {
    const send = touchRelay();
    const handle = document.createElement("div");
    const pannable = document.createElement("div");
    document.body.append(handle, pannable);
    // jsdom does not compute touch-action, so the test supplies it through a data attribute.
    const getComputedStyle = window.getComputedStyle.bind(window);
    vi.spyOn(window, "getComputedStyle").mockImplementation((element) => {
      const style = getComputedStyle(element);
      const touchAction = (element as HTMLElement).dataset?.touchAction;
      return touchAction ? ({ ...style, touchAction, overflowY: "visible" } as CSSStyleDeclaration) : style;
    });
    handle.dataset.touchAction = "none";
    pannable.dataset.touchAction = "pan-y";
    touch("touchstart", handle, [[10, 300]]);
    touch("touchmove", handle, [[10, 280]]);
    touch("touchend", handle, []);
    expect(send).not.toHaveBeenCalled();
    touch("touchstart", pannable, [[10, 300]]);
    touch("touchmove", pannable, [[10, 280]]);
    expect(send).toHaveBeenCalledExactlyOnceWith(20);
  });
  it("signals a stop on every touchstart, including multi-touch", () => {
    const send = vi.fn();
    cleanups.push(installBoundaryScrollRelay(window, send));
    touch("touchstart", document.body, [[10, 300]]);
    touch("touchstart", document.body, [
      [10, 300],
      [50, 300],
    ]);
    expect(send.mock.calls).toEqual([
      [0, undefined, true],
      [0, undefined, true],
    ]);
  });
  it("removes the touch listeners on cleanup", () => {
    const send = vi.fn();
    installBoundaryScrollRelay(window, send)();
    touch("touchstart", document.body, [[10, 300]]);
    touch("touchmove", document.body, [[10, 200]]);
    expect(send).not.toHaveBeenCalled();
  });
});

// A preview window with the bridge installed and one injected report iframe inside it.
async function liveReport() {
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
  return { child, wrapper, frame, send, cleanup };
}

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
    expect(withReportScrollBridge("<body data-website-preview-report>")).toBe(
      "<body data-website-preview-report>",
    );
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
    send.mockClear();
    for (const type of [
      "touchstart",
      "touchmove",
      "touchend",
      "touchcancel",
    ] as const) {
      const touchListener = add.mock.calls.find(([name]) => name === type)![1];
      cleanups.push(() =>
        window.removeEventListener(type, touchListener, true),
      );
    }
    touch("touchstart", document.body, [[10, 300]]);
    touch("touchmove", document.body, [[10, 270]]);
    expect(send).toHaveBeenCalledWith(
      { type: "octoboard-preview-report-scroll", deltaY: 30 },
      "*",
    );
  });
  it("forwards a report's release velocity and rejects invalid ones", async () => {
    const { child, frame, send } = await liveReport();
    const message = (deltaY: unknown, velocity?: unknown) =>
      child.dispatchEvent(
        new MessageEvent("message", {
          source: frame.contentWindow,
          data: { type: "octoboard-preview-report-scroll", deltaY, velocity },
        }),
      );
    message(0);
    message(0, Infinity);
    message(0, "1");
    expect(send).not.toHaveBeenCalled();
    message(0, 0.5);
    expect(send).toHaveBeenCalledExactlyOnceWith(
      { type: PREVIEW_SCROLL_MESSAGE, deltaY: 0, velocity: 0.5 },
      "*",
    );
    send.mockClear();
    const stopMessage = (stop: unknown) =>
      child.dispatchEvent(
        new MessageEvent("message", {
          source: frame.contentWindow,
          data: { type: "octoboard-preview-report-scroll", deltaY: 0, stop },
        }),
      );
    stopMessage(false);
    stopMessage("true");
    stopMessage(1);
    expect(send).not.toHaveBeenCalled();
    stopMessage(true);
    expect(send).toHaveBeenCalledExactlyOnceWith(
      { type: PREVIEW_SCROLL_MESSAGE, deltaY: 0, stop: true },
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
    const { child, frame, send, wrapper, cleanup } = await liveReport();
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
