// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { setLanguage } from "../i18n/language";
import { accountOf, consoleOf, projectOf, sessionOf, snapshotState } from "../gallery/fixtures/builders";
import { SESSION_REQUEST_NOT_WAITING } from "../protocol";
import type { SessionRequest, TrustPrompt } from "../store";
import { press } from "../components/detailsTestSupport";
import { findButton, mountPendingQuestionDialog, pressButton } from "./pendingQuestionTestSupport";

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
  action: "start",
};

const consoleRequest: SessionRequest = { ...request, requestId: "r-console", session: "s-2", role: "console", project: null, requestedAt: 1 };

const ack = () => Promise.resolve({ type: "ack", id: null });

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
  expect([...document.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toEqual([asked.session, "Codex", asked.project, asked.console].filter(Boolean));
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
  expect(document.querySelectorAll("[data-inline-reference] code")).toHaveLength(0);
});

// The branches an approval would hand over are named, each as code and joined as the language joins
// a list, and a request that hands none over reads as it did without them.
it.each([
  { branches: ["fix/refunds"], where: "The branch fix/refunds would be handed over to it." },
  { branches: ["fix/refunds", "100%25-done"], where: "The branches fix/refunds, 100%-done would be handed over to it." },
])("names the branches handed over: $branches", ({ branches, where }) => {
  mountPendingQuestionDialog({ sessionRequests: [{ ...request, branches }] }, ack);
  expect(dialogText()).toContain("The new session would report to you. " + where);
  const codes = [...document.querySelectorAll("[data-inline-reference] code")].map((code) => code.textContent);
  expect(codes).toEqual([request.session, request.project, request.console, ...branches.map((branch) => branch.replace("%25", "%"))]);
});

it("puts no space between the question and the branches in a language with none after its full stop", () => {
  setLanguage("zh-Hans");
  onTestFinished(() => setLanguage("en"));
  mountPendingQuestionDialog({ sessionRequests: [{ ...request, branches: ["fix/refunds", "feat/pay"] }] }, ack);
  expect(dialogText()).toContain("新会话将向你汇报。分支 fix/refunds、feat/pay 将移交给新会话。");
});

it("joins the question and the branches without a whitespace-only text node", () => {
  mountPendingQuestionDialog({ sessionRequests: [{ ...request, branches: ["fix/refunds"] }] }, ack);
  const walker = document.createTreeWalker(document.body.querySelector("[role=alertdialog]")!, NodeFilter.SHOW_TEXT);
  const blank: string[] = [];
  for (let node = walker.nextNode(); node; node = walker.nextNode()) if ((node as Text).data !== "" && (node as Text).data.trim() === "") blank.push((node as Text).data);
  expect(blank).toEqual([]);
  expect(dialogText()).toContain("The new session would report to you. The branch fix/refunds");
});

it("says nothing of branches when none is handed over", () => {
  mountPendingQuestionDialog({ sessionRequests: [request] }, ack);
  expect(dialogText()).not.toContain("handed over");
  expect([...document.querySelectorAll("[data-inline-reference] code")].map((code) => code.textContent)).toEqual([request.session, request.project, request.console]);
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
  if (toasted) expect(toasts[0]).toEqual([notWaiting, "s-1", "Could not approve the request"]);
});

// A reopen or resume fails with the target's own resume error, which says "this session": the toast
// is about the session the error names, else the target, not the one that asked.
it.each<{ label: string; params: Record<string, string>; subject: string }>([
  { label: "an error naming a session", params: { session: "t-9" }, subject: "t-9" },
  { label: "an error naming none", params: {}, subject: "t-1" },
])("an approval of a reopen that fails with $label toasts about the session brought back", async ({ params, subject }) => {
  const reopen: SessionRequest = { ...request, action: "reopen", target: "t-1", reopenedOwners: [] };
  const failure = new DaemonRequestError("could not be resumed", "session_already_starting", params);
  const dialog = mountPendingQuestionDialog({ sessionRequests: [reopen] }, () => Promise.reject(failure));
  await pressButton("Reopen session");
  expect(dialog.toasts()).toEqual([[failure, subject, "Could not approve the request"]]);
});

