/** The website accepts this only from its own preview iframe's contentWindow. */
export const PREVIEW_SCROLL_MESSAGE = "octoboard-preview-scroll";
const REPORT_SCROLL_MESSAGE = "octoboard-preview-report-scroll";
const REPORT_MARKER = "data-website-preview-report";
const INJECTED_MARKER = "data-website-preview-scroll-bridge";

/** Kept self-contained because the same function is serialized into our sample report's opaque
 * sandbox. It depends only on its arguments and browser APIs, never application internals. */
export function installBoundaryWheelRelay(
  win: Window,
  send: (deltaY: number) => void,
): () => void {
  const doc = win.document;
  const epsilon = 0.5;
  const hasRoom = (position: number, maximum: number, delta: number) =>
    maximum > epsilon &&
    (delta > 0 ? maximum - position > epsilon : position > epsilon);

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
    const path = event
      .composedPath()
      .filter((item): item is Element => (item as Node).nodeType === 1);
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
          if (size > 0 && hasRoom(top, height - size, delta)) return;
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
        return;
    }

    event.preventDefault();
    // Capture runs before xterm's own handler, which otherwise consumes the boundary gesture.
    event.stopImmediatePropagation();
    send(delta);
  };
  win.addEventListener("wheel", onWheel, { capture: true, passive: false });
  return () => win.removeEventListener("wheel", onWheel, true);
}

/** Add a relay only to our own report fixture, after the product has sanitized it and composed its
 * nonce-protected srcdoc. The report's normal sanitizer deliberately removes author scripts, so
 * putting a script in the fixture HTML would never run. This leaves product rendering unchanged. */
export function withReportScrollBridge(srcdoc: string): string | undefined {
  if (
    !srcdoc.includes(REPORT_MARKER) ||
    !srcdoc.includes("window.OCTOBOARD_PAGE") ||
    srcdoc.includes(INJECTED_MARKER)
  )
    return;
  const nonce = /<script nonce="([a-f0-9]+)">/.exec(srcdoc)?.[1];
  if (!nonce) return;
  const source = `(${installBoundaryWheelRelay.toString()})(window, function(deltaY) { parent.postMessage({ type: "${REPORT_SCROLL_MESSAGE}", deltaY: deltaY }, "*"); });`;
  return `${srcdoc}<script nonce="${nonce}" ${INJECTED_MARKER}>${source}</script>`;
}

/** Website-only boundary handoff. An independently opened preview keeps ordinary application
 * scrolling and never cancels wheel events or modifies the report's document. */
export function installPreviewScrollBridge(win: Window = window): () => void {
  if (win.parent === win) return () => {};
  const doc = win.document;
  const send = (deltaY: number) =>
    win.parent.postMessage({ type: PREVIEW_SCROLL_MESSAGE, deltaY }, "*");
  const removeWheel = installBoundaryWheelRelay(win, send);
  const reports = new WeakSet<HTMLIFrameElement>();
  const injectReports = () => {
    for (const frame of doc.querySelectorAll<HTMLIFrameElement>(
      ".website-preview-app iframe[srcdoc]",
    )) {
      const source = withReportScrollBridge(frame.srcdoc);
      if (source === undefined) continue;
      reports.add(frame);
      frame.srcdoc = source;
    }
  };
  const onMessage = (event: MessageEvent) => {
    const data = event.data as { type?: unknown; deltaY?: unknown } | null;
    if (
      data?.type !== REPORT_SCROLL_MESSAGE ||
      typeof data.deltaY !== "number" ||
      !Number.isFinite(data.deltaY) ||
      data.deltaY === 0
    )
      return;
    const ownReport = [
      ...doc.querySelectorAll<HTMLIFrameElement>(
        ".website-preview-app iframe[srcdoc]",
      ),
    ].some(
      (frame) => reports.has(frame) && event.source === frame.contentWindow,
    );
    if (ownReport) send(data.deltaY);
  };
  // Attribute observation also covers a new srcdoc on a reused report frame. The marker prevents
  // our own assignment from recursively reloading it; replacement frames get their own nonce.
  const observer = new MutationObserver(injectReports);
  observer.observe(doc.documentElement, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["srcdoc"],
  });
  injectReports();
  win.addEventListener("message", onMessage);
  return () => {
    removeWheel();
    observer.disconnect();
    win.removeEventListener("message", onMessage);
  };
}
