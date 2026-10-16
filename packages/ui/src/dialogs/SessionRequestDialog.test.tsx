// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { SESSION_REQUEST_NOT_WAITING } from "../protocol";
import type { SessionRequest, TrustPrompt } from "../store";
import { mountPendingQuestionDialog, pressButton } from "./pendingQuestionTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request: SessionRequest = {
  requestId: "r-1",
  session: "s-1",
  role: "project",
  console: "c-1",
  project: "p-1",
  agent: "codex",
  requestedAt: 2,
};

const consoleRequest: SessionRequest = { ...request, requestId: "r-console", session: "s-2", role: "console", project: null, requestedAt: 1 };

const ack = () => Promise.resolve({ type: "ack" });

const mount = (reply = ack) => mountPendingQuestionDialog({ sessionRequests: [request] }, reply);

// Without the marks that isolate each name, which say nothing about the words around them.
const dialogText = () => (document.body.querySelector("[role=alertdialog]")?.textContent ?? "").replace(/[\u2066-\u2069]/g, "");

// What the user decides on is on the dialog: the agent, where the session would be, that it would
// report to them — for a project session and a console session alike.
it.each([
  { asked: request, where: "The session s-1 asks to start a Codex session in the project p-1 of the console c-1" },
  { asked: consoleRequest, where: "The session s-2 asks to start a Codex console session in the console c-1" },
])("shows who asks, where the session would be, that it reports to the user: $asked.role", ({ asked, where }) => {
  mountPendingQuestionDialog({ sessionRequests: [asked] }, ack);
  expect(dialogText()).toContain("Start a session?");
  expect(dialogText()).toContain(where);
  expect(dialogText()).toContain("The new session would report to you.");
  expect([...document.querySelectorAll("[data-inline-chip]")].map((chip) => chip.textContent)).toEqual([asked.session, asked.project, asked.console].filter(Boolean));
});

// The requester may live in another project or console than the one it asks in, which the dialog is
// the only place to see; its console is named only where it is not the one the session is asked in.
it.each([
  { requester: sessionOf("s-1", "c-1", "p-3", "Billing agent", "working"), where: "The session Billing agent in the project Shop asks to start" },
  { requester: sessionOf("s-1", "c-1", undefined, "Planner", "working"), where: "The console session Planner asks to start" },
  { requester: sessionOf("s-1", "c-2", "p-2", "Billing agent", "working"), where: "The session Billing agent in the project Ledger of the console Other asks to start" },
  { requester: sessionOf("s-1", "c-2", undefined, "Planner", "working"), where: "The session Planner in the console Other asks to start" },
])("names where the requester is: $where", ({ requester, where }) => {
  mountPendingQuestionDialog(
    {
      sessions: new Map([[requester.id, requester]]),
      consoles: new Map([consoleOf("c-1", "Main"), consoleOf("c-2", "Other")].map((c) => [c.id, c])),
      projects: new Map([projectOf("p-2", "c-2", "Ledger"), projectOf("p-3", "c-1", "Shop")].map((p) => [p.id, p])),
      sessionRequests: [request],
    },
    ack,
  );
  expect(dialogText()).toContain(where);
});

// Dismissing the dialog without choosing is a refusal, not a "later": the daemon holds the call
// waiting until it is answered.
it("dismissing the dialog refuses the request", async () => {
  const dialog = mount();
  const element = document.body.querySelector("[role=alertdialog]");
  await act(async () => {
    element?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
  expect(dialog.sent()).toEqual([{ type: "answer_session_request", request_id: "r-1", approve: false }]);
});

// Once the session has stopped waiting, an approval started nothing, which the user has to be told;
// a refusal would have started nothing anyway.
it.each([
  { label: "Start session", toasted: 1 },
  { label: "Refuse", toasted: 0 },
])("an answer given after the session stopped waiting: $label", async ({ label, toasted }) => {
  const notWaiting = new DaemonRequestError("no longer waiting", SESSION_REQUEST_NOT_WAITING, { session: "s-1" });
  const dialog = mount(() => Promise.reject(notWaiting));
  await pressButton(label);
  const toasts = dialog.toasts();
  expect(toasts).toHaveLength(toasted);
  if (toasted) expect(toasts[0]).toEqual(["no longer waiting", "s-1"]);
});

// A request already on screen is not swapped under the user's pointer, neither for a trust prompt
// nor for a request that arrives later with an earlier time; the trust prompt follows once the
// request on screen has been answered.
it("what arrives while a request is on screen waits behind it", async () => {
  const prompt: TrustPrompt = { session: "s-1", agent: "claude", project: "p-1", path: "/tmp/shop", trustDir: null };
  const title = () => document.body.querySelector("[role=alertdialog] [slot=title]")?.textContent;
  const dialog = mount();
  await act(async () => dialog.store.setState({ trustPrompts: [prompt], sessionRequests: [consoleRequest, request] }));
  const shown = dialogText();
  await act(async () => dialog.store.setState({ sessionRequests: [consoleRequest] }));
  expect([shown.includes("session in the project p-1"), title()]).toEqual([true, "Trust this folder?"]);
});
