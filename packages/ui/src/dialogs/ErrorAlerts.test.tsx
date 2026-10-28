// @vitest-environment jsdom
import { act, StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { reportError } from "../reportError";
import { appConfig } from "../appConfig";
import { DaemonRequestError } from "../daemon-client";
import { ActionableError } from "../actionableError";
import { PlatformProvider } from "../platform/react";
import type { PlatformAdapter } from "../platform";
import { focusAfterLoss, setFocusAfterModalLoss } from "../components/regionFocus";
import { errorAlerts } from "../errorAlerts";
import { ConfirmDialog } from "./ConfirmDialog";
import { Dialog } from "./Dialog";
import { framesAsTimers, openable, settleFrames } from "./dialogTestSupport";
import { OpensAnotherDialog } from "./nestedDialogTestSupport";
import { ErrorAlerts, issueUrl } from "./ErrorAlerts";
import { TextInput } from "./TextInput";
import { createStateStore, type Daemon } from "../store";
import { reportDaemonExit } from "../lifecycle/exitFailure";
import { issueReportUrl, recordNames } from "../issueReport";
import { registerSettingsLauncher } from "../settings/settingsLauncher";

vi.mock("../issueReport", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../issueReport")>();
  return { ...actual, issueReportUrl: vi.fn(actual.issueReportUrl), recordNames: vi.fn(actual.recordNames) };
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => { vi.clearAllMocks(); act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); }); });

function mount(child?: React.ReactNode, platform: PlatformAdapter = { kind: "browser" }, daemon?: Daemon, strict = false): void {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const tree = <PlatformProvider value={platform}>{child}<ErrorAlerts daemon={daemon} /></PlatformProvider>;
  act(() => root.render(strict ? <StrictMode>{tree}</StrictMode> : tree));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
}

