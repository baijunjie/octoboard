// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { CONSOLE_REQUEST_NOT_WAITING } from "../protocol";
import { createStateStore, DaemonProvider, type ConsoleRequest, type Daemon, type TrustPrompt } from "../store";
import { PendingQuestionDialog } from "./PendingQuestionDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request: ConsoleRequest = {
  requestId: "r-1",
  session: "s-1",
  console: "c-1",
  project: "p-1",
  requestedAt: 1,
};

/** Renders the dialog over a daemon whose answer resolves or rejects as given, runs `interact`
 * against it, and returns the requests it sent, the toasts it raised and the console sessions it
 * handed on as started. */
async function withDialog(
  reply: () => Promise<unknown>,
  interact: (store: Daemon["store"]) => Promise<void>,
): Promise<{ sent: unknown[]; toasts: unknown[][]; started: unknown[] }> {
  const started: unknown[] = [];
  const daemon: Daemon = {
    store: createStateStore({ consoleRequests: [request] }),
    request: vi.fn(reply) as never,
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <PendingQuestionDialog onConsoleSessionStarted={(session) => started.push(session)} />
      </DaemonProvider>,
    ),
  );
  await interact(daemon.store);
  const result = {
    started,
    sent: vi.mocked(daemon.request).mock.calls.map(([body]) => body),
    toasts: vi.mocked(daemon.toastError).mock.calls,
  };
  act(() => root.unmount());
  container.remove();
  return result;
}

const pressButton = (label: string) => async () => {
  const button = Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === label);
  await act(async () => button?.click());
};

// Dismissing the dialog without choosing is a refusal, not a "later": the daemon holds the call
// waiting until it is answered.
it("dismissing the dialog refuses the request", async () => {
  const { sent } = await withDialog(
    () => Promise.resolve({ type: "ack" }),
    async () => {
      const dialog = document.body.querySelector("[role=alertdialog]");
      await act(async () => {
        dialog?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      });
    },
  );
  expect(sent).toEqual([{ type: "answer_console_session_request", request_id: "r-1", approve: false }]);
});

// Once the session has stopped waiting, an approval started nothing, which the user has to be told;
// a refusal would have started nothing anyway.
it.each([
  { label: "Start console session", toasted: 1 },
  { label: "Refuse", toasted: 0 },
])("an answer given after the session stopped waiting: $label", async ({ label, toasted }) => {
  const notWaiting = new DaemonRequestError("no longer waiting", CONSOLE_REQUEST_NOT_WAITING, { session: "s-1" });
  const { toasts } = await withDialog(() => Promise.reject(notWaiting), pressButton(label));
  expect(toasts).toHaveLength(toasted);
  if (toasted) expect(toasts[0]).toEqual(["no longer waiting", "s-1"]);
});

// The window follows an approval to the console session it started, so the reply that names it is
// handed on; a refusal starts nothing.
it("approving hands on the console session the daemon started", async () => {
  const opened = { type: "session_opened", session: { id: "hub-1" } };
  const { started } = await withDialog(() => Promise.resolve(opened), pressButton("Start console session"));
  expect(started).toEqual([opened.session]);
});

// A request already on screen is not swapped for a trust prompt under the user's pointer; the
// trust prompt follows once the request has been answered.
it("a trust prompt arriving later waits behind the request on screen", async () => {
  const title = () => document.body.querySelector("[role=alertdialog] [slot=title]")?.textContent;
  const prompt: TrustPrompt = { session: "s-1", agent: "claude", project: "p-1", path: "/tmp/shop", trustDir: null };
  const titles: (string | null | undefined)[] = [];
  await withDialog(
    () => Promise.resolve({ type: "ack" }),
    async (store) => {
      await act(async () => store.setState({ trustPrompts: [prompt] }));
      titles.push(title());
      await act(async () => store.setState({ consoleRequests: [] }));
      titles.push(title());
    },
  );
  expect(titles).toEqual(["Start a console session?", "Trust this folder?"]);
});
