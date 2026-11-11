// @vitest-environment jsdom
import { act, StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { FIRST_WRITE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { markdownContent, pageOf, snapshotState } from "../fixtures/builders";
import { press } from "../components/alerts/detailsTestSupport";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { PanelFrameProvider } from "../components/panel/PanelHeader";
import { focusLanding, focusLandingWithoutText } from "../components/focus/regionFocus";
import { PlatformProvider } from "../platform/react";
import { createFixtureDaemon } from "../fixtures/fixtureDaemon";
import type { Fixture } from "../fixtures/fixture";
import { CodedError } from "../errors/codedError";
import type { Page, PageContent, SubmittedValue } from "../protocol";
import { applyDaemonEvent, createStateStore, DaemonProvider, type Daemon } from "../store";
import { click, type } from "../test/dom";
import { testDaemon } from "../test/testDaemon";
import { ReportPanel } from "./ReportPanel";
import { BROWSER_HOST } from "../test/hosts";
import { fakeTimers } from "../test/clock";

vi.mock("../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => {
  const modality = getInteractionModality();
  onTestFinished(() => act(() => Reflect.apply(setInteractionModality, undefined, [modality])));
});

/** A page whose content has a text field to hold focus in, and a submit button. */
const formContent = (label: string): PageContent => ({
  body: [
    { type: "markdown", text: label },
    { type: "text", name: "note", label: "Note" },
    { type: "submit" },
  ],
});

function renderReport(pageCount = 2) {
  const pages = Array.from({ length: pageCount }, (_, i) => pageOf(`page-${i}`, "console-session", formContent(`Page ${i}`), i));
  const store = createStateStore({ pages: new Map([["console-session", pages]]) });
  const daemon = testDaemon({ store, request: vi.fn(), showNotice: vi.fn() });
  const container = document.body.appendChild(document.createElement("section"));
  container.dataset.region = "panel-report";
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(
    <StrictMode>
      <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
        <DaemonProvider value={daemon}>
          <PanelFrameProvider value={{ canMaximize: false, maximized: false, onToggleMaximized: () => {}, onClose: () => {} }}>
            <ReportPanel sessionId="console-session" />
          </PanelFrameProvider>
        </DaemonProvider>
      </PlatformProvider>
    </StrictMode>,
  ));
  return {
    container,
    append: (session = "console-session") => act(async () => {
      applyDaemonEvent(store, {
        type: "page_created", page: pageOf("new-page", session, formContent("New page"), 10),
      });
    }),
    remove: (...ids: string[]) => act(async () => {
      applyDaemonEvent(store, { type: "pages_deleted", session_id: "console-session", pages: ids });
    }),
  };
}

const noteField = (container: ParentNode) => container.querySelector<HTMLInputElement>('[data-page-field="note"] input')!;