it("keeps errors across host mounting, deduplicates queued copies and acknowledges one at a time", () => {
  errorAlerts.report({ message: "first", session: "one" });
  errorAlerts.report({ message: "first", session: "one" });
  errorAlerts.report({ message: "first", session: "two" });
  errorAlerts.report({ message: "last" });
  mount();
  expect(errorAlerts.getSnapshot()).toHaveLength(3);
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("first");
  act(() => document.querySelector<HTMLButtonElement>('[role="alertdialog"] button')?.click());
  expect(errorAlerts.getSnapshot()).toHaveLength(2);
  act(() => errorAlerts.acknowledge());
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("last");
  act(() => errorAlerts.acknowledge());
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

it("stays above an open form until acknowledged and leaves the form available for retry", async () => {
  const close = vi.fn();
  mount(<Dialog title="Edit project" onClose={close} submitLabel="Save" onSubmit={() => {}}><input aria-label="Name" defaultValue="Kept" /></Dialog>);
  act(() => errorAlerts.report({ message: "Save failed" }));
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  const alert = document.querySelector<HTMLElement>('[role="alertdialog"]')!;
  expect(alert).not.toBeNull();
  expect(alert.querySelector('[aria-label="Close"]')).toBeNull();
  expect(alert.contains(document.activeElement)).toBe(true);
  expect(document.activeElement).toBe(alert);
  act(() => alert.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
  expect(document.querySelector('[role="alertdialog"]')).toBe(alert);
  expect(close).not.toHaveBeenCalled();
  act(() => alert.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", repeat: true, bubbles: true, cancelable: true })));
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  act(() => alert.querySelector<HTMLButtonElement>("button")?.click());
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(document.querySelector<HTMLInputElement>('input[aria-label="Name"]')?.value).toBe("Kept");
  expect(close).not.toHaveBeenCalled();
});

it("reports an identical immediate failure again after acknowledgement and retry", async () => {
  const confirm = vi.fn().mockRejectedValue(new Error("Refused"));
  mount(<ConfirmDialog title="Save project" message="Save it?" errorTitle="Could not save the project" onConfirm={confirm} onCancel={() => {}} />);
  const submit = () => document.querySelector('[role="alertdialog"] form')!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await act(async () => { submit(); });
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  act(() => errorAlerts.acknowledge());
  await act(async () => { submit(); });
  expect(confirm).toHaveBeenCalledTimes(2);
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(errorAlerts.getSnapshot()[0].message).toBe("Refused");
});

it("gives known disk and lock failures specific remedies without bug-report instructions", () => {
  mount();
  act(() => errorAlerts.report({ message: "No space left on device (os error 28)", error: new DaemonRequestError("No space left on device (os error 28)", "internal_error", {}) }));
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Free space on the disk");
  expect(document.querySelector('[role="alertdialog"]')?.textContent).not.toContain("This may be a bug");
  act(() => errorAlerts.acknowledge());
  act(() => errorAlerts.report({ message: "Session locked", error: new DaemonRequestError("Session locked", "session_locked", { session: "s1" }) }));
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("choose Unlock");
});

it("offers Issue guidance for unexpected failures, opening only after a user click", async () => {
  let opened!: () => void;
  const open = vi.fn(() => new Promise<void>((resolve) => { opened = resolve; }));
  mount(undefined, { kind: "browser", links: { open } });
  const source = new Error("Renderer invariant failed");
  act(() => errorAlerts.report({ message: source.message, error: source }));
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.textContent).toContain("This may be a bug");
  expect(alert.textContent).toContain("Renderer invariant failed");
  expect(open).not.toHaveBeenCalled();
  const report = [...alert.querySelectorAll("a")].find((link) => link.textContent === "Report issue")!;
  // The link carries a description of the failure, with the title and the body of a new issue.
  const reportUrl = report.getAttribute("href")!;
  expect(reportUrl.startsWith(`${appConfig.issueUrl}?`)).toBe(true);
  const prefilled = new URL(reportUrl).searchParams;
  expect(prefilled.get("title")).toBe("Unexpected error: Renderer invariant failed");
  expect(prefilled.get("body")).toContain("Renderer invariant failed");
  expect(report.classList.contains("button--secondary")).toBe(true);
  expect(alert.querySelector("p [data-inline-reference]")?.textContent).toBe("Report issue");
  expect(alert.querySelector("p a")).toBeNull();
  expect(report.querySelector('svg[aria-hidden="true"]')).not.toBeNull();
  expect(alert.querySelectorAll(`a[href^="${appConfig.issueUrl}"]`)).toHaveLength(1);
  await act(async () => report.click());
  expect(open).toHaveBeenCalledExactlyOnceWith(reportUrl);
  expect(report.getAttribute("aria-busy")).toBe("true");
  await act(async () => report.click());
  expect(open).toHaveBeenCalledTimes(1);
  await act(async () => opened());
  expect(report.getAttribute("aria-busy") === "true").toBe(false);
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
});

it("immediately shows an actionable manual URL fallback if opening an Issue fails", async () => {
  const open = vi.fn().mockRejectedValue(Object.assign(new Error("Browser unavailable"), { code: "EACCES" }));
  mount(undefined, { kind: "tauri", links: { open } });
  act(() => errorAlerts.report({ message: "Unexpected failure" }));
  const report = [...document.querySelectorAll("a")].find((link) => link.textContent === "Report issue")!;
  await act(async () => report.click());
  expect(open).toHaveBeenCalledExactlyOnceWith(report.getAttribute("href"));
  expect(open.mock.calls[0]).toEqual([expect.stringMatching(/^[^?]+\?title=Unexpected\+error&body=/)]);
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.textContent).toContain("The browser did not open the Issue page, so its address is shown below.");
  // The address shown to be typed is the bare one, not the long prefilled link.
  expect(alert.textContent).toContain(issueUrl);
  expect(alert.textContent).not.toContain("title=");
  expect(alert.textContent).toContain("open it manually");
  // The system's reason is not part of the alert's words; it is behind Error details, on a surface of its own.
  const details = alert.querySelector("details")!;
  expect(details.querySelector("summary")?.textContent).toBe("Error details");
  expect(details.querySelector(".app-code")?.textContent).toContain("Browser unavailable");
  expect(alert.textContent?.replace(details.textContent ?? "", "")).not.toContain("Browser unavailable");
  expect([...alert.querySelectorAll("a")].some((link) => link.textContent === "Report issue")).toBe(false);
  expect(errorAlerts.getSnapshot()).toHaveLength(2);
  act(() => errorAlerts.acknowledge());
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Unexpected failure");
});

it("carries the plain text of an alert's own details into the report", () => {
  mount();
  act(() => errorAlerts.report({ message: "Crashed", details: <pre>shown</pre>, diagnostic: "Error: render failed\n    at /Users/alice/app/x.ts:1:2" }));
  const report = [...document.querySelectorAll("a")].find((link) => link.textContent === "Report issue")!;
  const prefilled = new URL(report.getAttribute("href")!).searchParams;
  expect(prefilled.get("title")).toBe("Unexpected error: Error: render failed");
  expect(prefilled.get("body")).toContain("```\nError: render failed\n    at <path>:1:2\n```");
});

const reportLink = () => [...document.querySelectorAll("a")].find((link) => link.textContent === "Report issue");
const reportOf = () => new URL(reportLink()!.getAttribute("href")!).searchParams;

it("redacts the daemon's record names and home directory from the report", () => {
  const store = createStateStore({
    homeDir: "/Users/alice",
    projects: new Map([["p", { name: "Zeta Project", path: "/Users/alice/zeta" }]]) as never,
  });
  mount(undefined, { kind: "browser" }, { store } as unknown as Daemon);
  const error = new Error("Cannot open Zeta Project at /Users/alice/zeta/a.ts");
  act(() => errorAlerts.report({ message: error.message, error }));
  const prefilled = reportOf();
  expect(prefilled.get("title")).toBe("Unexpected error: Cannot open <name> at <path>");
  expect(`${prefilled.get("title")}${prefilled.get("body")}`).not.toMatch(/Zeta|alice/);
});

it("leaves the alert's own words, session and body out of the report", () => {
  mount();
  const error = new DaemonRequestError("raw", "unexpected_error", {});
  error.stack = "Error: raw failure\n    at x (tauri://localhost/assets/a.js:1:2)";
  act(() => errorAlerts.report({ message: "Could not save Secret Plan", title: "Saving Secret Plan failed", session: "s-secret", content: <p>Body about Secret Plan</p>, error }));
  const prefilled = reportOf();
  expect(prefilled.get("title")).toBe("Unexpected error: raw");
  expect(`${prefilled.get("title")}${prefilled.get("body")}`).not.toMatch(/Secret|s-secret/);
});

it("puts the application's version, from the platform, in the report", async () => {
  mount(undefined, { kind: "tauri", links: { open: vi.fn() }, updater: { currentVersion: async () => "9.8.7" } as PlatformAdapter["updater"] });
  await act(async () => errorAlerts.report({ message: "x", error: new Error("boom") }));
  expect(reportOf().get("body")).toContain(`- ${appConfig.name}: 9.8.7`);
});

it("neither walks the records nor builds a report for an actionable failure that has only a message", () => {
  mount();
  act(() => errorAlerts.report({ message: "Copy failed", recovery: "clipboard" }));
  expect(document.querySelector('[role="alertdialog"]')).not.toBeNull();
  expect(reportLink()).toBeUndefined();
  expect(vi.mocked(recordNames)).not.toHaveBeenCalled();
  expect(vi.mocked(issueReportUrl)).not.toHaveBeenCalled();
  act(() => errorAlerts.report({ message: "Boom", error: new Error("boom") }, { immediate: true }));
  expect(vi.mocked(issueReportUrl)).toHaveBeenCalled();
  expect(vi.mocked(recordNames)).toHaveBeenCalled();
});

it("carries the detail of a lifecycle failure into the report", () => {
  mount();
  act(() => reportDaemonExit("daemon exited at /Users/alice/run: status 1"));
  expect(reportOf().get("body")).toContain("```\ndaemon exited at <path>: status 1\n```");
});

it("retains explicit clipboard recovery metadata through the queue", () => {
  mount();
  const error = new ActionableError("Copy failed", "clipboard");
  act(() => errorAlerts.report({ message: error.message, error }));
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Allow clipboard access");
});

it.each([
  ["connection", ["Try again"]],
  ["path", ["Browse"]],
  ["unlock", ["Unlock"]],
] as const)("renders %s recovery control references as chips", (recovery, labels) => {
  mount();
  act(() => errorAlerts.report({ message: "Action failed", recovery }));
  const chips = [...document.querySelectorAll('[role="alertdialog"] [data-inline-reference]')].map((chip) => chip.textContent);
  expect(chips).toEqual(labels);
});

it.each([
  ["account", "Manage accounts…", undefined],
  ["addedAgents", "Manage accounts…", undefined],
  ["accountDisabled", "Manage accounts…", undefined],
  ["accountNotSignedIn", "Manage accounts…", undefined],
  ["agentNotAdded", "Add agent…", true],
] as const)("offers a button for %s recovery that opens Settings' Agent accounts", (recovery, label, addAgent) => {
  mount();
  const open = vi.fn();
  onTestFinished(registerSettingsLauncher(open));
  act(() => errorAlerts.report({ message: "Action failed", recovery }));
  const button = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((b) => b.textContent === label);
  expect(button).toBeDefined();
  act(() => button!.click());
  expect(open).toHaveBeenCalledWith("accounts", { addAgent });
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

it("offers the footer action for a batch only when every failure shares that recovery", () => {
  mount();
  const labels = () => Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).map((b) => b.textContent);
  act(() => errorAlerts.report({ message: "Batch", failures: [{ error: "a", recovery: "account" }, { error: "b", recovery: "account" }] }));
  expect(labels()).toContain("Manage accounts…");
  act(() => errorAlerts.acknowledge());
  act(() => errorAlerts.report({ message: "Mixed", failures: [{ error: "a", recovery: "account" }, { error: "b", recovery: "path" }] }));
  expect(labels()).not.toContain("Manage accounts…");
});

it("hands focus to the window's fallback when acknowledging the last alert leaves it on the page body", async () => {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  mount();
  const fallback = vi.fn();
  onTestFinished(setFocusAfterModalLoss(fallback));
  act(() => errorAlerts.report({ message: "Plain", recovery: "none" }));
  expect(document.querySelector('[role="alertdialog"] p')).toBeNull();
  act(() => errorAlerts.acknowledge());
  (document.activeElement as HTMLElement | null)?.blur();
  await act(async () => { vi.advanceTimersByTime(500); });
  expect(fallback).toHaveBeenCalledOnce();
});

/** Mounts a dialog opened by a press without a pointer, so its own opening focus is deferred a frame and it holds no
 * focus yet. An alert raised over it must still end up focused itself, not on its first control: focusing the alert in
 * the commit that mounted it, by a layout or passive effect (its own, or its frame's), makes that dialog's focus
 * containment queue a move to the alert's first control, which lands a frame later. Each frame comes as a 0 ms timer,
 * so it runs ahead of any timer set after it was asked for, as a browser may render one before the next timer task:
 * the dialog's opening focus then lands before anything an alert put off to a timer, the order a loaded test run or a
 * busy window can produce. `strict` mounts it all in `StrictMode`, as the app and the gallery are, which runs every
 * effect a second time on mount. With `deferred`, the dialog is not opened but the function that opens it is
 * returned, for a test to call in one update with raising an alert, the way a press that fails at once does. */
function mountOverVirtuallyOpenedDialog(strict: boolean, deferred = false): () => void {
  framesAsTimers();
  const opener = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => { opener.remove(); });
  opener.focus();
  const { element, open } = openable(<Dialog title="Edit project" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}><input aria-label="Name" /></Dialog>, deferred);
  mount(element, undefined, undefined, strict);
  return open;
}

const alertHoldsFocus = (message: string) => {
  const alert = document.querySelector<HTMLElement>('[role="alertdialog"]')!;
  expect(alert.textContent).toContain(message);
  expect(alert.closest('[aria-hidden="true"], [inert]')).toBeNull();
  expect(document.activeElement).toBe(alert);
};

describe.each([{ strict: false }, { strict: true }])("over a dialog opened without a pointer (StrictMode: $strict)", ({ strict }) => {
  it("ends on the alert raised over it, not its first control", async () => {
    mountOverVirtuallyOpenedDialog(strict);
    act(() => errorAlerts.report({ message: "Refused", recovery: "accountNotSignedIn" }));
    await settleFrames();
    alertHoldsFocus("Refused");
  });

  it("ends on the alert raised in the same task as the press that opens it", async () => {
    const open = mountOverVirtuallyOpenedDialog(strict, true);
    act(() => {
      open();
      errorAlerts.report({ message: "Refused", recovery: "accountNotSignedIn" });
    });
    await settleFrames();
    alertHoldsFocus("Refused");
  });

  it.each([
    ["the queued alert shown once the first is acknowledged", false, false],
    ["an immediate alert put in front of the first", false, true],
    ["an immediate alert raised once the first has settled", true, true],
  ])("ends on %s, not its first control", async (_case, settleFirst, immediate) => {
    mountOverVirtuallyOpenedDialog(strict);
    act(() => errorAlerts.report({ message: "Refused", recovery: "accountNotSignedIn" }));
    if (settleFirst) await settleFrames();
    act(() => errorAlerts.report({ message: "Still refused", recovery: "accountNotSignedIn" }, { immediate }));
    if (!immediate) {
      await settleFrames();
      act(() => errorAlerts.acknowledge());
    }
    await settleFrames();
    alertHoldsFocus("Still refused");
  });

  // react-aria's dialog takes focus off itself half a second after it opens, when focus was not inside it as it
  // mounted, and puts it back once no transition is running on the page. A transition that never reports its end (in
  // Chrome, one still running inside a disclosure as it folds away) leaves focus on the page body until it ends.
  it("keeps focus on the alert while a transition underneath never ends", async () => {
    mountOverVirtuallyOpenedDialog(strict);
    const name = document.querySelector('input[aria-label="Name"]')!;
    name.dispatchEvent(Object.assign(new Event("transitionrun", { bubbles: true }), { propertyName: "translate" }));
    act(() => errorAlerts.report({ message: "Refused", recovery: "accountNotSignedIn" }));
    await settleFrames();
    await act(() => new Promise((resolve) => setTimeout(resolve, 600)));
    await settleFrames();
    alertHoldsFocus("Refused");
  });
});

// An alert raised in the same task as a press without a pointer that opens a dialog takes focus before the dialog's own
// opening focus, which react-aria puts off a frame and then drops, so acknowledging it gives focus back to nothing: the
// press left it on `<body>`. The window's fallback then puts it where that dialog's opening focus would have gone.
describe.each([{ strict: false }, { strict: true }])("acknowledged over a dialog its own press opened in the same task (StrictMode: $strict)", ({ strict }) => {
  const alertdialogWith = (text: string) => Array.from(document.querySelectorAll('[role="alertdialog"]')).find((dialog) => dialog.textContent?.includes(text)) ?? null;
  const cases: [string, () => React.ReactElement, () => Element | null][] = [
    [
      "a form dialog, on its first text field",
      () => <Dialog title="Open session" onClose={() => {}} submitLabel="Open" onSubmit={() => {}}><TextInput label="Name" value="" onChange={() => {}} autoFocus /></Dialog>,
      () => document.querySelector('[role="dialog"] input'),
    ],
    [
      "a dialog with no text field, on its submit button",
      () => <Dialog title="Export" onClose={() => {}} submitLabel="Export" autoFocusSubmit onSubmit={() => {}}><p>Everything is exported.</p></Dialog>,
      () => document.querySelector('[role="dialog"] button[type="submit"]'),
    ],
    [
      "a confirmation, on its confirm button",
      () => <ConfirmDialog title="Save project" message="Save it?" confirmLabel="Save" errorTitle="Could not save" onConfirm={async () => {}} onCancel={() => {}} />,
      () => alertdialogWith("Save it?")?.querySelector('button[type="submit"]') ?? null,
    ],
    [
      "a destructive confirmation, on the confirmation itself",
      () => <ConfirmDialog title="Delete project" message="Delete it?" confirmLabel="Delete" destructive errorTitle="Could not delete" onConfirm={async () => {}} onCancel={() => {}} />,
      () => alertdialogWith("Delete it?"),
    ],
    [
      "a confirmation asking for a typed word, on that field",
      () => <ConfirmDialog title="Delete project" message="Delete it?" confirmLabel="Delete" destructive typeToConfirm="delete" errorTitle="Could not delete" onConfirm={async () => {}} onCancel={() => {}} />,
      () => alertdialogWith("Delete it?")?.querySelector("input") ?? null,
    ],
  ];

  it.each(cases)("returns focus to %s", async (_case, dialog, landing) => {
    framesAsTimers();
    onTestFinished(setFocusAfterModalLoss(() => focusAfterLoss(() => {})));
    (document.activeElement as HTMLElement | null)?.blur();
    const { element, open } = openable(dialog(), true);
    mount(element, undefined, undefined, strict);
    act(() => {
      open();
      errorAlerts.report({ message: "Refused" });
    });
    await settleFrames();
    expect(document.activeElement).toBe(alertdialogWith("Refused"));
    // A person reads the alert before acknowledging it. Acknowledging within half a second of the dialog's mount would
    // race react-aria's `useDialog`, which then focuses the dialog itself if it finds focus on `<body>`, as it is in the
    // moment between the alert closing and the fallback running.
    await act(() => new Promise((resolve) => setTimeout(resolve, 600)));
    act(() => errorAlerts.acknowledge());
    await settleFrames();
    await act(() => new Promise((resolve) => setTimeout(resolve, 450)));
    await settleFrames();
    expect(landing()).not.toBeNull();
    expect(document.activeElement).toBe(landing());
  });

  it("returns focus to the control that had it when the alert came after the dialog settled", async () => {
    onTestFinished(setFocusAfterModalLoss(() => focusAfterLoss(() => {})));
    mount(<Dialog title="Open session" onClose={() => {}} submitLabel="Open" onSubmit={() => {}}><TextInput label="Name" value="" onChange={() => {}} autoFocus /></Dialog>, undefined, undefined, strict);
    await settleFrames();
    const cancel = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find((button) => button.textContent === "Cancel")!;
    act(() => cancel.focus());
    act(() => errorAlerts.report({ message: "Refused" }));
    await settleFrames();
    act(() => errorAlerts.acknowledge());
    await settleFrames();
    await act(() => new Promise((resolve) => setTimeout(resolve, 450)));
    expect(document.activeElement).toBe(cancel);
  });
});

it("shows foreign-realm diagnostic stacks and an accurate manual Issue fallback", () => {
  mount(undefined, { kind: "tauri" });
  const error = { message: "Foreign failure", stack: "Foreign stack trace" };
  act(() => errorAlerts.report({ message: error.message, error }));
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.querySelector("details")?.textContent).toContain("Foreign stack trace");
  expect(alert.textContent).toContain("Open https://github.com/baijunjie/octoboard/issues/new in your browser");
  expect(alert.textContent).toContain(issueUrl);
  expect(alert.textContent).not.toContain("use Report issue");
});

