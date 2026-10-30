// @vitest-environment jsdom
import { act, StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { pageOf } from "../gallery/fixtures/builders";
import { press } from "../components/detailsTestSupport";
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
            <ReportPanel sessionId="console-session" peek={undefined}
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
            <ReportPanel sessionId="console-session" peek={undefined}
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
    remove: (...ids: string[]) => act(async () => {
      applyDaemonEvent(store, { type: "pages_deleted", session_id: "console-session", pages: ids });
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

it("shows a failed page listing in the panel with its reason and Try again, and opens no error alert dialog", async () => {
  const request = vi.fn().mockRejectedValueOnce(new Error("Daemon busy")).mockResolvedValue({ type: "ack", id: null });
  const toastError = vi.fn();
  const store = createStateStore({ connectionState: "open" });
  const daemon: Daemon = {
    store, request, toastError, toastNotice: vi.fn(),
    onToast: () => () => {}, dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("section"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(
    <PlatformProvider value={{ kind: "browser" }}>
      <DaemonProvider value={daemon}>
        <PanelFrameProvider value={{ canMaximize: false, maximized: false, onToggleMaximized: () => {}, onClose: () => {} }}>
          <ReportPanel sessionId="console-session" peek={undefined}
            onEscape={() => {}} onCycleRegion={() => {}} onMoveHistory={() => {}} onMoveConsoleSession={() => {}} />
        </PanelFrameProvider>
      </DaemonProvider>
    </PlatformProvider>,
  ));
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(panel.querySelector("h2")?.textContent).toBe("Could not load the pages of this session");
  expect(panel.textContent).toContain("Something unexpected went wrong.");
  expect(container.querySelector("[role=alert]")).not.toBeNull();
  expect(toastError).not.toHaveBeenCalled();
  const retry = [...panel.querySelectorAll("button")].find((button) => button.textContent === "Try again")!;
  await act(async () => retry.click());
  expect(request).toHaveBeenCalledTimes(2);
  expect(container.querySelector('[data-slot="empty-state"]')).toBeNull();
});

const MARK = 'button[aria-label="Could not load the pages of this session"]';

/** The panel with two pages in hand, whose first listing is answered by `request`'s first reply. */
async function renderListed(request: Daemon["request"]) {
  const pages = [pageOf("page-0", "console-session", "<p>Page 0</p>", 0), pageOf("page-1", "console-session", "<p>Page 1</p>", 1)];
  return renderWith(request, createStateStore({ connectionState: "open", pages: new Map([["console-session", pages]]) }));
}

async function renderWith(request: Daemon["request"], store: ReturnType<typeof createStateStore>) {
  const daemon: Daemon = {
    store, request, toastError: vi.fn(), toastNotice: vi.fn(),
    onToast: () => () => {}, dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("section"));
  container.dataset.region = "panel-report";
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(
    <PlatformProvider value={{ kind: "browser" }}>
      <DaemonProvider value={daemon}>
        <PanelFrameProvider value={{ canMaximize: false, maximized: false, onToggleMaximized: () => {}, onClose: () => {} }}>
          <ReportPanel sessionId="console-session" peek={undefined}
            onEscape={() => {}} onCycleRegion={() => {}} onMoveHistory={() => {}} onMoveConsoleSession={() => {}} />
        </PanelFrameProvider>
      </DaemonProvider>
    </PlatformProvider>,
  ));
  return container;
}

const popoverRetry = () => [...document.querySelectorAll('[role="dialog"] button')].find((button) => button.textContent === "Try again") as HTMLButtonElement;
const announcement = (container: HTMLElement) => container.querySelector('[role="status"].sr-only')?.textContent ?? "";

it("marks a failed refresh of the listed pages at the panel title with its reason and Try again in the popover, with no Alert or loose text", async () => {
  const request = vi.fn().mockRejectedValueOnce(new Error("Daemon busy")).mockResolvedValue({ type: "ack", id: null });
  const container = await renderListed(request);
  const mark = container.querySelector<HTMLButtonElement>(MARK)!;
  expect(mark).not.toBeNull();
  expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(container.querySelector("[role=alert]")).toBeNull();
  expect(container.textContent).not.toContain("Something unexpected went wrong.");
  const heading = container.querySelector("h2")!;
  expect(heading.textContent).toBe("Report");
  expect(heading.compareDocumentPosition(mark) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(mark.closest("h2")).toBeNull();
  press(mark);
  const dialog = document.querySelector('[role="dialog"]')!;
  expect(dialog.textContent).toContain("Something unexpected went wrong.");
  expect(popoverRetry()).toBeDefined();
  await act(async () => popoverRetry().click());
  expect(request).toHaveBeenCalledTimes(2);
  expect(container.querySelector(MARK)).toBeNull();
});

it("says a failed refresh of the listed pages once through the panel's status region", async () => {
  const request = vi.fn().mockRejectedValueOnce(new Error("Daemon busy")).mockResolvedValue({ type: "ack", id: null });
  const container = await renderListed(request);
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(announcement(container)).toBe("Could not load the pages of this session");
  expect(container.querySelectorAll('[role="status"]')).toHaveLength(1);
});

it("does not say the listing failure again when pages arrive after the whole card announced it, and says a new failure", async () => {
  const request = vi.fn().mockRejectedValueOnce(new Error("Daemon busy")).mockRejectedValueOnce(new Error("Daemon busy"));
  const store = createStateStore({ connectionState: "open" });
  const container = await renderWith(request, store);
  expect(container.querySelector('[data-slot="empty-state"]')).not.toBeNull();
  expect(container.querySelector(MARK)).toBeNull();
  await act(async () => store.setState({ pages: new Map([["console-session", [pageOf("page-0", "console-session", "<p>Page 0</p>", 0)]]]) }));
  expect(container.querySelector(MARK)).not.toBeNull();
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(announcement(container)).toBe("");
  // A failure after that, with the pages in hand, is a new one.
  await act(async () => store.setState({ snapshotEpoch: store.getState().snapshotEpoch + 1 }));
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(announcement(container)).toBe("Could not load the pages of this session");
});

it("hands focus on from the mark to a header control when the listing succeeds and the mark goes", async () => {
  let answer!: (event: { type: "ack"; id: null }) => void;
  const request = vi
    .fn()
    .mockRejectedValueOnce(new Error("Daemon busy"))
    .mockReturnValueOnce(new Promise((resolve) => void (answer = resolve)));
  const container = await renderListed(request);
  const mark = container.querySelector<HTMLButtonElement>(MARK)!;
  press(mark);
  act(() => popoverRetry().focus());
  await act(async () => popoverRetry().click());
  // The mark stays while the listing is out, and focus is on it when the listing answers.
  await act(() => new Promise((resolve) => setTimeout(resolve, 100)));
  act(() => mark.focus());
  expect(document.activeElement).toBe(mark);
  await act(async () => answer({ type: "ack", id: null }));
  expect(container.querySelector(MARK)).toBeNull();
  expect(document.activeElement).not.toBe(document.body);
  expect(container.contains(document.activeElement)).toBe(true);
  expect(document.activeElement?.tagName).toBe("BUTTON");
});

it("keeps focus on the header's button when the first listing arrives and the pages replace the bare header", async () => {
  const store = createStateStore({ connectionState: "open" });
  const container = await renderWith(vi.fn().mockReturnValue(new Promise(() => {})), store);
  const button = container.querySelector("button")!;
  act(() => button.focus());
  await act(async () => {
    applyDaemonEvent(store, { type: "page_created", page: pageOf("page-0", "console-session", "<p>Page 0</p>", 0) });
  });
  expect(button.isConnected).toBe(true);
  expect(document.activeElement).toBe(button);
});

it("keeps the mark while Try again's listing is out and across a second failure, which is not said again", async () => {
  let finish!: (error: Error) => void;
  const request = vi
    .fn()
    .mockRejectedValueOnce(new Error("Daemon busy"))
    .mockReturnValueOnce(new Promise((_, reject) => void (finish = reject)));
  const container = await renderListed(request);
  const mark = container.querySelector<HTMLButtonElement>(MARK)!;
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  press(mark);
  await act(async () => popoverRetry().click());
  expect(request).toHaveBeenCalledTimes(2);
  expect(container.querySelector(MARK)).toBe(mark);
  await act(async () => finish(new Error("Daemon busy")));
  expect(container.querySelector(MARK)).toBe(mark);
  expect(container.querySelector("[role=alert]")).toBeNull();
  expect(announcement(container)).toBe("Could not load the pages of this session");
});

const position = (container: HTMLElement) => container.querySelector("[dir=ltr]")?.textContent;

it("adjusts the pager when pages are deleted: the total shrinks, and the page paged to going falls back to the newest", async () => {
  const { container, remove } = renderReport(3);
  expect(position(container)).toBe("3 / 3");
  const previous = () => container.querySelector<HTMLButtonElement>('[aria-label="Previous page"]')!;
  await act(async () => previous().click());
  await act(async () => previous().click());
  expect(position(container)).toBe("1 / 3");

  // A page other than the one on screen goes: the view stays where it is.
  await remove("page-2");
  expect(position(container)).toBe("1 / 2");
  expect(container.textContent).toContain("Read-only");

  // The page on screen goes: the view follows the newest, which is not history.
  await remove("page-0");
  expect(position(container)).toBe("1 / 1");
  expect(container.textContent).not.toContain("Read-only");
});

it("shows the empty panel once every page is deleted", async () => {
  const { container, remove } = renderReport(2);
  await remove("page-0", "page-1");
  expect(container.querySelector("iframe")).toBeNull();
  expect(container.textContent).toContain("No pages yet");
});
