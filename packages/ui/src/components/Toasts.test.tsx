// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { toast } from "@heroui/react";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { daemonMessage } from "../daemonMessage";
import { noticeErrorPresentation, noticeKind } from "../errorPresentation";
import { sessionOf } from "../gallery/fixtures/builders";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ErrorAlerts } from "../dialogs/ErrorAlerts";
import { errorAlerts } from "../errorAlerts";
import { Toasts } from "./Toasts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const daemon: Daemon = {
  store: createStateStore({}),
  request: vi.fn(),
  toastError: vi.fn(),
  toastNotice: vi.fn(),
  onToast: () => () => {},
  dismissTrustPrompt: () => {},
  reconnect: () => {},
} as unknown as Daemon;

class FakeResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

/** A `matchMedia` that answers the toast breakpoint's `min-width` query for a window `width` wide. */
function stubWidth(width: number): void {
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: width >= Number(/min-width: (\d+)px/.exec(query)?.[1]),
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}

afterEach(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); vi.unstubAllGlobals(); });

// The toast region only mounts once a toast is queued, so the placement is read off a shown one.
it("puts the toasts at the top in a window narrower than 640px and at the bottom end otherwise", () => {
  // The toast queue is a module-level singleton that closes a toast on a timer of its own, which an
  // unmounted root cannot cancel; faked timers let the test run those exits to the end itself.
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  for (const [width, expected] of [
    [390, "toast-region--top"],
    [639, "toast-region--top"],
    [640, "toast-region--bottom-end"],
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
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  stubWidth(1280);
  let deliver!: (request: import("../store").ToastRequest) => void;
  const source: Daemon = { ...daemon, onToast: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new DaemonRequestError("plain fallback", "shell_environment_timeout", { shell: "/bin/zsh", command: "env -0", timeout: "10s" });
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
    act(() => deliver({ kind: "error", message: "This request is no longer waiting.", daemonError: new DaemonRequestError("", "session_request_not_waiting", {}) }));
    expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("This request is no longer waiting.");
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it.each([
  ["account_in_use", { account: "a1", name: "Work", sessions: "2", signing_in: "false" }, "2 of its sessions are working", "Wait for the account’s working sessions to finish"],
  ["account_in_use", { account: "a1", name: "Work", sessions: "0", signing_in: "true" }, "being signed in", "for any sign-in of it to end"],
  ["account_sharing_in_progress", { account: "a1", name: "Work" }, "is being changed", "Wait for the change to finish"],
  ["account_sharing_failed", { account: "a1", name: "Work", detail: "x", items: "[]" }, "Nothing was rolled back", "Share default configuration switch in the Agent accounts settings off again"],
])("words the sharing refusal %s with the account and a remedy, not as an unexpected failure", (code, params, sentence, remedy) => {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  stubWidth(1280);
  let deliver!: (request: import("../store").ToastRequest) => void;
  const source: Daemon = { ...daemon, onToast: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new DaemonRequestError("plain fallback", code, params);
    act(() => deliver({ kind: "error", message: error.message, daemonError: error }));
    const text = document.querySelector('[role="alertdialog"]')?.textContent ?? "";
    expect(text).toContain("Work");
    expect(text).toContain(sentence);
    expect(text).toContain(remedy);
    expect(text).not.toContain("plain fallback");
    expect(text).not.toContain("Take a screenshot");
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("sets the path of a coded daemon error as code in its acknowledgement dialog", () => {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  stubWidth(1280);
  let deliver!: (request: import("../store").ToastRequest) => void;
  const source: Daemon = { ...daemon, onToast: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new DaemonRequestError("plain fallback", "path_not_found", { path: "/Users/dev/missing" });
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
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  stubWidth(1280);
  let deliver!: (request: import("../store").ToastRequest) => void;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({ sessions: new Map([["s1", sessionOf("s1", "c1", undefined, "Hub", "idle")]]) }),
    onToast: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new DaemonRequestError("plain fallback", "trust_not_carried_over", { agent: "Codex", path: "/Users/dev/code", reason: "why" });
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
  let deliver!: (request: import("../store").ToastRequest) => void;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({ sessions: new Map([["s1", sessionOf("s1", "c1", undefined, "Hub", "idle")]]) }),
    onToast: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new DaemonRequestError("plain fallback", code, params);
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
    act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("shows a failed launch's shell timeout once: the title names the launch, the details sit beneath it", () => {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  stubWidth(1280);
  let deliver!: (request: import("../store").ToastRequest) => void;
  const source: Daemon = { ...daemon, onToast: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    // Worded as the store does: the title without the reason, since the details carry it.
    const params = { title: "Draft the notes", reason_code: "shell_environment_timeout", shell: "/bin/zsh", command: "env -0", timeout: "10s" };
    const error = noticeErrorPresentation("launch_failed", params, "fallback");
    const message = daemonMessage("en", "launch_failed.title", params, "fallback", daemon.store.getState());
    act(() => deliver({ kind: "notice", message, daemonError: error, daemonTitle: new DaemonRequestError(message, "launch_failed.title", params) }));
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
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  stubWidth(1280);
  let deliver!: (request: import("../store").ToastRequest) => void;
  const session = { id: "s1", console_id: "c1", project_id: "p1" } as unknown as import("../protocol").Session;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({ sessions: new Map([["s1", session]]) }),
    onToast: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const error = new DaemonRequestError("fallback", "shell_environment_timeout", { shell: "/bin/zsh", command: "env -0", timeout: "10s" });
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

it("titles an error alert for what failed and names the session and its project in the body", () => {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  stubWidth(1280);
  let deliver!: (request: import("../store").ToastRequest) => void;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({
      sessions: new Map([["s1", sessionOf("s1", "c1", "p1", "Fix login", "idle")]]),
      projects: new Map([["p1", { id: "p1", name: "Hub" } as unknown as import("../protocol").Project]]),
    }),
    onToast: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    act(() => deliver({ kind: "error", message: "Not sent.", session: "s1", daemonError: new DaemonRequestError("", "message_not_sent", { agent: "Claude Code" }) }));
    const alert = document.querySelector('[role="alertdialog"]');
    expect(alert?.querySelector("h2")?.textContent).toBe("Message not sent");
    expect(alert?.textContent).toContain("Fix login");
    expect(alert?.textContent).toContain("Hub");
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("titles a failed launch's alert for the failure, never with the session's name, which is in its body, even when the session is gone", () => {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  stubWidth(1280);
  let deliver!: (request: import("../store").ToastRequest) => void;
  const source: Daemon = {
    ...daemon,
    store: createStateStore({
      sessions: new Map([["s1", sessionOf("s1", "c1", "p1", "Fix login", "idle")]]),
      projects: new Map([["p1", { id: "p1", name: "Hub" } as unknown as import("../protocol").Project]]),
    }),
    onToast: (listener) => { deliver = listener; return () => {}; },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const params = { title: "Fix login", reason_code: "path_not_found", path: "/missing" };
    const message = daemonMessage("en", "launch_failed", params, "fallback", source.store.getState());
    const request = { kind: "error" as const, message, daemonError: noticeErrorPresentation("launch_failed", params, message), daemonTitle: new DaemonRequestError(message, "launch_failed.title", params) };
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
  } finally {
    act(() => { toast.clear(); vi.runOnlyPendingTimers(); });
    act(() => root.unmount());
    container.remove();
  }
});

it("preserves a failed launch's name and nested path, and renders rich notices from their plain signature", () => {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  stubWidth(1280);
  let deliver!: (request: import("../store").ToastRequest) => void;
  const source: Daemon = { ...daemon, onToast: (listener) => { deliver = listener; return () => {}; } };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<DaemonProvider value={source}><Toasts focusTerminal={() => {}} /><ErrorAlerts daemon={source} /></DaemonProvider>));
    const params = { title: "Draft", reason_code: "path_not_found", path: "/missing" };
    const message = daemonMessage("en", "launch_failed", params, "fallback", daemon.store.getState());
    act(() => deliver({ kind: "notice", message, daemonError: noticeErrorPresentation("launch_failed", params, message), daemonTitle: new DaemonRequestError(message, "launch_failed.title", params) }));
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
