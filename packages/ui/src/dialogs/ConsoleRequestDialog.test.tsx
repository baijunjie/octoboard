// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { CONSOLE_REQUEST_NOT_WAITING } from "../protocol";
import { type ConsoleRequest, type TrustPrompt } from "../store";
import { mountPendingQuestionDialog, pressButton } from "./pendingQuestionTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request: ConsoleRequest = {
  requestId: "r-1",
  session: "s-1",
  console: "c-1",
  project: "p-1",
  requestedAt: 1,
};

const ack = () => Promise.resolve({ type: "ack" });

const mount = (reply = ack) => mountPendingQuestionDialog({ consoleRequests: [request] }, reply);

// Dismissing the dialog without choosing is a refusal, not a "later": the daemon holds the call
// waiting until it is answered.
it("dismissing the dialog refuses the request", async () => {
  const dialog = mount();
  const element = document.body.querySelector("[role=alertdialog]");
  await act(async () => {
    element?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
  expect(dialog.sent()).toEqual([{ type: "answer_console_session_request", request_id: "r-1", approve: false }]);
});

// Once the session has stopped waiting, an approval started nothing, which the user has to be told;
// a refusal would have started nothing anyway.
it.each([
  { label: "Start console session", toasted: 1 },
  { label: "Refuse", toasted: 0 },
])("an answer given after the session stopped waiting: $label", async ({ label, toasted }) => {
  const notWaiting = new DaemonRequestError("no longer waiting", CONSOLE_REQUEST_NOT_WAITING, { session: "s-1" });
  const dialog = mount(() => Promise.reject(notWaiting));
  await pressButton(label);
  const toasts = dialog.toasts();
  expect(toasts).toHaveLength(toasted);
  if (toasted) expect(toasts[0]).toEqual(["no longer waiting", "s-1"]);
});

// The window follows an approval to the console session it started, so the reply that names it is
// handed on; a refusal starts nothing.
it("approving hands on the console session the daemon started", async () => {
  const opened = { type: "session_opened", session: { id: "hub-1" } };
  const dialog = mount(() => Promise.resolve(opened));
  await pressButton("Start console session");
  expect(dialog.started()).toEqual([opened.session]);
});

// A request already on screen is not swapped for a trust prompt under the user's pointer; the
// trust prompt follows once the request has been answered.
it("a trust prompt arriving later waits behind the request on screen", async () => {
  const title = () => document.body.querySelector("[role=alertdialog] [slot=title]")?.textContent;
  const prompt: TrustPrompt = { session: "s-1", agent: "claude", project: "p-1", path: "/tmp/shop", trustDir: null };
  const dialog = mount();
  const titles: (string | null | undefined)[] = [];
  await act(async () => dialog.store.setState({ trustPrompts: [prompt] }));
  titles.push(title());
  await act(async () => dialog.store.setState({ consoleRequests: [] }));
  titles.push(title());
  expect(titles).toEqual(["Start a console session?", "Trust this folder?"]);
});
