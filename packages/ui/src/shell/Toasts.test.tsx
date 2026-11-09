// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { toast } from "@heroui/react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../errors/codedError";
import { daemonMessage } from "../errors/daemonMessage";
import { noticeErrorPresentation, noticeKind } from "../errors/errorPresentation";
import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { createStateStore, DaemonProvider, errorNotification, type Daemon } from "../store";
import { broadcastNotification } from "../store/notifications";
import { testDaemon } from "../test/testDaemon";
import { ErrorAlerts } from "./ErrorAlerts";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { Toasts } from "./Toasts";
import { matchesWidthQuery } from "../test/viewport";
import { fakeTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const daemon = testDaemon({ request: vi.fn(), showNotice: vi.fn() });

class FakeResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

/** A `matchMedia` that answers the toast breakpoint's width query for a window `width` wide. */
function stubWidth(width: number): void {
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: matchesWidthQuery(query, width),
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}
// Registered before any test's `fakeTimers()`, so the globals stay stubbed while its run-out fires the timers still pending.
beforeEach(() => {
  onTestFinished(() => void vi.unstubAllGlobals());
});

it.each(["model_change_did_not_stop", "model_change_did_not_come_up"])("names the affected session once in a %s alert and retains the failure metadata", (code) => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({ sessions: new Map([["s1", sessionOf("s1", "c1", "p1", "Fix login", "idle")]]) }),
    onNotification: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const params = { session: "s1" };
    const error = new CodedError(daemonMessage("en", code, params, "fallback", source.store.getState()), code, params);
    act(() => deliver(errorNotification(error, { title: "Could not change the model", session: "s1" })));
    const alert = document.querySelector('[role="alertdialog"]')!;
    expect(alert.querySelector("h2")?.textContent).toBe("Could not change the model");
    expect(alert.textContent?.split("Fix login")).toHaveLength(2);
    expect(alert.textContent).toContain(code === "model_change_did_not_stop" ? "The session did not stop in time" : "The session ended as soon as it was relaunched");
    expect(errorAlerts.getSnapshot()[0].error).toBe(error);
    expect(error.params).toEqual({ session: "s1" });
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

// The toast region only mounts once a toast is queued, so the placement is read off a shown one.
it("puts the toasts at the top through iPad portrait and at the bottom end above 768px", () => {
  // The toast queue is a module-level singleton that closes a toast on a timer of its own, which an
  // unmounted root cannot cancel; faked timers let the test run those exits to the end itself.
  fakeTimers();
  for (const [width, expected] of [
    [390, "toast-region--top"],
    [767, "toast-region--top"],
    [768, "toast-region--top"],
    [768.5, "toast-region--bottom-end"],
    [769, "toast-region--bottom-end"],
    [1280, "toast-region--bottom-end"],
  ] as const) {
    stubWidth(width);
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    try {
      act(() => {
        root.render(
          <DaemonProvider value={daemon}>
            <Toasts focusTerminal={() => {}} />
          </DaemonProvider>,
        );
      });
      act(() => {
        toast("hello");
      });
      expect(document.querySelector(".toast-region")?.className).toContain(expected);
    } finally {
      // Clearing notifies the mounted region, so it is a React update; the exits it starts finish here
      // so that no toast is left queued or exiting for the next width.
      act(() => {
        toast.clear();
        vi.runOnlyPendingTimers();
      });
      act(() => root.unmount());
      container.remove();
      // After the teardown, so a failure here cannot leave the root mounted.
      expect(vi.getTimerCount()).toBe(0);
      expect(toast.getQueue().visibleToasts).toHaveLength(0);
    }
  }
});

it("renders a coded shell timeout with a separate title and command while plain messages stay plain", () => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = { ...daemon, onNotification: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new CodedError("plain fallback", "shell_environment_timeout", { shell: "/bin/zsh", command: "env -0", timeout: "10s" });
    act(() => deliver({ kind: "error", message: error.message, daemonError: error }));
    const alert = document.querySelector('[role="alertdialog"]');
    expect(alert?.textContent).toContain("Shell environment timed out");
    expect(alert?.querySelector("code")?.textContent).toBe("/bin/zsh -l -i -c 'env -0'");
    expect(alert?.textContent).toContain("10s");
    expect(alert?.textContent).not.toContain("plain fallback");
    expect(document.querySelector('[data-slot="toast-title"]')).toBeNull();
    act(() => deliver({ kind: "error", message: "a plain failure" }));
    expect(errorAlerts.getSnapshot()).toHaveLength(2);
    act(() => errorAlerts.acknowledge());
    expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("a plain failure");
    act(() => errorAlerts.acknowledge());
    act(() => deliver({ kind: "error", message: "This request is no longer waiting.", daemonError: new CodedError("", "session_request_not_waiting", {}) }));
    expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("This request is no longer waiting.");
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it.each([
  ["account_in_use", { account: "a1", name: "Work", sessions: "2", signing_in: "false" }, "2 of its sessions are working", "Wait for the account’s working sessions and login to finish, then try again."],
  ["account_in_use", { account: "a1", name: "Work", sessions: "0", signing_in: "true" }, "being signed in", "Wait for the account’s working sessions and login to finish, then try again."],
  ["account_sharing_in_progress", { account: "a1", name: "Work" }, "is being changed", "Wait for the change to finish"],
  ["account_sharing_failed", { account: "a1", name: "Work", detail: "x", items: "[]" }, "Nothing was rolled back", "try once more with the account’s Share default configuration switch"],
])("words the sharing refusal %s with the account and a remedy, not as an unexpected failure", (code, params, sentence, remedy) => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = { ...daemon, onNotification: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new CodedError("plain fallback", code, params);
    act(() => deliver({ kind: "error", message: error.message, daemonError: error }));
    const text = document.querySelector('[role="alertdialog"]')?.textContent ?? "";
    expect(text).toContain("Work");
    expect(text).toContain(sentence);
    expect(text).toContain(remedy);
    expect(text).not.toContain("plain fallback");
    expect(text).not.toContain("This may be a bug");
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it.each([
  ["account_sharing_in_progress", { account: "a1", name: "Work" }, "is being changed", "Wait for the change to finish"],
  ["config_dir_unreachable", { agent: "Claude Code", path: "/home/w/.claude" }, "is not a directory the daemon can reach", "correct the path in the dialog"],
  ["relocation_failed", { path: "/t", stage: "copy", cause: "other", detail: "odd" }, "could not be copied into", "Then try the switch again"],
])("shows a failed automatic switch whose cause is %s with its own sentence as the only remedy", (reasonCode, nested, reasonLine, remedy) => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = { ...daemon, onNotification: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    // The notice's own params over the failure's, as the daemon puts them.
    const params = { ...nested, agent: "Claude Code", from: "Work", account: "Spare", reason: "raw", reason_code: reasonCode };
    const error = new CodedError("plain fallback", "account_switch_automatically_failed", params);
    act(() => deliver({ kind: "error", message: error.message, daemonError: error }));
    const alert = document.querySelector('[role="alertdialog"]')!;
    expect(alert.textContent).toContain("Could not switch account");
    expect(alert.textContent).toContain("nor bring it back up on");
    expect(alert.textContent).toContain(reasonLine);
    expect(alert.textContent).not.toContain(remedy);
    expect(alert.textContent).not.toContain("Report issue");
    // Diagnostic copy controls stay in Error details; the only action outside is the acknowledgement.
    const actions = [...alert.querySelectorAll("button")].filter((button) => !button.closest("details"));
    expect(actions.map((button) => button.textContent)).toEqual(["OK"]);
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("tells a failed automatic switch's user to copy out the older item it kept aside, without asking them to try the switch again", () => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = { ...daemon, onNotification: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const params = { path: "/t", stage: "kept_staging", cause: "not_restored", kept_in: "/t/.octoboard-switch/x", detail: "disk full", agent: "Claude Code", from: "Work", account: "Spare", reason: "raw", reason_code: "relocation_failed" };
    const error = new CodedError("plain fallback", "account_switch_automatically_failed", params);
    act(() => deliver({ kind: "error", message: error.message, daemonError: error }));
    const text = document.querySelector('[role="alertdialog"]')!.textContent;
    expect(text).toContain("nor bring it back up on");
    expect(text).toContain("An older item could not be put back");
    expect(text).toContain("copy it out of the folder named above");
    expect(text).not.toContain("Then try the switch again");
    // When the older item is cleared is said once, by the reason; the advice does not repeat it.
    expect(text.split("until the daemon next starts")).toHaveLength(2);
    expect(text.split("next switched into that account")).toHaveLength(2);
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("adds the remedy for what the system said to the copy-out advice of a failed automatic switch, each once", () => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = { ...daemon, onNotification: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const params = { path: "/t", stage: "kept_staging", cause: "not_restored", kept_in: "/t/.octoboard-switch/x", detail: "No space left on device (os error 28)", agent: "Claude Code", from: "Work", account: "Spare", reason: "raw", reason_code: "relocation_failed" };
    const error = new CodedError("plain fallback", "account_switch_automatically_failed", params);
    act(() => deliver({ kind: "error", message: error.message, daemonError: error }));
    const text = document.querySelector('[role="alertdialog"]')!.textContent ?? "";
    const copyOut = text.indexOf("copy it out of the folder named above");
    const free = text.indexOf("Free space on the disk");
    expect(copyOut).toBeGreaterThan(-1);
    expect(free).toBeGreaterThan(copyOut);
    expect(text.split("Free space on the disk")).toHaveLength(2);
    expect(text.split("copy it out of the folder named above")).toHaveLength(2);
    expect(text).not.toContain("Then try the switch again");
    expect(text).not.toContain("retry the operation");
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("adds the remedy for what the system said to a failed automatic switch with a worded cause", () => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = { ...daemon, onNotification: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const params = { path: "/t", stage: "copy", cause: "other", detail: "No space left on device (os error 28)", agent: "Claude Code", from: "Work", account: "Spare", reason: "raw", reason_code: "relocation_failed" };
    const error = new CodedError("plain fallback", "account_switch_automatically_failed", params);
    act(() => deliver({ kind: "error", message: error.message, daemonError: error }));
    const text = document.querySelector('[role="alertdialog"]')!.textContent ?? "";
    expect(text).toContain("nor bring it back up on");
    expect(text).toContain("could not be copied into");
    expect(text.split("Free space on the disk")).toHaveLength(2);
    expect(text).not.toContain("retry the operation");
    // The sentence, then why, then what to do.
    expect(text.indexOf("nor bring it back up on")).toBeLessThan(text.indexOf("could not be copied into"));
    expect(text.indexOf("could not be copied into")).toBeLessThan(text.indexOf("Free space on the disk"));
    expect(text).not.toContain("Report issue");
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it.each([
  ["a cause the application has no words for", "future_code", "a newer daemon's reason", true],
  ["an internal cause with nothing recognisable in its text", "internal_error", "spawn failed", true],
  ["an internal cause whose text names a full disk", "internal_error", "No space left on device (os error 28)", false],
])("keeps the report guidance for a failed automatic switch with %s only when nothing in it is recognised", (_case, reasonCode, reason, reported) => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = { ...daemon, onNotification: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const params = { agent: "Claude Code", from: "Work", account: "Spare", reason, reason_code: reasonCode };
    const error = new CodedError("plain fallback", "account_switch_automatically_failed", params);
    act(() => deliver({ kind: "error", message: error.message, daemonError: error }));
    const alert = document.querySelector('[role="alertdialog"]')!;
    expect(alert.textContent).toContain("nor bring it back up on");
    expect([...alert.querySelectorAll("a")].some((link) => link.textContent === "Report issue")).toBe(reported);
    expect(alert.textContent?.includes("This may be a bug")).toBe(reported);
    // The reason line is the generic sentence for what the system said, or for nothing recognised.
    expect(alert.textContent).toContain(reported ? "Something unexpected went wrong." : "The disk is full.");
    // A recognised system failure adds its remedy to the sentence's.
    expect(alert.textContent?.includes("Free space on the disk")).toBe(!reported);
    // The raw text is behind "Error details" either way.
    expect(alert.querySelector("pre")?.textContent).toBe(reason);
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("sets the path of a coded daemon error as code in its acknowledgement dialog", () => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = { ...daemon, onNotification: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new CodedError("plain fallback", "path_not_found", { path: "/Users/dev/missing" });
    act(() => deliver({ kind: "error", message: error.message, daemonError: error }));
    const alert = document.querySelector('[role="alertdialog"]');
    expect(alert?.querySelector("code")?.textContent).toBe("/Users/dev/missing");
    expect(alert?.textContent).toContain("/Users/dev/missing does not exist.");
    expect(document.querySelector('[data-slot="toast-title"]')).toBeNull();
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("shows a coded notice under the session's location, the path as code in the description", () => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({ sessions: new Map([["s1", sessionOf("s1", "c1", undefined, "Hub", "idle")]]) }),
    onNotification: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new CodedError("plain fallback", "trust_not_carried_over", { agent: "Codex", path: "/Users/dev/code", reason: "why" });
    act(() => deliver({ kind: "notice", message: error.message, daemonError: error, session: "s1" }));
    const description = document.querySelector('[data-slot="toast-description"]');
    expect(description?.querySelector("code")?.textContent).toBe("/Users/dev/code");
    expect(document.querySelector('[data-slot="toast-title"]')?.querySelector("code")).toBeNull();
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it.each<{ code: string; params: Record<string, string>; title: string; sentence: string; raw: string }>([
  { code: "sharing_resume_failed", params: { account: "a1", name: "Work", detail: "no such file or directory" }, title: "Could not resume the session", sentence: "could not be resumed after its sharing changed", raw: "no such file or directory" },
  { code: "trust_not_carried_over", params: { agent: "Grok Build", path: "/Users/dev/.grok/trust.json", reason: "x", reason_code: "store_io_failed", detail: "Permission denied (os error 13)" }, title: "Trust not carried over", sentence: "the file could not be read or written", raw: "Permission denied (os error 13)" },
  // The repair's own message is its raw reason; the cause is said with its fix in the body.
  { code: "account_sharing_repair_failed", params: { account: "a1", name: "Work", items: "[]", cause: "permission_denied" }, title: "Could not repair sharing links", sentence: "You do not have permission to change the account’s directory.", raw: "plain fallback" },
])("shows a $code notice as an error alert, not a toast, its sentence the body and the system's text behind Error details, under a plain title", ({ code, params, title, sentence, raw }) => {
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({ sessions: new Map([["s1", sessionOf("s1", "c1", undefined, "Hub", "idle")]]) }),
    onNotification: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new CodedError("plain fallback", code, params);
    // As the store routes a session notice.
    act(() => deliver({ kind: noticeKind(code, params), message: error.message, daemonError: error, session: "s1" }));
    expect(document.querySelector('[data-slot="toast-description"]')).toBeNull();
    const alert = document.querySelector('[role="alertdialog"]')!;
    expect(alert.querySelector("h2")?.textContent).toBe(title);
    expect(alert.textContent).toContain("Hub");
    expect(alert.querySelector("details pre")?.textContent).toBe(raw);
    const explanation = alert.cloneNode(true) as Element;
    explanation.querySelector("details")!.remove();
    expect(explanation.textContent).toContain(sentence);
    expect(explanation.textContent).not.toContain(raw);
    // An expected failure, not one to report as a bug.
    expect(explanation.textContent).not.toContain("Report issue");
  } finally {
    act(() => root.unmount());
    container.remove();
  }
});

it("shows a failed launch's shell timeout once: the title names the launch, the details sit beneath it", () => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = { ...daemon, onNotification: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    // Worded as the store does: the title without the reason, since the details carry it.
    const params = { title: "Draft the notes", reason_code: "shell_environment_timeout", shell: "/bin/zsh", command: "env -0", timeout: "10s" };
    const error = noticeErrorPresentation("launch_failed", params, "fallback");
    const message = daemonMessage("en", "launch_failed.title", params, "fallback", daemon.store.getState());
    act(() => deliver({ kind: "notice", message, daemonError: error, daemonTitle: new CodedError(message, "launch_failed.title", params) }));
    expect(document.querySelector('[data-slot="toast-title"]')?.textContent).toBe("Could not start Draft the notes");
    expect(document.querySelector('[data-slot="toast-title"] [data-inline-reference]')?.textContent).toBe("Draft the notes");
    const description = document.querySelector('[data-slot="toast-description"]');
    expect(description?.querySelector("h3")).toBeNull();
    expect(description?.querySelector("code")?.textContent).toBe("/bin/zsh -l -i -c 'env -0'");
    expect(document.body.textContent?.split("did not finish within").length).toBe(2);
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("shows the error heading above a structured error's details when the alert has a session", () => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const session = { id: "s1", console_id: "c1", project_id: "p1" } as unknown as import("../protocol").Session;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({ sessions: new Map([["s1", session]]) }),
    onNotification: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new CodedError("fallback", "shell_environment_timeout", { shell: "/bin/zsh", command: "env -0", timeout: "10s" });
    act(() => deliver({ kind: "error", message: error.message, session: "s1", daemonError: error }));
    const alert = document.querySelector('[role="alertdialog"]');
    expect(alert?.querySelector("h3")?.textContent).toBe("Shell environment timed out");
    expect(alert?.querySelector("code")?.textContent).toBe("/bin/zsh -l -i -c 'env -0'");
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it.each([
  ["project", "p1", true, "Project: Search API", "Search API"],
  ["console", undefined, true, "Console: Main", "Main"],
  ["missing project", "p1", false, "Project: Unknown", undefined],
  ["missing console", undefined, false, "Console: Unknown", undefined],
] as const)("labels the session and its %s separately in an error alert", (_case, projectId, known, placeText, placeName) => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({
      sessions: new Map([["s1", sessionOf("s1", "c1", projectId, "Fix 登录 مراجعة", "idle")]]),
      projects: new Map(known ? [["p1", projectOf("p1", "c1", "Search API")]] : []),
      consoles: new Map(known ? [["c1", consoleOf("c1", "Main")]] : []),
    }),
    onNotification: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    act(() => deliver({ kind: "error", message: "Not sent.", session: "s1", daemonError: new CodedError("", "message_not_sent", { agent: "Claude Code" }) }));
    const alert = document.querySelector('[role="alertdialog"]');
    expect(alert?.querySelector("h2")?.textContent).toBe("Message not sent");
    const sessionLine = [...alert!.querySelectorAll("p")].find((line) => line.textContent === "Session: Fix 登录 مراجعة")!;
    const context = sessionLine.parentElement!;
    expect([...context.children].map((line) => line.textContent)).toEqual(["Session: Fix 登录 مراجعة", placeText]);
    expect(context.children[0].querySelector("[data-inline-reference]")?.textContent).toBe("Fix 登录 مراجعة");
    expect(context.children[1].querySelector("[data-inline-reference]")?.textContent).toBe(placeName);
    for (const chip of context.querySelectorAll("[data-inline-reference]")) {
      expect(chip.getAttribute("dir")).toBe("auto");
      expect(chip.textContent).not.toMatch(/[\u2066-\u2069]/);
    }
    expect(alert?.textContent?.split("Fix 登录 مراجعة")).toHaveLength(2);
    if (placeName) expect(alert?.textContent?.split(placeName)).toHaveLength(2);
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("titles a failed launch's alert for the failure, never with the session's name, which is in its body, even when the session is gone", () => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({
      sessions: new Map([["s1", sessionOf("s1", "c1", "p1", "Fix login", "idle")]]),
      projects: new Map([["p1", { id: "p1", name: "Hub" } as unknown as import("../protocol").Project]]),
    }),
    onNotification: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const params = { title: "Fix login", reason_code: "path_not_found", path: "/missing" };
    const message = daemonMessage("en", "launch_failed", params, "fallback", source.store.getState());
    const request = { kind: "error" as const, message, daemonError: noticeErrorPresentation("launch_failed", params, message), daemonTitle: new CodedError(message, "launch_failed.title", params) };
    act(() => deliver({ ...request, session: "s1" }));
    const alert = document.querySelector('[role="alertdialog"]');
    expect(alert?.querySelector("h2")?.textContent).toBe("Could not start the session");
    expect(alert?.textContent).toContain("Fix login");
    act(() => document.querySelector<HTMLElement>('[role="alertdialog"] button')?.click());
    act(() => deliver({ ...request, session: "gone" }));
    const gone = document.querySelector('[role="alertdialog"]');
    expect(gone?.querySelector("h2")?.textContent).toBe("Could not start the session");
    expect(gone?.querySelector("h2")?.textContent).not.toContain("Fix login");
    expect(gone?.textContent).toContain("Session: Fix login");
    expect(gone?.textContent).not.toMatch(/Project:|Console:/);
    act(() => errorAlerts.acknowledge());
    act(() => deliver(errorNotification(new CodedError("The session is gone.", "session_archived", {}), { title: undefined, session: "unknown" })));
    expect(document.querySelector('[role="alertdialog"]')?.textContent).not.toMatch(/Session:|Project:|Console:/);
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("titles the toast of an automatic account switch with the session's project and names the switched session in its description", () => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({
      sessions: new Map([["s1", sessionOf("s1", "c1", "p1", "Fix login", "idle")]]),
      projects: new Map([["p1", { id: "p1", name: "Hub" } as unknown as import("../protocol").Project]]),
    }),
    onNotification: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const params = { agent: "Claude Code", from: "Work", account: "Spare", session: "s1" };
    const message = daemonMessage("en", "account_switched_automatically", params, "fallback", source.store.getState());
    const error = new CodedError(message, "account_switched_automatically", params);
    act(() => deliver({ kind: "notice", message, daemonError: error, session: "s1" }));
    // The title says where the session is (its project); the sentence names the session itself.
    expect(document.querySelector('[data-slot="toast-title"]')?.textContent).toBe("Hub");
    expect(document.querySelector('[data-slot="toast-description"]')?.textContent).toContain("switched Fix login to");
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("preserves a failed launch's name and nested path, and renders rich notices from their plain signature", () => {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = { ...daemon, onNotification: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const params = { title: "Draft", reason_code: "path_not_found", path: "/missing" };
    const message = daemonMessage("en", "launch_failed", params, "fallback", daemon.store.getState());
    act(() => deliver({ kind: "notice", message, daemonError: noticeErrorPresentation("launch_failed", params, message), daemonTitle: new CodedError(message, "launch_failed.title", params) }));
    const title = document.querySelector('[data-slot="toast-title"]');
    const description = document.querySelector('[data-slot="toast-description"]');
    expect(title?.querySelector("[data-inline-reference]")?.textContent).toBe("Draft");
    expect(title?.querySelector("code")).toBeNull();
    expect(description?.querySelector("code")?.textContent).toBe("/missing");
    expect(description?.textContent).toBe("/missing does not exist.");
    expect(message).toContain("does not exist.");
    act(() => deliver({ kind: "notice", message: "Saved export.json", content: <span>Saved <em>export.json</em></span> }));
    expect(document.querySelector('[data-slot="toast-title"] em')?.textContent).toBe("export.json");
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

/** Mounts the toasts and the alert over a daemon whose notifications `deliver` sends, and gives the daemon's two
 * broadcasts about a session's `code` as notifications. */
function mountForWithdrawal() {
  fakeTimers();
  stubWidth(1280);
  let deliver!: (request: import("../store").WindowNotification) => void;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({ sessions: new Map([["s1", sessionOf("s1", "c1", "p1", "Fix login", "idle")], ["s2", sessionOf("s2", "c1", "p1", "Add filters", "idle")]]) }),
    onNotification: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
  onTestFinished(() => {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  });
  const state = source.store.getState();
  return {
    notice: (session: string, code = "message_not_sent") => act(() => deliver(broadcastNotification({ type: "session_notice", session, code, params: {}, message: "raw" }, state, "en"))),
    withdraw: (session: string, code = "message_not_sent") => act(() => deliver(broadcastNotification({ type: "session_notice_withdrawn", session, code }, state, "en"))),
  };
}
const shownAlert = () => document.querySelector('[role="alertdialog"]');

it("closes the shown alert of a notice the daemon withdraws, and shows the one queued behind it", () => {
  const { notice, withdraw } = mountForWithdrawal();
  notice("s1");
  notice("s2");
  expect(shownAlert()?.textContent).toContain("Fix login");
  withdraw("s1");
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(shownAlert()?.textContent).toContain("Add filters");
  withdraw("s2");
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  expect(shownAlert()).toBeNull();
});

it("takes a queued alert out of the queue when its notice is withdrawn, leaving the one shown", () => {
  const { notice, withdraw } = mountForWithdrawal();
  notice("s1");
  notice("s2");
  withdraw("s2");
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(shownAlert()?.textContent).toContain("Fix login");
});

it("leaves the alerts of another session or another code alone when a notice is withdrawn", () => {
  const { notice, withdraw } = mountForWithdrawal();
  notice("s1");
  notice("s1", "queued_messages_dropped");
  withdraw("s2");
  withdraw("s1", "draft_save_unconfirmed");
  expect(errorAlerts.getSnapshot()).toHaveLength(2);
  expect(shownAlert()?.textContent).toContain("Fix login");
});

it("ignores the withdrawal of a notice nothing shows, and of one already acknowledged", () => {
  const { notice, withdraw } = mountForWithdrawal();
  withdraw("s1");
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  notice("s1");
  act(() => errorAlerts.acknowledge());
  withdraw("s1");
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  expect(document.querySelector('[data-slot="toast-title"]')).toBeNull();
});
