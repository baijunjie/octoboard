import { isReportBridgeSource, withReportBridge } from "./reportBridge";

/** The website accepts this only from its own preview iframe's contentWindow. */
export const PREVIEW_SCROLL_MESSAGE = "octoboard-preview-scroll";
const REPORT_SCROLL_MESSAGE = "octoboard-preview-report-scroll";
const INJECTED_MARKER = "data-website-preview-scroll-bridge";

/** Kept self-contained because the same function is serialized into our sample report's opaque
 * sandbox. It depends only on its arguments and browser APIs, never application internals. */
export function installBoundaryScrollRelay(
  win: Window,
  send: (deltaY: number, velocity?: number, stop?: boolean) => void,
): () => void {
  const doc = win.document;
  const epsilon = 0.5;
  const hasRoom = (position: number, maximum: number, delta: number) =>
    maximum > epsilon &&
    (delta > 0 ? maximum - position > epsilon : position > epsilon);

  const elementPath = (event: Event) =>
    event
      .composedPath()
      .filter((item): item is Element => (item as Node).nodeType === 1);

  /** True when a scroller on the path, up to and including the document's, can still move in this direction. */
  const innerScrollerHasRoom = (path: Element[], delta: number) => {
    // composedPath retains the diff renderer's shadow-root scrollers. Looking only at target or
    // parentElement would miss them and hand a scrollable code view's gesture to the website.
    const scrollingElement = doc.scrollingElement ?? doc.documentElement;
    if (!path.includes(scrollingElement)) path.push(scrollingElement);
    for (const element of path) {
      // xterm 6 uses a virtual viewport: DOM scrollHeight does not describe its scrollback.
      // Its rendered vertical thumb is the observable scroll position, including while faded.
      if (element.matches(".xterm-scrollable-element")) {
        const track = element.querySelector<HTMLElement>(
          ":scope > .scrollbar.vertical",
        );
        const thumb = track?.querySelector<HTMLElement>(":scope > .slider");
        if (track && thumb) {
          const height = parseFloat(track.style.height);
          const size = parseFloat(thumb.style.height);
          const top = parseFloat(thumb.style.top);
          if (size > 0 && hasRoom(top, height - size, delta)) return true;
        }
      }
      const overflow = win.getComputedStyle(element).overflowY;
      const nativeScroller =
        /^(auto|scroll|overlay)$/.test(overflow) ||
        (element === scrollingElement &&
          overflow !== "hidden" &&
          overflow !== "clip");
      if (
        nativeScroller &&
        hasRoom(
          element.scrollTop,
          element.scrollHeight - element.clientHeight,
          delta,
        )
      )
        return true;
    }
    return false;
  };

  // Elements that opt out of vertical panning (touch-action: none, as on drag handles) handle the
  // touch themselves, so the page must not scroll along with them.
  const allowsVerticalPan = (element: Element) => {
    const action = win.getComputedStyle(element).touchAction;
    return (
      !action ||
      action === "auto" ||
      action === "manipulation" ||
      action.includes("pan-y")
    );
  };

  const onWheel = (event: WheelEvent) => {
    // A non-cancelable event may already be scrolling natively. Relaying it would scroll twice.
    // Shift-wheel is commonly horizontal even when the device reports only deltaY.
    if (
      !event.cancelable ||
      event.defaultPrevented ||
      event.ctrlKey ||
      event.metaKey ||
      event.shiftKey
    )
      return;
    if (
      !Number.isFinite(event.deltaY) ||
      !Number.isFinite(event.deltaX) ||
      Math.abs(event.deltaY) <= Math.abs(event.deltaX)
    )
      return;
    const path = elementPath(event);
    const target = path[0] ?? doc.documentElement;
    const style = win.getComputedStyle(target);
    const lineHeight =
      parseFloat(style.lineHeight) || (parseFloat(style.fontSize) || 16) * 1.2;
    const unit =
      event.deltaMode === 1
        ? lineHeight
        : event.deltaMode === 2
          ? win.innerHeight
          : 1;
    const delta = event.deltaY * unit;
    if (!Number.isFinite(delta) || delta === 0) return;

    if (innerScrollerHasRoom(path, delta)) return;

    event.preventDefault();
    // Capture runs before xterm's own handler, which otherwise consumes the boundary gesture.
    event.stopImmediatePropagation();
    send(delta);
  };
  win.addEventListener("wheel", onWheel, { capture: true, passive: false });

  // A drag is decided once, on its first significant vertical movement, and keeps that verdict:
  // native scrolling that hits a boundary mid-gesture must not start scrolling the page as well.
  // Positions are screen coordinates: every relayed scroll moves this frame under the finger, so
  // client coordinates would feed that movement back into the next delta.
  const slop = 8;
  const velocityWindow = 100;
  let drag: {
    path: Element[];
    startX: number;
    startY: number;
    lastY: number;
    mode: "pending" | "native" | "relay";
    samples: { time: number; y: number }[];
  } | null = null;
  const onTouchStart = (event: TouchEvent) => {
    // A finger landing stops any glide the page is still doing from the previous drag.
    send(0, undefined, true);
    const touch = event.touches[0];
    drag =
      event.touches.length === 1 && touch
        ? {
          path: elementPath(event),
          startX: touch.screenX,
          startY: touch.screenY,
          lastY: touch.screenY,
          mode: "pending",
          samples: [],
        }
        : null;
  };
  const onTouchMove = (event: TouchEvent) => {
    const touch = event.touches[0];
    if (!drag || event.touches.length !== 1 || !touch) {
      // A second finger turns the gesture into a pinch for the rest of its life.
      drag = null;
      return;
    }
    const { screenX, screenY } = touch;
    if (!Number.isFinite(screenX) || !Number.isFinite(screenY)) return;
    if (drag.mode === "pending") {
      const dx = screenX - drag.startX;
      const dy = screenY - drag.startY;
      if (Math.max(Math.abs(dx), Math.abs(dy)) < slop) return;
      const path = drag.path;
      // Content follows the finger, so an upward drag (negative dy) scrolls down like a wheel.
      drag.mode =
        Math.abs(dy) > Math.abs(dx) &&
        event.cancelable &&
        !event.defaultPrevented &&
        path.every(allowsVerticalPan) &&
        !innerScrollerHasRoom(path, -dy)
          ? "relay"
          : "native";
      // Relay what was travelled while undecided, so the page does not lag behind the finger.
      if (drag.mode === "relay") drag.lastY = drag.startY;
    }
    if (drag.mode !== "relay") return;
    const delta = drag.lastY - screenY;
    drag.lastY = screenY;
    drag.samples.push({ time: event.timeStamp, y: screenY });
    if (event.cancelable) event.preventDefault();
    event.stopImmediatePropagation();
    if (delta !== 0) send(delta);
  };
  const onTouchEnd = (event: TouchEvent) => {
    if (event.touches.length !== 0) return;
    const ended = drag;
    drag = null;
    // Hand the release velocity (px/ms, scroll direction) over so the page can keep gliding.
    // A finger that paused before lifting has none.
    if (!ended || ended.mode !== "relay" || event.type !== "touchend") return;
    const last = ended.samples[ended.samples.length - 1];
    if (!last || event.timeStamp - last.time > velocityWindow) return;
    const first = ended.samples.find(
      (sample) => last.time - sample.time <= velocityWindow,
    )!;
    const velocity = (first.y - last.y) / (last.time - first.time);
    if (Number.isFinite(velocity) && velocity !== 0) send(0, velocity);
  };
  const touchOptions = { capture: true, passive: false };
  win.addEventListener("touchstart", onTouchStart, touchOptions);
  win.addEventListener("touchmove", onTouchMove, touchOptions);
  win.addEventListener("touchend", onTouchEnd, true);
  win.addEventListener("touchcancel", onTouchEnd, true);
  return () => {
    win.removeEventListener("wheel", onWheel, true);
    win.removeEventListener("touchstart", onTouchStart, true);
    win.removeEventListener("touchmove", onTouchMove, true);
    win.removeEventListener("touchend", onTouchEnd, true);
    win.removeEventListener("touchcancel", onTouchEnd, true);
  };
}

