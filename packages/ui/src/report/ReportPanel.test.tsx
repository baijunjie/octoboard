// @vitest-environment jsdom
import { act, StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { pageOf } from "../gallery/fixtures/builders";
import { PanelFrameProvider } from "../components/PanelHeader";
import { focusLanding, focusLandingWithoutText } from "../components/regionFocus";
import { PlatformProvider } from "../platform/react";
import { applyDaemonEvent, createStateStore, DaemonProvider, type Daemon } from "../store";
import { withReportScrollBridge } from "../website-preview/scrollBridge";
import { PageDocumentTransform } from "./pageDocumentTransform";
import { ReportPanel } from "./ReportPanel";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => {
  const modality = getInteractionModality();
  onTestFinished(() => act(() => Reflect.apply(setInteractionModality, undefined, [modality])));
});

it("composes the embedded report relay before its only srcdoc assignment, including on page replacement", () => {
  const attributes = vi.spyOn(Element.prototype, "setAttribute");
  const documents = () => attributes.mock.calls.filter(([name]) => name.toLowerCase() === "srcdoc").map(([, value]) => value);
  const store = createStateStore({
    pages: new Map([["console-session", [pageOf("first", "console-session", "<body data-website-preview-report>First</body>", 0)]]]),
  });
  const daemon: Daemon = {
    store, request: vi.fn(), toastError: vi.fn(), toastNotice: vi.fn(),
    onToast: () => () => {}, dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    vi.restoreAllMocks();
  });
  act(() => root.render(
    <StrictMode>
      <PlatformProvider value={{ kind: "browser" }}>
        <PageDocumentTransform value={withReportScrollBridge}>
          <DaemonProvider value={daemon}>
            <ReportPanel consoleSessionId="console-session" peek={undefined}
              onEscape={() => {}} onCycleRegion={() => {}} onMoveHistory={() => {}} onMoveConsoleSession={() => {}} />
          </DaemonProvider>
        </PageDocumentTransform>
      </PlatformProvider>
    </StrictMode>,
  ));
  expect(documents()).toHaveLength(1);
  expect(documents()[0]).toContain("data-website-preview-scroll-bridge");
  const firstFrame = container.querySelector("iframe");
  act(() => store.setState({ pages: new Map([["console-session", [pageOf("second", "console-session", "<body data-website-preview-report>Second</body>", 1)]]]) }));
  expect(container.querySelector("iframe")).not.toBe(firstFrame);
  expect(documents()).toHaveLength(2);
  expect(documents()[1]).toContain("data-website-preview-scroll-bridge");
});

function renderReport(pageCount = 2) {
  const pages = Array.from({ length: pageCount }, (_, i) => pageOf(`page-${i}`, "console-session", `<p>Page ${i}</p>`, i));
  const store = createStateStore({ pages: new Map([["console-session", pages]]) });
  const daemon: Daemon = {
    store, request: vi.fn(), toastError: vi.fn(), toastNotice: vi.fn(),
    onToast: () => () => {}, dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("section"));
  container.dataset.region = "panel-report";
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(
    <StrictMode>
      <PlatformProvider value={{ kind: "browser" }}>
        <DaemonProvider value={daemon}>
          <PanelFrameProvider value={{ canMaximize: false, maximized: false, onToggleMaximized: () => {}, onClose: () => {} }}>
            <ReportPanel consoleSessionId="console-session" peek={undefined}
              onEscape={() => {}} onCycleRegion={() => {}} onMoveHistory={() => {}} onMoveConsoleSession={() => {}} />
          </PanelFrameProvider>
        </DaemonProvider>
      </PlatformProvider>
    </StrictMode>,
  ));
  return {
    container,
    append: (session = "console-session") => act(async () => {
      applyDaemonEvent(store, {
        type: "page_created", page: pageOf("new-page", session, "<p>New page</p>", 10),
      });
    }),
  };
}

it.each([1, 2])("hands a focused frame to a surviving report control with keyboard modality (%i prior pages)", async (count) => {
  const { container, append } = renderReport(count);
  const oldFrame = container.querySelector("iframe")!;
  act(() => {
    setInteractionModality("pointer");
    oldFrame.focus();
  });
  expect(document.activeElement).toBe(oldFrame);
  await append();
  expect(oldFrame.isConnected).toBe(false);
  expect(container.querySelector("iframe")).not.toBe(oldFrame);
  const active = document.activeElement as HTMLButtonElement;
  expect(active.tagName).toBe("BUTTON");
  expect(active.disabled).toBe(false);
  expect(container.contains(active)).toBe(true);
  expect(active.getAttribute("data-focus-visible")).toBe("true");
});

it("leaves focus on a surviving pager or outside control, including a modal", async () => {
  for (const location of ["pager", "elsewhere", "modal"] as const) {
    const { container, append } = renderReport();
    const holder = document.body.appendChild(document.createElement("div"));
    if (location === "modal") {
      holder.setAttribute("role", "dialog");
      holder.setAttribute("aria-modal", "true");
    }
    const outside = holder.appendChild(document.createElement("button"));
    onTestFinished(() => holder.remove());
    const target = location === "pager" ? container.querySelector("button")! : outside;
    act(() => target.focus());
    await append();
    expect(document.activeElement).toBe(target);
  }
});

it("ignores another session's new page without replacing the current frame or moving focus", async () => {
  const { container, append } = renderReport();
  const frame = container.querySelector("iframe")!;
  act(() => frame.focus());
  await append("other-session");
  expect(container.querySelector("iframe")).toBe(frame);
  expect(document.activeElement).toBe(frame);
});

it("lands a tap's focus on the page, and F6's or a pointer's on the header's first enabled control", () => {
  const { container } = renderReport();
  expect(focusLandingWithoutText(container)).toBe(true);
  expect(document.activeElement).toBe(container.querySelector("iframe"));
  expect(focusLanding(container)).toBe(true);
  expect(document.activeElement?.tagName).toBe("BUTTON");
  expect(container.contains(document.activeElement)).toBe(true);
});
