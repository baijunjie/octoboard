// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { toast } from "@heroui/react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../errors/codedError";
import { noticeKind } from "../errors/errorPresentation";
import { classifyError } from "../errors/errorRecovery";
import { sessionOf } from "../fixtures/builders";
import { format } from "../i18n/catalog";
import { LANGUAGES } from "../i18n/languages";
import { createStateStore, DaemonProvider, type Daemon, type WindowNotification } from "../store";
import { broadcastNotification, type NotifiedBroadcast } from "../store/notifications";
import { LEFTOVERS_NOTICE, PLAIN_NOTICE, SETTINGS_NOTICE } from "../test/acknowledgedNoticeProvider";
import { fakeTimers } from "../test/clock";
import { testDaemon } from "../test/testDaemon";
import { matchesWidthQuery } from "../test/viewport";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { raiseError } from "../components/alerts/raiseError";
import { MODAL_DIALOG } from "../components/focus/selectors";
import { withoutIsolates } from "../test/dom";
import { ErrorAlerts } from "./ErrorAlerts";
import { NOTICE_DURATION_MS, Toasts } from "./Toasts";

vi.mock("../providers/registry", async (importOriginal) => (await import("../test/acknowledgedNoticeProvider")).withAcknowledgedNotices(await importOriginal()));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

class FakeResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

// Registered before any test's `fakeTimers()`, so the globals stay stubbed while its run-out fires the timers still pending.
beforeEach(() => {
  onTestFinished(() => void vi.unstubAllGlobals());
});

const SETTINGS = "/Users/dev/.test-agent/settings.json";
const LEFTOVERS = "/Users/dev/.test-agent-old";

const notice = (code: string, params: Record<string, string>): NotifiedBroadcast => ({ type: "session_notice", session: "s1", code, params: { agent: "Test Agent", ...params }, message: "raw" });

/** The window's toasts and alerts over a store holding the session the notices are about, with `deliver` routing a
 * daemon notice as the store does. The alert queue is emptied when the test ends. */