it.each(["Module load reason", { message: "Module load reason" }])("shows original diagnostics when a generic failure summary has no source stack: %s", (error) => {
  mount();
  act(() => errorAlerts.report({ message: "Formula rendering failed", content: <p>Formula rendering failed</p>, error }));
  expect(document.querySelector('[role="alertdialog"] details')?.textContent).toContain("Module load reason");
});

it("preserves an unhandled foreign rejection's message and stack through the reporting normalizer", () => {
  mount();
  act(() => reportError({ message: "Foreign rejection", stack: "Foreign rejection stack" }));
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.textContent).toContain("Foreign rejection");
  expect(alert.querySelector("details")?.textContent).toContain("Foreign rejection stack");
});

it("shows a failed account-switch batch as prose: the target once, a short reason per session, the remedy once, and the raw text only in Error details", async () => {
  const { runAccountSwitchBatch } = await import("../accountSwitchBatch");
  const { createStateStore } = await import("../store");
  const { sessionOf } = await import("../gallery/fixtures/builders");
  const sessions = ["one", "two"].map((id) => sessionOf(id, "c-1", "p-1", `Session ${id}`, "idle"));
  const store = createStateStore({ sessions: new Map(sessions.map((s) => [s.id, s])) });
  const switchOne = vi.fn(async () => {
    throw new DaemonRequestError("No space left on device (os error 28)", "relocation_failed", { path: "/home/w/.claude", detail: "No space left on device (os error 28)" });
  });
  mount();
  await act(async () => { await runAccountSwitchBatch(store, ["one", "two"], "a-1", { accountName: "Work", switchOne }); });
  const alert = document.querySelector<HTMLElement>('[role="alertdialog"]')!;
  expect(alert.querySelector("h2")?.textContent).toBe("Could not switch account");
  expect(alert.textContent).toContain("2 sessions could not move to Work");
  expect(alert.textContent?.match(/They stay on their current account\./g)).toHaveLength(1);
  expect(alert.textContent).toContain("Session one");
  expect(alert.textContent).toContain("Session two");
  expect(alert.textContent?.match(/Free space on the disk/g)).toHaveLength(1);
  expect(alert.textContent).not.toContain("Report issue");
  const details = alert.querySelector("details")!;
  expect(details.textContent).toContain("No space left on device (os error 28)");
  const visible = (alert.textContent ?? "").replace(details.textContent ?? "", "");
  expect(visible).not.toContain("os error 28");
  expect([...alert.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["OK"]);
});

async function failedBatch(failures: Record<string, Error>, accountName = "Work") {
  const { runAccountSwitchBatch } = await import("../accountSwitchBatch");
  const { createStateStore } = await import("../store");
  const { sessionOf } = await import("../gallery/fixtures/builders");
  const sessions = Object.keys(failures).map((id) => sessionOf(id, "c-1", "p-1", `Session ${id}`, "idle"));
  const store = createStateStore({ sessions: new Map(sessions.map((s) => [s.id, s])) });
  mount();
  await act(async () => {
    await runAccountSwitchBatch(store, Object.keys(failures), "a-1", { accountName, switchOne: async (id) => { throw failures[id]; } });
  });
  const alert = document.querySelector<HTMLElement>('[role="alertdialog"]')!;
  const details = alert.querySelector("details");
  const visible = (alert.textContent ?? "").replace(details?.textContent ?? "\u0000", "");
  return { alert, details, visible };
}

it("says a code's own remedy once for the code, and not the kind's generic remedy for it", async () => {
  const unreachable = () => new DaemonRequestError("raw", "config_dir_unreachable", { agent: "Claude Code", path: "/home/w/.claude" });
  const { visible } = await failedBatch({ a: unreachable(), b: unreachable(), c: new DaemonRequestError("Session is locked", "session_locked", {}) });
  expect(visible.match(/Recreate it, or point the account at a different directory\./g)).toHaveLength(1);
  expect(visible).not.toContain("correct the path in the dialog");
  expect(visible.match(/Unlock/g)?.length).toBe(1);
});

it("gives an unexpected failure a short reason in its row and keeps its stack for Error details only", async () => {
  const boom = new Error("Boom happened");
  const { visible, details } = await failedBatch({ a: boom });
  expect(visible).toContain("Failed unexpectedly.");
  expect(visible).not.toContain("Boom happened");
  expect(visible).toContain("Report issue");
  expect(details!.textContent).toContain("Boom happened");
  expect(details!.querySelector("h3")?.textContent).toBe("Session a");
  expect(details!.querySelector('[role="region"]')).toBeNull();
});

it("words each stage of a failed conversation copy, and says where an older item is kept", async () => {
  const stage = (stageName: string, extra: Record<string, string> = {}) => new DaemonRequestError("raw", "relocation_failed", { path: "/t", stage: stageName, detail: "disk full", ...extra });
  const { visible } = await failedBatch({
    a: stage("thread_record"),
    b: stage("kept_staging", { kept_in: "/t/.octoboard-switch/x" }),
  });
  expect(visible).toContain("was copied into /t, but the target account's record of the thread could not be updated");
  expect(visible).toContain("kept in /t/.octoboard-switch/x until the daemon next starts");
  expect(visible).not.toContain("disk full");
});

it("shows an alert's own details in place of the diagnostics it would work out for an unexpected failure", () => {
  mount();
  act(() => errorAlerts.report({ message: "Failed", error: new Error("worked out"), details: <p>supplied detail</p> }));
  const details = document.querySelector('[role="alertdialog"] details')!;
  expect(document.querySelectorAll('[role="alertdialog"] details')).toHaveLength(1);
  expect(details.textContent).toContain("supplied detail");
  expect(details.textContent).not.toContain("worked out");
});

it("puts a wrapped error's message ahead of a stack that has only frames, and a coded failure's message alone", async () => {
  const jsc = new Error("Boom happened");
  jsc.stack = "runBatch@http://localhost/app.js:10:5\nasync@[native code]";
  const { details } = await failedBatch({ a: jsc });
  expect(details!.textContent).toContain("Boom happened\nrunBatch@http://localhost/app.js:10:5");
  while (errorAlerts.getSnapshot().length) act(() => errorAlerts.acknowledge());
  const coded = new DaemonRequestError("Something internal", "internal_error", { detail: "no sentence for this" });
  const { details: codedDetails } = await failedBatch({ b: coded });
  expect(codedDetails?.textContent ?? "").not.toContain("@http");
});

it("puts the message ahead of a stack whose first line only ends the same way as a short message", async () => {
  const short = new Error("no");
  short.stack = "Error: another\nframe@x:1:1";
  const { details } = await failedBatch({ a: short });
  expect(details!.textContent).toContain("no\nError: another");
});

it("shows no Error details at all when no session has any, rather than every stack together", async () => {
  const unreachable = () => new DaemonRequestError("raw", "config_dir_unreachable", { agent: "Claude Code", path: "/home/w/.claude" });
  const first = unreachable();
  first.stack = "frames only";
  const { details } = await failedBatch({ a: first, b: unreachable() });
  expect(details).toBeNull();
});

it("keeps a shell-environment failure's own remedy and shows its command only in the details", async () => {
  const timeout = new DaemonRequestError("raw", "shell_environment_timeout", { shell: "/bin/zsh", command: "env", timeout: "5" });
  const { visible, details } = await failedBatch({ a: timeout });
  expect(visible).not.toContain("/bin/zsh");
  expect(details!.textContent).toContain("/bin/zsh -l -i -c 'env'");
  expect(visible).toContain("Fix or skip the slow step in the shell's rc files.");
});

it.each([
  ["permission_denied", "Check the folder’s file permissions"],
  ["no_space", "Free space on the disk"],
  ["busy", "Close other programs using the account's files"],
  ["config_unreadable", "readable file of valid TOML"],
  ["sqlite_home", "to an absolute path"],
  ["symlink", "contain a symbolic link"],
  ["not_restored", "copy it out of the folder named above"],
])("treats a conversation move that failed for %s as something to fix, not as a bug", async (cause, remedy) => {
  const stage = cause === "not_restored" ? "kept_staging" : cause === "symlink" ? "copy" : "thread_record";
  const failure = new DaemonRequestError("raw", "relocation_failed", { path: "/t", stage, cause, detail: "the system's own words", kept_in: "/t/.octoboard-switch/x" });
  const { visible, details } = await failedBatch({ a: failure });
  expect(visible).toContain(remedy);
  expect(visible).not.toContain("Report issue");
  expect(visible).not.toContain("the system's own words");
  expect(details!.textContent).toContain("the system's own words");
});

it("still treats a conversation move with cause other as unexpected, and an os error in its detail as the disk's", async () => {
  const other = new DaemonRequestError("raw", "relocation_failed", { path: "/t", stage: "copy", cause: "other", detail: "odd" });
  expect((await failedBatch({ a: other })).visible).toContain("Report issue");
  while (errorAlerts.getSnapshot().length) act(() => errorAlerts.acknowledge());
  const full = new DaemonRequestError("raw", "relocation_failed", { path: "/t", stage: "copy", cause: "other", detail: "No space left on device (os error 28)" });
  const { visible } = await failedBatch({ b: full });
  expect(visible).toContain("Free space on the disk");
});

it("says both remedies when an older item is kept aside and the system also reported a full disk", async () => {
  const failure = new DaemonRequestError("raw", "relocation_failed", { path: "/t", stage: "kept_staging", cause: "not_restored", detail: "No space left on device (os error 28)", kept_in: "/t/.octoboard-switch/x" });
  const { visible } = await failedBatch({ a: failure });
  expect(visible).toContain("copy it out of the folder named above");
  expect(visible).toContain("Free space on the disk");
  expect(visible.indexOf("copy it out")).toBeLessThan(visible.indexOf("Free space"));
  expect(visible).not.toContain("Report issue");
});

it.each([
  { case: "an internal error", error: new DaemonRequestError("database is locked", "internal_error", { detail: "database is locked" }), sentence: "Something unexpected went wrong.", raw: "database is locked" },
  { case: "a code the catalog lacks", error: new DaemonRequestError("a newer daemon's text", "a_newer_code", {}), sentence: "Something unexpected went wrong.", raw: "a newer daemon's text" },
  { case: "a code whose sentence leaves its detail out", error: new DaemonRequestError("fallback", "git_failed", { detail: "fatal: bad object" }), sentence: "Git could not read this.", raw: "fatal: bad object" },
])("says $case in the application's own words, the daemon's own text, not this window's stack, behind Error details", ({ error, sentence, raw }) => {
  mount();
  act(() => reportError(error));
  const alert = document.querySelector('[role="alertdialog"]')!;
  const details = alert.querySelectorAll("details");
  expect(details).toHaveLength(1);
  expect(details[0].querySelector("summary")?.textContent).toBe("Error details");
  expect(details[0].querySelector("pre")?.textContent).toBe(raw);
  const explanation = alert.cloneNode(true) as Element;
  explanation.querySelector("details")!.remove();
  expect(explanation.textContent?.replace(/[⁦-⁩]/g, "")).toContain(sentence);
  expect(explanation.textContent).not.toContain(raw);
  expect(explanation.querySelector("pre")).toBeNull();
});

it("keeps the sentence of a failure the client raises itself, which is already the application's own words", () => {
  mount();
  act(() => reportError(new DaemonRequestError("The request timed out.", "request_timed_out", {})));
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.textContent).toContain("The request timed out.");
  expect(alert.textContent).not.toContain("Something unexpected went wrong.");
});