it.each([1, 2])("hands a focused field of a page that goes to a surviving report control with keyboard modality (%i prior pages)", async (count) => {
  const { container, append } = renderReport(count);
  const oldField = noteField(container);
  act(() => {
    setInteractionModality("pointer");
    oldField.focus();
  });
  expect(document.activeElement).toBe(oldField);
  await append();
  expect(oldField.isConnected).toBe(false);
  expect(noteField(container)).not.toBe(oldField);
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

it("ignores another session's new page without replacing the current page or moving focus", async () => {
  const { container, append } = renderReport();
  const field = noteField(container);
  act(() => field.focus());
  await append("other-session");
  expect(noteField(container)).toBe(field);
  expect(document.activeElement).toBe(field);
});

it("lands a tap's focus on a control that is not a text field, and F6's or a pointer's on the header's first enabled control", () => {
  const { container } = renderReport();
  expect(focusLandingWithoutText(container)).toBe(true);
  expect(document.activeElement).not.toBe(noteField(container));
  expect(document.activeElement?.tagName).toBe("BUTTON");
  expect(focusLanding(container)).toBe(true);
  expect(document.activeElement?.tagName).toBe("BUTTON");
  expect(container.contains(document.activeElement)).toBe(true);
});

it("shows a failed page listing in the panel with its reason and Try again, and opens no error alert dialog", async () => {
  const request = vi.fn().mockRejectedValueOnce(new Error("Daemon busy")).mockResolvedValue({ type: "ack", id: null });
  const store = createStateStore({ connectionState: "open" });
  const daemon = testDaemon({ store, request, showNotice: vi.fn() });
  const container = document.body.appendChild(document.createElement("section"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
      <DaemonProvider value={daemon}>
        <PanelFrameProvider value={{ canMaximize: false, maximized: false, onToggleMaximized: () => {}, onClose: () => {} }}>
          <ReportPanel sessionId="console-session" />
        </PanelFrameProvider>
      </DaemonProvider>
    </PlatformProvider>,
  ));
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(panel.querySelector("h2")?.textContent).toBe("Could not load the pages of this session");
  expect(panel.textContent).toContain("Something unexpected went wrong.");
  expect(container.querySelector("[role=alert]")).not.toBeNull();
  expect(errorAlerts.getSnapshot()).toEqual([]);
  const retry = [...panel.querySelectorAll("button")].find((button) => button.textContent === "Try again")!;
  await act(async () => retry.click());
  expect(request).toHaveBeenCalledTimes(2);
  expect(container.querySelector('[data-slot="empty-state"]')).toBeNull();
});

const MARK = 'button[aria-label="Could not load the pages of this session"]';

/** The panel with two pages in hand, whose first listing is answered by `request`'s first reply. */
async function renderListed(request: Daemon["request"]) {
  const pages = [pageOf("page-0", "console-session", markdownContent("Page 0"), 0), pageOf("page-1", "console-session", markdownContent("Page 1"), 1)];
  return renderWith(request, createStateStore({ connectionState: "open", pages: new Map([["console-session", pages]]) }));
}

async function renderWith(request: Daemon["request"], store: ReturnType<typeof createStateStore>) {
  const daemon = testDaemon({ store, request, showNotice: vi.fn() });
  const container = document.body.appendChild(document.createElement("section"));
  container.dataset.region = "panel-report";
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
      <DaemonProvider value={daemon}>
        <PanelFrameProvider value={{ canMaximize: false, maximized: false, onToggleMaximized: () => {}, onClose: () => {} }}>
          <ReportPanel sessionId="console-session" />
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
  fakeTimers();
  const request = vi.fn().mockRejectedValueOnce(new Error("Daemon busy")).mockResolvedValue({ type: "ack", id: null });
  const container = await renderListed(request);
  await act(() => vi.advanceTimersByTimeAsync(FIRST_WRITE_DELAY_MS));
  expect(announcement(container)).toBe("Could not load the pages of this session");
  expect([...container.querySelectorAll('[role="status"]')].filter((region) => region.textContent)).toHaveLength(1);
});

it("does not say the listing failure again when pages arrive after the whole card announced it, and says a new failure", async () => {
  fakeTimers();
  const request = vi.fn().mockRejectedValueOnce(new Error("Daemon busy")).mockRejectedValueOnce(new Error("Daemon busy"));
  const store = createStateStore({ connectionState: "open" });
  const container = await renderWith(request, store);
  expect(container.querySelector('[data-slot="empty-state"]')).not.toBeNull();
  expect(container.querySelector(MARK)).toBeNull();
  await act(async () => store.setState({ pages: new Map([["console-session", [pageOf("page-0", "console-session", markdownContent("Page 0"), 0)]]]) }));
  expect(container.querySelector(MARK)).not.toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(FIRST_WRITE_DELAY_MS));
  expect(announcement(container)).toBe("");
  // A failure after that, with the pages in hand, is a new one.
  await act(async () => store.setState({ snapshotEpoch: store.getState().snapshotEpoch + 1 }));
  await act(() => vi.advanceTimersByTimeAsync(FIRST_WRITE_DELAY_MS));
  expect(announcement(container)).toBe("Could not load the pages of this session");
});

/** Fake time the retried listing stays out for: past the status region's waits, so what it says of the earlier failure
 * has been said before the listing answers. */
const LISTING_OUT_MS = FIRST_WRITE_DELAY_MS;

it("hands focus on from the mark to a header control when the listing succeeds and the mark goes", async () => {
  fakeTimers();
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
  await act(() => vi.advanceTimersByTimeAsync(LISTING_OUT_MS));
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
    applyDaemonEvent(store, { type: "page_created", page: pageOf("page-0", "console-session", markdownContent("Page 0"), 0) });
  });
  expect(button.isConnected).toBe(true);
  expect(document.activeElement).toBe(button);
});

