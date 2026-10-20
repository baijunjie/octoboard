// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { setLanguage } from "../i18n/language";
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
  branches: [],
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
  expect([...document.querySelectorAll("[data-inline-chip]")].map((chip) => chip.textContent)).toEqual([asked.session, "Codex", asked.project, asked.console].filter(Boolean));
});

it("keeps known names as names even when they equal record IDs", () => {
  const session = sessionOf("s-1", "c-1", "p-1", "s-1", "working");
  mountPendingQuestionDialog({
    sessions: new Map([[session.id, session]]),
    consoles: new Map([["c-1", consoleOf("c-1", "c-1")]]),
    projects: new Map([["p-1", projectOf("p-1", "c-1", "p-1")]]),
    sessionRequests: [request],
  }, ack);
  expect(dialogText()).toContain("The session s-1 asks to start a Codex session in the project p-1 of the console c-1");
  expect(document.querySelectorAll("[data-inline-chip] code")).toHaveLength(0);
});

// The branches an approval would hand over are named, each as code and joined as the language joins
// a list, and a request that hands none over reads as it did without them.
it.each([
  { branches: ["fix/refunds"], where: "The branch fix/refunds would be handed over to it." },
  { branches: ["fix/refunds", "100%25-done"], where: "The branches fix/refunds, 100%-done would be handed over to it." },
])("names the branches handed over: $branches", ({ branches, where }) => {
  mountPendingQuestionDialog({ sessionRequests: [{ ...request, branches }] }, ack);
  expect(dialogText()).toContain("The new session would report to you. " + where);
  const codes = [...document.querySelectorAll("[data-inline-chip] code")].map((code) => code.textContent);
  expect(codes).toEqual([request.session, request.project, request.console, ...branches.map((branch) => branch.replace("%25", "%"))]);
});

it("puts no space between the question and the branches in a language with none after its full stop", () => {
  setLanguage("zh-Hans");
  onTestFinished(() => setLanguage("en"));
  mountPendingQuestionDialog({ sessionRequests: [{ ...request, branches: ["fix/refunds", "feat/pay"] }] }, ack);
  expect(dialogText()).toContain("新会话将向你汇报。分支 fix/refunds、feat/pay 将移交给新会话。");
});

it("says nothing of branches when none is handed over", () => {
  mountPendingQuestionDialog({ sessionRequests: [request] }, ack);
  expect(dialogText()).not.toContain("handed over");
  expect([...document.querySelectorAll("[data-inline-chip] code")].map((code) => code.textContent)).toEqual([request.session, request.project, request.console]);
});

// The requester may live in another project or console than the one it asks in, which the dialog is
// the only place to see; its project and console are named only where they are not the ones the
// session is asked in, and it goes by its display title, its own name often being just its project's.
it.each([
  { requester: sessionOf("s-1", "c-1", "p-1", "Shop", "working", { terminal_title: "Refund flow" }), where: "The session Refund flow｜Shop asks to start" },
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
  if (toasted) expect(toasts[0]).toEqual([notWaiting, "s-1"]);
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