it.each<"reopen" | "resume">(["reopen", "resume"])("an approval of a %s that finds the request no longer waiting toasts about the requester", async (action) => {
  const asked: SessionRequest = { ...request, action, target: "t-1", reopenedOwners: [] };
  const failure = new DaemonRequestError("no longer waiting", SESSION_REQUEST_NOT_WAITING, { session: "t-9" });
  const dialog = mountPendingQuestionDialog({ sessionRequests: [asked] }, () => Promise.reject(failure));
  await pressButton(action === "reopen" ? "Reopen session" : "Resume session");
  expect(dialog.toasts()).toEqual([[failure, "s-1", "Could not approve the request"]]);
});

it("an approval of a start that fails toasts about the requesting session", async () => {
  const failure = new DaemonRequestError("could not start", "launch_failed", { session: "other" });
  const dialog = mount(() => Promise.reject(failure));
  await pressButton("Start session");
  expect(dialog.toasts()).toEqual([[failure, "s-1", "Could not approve the request"]]);
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

// A request to resume a conversation names the session it would bring back with its agent, account
// and place, whom it reports to and the owners that come back with it; what the agent wrote is
// nowhere on it. The title and the button say which of the three things an approval does.
const names = {
  consoles: new Map([consoleOf("c-1", "Main")].map((c) => [c.id, c])),
  projects: new Map([projectOf("p-1", "c-1", "Shop")].map((p) => [p.id, p])),
};

const reopening: SessionRequest = { ...request, action: "reopen", target: "s-held", reopenedOwners: [] };

it.each([
  {
    name: "reopen",
    asked: { ...reopening, owner: "s-boss", reopenedOwners: ["s-boss"] },
    title: "Reopen a session?",
    button: "Reopen session",
    text: "asks to reopen the session Old work, a Codex session on Default in the project Shop of the console Main. It reports to Boss. Its archived owner Boss would be reopened with it.",
  },
  {
    name: "resume",
    asked: { ...reopening, action: "resume" as const },
    title: "Resume a session?",
    button: "Resume session",
    text: "asks to resume the interrupted session Old work, a Codex session on Default in the project Shop of the console Main. It reports to you.",
  },
  {
    name: "record",
    asked: { ...request, action: "resume_record" as const, account: "a-work", conversation: "11111111-2222-3333-4444-555555555555" },
    title: "Resume a conversation?",
    button: "Resume conversation",
    text: "asks to resume the conversation 11111111-2222-3333-4444-555555555555 in a new Codex session on Work, in the project Shop of the console Main. The new session would report to you.",
  },
])("words the question for what an approval does: $name", async ({ asked, title, button, text }) => {
  const held = sessionOf("s-held", "c-1", "p-1", "Old work", "archived");
  const boss = sessionOf("s-boss", "c-1", "p-1", "Boss", "archived");
  const dialog = mountPendingQuestionDialog({
    ...snapshotState({ sessions: [held, boss] }),
    ...names,
    settings: { ...snapshotState({}).settings!, accounts: [accountOf("a-work", "codex", "Work", "/a/work")] },
    sessionRequests: [asked],
  }, ack);
  expect(document.body.querySelector("[role=alertdialog] [slot=title]")?.textContent).toBe(title);
  expect(dialogText()).toContain(text);
  expect(dialogText()).not.toContain("sharing change");
  await pressButton(button);
  expect(dialog.sent()).toEqual([{ type: "answer_session_request", request_id: "r-1", approve: true }]);
});

// An account the store does not know is named by its id, as any other id it does not know is: for the
// account that holds a record, and for a session that recorded one; a session the store does not know
// has none to name.
it.each([
  { name: "a record's account", asked: { ...request, action: "resume_record" as const, account: "a-gone", conversation: "x" }, sessions: [], id: "a-gone" },
  { name: "a session's account", asked: reopening, sessions: [sessionOf("s-held", "c-1", "p-1", "Old work", "archived", { account_id: "a-gone" })], id: "a-gone" },
])("shows an unknown account by its id: $name", ({ asked, sessions, id }) => {
  mountPendingQuestionDialog({ ...snapshotState({ sessions }), ...names, sessionRequests: [asked] }, ack);
  const chip = [...document.body.querySelectorAll("[role=alertdialog] *")].find((el) => el.children.length === 0 && el.textContent === id);
  expect(chip).toBeDefined();
  expect(chip?.closest("[dir=ltr]")).not.toBeNull();
});

it("shows a dash for a session the store does not know", () => {
  mountPendingQuestionDialog({ ...snapshotState({}), ...names, sessionRequests: [reopening] }, ack);
  expect(dialogText()).toContain("a Codex session on —");
});

// The account's unfinished sharing change is warned of in the dialog, which is the confirmation, by a
// warning mark right after the account in the sentence, for the account of the session that comes back
// and for the one that holds a record, and not otherwise.
const incompleteMark = () => document.body.querySelector<HTMLElement>('[role=alertdialog] button[aria-label="Sharing may be incomplete"]');

it.each([
  { name: "a session's account", asked: reopening },
  { name: "a record's account", asked: { ...request, action: "resume_record" as const, account: "a-half", conversation: "x" } },
])("marks the account whose last sharing change did not finish, in the sentence: $name", ({ asked }) => {
  const held = sessionOf("s-held", "c-1", "p-1", "Old work", "archived", { account_id: "a-half" });
  const half = { id: "a-half", agent: "codex" as const, name: "Half", config_dir: "/a/half", share_default_config: true, share_default_config_incomplete: true, disabled: false };
  mountPendingQuestionDialog({
    ...snapshotState({ sessions: [held] }),
    ...names,
    settings: { ...snapshotState({}).settings!, accounts: [half] },
    sessionRequests: [asked],
  }, ack);
  const mark = incompleteMark()!;
  expect(mark).not.toBeNull();
  // In the question's paragraph, right after the account's reference, and nothing of it written beside.
  expect(mark.closest("p")?.textContent).toContain("Half");
  expect(mark.previousElementSibling).toBeNull();
  expect(mark.parentElement?.previousElementSibling?.textContent).toBe("Half");
  expect(document.body.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(dialogText()).not.toContain("The last sharing change");
  // The popover holds the sentence and the consequence.
  press(mark);
  const popover = document.body.querySelector('[role="dialog"]')!;
  expect(popover.getAttribute("aria-label")).toBe("Sharing may be incomplete");
  expect(popover.textContent?.replace(/[\u2066-\u2069]/g, "")).toBe("The last sharing change of Half did not finish.Its configuration and conversations may be incomplete.");
});

it("does not warn while the account's sharing change is running", () => {
  const held = sessionOf("s-held", "c-1", "p-1", "Old work", "archived", { account_id: "a-half" });
  const half = { id: "a-half", agent: "codex" as const, name: "Half", config_dir: "/a/half", share_default_config: true, share_default_config_incomplete: true, disabled: false };
  mountPendingQuestionDialog({
    ...snapshotState({ sessions: [held], accountSharing: [{ account: "a-half", progress: { share: true, phase: "changing", done: 1, total: 3 } }] }),
    ...names,
    settings: { ...snapshotState({}).settings!, accounts: [half] },
    sessionRequests: [reopening],
  }, ack);
  expect(incompleteMark()).toBeNull();
});

const halfCodex = { id: "a-half", agent: "codex" as const, name: "Half", config_dir: "/a/half", share_default_config: true, share_default_config_incomplete: true, disabled: false };
const settingsWith = (accounts: unknown[]) => ({ ...snapshotState({}).settings!, accounts: accounts as never });
const statusTexts = () => [...document.body.querySelectorAll('[role="status"]')].map((node) => node.textContent).filter(Boolean);

it.each([
  { name: "a switch of a session the window does not know", asked: { ...request, action: "switch" as const, agent: "codex" as const, target: "s-gone", account: "a-half" }, state: { ...names } },
  { name: "a handoff in a console the window does not know", asked: { ...request, action: "handoff" as const, agent: "codex" as const, target: "s-held", account: "a-half" }, state: {} },
])("marks the account in the sentence where the form is not drawn: $name", ({ asked, state }) => {
  const held = sessionOf("s-held", "c-1", "p-1", "Old work", "idle", { account_id: "a-one" });
  mountPendingQuestionDialog({ ...snapshotState({ sessions: [held] }), ...state, settings: settingsWith([halfCodex]), sessionRequests: [asked] }, ack);
  const mark = incompleteMark()!;
  expect(mark).not.toBeNull();
  expect(mark.closest("p")?.textContent).toContain("Half");
  expect(mark.parentElement?.previousElementSibling?.textContent).toBe("Half");
  expect(document.body.querySelectorAll('[role=alertdialog] button[aria-label="Sharing may be incomplete"]')).toHaveLength(1);
});

it("says a sharing change that ends unfinished while the dialog is open once, and not one it opened with", async () => {
  const held = sessionOf("s-held", "c-1", "p-1", "Old work", "archived", { account_id: "a-half" });
  const { store } = mountPendingQuestionDialog({
    ...snapshotState({ sessions: [held] }),
    ...names,
    settings: settingsWith([{ ...halfCodex, share_default_config_incomplete: false }]),
    sessionRequests: [reopening],
  }, ack);
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(incompleteMark()).toBeNull();
  expect(statusTexts()).toEqual([]);
  await act(async () => store.setState((s) => ({ settings: { ...s.settings, accounts: [halfCodex] } })));
  expect(incompleteMark()).not.toBeNull();
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(statusTexts().map((text) => text?.replace(/[\u2066-\u2069]/g, ""))).toEqual(["The last sharing change of Half did not finish."]);
});

it("does not say an unfinished sharing change the dialog opened with", async () => {
  const held = sessionOf("s-held", "c-1", "p-1", "Old work", "archived", { account_id: "a-half" });
  mountPendingQuestionDialog({ ...snapshotState({ sessions: [held] }), ...names, settings: settingsWith([halfCodex]), sessionRequests: [reopening] }, ack);
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(incompleteMark()).not.toBeNull();
  expect(statusTexts()).toEqual([]);
});

it("does not say an unfinished sharing change of the request that comes up after the first is answered", async () => {
  const held = sessionOf("s-held", "c-1", "p-1", "Old work", "archived", { account_id: "a-one" });
  const other = sessionOf("s-other", "c-1", "p-1", "Other work", "archived", { account_id: "a-half" });
  const first: SessionRequest = { ...reopening, requestId: "r-first", requestedAt: 1 };
  const second: SessionRequest = { ...reopening, requestId: "r-second", target: "s-other", requestedAt: 2 };
  const { store } = mountPendingQuestionDialog({
    ...snapshotState({ sessions: [held, other] }),
    ...names,
    settings: settingsWith([{ ...halfCodex, id: "a-one", name: "One", share_default_config_incomplete: false }, halfCodex]),
    sessionRequests: [first, second],
  }, ack);
  expect(incompleteMark()).toBeNull();
  await act(async () => store.setState({ sessionRequests: [second] }));
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(incompleteMark()).not.toBeNull();
  expect(statusTexts()).toEqual([]);
});

// A request to switch a session's account or hand it off names the session, the agent and the account
// asked for, and holds a form that starts on those values: an approval carries what the form ended on,
// and nothing it was not asked to change.
const owned = sessionOf("s-held", "c-1", "p-1", "Old work", "idle", { account_id: "a-one" });
const claudeAccounts = [
  accountOf("a-one", "claude", "One", "/a/one"),
  accountOf("a-two", "claude", "Two", "/a/two"),
];
const switching: SessionRequest = { ...request, action: "switch", agent: "claude", target: "s-held", account: "a-two" };

const mountOwned = (asked: SessionRequest) => mountPendingQuestionDialog({
  ...snapshotState({ sessions: [owned] }),
  ...names,
  settings: { ...snapshotState({}).settings!, accounts: claudeAccounts },
  sessionRequests: [asked],
}, ack);

it("a switch names the session and the account asked for, and approves with that account", async () => {
  const dialog = mountOwned(switching);
  expect(document.body.querySelector("[role=alertdialog] [slot=title]")?.textContent).toBe("Switch account?");
  expect(dialogText()).toContain("asks to switch its session Old work to the Claude Code account Two.");
  expect(dialogText()).toContain("continued automatically after the switch where that is safe.");
  await pressButton("Switch account");
  expect(dialog.sent()).toEqual([{ type: "answer_session_request", request_id: "r-1", approve: true, account: "a-two" }]);
});

it("a switch to the account the session is on cannot be confirmed", () => {
  mountOwned({ ...switching, account: "a-one" });
  expect(dialogText()).toContain("The session is on One already");
  expect(findButton("Switch account")?.disabled).toBe(true);
});

it("refusing a switch sends no values", async () => {
  const dialog = mountOwned(switching);
  await pressButton("Refuse");
  expect(dialog.sent()).toEqual([{ type: "answer_session_request", request_id: "r-1", approve: false }]);
});

it("a handoff approves with the agent and account asked for", async () => {
  const dialog = mountOwned({ ...request, action: "handoff", agent: "codex", target: "s-held", account: null, model: "gpt-x" });
  expect(document.body.querySelector("[role=alertdialog] [slot=title]")?.textContent).toBe("Hand off a session?");
  expect(dialogText()).toContain("asks to hand off its session Old work to a new Codex session on the account Default.");
  expect(dialogText()).not.toContain("Fix the build");
  await pressButton("Hand off session");
  expect(dialog.sent()).toEqual([{ type: "answer_session_request", request_id: "r-1", approve: true, agent: "codex", account: null, model: "gpt-x" }]);
});

it("a handoff approves with the agent the user changed it to", async () => {
  const dialog = mountOwned({ ...request, action: "handoff", agent: "codex", target: "s-held", account: null });
  const trigger = document.body.querySelector<HTMLElement>('[role=alertdialog] [data-slot="select-trigger"]');
  await act(async () => trigger?.click());
  const grok = [...document.body.querySelectorAll<HTMLElement>("[role=option]")].find((option) => option.textContent?.includes("Grok"));
  await act(async () => grok?.click());
  await pressButton("Hand off session");
  expect(dialog.sent()).toEqual([expect.objectContaining({ type: "answer_session_request", approve: true, agent: "grok", account: null })]);
});

it("a handoff of a session that is working cannot be confirmed", () => {
  const working = sessionOf("s-held", "c-1", "p-1", "Old work", "working", { account_id: "a-one" });
  mountPendingQuestionDialog({
    ...snapshotState({ sessions: [working] }),
    ...names,
    settings: { ...snapshotState({}).settings!, accounts: claudeAccounts },
    sessionRequests: [{ ...request, action: "handoff", agent: "codex", target: "s-held", account: null }],
  }, ack);
  expect(dialogText()).toContain("in the middle of a turn");
  expect(findButton("Hand off session")?.disabled).toBe(true);
});

it("a switch to an account that no longer exists says so and cannot be confirmed", () => {
  mountOwned({ ...switching, account: "a-gone" });
  expect(dialogText()).toContain("The account asked for no longer exists");
  expect(findButton("Switch account")?.disabled).toBe(true);
});

// An account the user turned off is listed but cannot be chosen, whether it is a stored account or the
// agent's default one, and the dialog says why; Confirm stays off until the user picks another.
it.each([
  { name: "a stored account", asked: switching, accounts: [claudeAccounts[0], { ...claudeAccounts[1], disabled: true }], extra: {} },
  { name: "the default account", asked: { ...switching, account: null }, accounts: claudeAccounts, extra: { settings: { ...snapshotState({}).settings!, accounts: claudeAccounts, disabled_default_accounts: ["claude" as const] } } },
])("a switch to a turned-off account cannot be confirmed: $name", ({ asked, accounts, extra }) => {
  mountPendingQuestionDialog({
    ...snapshotState({ sessions: [owned] }),
    ...names,
    settings: { ...snapshotState({}).settings!, accounts: accounts as never },
    sessionRequests: [asked],
    ...extra,
  }, ack);
  expect(dialogText()).toContain("is disabled, so a session cannot use it");
  // The error of the account field, which is marked invalid.
  expect(document.querySelector('[data-slot="field-error"] [role="alert"]')?.textContent).toContain("is disabled, so a session cannot use it");
  // The account's own error says it all: no second alert for the hold on the session sits outside the field.
  expect([...document.querySelectorAll('[role="alert"]')].filter((alert) => !alert.closest('[data-slot="field-error"]'))).toEqual([]);
  expect(findButton("Switch account")?.disabled).toBe(true);
});

// An account found not logged in is listed but cannot be chosen, and the dialog says why.
it("a switch to an account that is not logged in cannot be confirmed", () => {
  mountPendingQuestionDialog({
    ...snapshotState({ sessions: [owned] }),
    ...names,
    settings: { ...snapshotState({}).settings!, accounts: claudeAccounts },
    accountQuotas: [{ agent: "claude", account: "a-two", fetched_at: 0, plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" }, email: null }],
    sessionRequests: [switching],
  }, ack);
  expect(dialogText()).toContain("is not logged in, so a session cannot use it");
  // The error of the account field, which is marked invalid.
  expect(document.querySelector('[data-slot="field-error"] [role="alert"]')?.textContent).toContain("is not logged in, so a session cannot use it");
  // The account's own error says it all: no second alert for the hold on the session sits outside the field.
  expect([...document.querySelectorAll('[role="alert"]')].filter((alert) => !alert.closest('[data-slot="field-error"]'))).toEqual([]);
  expect(findButton("Switch account")?.disabled).toBe(true);
});

const half = { id: "a-two", agent: "claude" as const, name: "Two", config_dir: "/a/two", share_default_config: true, share_default_config_incomplete: true, disabled: false };
const mountWith = (asked: SessionRequest, session = owned, accounts: typeof claudeAccounts | (typeof claudeAccounts[number] | typeof half)[] = claudeAccounts, extra: Parameters<typeof mountPendingQuestionDialog>[0] = {}, reply = ack) => mountPendingQuestionDialog({
  ...snapshotState({ sessions: [session] }),
  ...names,
  settings: { ...snapshotState({}).settings!, accounts: accounts as never },
  sessionRequests: [asked],
  ...extra,
}, reply);
const pickOption = async (text: string, which = 0) => {
  const trigger = document.body.querySelectorAll<HTMLElement>('[role=alertdialog] [data-slot="select-trigger"]')[which];
  await act(async () => trigger.click());
  const option = [...document.body.querySelectorAll<HTMLElement>("[role=option]")].find((item) => item.textContent?.includes(text));
  await act(async () => option?.click());
};

it("a switch leaves out a turned-off account that is neither the session's nor the one chosen", async () => {
  const three = accountOf("a-three", "claude", "Three", "/a/three", { disabled: true });
  mountWith(switching, owned, [...claudeAccounts, three] as never);
  const trigger = document.body.querySelector<HTMLElement>('[role=alertdialog] [data-slot="select-trigger"]');
  await act(async () => trigger?.click());
  const options = [...document.body.querySelectorAll<HTMLElement>("[role=option]")].map((option) => option.textContent ?? "");
  expect(options.some((text) => text.includes("Two"))).toBe(true);
  expect(options.some((text) => text.includes("Three"))).toBe(false);
});

it("a switch lists an account that is not logged in with why, in short under its name and in full as its description", async () => {
  mountWith({ ...switching, account: null }, owned, claudeAccounts, {
    accountQuotas: [{ agent: "claude", account: "a-two", fetched_at: 0, plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" }, email: null }],
  });
  const trigger = document.body.querySelector<HTMLElement>('[role=alertdialog] [data-slot="select-trigger"]');
  await act(async () => trigger?.click());
  const two = [...document.body.querySelectorAll<HTMLElement>("[role=option]")].find((option) => option.textContent?.includes("Two"));
  expect(two?.getAttribute("aria-disabled")).toBe("true");
  expect(two?.querySelector('[aria-hidden="true"].text-xs')?.textContent).toBe("Not logged in");
  expect(two?.querySelector(".sr-only")?.textContent).toBe("Unavailable until it is logged in");
});

it("a switch marks an unfinished sharing change of the account its form is on, inside the account field, and follows the form", async () => {
  mountWith(switching, owned, [claudeAccounts[0], half]);
  const mark = incompleteMark()!;
  expect(mark).not.toBeNull();
  // Inside the account picker's box, beside its trigger, not written under it.
  expect(mark.closest(".relative")?.querySelector('[data-slot="select-trigger"]')).not.toBeNull();
  expect(document.body.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(dialogText()).not.toContain("The last sharing change");
  press(mark);
  expect(document.body.querySelector('[role="dialog"]')?.textContent?.replace(/[\u2066-\u2069]/g, "")).toContain("The last sharing change of Two did not finish");
  expect(document.body.querySelector('[role="dialog"]')?.textContent).toContain("Its configuration and conversations may be incomplete.");
  await act(async () => document.body.querySelector('[role="dialog"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await pickOption("Default");
  expect(incompleteMark()).toBeNull();
});

it("a switch says an unfinished sharing change of its account once when it appears after the dialog opened", async () => {
  const { store } = mountWith(switching, owned, [claudeAccounts[0], { ...half, share_default_config_incomplete: false }]);
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(incompleteMark()).toBeNull();
  await act(async () => store.setState((s) => ({ settings: { ...s.settings, accounts: [claudeAccounts[0], half] as never } })));
  expect(incompleteMark()).not.toBeNull();
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(statusTexts().map((text) => text?.replace(/[\u2066-\u2069]/g, ""))).toEqual(["The last sharing change of Two did not finish."]);
});

it("a handoff marks an unfinished sharing change of the account its form is on, inside the joined account picker", () => {
  const codexHalf = { ...half, id: "a-codex", agent: "codex" as const, name: "Half" };
  const handoff: SessionRequest = { ...request, action: "handoff", agent: "codex", target: "s-held", account: "a-codex" };
  mountWith(handoff, owned, [codexHalf] as never);
  const mark = incompleteMark()!;
  expect(mark).not.toBeNull();
  const triggers = [...document.body.querySelectorAll('[role=alertdialog] [data-slot="select-trigger"]')];
  expect(mark.closest(".relative")?.querySelector('[data-slot="select-trigger"]')).toBe(triggers[1]);
  expect(document.body.querySelector('[data-slot="alert-root"]')).toBeNull();
});

it("a switch approves with the account the user picked instead", async () => {
  const dialog = mountWith(switching);
  await pickOption("Default");
  await pressButton("Switch account");
  expect(dialog.sent()).toEqual([{ type: "answer_session_request", request_id: "r-1", approve: true, account: null }]);
});

it("a switch has no clear button on its account picker", () => {
  mountWith(switching);
  const picker = document.body.querySelector('[role=alertdialog] [data-slot="select-trigger"]')?.closest('[data-slot="select"]');
  expect(picker).not.toBeNull();
  expect(picker?.querySelector('[data-slot="select-clear-button"]')).toBeNull();
});

it("the form is locked while the approval is carried out", async () => {
  const dialog = mountWith(switching, owned, claudeAccounts, {}, () => new Promise(() => {}));
  await pressButton("Switch account");
  const trigger = document.body.querySelector<HTMLElement>('[role=alertdialog] [data-slot="select-trigger"]');
  expect(trigger?.hasAttribute("disabled") || trigger?.getAttribute("aria-disabled") === "true" || trigger?.hasAttribute("data-disabled")).toBe(true);
  expect(dialog.sent()).toHaveLength(1);
});

it.each([
  { name: "locked", session: sessionOf("s-held", "c-1", "p-1", "Old work", "idle", { locked: true }), text: "is locked" },
  { name: "being switched", session: sessionOf("s-held", "c-1", "p-1", "Old work", "idle", { switching_account: true }), text: "switch" },
])("a handoff of a session that is $name cannot be confirmed", ({ session, text }) => {
  mountWith({ ...request, action: "handoff", agent: "codex", target: "s-held", account: null }, session);
  expect(dialogText()).toContain(text);
  expect(findButton("Hand off session")?.disabled).toBe(true);
});

it("a handoff of a session whose account changes its sharing cannot be confirmed", () => {
  const session = sessionOf("s-held", "c-1", "p-1", "Old work", "idle", { account_id: "a-one" });
  mountWith({ ...request, action: "handoff", agent: "codex", target: "s-held", account: null }, session, claudeAccounts, {
    accountSharing: new Map([["a-one", { account: "a-one", progress: { share: true, phase: "changing", done: 1, total: 3 } }]]) as never,
  });
  expect(findButton("Hand off session")?.disabled).toBe(true);
});

it("a handoff whose account was removed says so, shows empty pickers, and can be confirmed once the user picks", async () => {
  const dialog = mountWith({ ...request, action: "handoff", agent: "codex", target: "s-held", account: "a-gone" });
  expect(dialogText()).toContain("no longer available");
  expect(findButton("Hand off session")?.disabled).toBe(true);
  expect(dialogText()).toContain("Choose an agent");
  await pickOption("Codex");
  expect(findButton("Hand off session")?.disabled).toBe(false);
  await pressButton("Hand off session");
  expect(dialog.sent()).toEqual([expect.objectContaining({ approve: true, agent: "codex" })]);
});

it("a handoff whose agent can no longer be picked shows empty pickers until the user picks", async () => {
  mountWith({ ...request, action: "handoff", agent: "codex", target: "s-held", account: null }, owned, claudeAccounts, {
    settings: { ...snapshotState({}).settings!, accounts: claudeAccounts as never, added_agents: ["claude", "grok"] },
  });
  expect(dialogText()).toContain("no longer available");
  expect(findButton("Hand off session")?.disabled).toBe(true);
  await pickOption("Grok");
  expect(findButton("Hand off session")?.disabled).toBe(false);
});

it("a switch of a session that is still starting cannot be confirmed", () => {
  mountWith(switching, sessionOf("s-held", "c-1", "p-1", "Old work", "starting", { account_id: "a-one" }));
  expect(dialogText()).toContain("still starting");
  expect(findButton("Switch account")?.disabled).toBe(true);
});

// The session being archived ends the dialog's purpose: the dialog is the failure, and acknowledging it refuses.
it("a switch of a session that is archived turns the dialog into a failure modal, and acknowledging it refuses the request", async () => {
  const dialog = mountWith(switching, sessionOf("s-held", "c-1", "p-1", "Old work", "archived", { account_id: "a-one" }));
  expect(document.querySelector('[role="alertdialog"] [data-slot$="icon"]')).not.toBeNull();
  expect(dialogText()).toContain("Cannot switch the account");
  expect(dialogText()).toContain("The session is archived, so its account cannot be switched.");
  expect(findButton("Switch account")).toBeUndefined();
  expect(findButton("Refuse")).toBeUndefined();
  await pressButton("OK");
  expect(dialog.sent()).toEqual([expect.objectContaining({ approve: false })]);
});