it("keeps the mark while Try again's listing is out and across a second failure, which is not said again", async () => {
  fakeTimers();
  let finish!: (error: Error) => void;
  const request = vi
    .fn()
    .mockRejectedValueOnce(new Error("Daemon busy"))
    .mockReturnValueOnce(new Promise((_, reject) => void (finish = reject)));
  const container = await renderListed(request);
  const mark = container.querySelector<HTMLButtonElement>(MARK)!;
  await act(() => vi.advanceTimersByTimeAsync(FIRST_WRITE_DELAY_MS));
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
  expect(noteField(container)).toBeNull();
  expect(container.textContent).toContain("No pages yet");
});

const SURVEY: PageContent = { body: [{ type: "text", name: "note", label: "Note" }, { type: "submit", label: "Send" }] };
const REFUSED = { code: "session_not_running", params: { session: "console-session" }, message: "This session is not running." };

/** The panel over the fixture daemon, which answers `submit_page` as the real one does, with `pages` held for the session. */
function renderSubmittable(extra: Partial<Fixture> = {}, pages = [pageOf("page-0", "console-session", SURVEY, 0)]) {
  const daemon = createFixtureDaemon({ state: snapshotState({ pages: { "console-session": pages } }), ...extra });
  const request = vi.spyOn(daemon, "request");
  const container = document.body.appendChild(document.createElement("section"));
  container.dataset.region = "panel-report";
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => { while (errorAlerts.getSnapshot().length > 0) errorAlerts.acknowledge(); });
  });
  act(() => root.render(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
      <DaemonProvider value={daemon}>
        <PanelFrameProvider value={{ canMaximize: false, maximized: false, onToggleMaximized: () => {}, onClose: () => {} }}>
          <ReportPanel sessionId="console-session" />
        </PanelFrameProvider>
      </DaemonProvider>
    </PlatformProvider>,
  ));
  const submits = () => request.mock.calls.filter(([body]) => body.type === "submit_page");
  const send = () => [...container.querySelectorAll("button")].find((candidate) => candidate.textContent === "Send")!;
  const stored = () => daemon.store.getState().pages.get("console-session")!.map((page) => page.submission);
  const badges = () => container.textContent!;
  let revision = 0;
  const event = (submission: Record<string, SubmittedValue> | null) => act(async () => applyDaemonEvent(daemon.store, { type: "page_submission", session_id: "console-session", page: "page-0", submission, submission_revision: ++revision }));
  return { container, daemon, submits, send, stored, badges, event };
}

