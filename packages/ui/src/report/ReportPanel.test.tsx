// @vitest-environment jsdom
import { act, StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { pageOf } from "../gallery/fixtures/builders";
import { PlatformProvider } from "../platform/react";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { withReportScrollBridge } from "../website-preview/scrollBridge";
import { PageDocumentTransform } from "./pageDocumentTransform";
import { ReportPanel } from "./ReportPanel";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

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