/** Add a relay only to our own report fixture, after the product has composed its nonce-protected
 * srcdoc. The report's normal sanitizer deliberately removes author scripts, so
 * putting a script in the fixture HTML would never run. This leaves product rendering unchanged. */
export function withReportScrollBridge(srcdoc: string): string {
  const source = `(${installBoundaryScrollRelay.toString()})(window, function(deltaY, velocity, stop) { parent.postMessage({ type: "${REPORT_SCROLL_MESSAGE}", deltaY: deltaY, velocity: velocity, stop: stop }, "*"); });`;
  return withReportBridge(srcdoc, INJECTED_MARKER, source);
}

/** Website-only boundary handoff. An independently opened preview keeps ordinary application
 * scrolling and never cancels wheel events or modifies the report's document. */
export function installPreviewScrollBridge(win: Window = window): () => void {
  if (win.parent === win) return () => {};
  const doc = win.document;
  const send = (deltaY: number, velocity?: number, stop?: boolean) =>
    win.parent.postMessage(
      { type: PREVIEW_SCROLL_MESSAGE, deltaY, velocity, stop },
      "*",
    );
  const removeRelay = installBoundaryScrollRelay(win, send);
  const onMessage = (event: MessageEvent) => {
    const data = event.data as {
      type?: unknown;
      deltaY?: unknown;
      velocity?: unknown;
      stop?: unknown;
    } | null;
    if (
      data?.type !== REPORT_SCROLL_MESSAGE ||
      typeof data.deltaY !== "number" ||
      !Number.isFinite(data.deltaY)
    )
      return;
    // A message carries a scroll delta, a release velocity, or a stop signal.
    const stop = data.stop === true;
    const velocity =
      typeof data.velocity === "number" && Number.isFinite(data.velocity)
        ? data.velocity
        : undefined;
    if (data.deltaY === 0 && !velocity && !stop) return;
    if (isReportBridgeSource(doc, event.source, INJECTED_MARKER))
      send(data.deltaY, velocity, stop || undefined);
  };
  win.addEventListener("message", onMessage);
  return () => {
    removeRelay();
    win.removeEventListener("message", onMessage);
  };
}