function mountWindow(): { deliver: (event: NotifiedBroadcast) => void } {
  // The toast queue closes a toast on a timer of its own, which the teardown below runs out.
  fakeTimers();
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  vi.stubGlobal("matchMedia", (query: string) => ({ matches: matchesWidthQuery(query, 1280), addEventListener: () => {}, removeEventListener: () => {} }));
  let listener!: (request: WindowNotification) => void;
  const source: Daemon = {
    ...testDaemon({ request: vi.fn(), showNotice: vi.fn() }),
    store: createStateStore({ sessions: new Map([["s1", sessionOf("s1", "c1", undefined, "Hub", "idle")]]) }),
    onNotification: (registered) => { listener = registered; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
  onTestFinished(() => {
    act(() => {
      while (errorAlerts.getSnapshot().length > 0) errorAlerts.acknowledge();
      toast.clear();
      vi.runOnlyPendingTimers();
    });
    act(() => root.unmount());
    container.remove();
  });
  return { deliver: (event) => act(() => listener(broadcastNotification(event, source.store.getState(), "en"))) };
}

/** The shared alert on screen: a modal one, which a toast, an `alertdialog` too, is not. */
const shownAlert = () => document.querySelector(MODAL_DIALOG);

/** What `element` says, without the bidi isolates around the values substituted into it. */
const plain = (element: Element | null | undefined): string => withoutIsolates(element?.textContent) ?? "";

it("tells a notice its record marks from one it does not", () => {
  expect(noticeKind(SETTINGS_NOTICE, {})).toBe("acknowledged");
  expect(noticeKind(LEFTOVERS_NOTICE, {})).toBe("acknowledged");
  expect(noticeKind(PLAIN_NOTICE, {})).toBe("notice");
});

it("takes a marked notice as nothing to report, with its own remedy where it has one and none otherwise", () => {
  expect(classifyError(new CodedError("raw", SETTINGS_NOTICE, {}))).toEqual({ kind: "actionable", recovery: "none" });
  expect(classifyError(new CodedError("raw", LEFTOVERS_NOTICE, {}))).toEqual({ kind: "actionable", recovery: `${LEFTOVERS_NOTICE}.remedy` });
  // Unmarked, with no remedy: not classified by its code, as before.
  expect(classifyError(new CodedError("raw", PLAIN_NOTICE, {}))).toEqual({ kind: "unexpected" });
});

it("shows a marked notice in the shared alert, under the title such notices share, naming its session, and keeps it past a toast's time", () => {
  const { deliver } = mountWindow();
  deliver(notice(SETTINGS_NOTICE, { path: SETTINGS }));
  expect(document.querySelector('[data-slot="toast"]')).toBeNull();
  const alert = shownAlert()!;
  expect(alert.querySelector("h2")?.textContent).toBe("Session notice");
  expect(plain(alert)).toContain("Hub");
  expect(plain(alert)).toContain(`Test Agent did not read ${SETTINGS}`);
  expect(plain(alert)).not.toContain("raw");
  // No failure to report, and nothing to do beyond reading it.
  expect(plain(alert)).not.toContain("Report issue");
  expect(alert.querySelector("details")).toBeNull();
  act(() => void vi.advanceTimersByTime(NOTICE_DURATION_MS + 1));
  expect(plain(shownAlert())).toContain(SETTINGS);
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
});

/** The status icon of an alert and the class of each of its footer's buttons. */
const tone = (alert: Element) => ({
  icon: [...alert.querySelector('[data-slot="alert-dialog-icon"]')!.classList].filter((name) => name.startsWith("alert-dialog__icon--")),
  danger: [...alert.querySelectorAll("button")].filter((button) => button.className.includes("button--danger")),
});

it("draws a marked notice in the warning tone with an acknowledgement in the accent colour, where a failure keeps the danger tone", () => {
  const { deliver } = mountWindow();
  deliver(notice(SETTINGS_NOTICE, { path: SETTINGS }));
  const alert = shownAlert()!;
  expect(tone(alert)).toEqual({ icon: ["alert-dialog__icon--warning"], danger: [] });
  const ok = [...alert.querySelectorAll("button")].find((button) => button.textContent === "OK")!;
  expect(ok.className).toContain("button--primary");
  act(() => errorAlerts.acknowledge());
  // A failure in the same alert is drawn as one.
  act(() => raiseError(new Error("boom"), { title: "Could not save" }));
  const failure = shownAlert()!;
  expect(tone(failure).icon).toEqual(["alert-dialog__icon--danger"]);
  expect(tone(failure).danger.map((button) => button.textContent)).toContain("OK");
});

it("shows both marked notices of one launch, one after the other, neither replacing the other", () => {
  const { deliver } = mountWindow();
  deliver(notice(SETTINGS_NOTICE, { path: SETTINGS }));
  deliver(notice(LEFTOVERS_NOTICE, { path: LEFTOVERS }));
  expect(errorAlerts.getSnapshot().map((alert) => alert.notice)).toEqual([{ session: "s1", code: SETTINGS_NOTICE }, { session: "s1", code: LEFTOVERS_NOTICE }]);
  expect(plain(shownAlert())).toContain(SETTINGS);
  expect(plain(shownAlert())).not.toContain(LEFTOVERS);
  act(() => errorAlerts.acknowledge());
  const second = shownAlert()!;
  expect(second.querySelector("h2")?.textContent).toBe("Session notice");
  expect(plain(second)).toContain(`left in ${LEFTOVERS}`);
  // Its own remedy follows its sentence.
  expect(plain(second)).toContain(`Move the rules out of ${LEFTOVERS}`);
  act(() => errorAlerts.acknowledge());
  expect(shownAlert()).toBeNull();
});

it("keeps two notices of one code about one session that say different things, and one copy of a pending one", () => {
  const { deliver } = mountWindow();
  deliver(notice(SETTINGS_NOTICE, { path: SETTINGS }));
  deliver(notice(SETTINGS_NOTICE, { path: "/etc/test-agent/settings.json" }));
  deliver(notice(SETTINGS_NOTICE, { path: SETTINGS }));
  expect(errorAlerts.getSnapshot().map((alert) => withoutIsolates(alert.message))).toEqual([
    `Test Agent did not read ${SETTINGS}, so the permission rules in it are not in force for this launch.`,
    "Test Agent did not read /etc/test-agent/settings.json, so the permission rules in it are not in force for this launch.",
  ]);
});

it("leaves a notice that is not marked a toast, with no alert", () => {
  const { deliver } = mountWindow();
  deliver(notice(PLAIN_NOTICE, {}));
  expect(shownAlert()).toBeNull();
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  expect(plain(document.querySelector('[data-slot="toast"]'))).toContain("Test Agent runs with its hooks off.");
});

it("words the shared title in every language", () => {
  for (const language of LANGUAGES) {
    const title = format(language, "notice.acknowledged.title");
    expect(title, language).not.toBe("");
    if (language !== "en") expect(title, language).not.toBe(format("en", "notice.acknowledged.title"));
  }
});