it("adds no remedy line of the failure's kind after a daemon message that says what to do itself", () => {
  mount();
  act(() => reportError(new DaemonRequestError("archived", "session_archived", { session: "s1" })));
  const text = document.querySelector('[role="alertdialog"]')?.textContent;
  expect(text).toContain("Reopen it first.");
  expect(text).not.toContain("Select the current session in the sidebar");
  act(() => errorAlerts.acknowledge());
  // A body of the caller's own says nothing of the daemon's remedy, so the kind's line stays.
  act(() => reportError(new DaemonRequestError("archived", "session_archived", { session: "s1" }), undefined, undefined, <p>Archived.</p>));
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Select the current session in the sidebar");
});

// A dialog that opens a second one from a passive effect as it mounts (Settings opening an account's settings dialog for
// View in Settings, or its Add agent dialog for an alert's Add agent…) mounts that second one in the same commit as an
// alert raised in the task of the press that opened it: the alert must stay usable over both, and the second dialog
// be usable once the alert is acknowledged.
describe.each([{ strict: false }, { strict: true }])("raised as a dialog opens another dialog a commit later (StrictMode: $strict)", ({ strict }) => {
  it.each(["virtual", "pointer"] as const)("ends on the alert, which nothing hides (press: %s)", async (modality) => {
    framesAsTimers(modality);
    const { element, open } = openable(<OpensAnotherDialog />, true);
    mount(element, undefined, undefined, strict);
    act(() => {
      open();
      errorAlerts.report({ message: "Refused" });
    });
    await settleFrames();
    expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(2);
    alertHoldsFocus("Refused");
    // The alert's own hiding still covers both, as an alert raised over an open dialog's does.
    for (const covered of document.querySelectorAll('[role="dialog"]')) expect(covered.closest('[aria-hidden="true"], [inert]')).not.toBeNull();
  });

  it("leaves the second dialog usable once the alert is acknowledged", async () => {
    framesAsTimers();
    const { element, open } = openable(<OpensAnotherDialog />, true);
    mount(element, undefined, undefined, strict);
    act(() => {
      open();
      errorAlerts.report({ message: "Refused" });
    });
    await settleFrames();
    act(() => errorAlerts.acknowledge());
    await settleFrames();
    const account = Array.from(document.querySelectorAll('[role="dialog"]')).find((dialog) => dialog.textContent?.includes("Account settings"))!;
    expect(account.closest('[aria-hidden="true"], [inert]')).toBeNull();
  });
});