it("submits an editable page's values and then shows them read-only, marked Submitted, with the submitter unavailable", async () => {
  const { container, submits, send, stored, badges } = renderSubmittable();
  expect(badges()).not.toContain("Submitted");
  type(noteField(container), "Ada");
  await click(send());
  expect(submits().map(([body]) => body)).toEqual([{ type: "submit_page", page: "page-0", data: { note: "Ada" } }]);
  expect(stored()).toEqual([{ note: "Ada" }]);
  expect(noteField(container).value).toBe("Ada");
  expect(noteField(container).readOnly).toBe(true);
  expect(send().disabled).toBe(true);
  expect(badges()).toContain("Submitted");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("locks the form with the pressed button pending while the submission is out, and a second press sends nothing", async () => {
  const { container, submits, send, stored, badges } = renderSubmittable({ pageSubmit: "pending" });
  type(noteField(container), "Ada");
  await click(send());
  expect(submits()).toHaveLength(1);
  expect(send().getAttribute("aria-busy")).toBe("true");
  expect(noteField(container).readOnly).toBe(true);
  expect(noteField(container).value).toBe("Ada");
  // The daemon has marked the page submitted before delivering, and told every window; this window, which is still
  // waiting for the outcome, does not say "Submitted" yet.
  expect(stored()).toEqual([{ note: "Ada" }]);
  expect(badges()).not.toContain("Submitted");
  await click(send());
  expect(submits()).toHaveLength(1);
});

it("raises the error of a failed delivery that certainly did not happen and unlocks the form with the user's values, to submit again", async () => {
  const { container, submits, send, stored, badges } = renderSubmittable({ pageSubmit: { error: REFUSED, delivered: false } });
  type(noteField(container), "Ada");
  await click(send());
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(errorAlerts.getSnapshot()[0]).toMatchObject({ title: "Could not submit the page", message: "This session is not running.", session: "console-session" });
  expect(stored()).toEqual([null]);
  expect(noteField(container).readOnly).toBe(false);
  expect(noteField(container).value).toBe("Ada");
  expect(send().disabled).toBe(false);
  expect(send().getAttribute("aria-busy")).toBeNull();
  expect(badges()).not.toContain("Submitted");
  await click(send());
  expect(submits()).toHaveLength(2);
});

it("leaves the page submitted when the delivery may have arrived, and says so in the error", async () => {
  const unconfirmed = { code: "message_acceptance_unconfirmed", params: { agent: "Codex" }, message: "Codex did not confirm that it accepted a message." };
  const { container, submits, send, stored, badges } = renderSubmittable({ pageSubmit: { error: unconfirmed, delivered: true } });
  type(noteField(container), "Ada");
  await click(send());
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(errorAlerts.getSnapshot()[0].error).toMatchObject({ code: "message_acceptance_unconfirmed" });
  expect(String((errorAlerts.getSnapshot()[0].error as Error).message)).toContain("may already be queued");
  expect(stored()).toEqual([{ note: "Ada" }]);
  expect(noteField(container).readOnly).toBe(true);
  expect(send().disabled).toBe(true);
  expect(badges()).toContain("Submitted");
  await click(send());
  expect(submits()).toHaveLength(1);
});

it("shows a page another window submitted read-only with what it was submitted with, and an editable one again when the mark is cleared", async () => {
  const { container, submits, send, event, badges } = renderSubmittable();
  type(noteField(container), "typed here");
  await event({ note: "Ada" });
  expect(noteField(container).value).toBe("Ada");
  expect(noteField(container).readOnly).toBe(true);
  expect(send().disabled).toBe(true);
  expect(badges()).toContain("Submitted");
  await event(null);
  expect(noteField(container).readOnly).toBe(false);
  expect(noteField(container).value).toBe("typed here");
  expect(send().disabled).toBe(false);
  expect(badges()).not.toContain("Submitted");
  expect(submits()).toHaveLength(0);
});

it("shows a page submitted before the window loaded read-only", () => {
  const { container, send, badges } = renderSubmittable({}, [pageOf("page-0", "console-session", SURVEY, 0, { note: "Ada" })]);
  expect(noteField(container).value).toBe("Ada");
  expect(noteField(container).readOnly).toBe(true);
  expect(send().disabled).toBe(true);
  expect(badges()).toContain("Submitted");
});

it("shows a history page read-only, with its Read-only badge, and no way to submit it", async () => {
  const { container, send, submits, badges } = renderSubmittable({}, [pageOf("page-0", "console-session", SURVEY, 1), pageOf("page-1", "console-session", markdownContent("Newest"), 0)]);
  await click(container.querySelector('[aria-label="Previous page"]'));
  expect(noteField(container).readOnly).toBe(true);
  expect(send().disabled).toBe(true);
  expect(badges()).toContain("Read-only");
  expect(badges()).not.toContain("Submitted");
  await click(send());
  expect(submits()).toHaveLength(0);
});

it("keeps a failed validation on the page: nothing is sent and the field says what is wrong", async () => {
  const required: PageContent = { body: [{ type: "text", name: "note", label: "Note", required: true }, { type: "submit", label: "Send" }] };
  const { container, submits, send } = renderSubmittable({}, [pageOf("page-0", "console-session", required, 0)]);
  await click(send());
  expect(submits()).toHaveLength(0);
  expect(container.querySelector('[data-page-field="note"] [role=alert]')?.textContent).toBe("Enter a value.");
  expect(document.activeElement).toBe(noteField(container));
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

/** The panel over two pages of one session, the older a history page and the newer the one that takes a submission, whose
 * `submit_page` is answered when the test says so. The listing is not asked for (the store is not connected). */
function renderSubmitting(older: Page = pageOf("page-0", "console-session", markdownContent("Older"), 1), newest: PageContent = SURVEY) {
  let reply!: { accept: () => void; refuse: (error: Error) => void };
  const outcome = new Promise<{ type: "ack"; id: null }>((resolve, reject) => {
    reply = { accept: () => resolve({ type: "ack", id: null }), refuse: reject };
  });
  const request = vi.fn((body: { type: string }) => (body.type === "submit_page" ? outcome : Promise.resolve({ type: "ack", id: null })));
  const pages = [older, pageOf("page-1", "console-session", newest, 0)];
  const store = createStateStore({ pages: new Map([["console-session", pages]]) });
  const daemon = testDaemon({ store, request: request as Daemon["request"], showNotice: vi.fn() });
  const container = document.body.appendChild(document.createElement("section"));
  container.dataset.region = "panel-report";
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => { while (errorAlerts.getSnapshot().length > 0) errorAlerts.acknowledge(); });
  });
  act(() => root.render(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
      <DaemonProvider value={daemon}>
        <PanelFrameProvider value={{ canMaximize: false, maximized: false, onToggleMaximized: () => {}, onClose: () => {} }}>
          <ReportPanel sessionId="console-session" />
        </PanelFrameProvider>
      </DaemonProvider>
    </PlatformProvider>,
  ));
  const send = () => [...container.querySelectorAll("button")].find((candidate) => candidate.textContent === "Send")!;
  const goTo = (label: "Previous page" | "Next page") => click(container.querySelector(`[aria-label="${label}"]`));
  const submits = () => request.mock.calls.filter(([body]) => body.type === "submit_page");
  return { container, store, send, goTo, submits, reply };
}

it("keeps a page whose submission is out pending when the user pages away and back, without calling it Submitted", async () => {
  const { container, send, goTo, submits, reply } = renderSubmitting();
  type(noteField(container), "Ada");
  await click(send());
  expect(submits()).toHaveLength(1);

  await goTo("Previous page");
  expect(container.textContent).toContain("Older");
  await goTo("Next page");

  expect(noteField(container).readOnly).toBe(true);
  // The button that was pressed is still the pending one, found again by its name and value, not by identity.
  expect(send().getAttribute("aria-busy")).toBe("true");
  expect(container.textContent).not.toContain("Submitted");
  await click(send());
  expect(submits()).toHaveLength(1);

  await act(async () => reply.accept());
  expect(container.textContent).toContain("Submitted");
  expect(noteField(container).readOnly).toBe(true);
});

it("keeps the pressed one of two unnamed submit buttons the pending one when the user pages away and back", async () => {
  const content: PageContent = { body: [{ type: "text", name: "note", label: "Note" }, { type: "submit", label: "Send" }, { type: "submit", label: "Send later" }] };
  const { container, goTo, reply } = renderSubmitting(undefined, content);
  const named = (name: string) => [...container.querySelectorAll("button")].find((candidate) => candidate.textContent === name)!;
  type(noteField(container), "Ada");
  await click(named("Send later"));
  const busy = () => [named("Send"), named("Send later")].map((button) => button.getAttribute("aria-busy") === "true");
  expect(busy()).toEqual([false, true]);
  await goTo("Previous page");
  await goTo("Next page");
  expect(busy()).toEqual([false, true]);
  await act(async () => reply.accept());
  expect(busy()).toEqual([false, false]);
});

it("tracks a pending submission by its page: another page, shown meanwhile, keeps its own badges", async () => {
  const { container, send, goTo, reply } = renderSubmitting(pageOf("page-0", "console-session", SURVEY, 1, { note: "earlier" }));
  type(noteField(container), "Ada");
  await click(send());
  await goTo("Previous page");
  // The older page was submitted long ago: it says so while the newest page's submission is out.
  expect(container.textContent).toContain("Read-only");
  expect(container.textContent).toContain("Submitted");
  expect(noteField(container).value).toBe("earlier");
  await act(async () => reply.accept());
  expect(noteField(container).value).toBe("earlier");
  await goTo("Next page");
  expect(noteField(container).value).toBe("Ada");
});

it("shows a submission accepted while the user was on another page as Submitted when they come back, before any page_submission arrives", async () => {
  const { container, send, goTo, reply } = renderSubmitting();
  type(noteField(container), "Ada");
  await click(send());
  await goTo("Previous page");
  await act(async () => reply.accept());
  await goTo("Next page");
  expect(container.textContent).toContain("Submitted");
  expect(noteField(container).value).toBe("Ada");
  expect(noteField(container).readOnly).toBe(true);
  expect(send().disabled).toBe(true);
});

it("hands focus on to the start of the panel when the submission completes under the submitter that held it", async () => {
  const { container, send, reply } = renderSubmitting();
  type(noteField(container), "Ada");
  act(() => send().focus());
  await click(send());
  expect(document.activeElement).toBe(send());
  await act(async () => reply.accept());
  expect(send().disabled).toBe(true);
  expect(document.activeElement).not.toBe(document.body);
  expect(document.activeElement).not.toBe(send());
  expect(container.contains(document.activeElement)).toBe(true);
  expect(document.activeElement?.getAttribute("aria-label")).toBe("Previous page");
});

it("unlocks a page whose submission certainly failed while the user was on another page, so it can be submitted again", async () => {
  const { container, send, goTo, submits, reply } = renderSubmitting();
  type(noteField(container), "Ada");
  await click(send());
  await goTo("Previous page");
  await act(async () => reply.refuse(new CodedError("This session is not running.", "session_not_running", { session: "console-session" })));
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  await goTo("Next page");
  expect(container.textContent).not.toContain("Submitted");
  expect(noteField(container).readOnly).toBe(false);
  expect(send().disabled).toBe(false);
  type(noteField(container), "Ada");
  await click(send());
  expect(submits()).toHaveLength(2);
});

it("keeps a page submitted when the delivery was unconfirmed while the user was on another page", async () => {
  const { container, send, goTo, reply } = renderSubmitting();
  type(noteField(container), "Ada");
  await click(send());
  await goTo("Previous page");
  await act(async () => reply.refuse(new CodedError("Codex did not confirm.", "message_acceptance_unconfirmed", { agent: "Codex" })));
  await goTo("Next page");
  expect(container.textContent).toContain("Submitted");
  expect(send().disabled).toBe(true);
});

it("unlocks a page whose submission certainly failed while the user was on another page with the values they submitted", async () => {
  const { container, send, goTo, submits, reply } = renderSubmitting();
  type(noteField(container), "Ada");
  await click(send());
  await goTo("Previous page");
  await act(async () => reply.refuse(new CodedError("This session is not running.", "session_not_running", { session: "console-session" })));
  await goTo("Next page");
  expect(noteField(container).readOnly).toBe(false);
  expect(noteField(container).value).toBe("Ada");
  await click(send());
  expect(submits()).toHaveLength(2);
  expect(submits()[1][0]).toMatchObject({ data: { note: "Ada" } });
});

it("keeps a draft typed into the newest page when the user pages to a history page and back", async () => {
  const { container, goTo } = renderSubmitting();
  type(noteField(container), "half-typed");
  await goTo("Previous page");
  await goTo("Next page");
  expect(noteField(container).value).toBe("half-typed");
});

it("puts the page's timestamp and badges after the pager in the panel header", async () => {
  fakeTimers({ now: new Date(2026, 10, 11, 22, 30) });
  const stamped = (id: string, at: Date, submission: Page["submission"] = null): Page => ({ ...pageOf(id, "console-session", SURVEY, 0, submission), created_at: at.getTime() });
  const pages = [stamped("page-0", new Date(2026, 10, 10, 21, 7)), stamped("page-1", new Date(2026, 10, 11, 9, 5), { note: "Ada" })];
  const store = createStateStore({ pages: new Map([["console-session", pages]]) });
  const daemon = testDaemon({ store, request: vi.fn(), showNotice: vi.fn() });
  const container = document.body.appendChild(document.createElement("section"));
  container.dataset.region = "panel-report";
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
      <DaemonProvider value={daemon}>
        <PanelFrameProvider value={{ canMaximize: false, maximized: false, onToggleMaximized: () => {}, onClose: () => {} }}>
          <ReportPanel sessionId="console-session" />
        </PanelFrameProvider>
      </DaemonProvider>
    </PlatformProvider>,
  ));
  const header = container.querySelector("h2")!.parentElement!;
  const follows = (earlier: Element, later: Element) => Boolean(earlier.compareDocumentPosition(later) & Node.DOCUMENT_POSITION_FOLLOWING);
  const next = container.querySelector('[aria-label="Next page"]')!;
  // Some ICU versions separate the time from AM/PM with a narrow no-break space.
  const find = (text: string) => [...header.querySelectorAll("*")].find((element) => element.children.length === 0 && element.textContent?.replace(/\u202f/g, " ") === text)!;

  // The newest page was pushed today, so it shows the time alone, and is submitted.
  expect(find("9:05 AM")).toBeTruthy();
  expect(follows(next, find("9:05 AM"))).toBe(true);
  expect(follows(find("9:05 AM"), find("Submitted"))).toBe(true);
  expect(header.textContent).not.toContain("Read-only");
  expect(header.textContent).not.toContain("11/10/2026");

  // The older page was pushed on another day: its date and time, and the read-only badge.
  await click(container.querySelector('[aria-label="Previous page"]'));
  expect(follows(next, find("11/10/2026, 9:07 PM"))).toBe(true);
  expect(follows(find("11/10/2026, 9:07 PM"), find("Read-only"))).toBe(true);
  expect(header.textContent).not.toContain("Submitted");
});