// A dialog that opens while an alert is up (a question the daemon pushes, a login taken up) goes on top of it, holding
// focus, and closing it puts focus back in the alert, which is still waiting for its acknowledgement.
describe.each([{ strict: false }, { strict: true }])("with a dialog opened after it (StrictMode: $strict)", ({ strict }) => {
  it.each(["pointer", "keyboard"] as const)("lets that dialog hold focus, then returns focus to the alert as it closes (press: %s)", async (modality) => {
    framesAsTimers(modality);
    let openLater = () => {};
    function Later() {
      const [shown, setShown] = useState(false);
      openLater = () => setShown(true);
      return shown ? <Dialog title="Later" onClose={() => setShown(false)} footer={null}><TextInput label="Field" value="" onChange={() => {}} autoFocus /></Dialog> : null;
    }
    mount(<Later />, undefined, undefined, strict);
    act(() => errorAlerts.report({ message: "Refused" }));
    await settleFrames();
    alertHoldsFocus("Refused");
    act(() => openLater());
    await settleFrames();
    const later = document.querySelector<HTMLElement>('[role="dialog"]')!;
    const field = later.querySelector("input")!;
    const covered = document.querySelector<HTMLElement>('[role="alertdialog"]')!;
    expect(later.closest('[aria-hidden="true"], [inert]')).toBeNull();
    expect(document.activeElement).toBe(field);
    // On top by its later place in the page, and the alert under it hidden by its hiding.
    expect(covered.compareDocumentPosition(later) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(covered.closest('[aria-hidden="true"], [inert]')).not.toBeNull();
    // jsdom has no Tab of its own: each press moves focus only through the later dialog's focus containment, from its
    // field to its close button and round again.
    for (let press = 0; press < 4; press++) {
      const before = document.activeElement;
      act(() => before!.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true })));
      expect(document.activeElement).not.toBe(before);
      expect(later.contains(document.activeElement)).toBe(true);
    }
    act(() => field.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
    await settleFrames();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    const alert = document.querySelector<HTMLElement>('[role="alertdialog"]')!;
    expect(alert.closest('[aria-hidden="true"], [inert]')).toBeNull();
    expect(alert.contains(document.activeElement)).toBe(true);
  });
});
