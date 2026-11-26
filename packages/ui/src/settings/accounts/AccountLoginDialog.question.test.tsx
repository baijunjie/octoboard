// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { settleFrames } from "../../components/dialog/dialogTestSupport";
import { CodedError } from "../../errors/codedError";
import type { AccountLogin } from "../../protocol";
import { click, typeInto, withoutIsolates } from "../../test/dom";
import { KEEP_TOKEN, SHARE_USAGE } from "../../test/loginQuestionProvider";
import { TestHost } from "../../test/TestHost";
import { testDaemon } from "../../test/testDaemon";
import { AccountLoginDialog } from "./AccountLoginDialog";

// The test-only provider is given the wording of two questions, so the test leans on no real agent's module.
vi.mock("../../providers/registry", async (importOriginal) => (await import("../../test/loginQuestionProvider")).withLoginQuestionProvider(await importOriginal()));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The login of the test-only provider's default account, waiting on a question its wording is known for, with a page
// and a code the flow gave before it asked.
const asking = (patch: Partial<AccountLogin> = {}): AccountLogin => ({
  agent: "test_agent", account: null, status: "waiting", authorization_url: "https://test.example/authorize", device_code: "ABCD-1234", opened_by_agent: false, failure: null,
  question: { id: KEEP_TOKEN, params: { path: "/home/dev/.test_agent/token" }, message: "Keep the token in a file?" },
  ...patch,
});

const ACK = { type: "ack", id: null };

function setup(login: AccountLogin = asking(), reply: () => Promise<unknown> = () => Promise.resolve(ACK)) {
  const request = vi.fn(reply);
  const onClose = vi.fn();
  const daemon = testDaemon({ state: { accountLogins: [login] }, request: request as never });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<TestHost daemon={daemon}><AccountLoginDialog agent={login.agent} account={null} start={false} onClose={onClose} /></TestHost>));
  const dialog = () => document.querySelector<HTMLElement>('[role="alertdialog"], [role="dialog"]')!;
  const button = (name: string) => Array.from(dialog().querySelectorAll("button")).find((found) => found.textContent === name);
  const title = () => document.getElementById(dialog().getAttribute("aria-labelledby") ?? "")?.textContent;
  const set = (logins: AccountLogin[]) => act(() => daemon.store.setState({ accountLogins: logins }));
  return { request, onClose, dialog, button, title, set };
}

it("shows the agent's question in the provider's words, with its two answers, in place of the code and the page", async () => {
  const { request, dialog, button, title } = setup();
  await settleFrames();
  expect(title()).toBe("Login question");
  const text = withoutIsolates(dialog().textContent)!;
  expect(text).toContain("Test Agent needs your answer to finish logging in to account Default.");
  expect(text).toContain("Keep the token in the file /home/dev/.test_agent/token?");
  expect(text).not.toContain("ABCD-1234");
  expect(dialog().querySelector('a[href="https://test.example/authorize"]')).toBeNull();
  expect(button("Keep in the file")).toBeDefined();
  expect(button("Do not keep")).toBeDefined();
  expect(button("Cancel login")).toBeUndefined();
  // The question, with the sentence before it, is what the alert dialog is described by.
  const description = document.getElementById(dialog().getAttribute("aria-describedby") ?? "");
  expect(withoutIsolates(description?.textContent)).toContain("Keep the token in the file /home/dev/.test_agent/token?");
  // A question the user did not ask for: no answer holds focus, so a key meant for the dialog it replaced answers
  // nothing, and nothing is sent until an answer is pressed.
  expect(document.activeElement?.tagName).not.toBe("BUTTON");
  expect(dialog().contains(document.activeElement)).toBe(true);
  expect(request).not.toHaveBeenCalled();
});

it("sends a yes for the question that is open, and nothing else", async () => {
  const { request, button } = setup();
  await click(button("Keep in the file"));
  expect(request.mock.calls).toEqual([[{ type: "answer_account_login_question", agent: "test_agent", account: null, question: KEEP_TOKEN, answer: "yes" }]]);
});

it("sends a no for the question that is open, and nothing else", async () => {
  const { request, button } = setup();
  await click(button("Do not keep"));
  expect(request.mock.calls).toEqual([[{ type: "answer_account_login_question", agent: "test_agent", account: null, question: KEEP_TOKEN, answer: "no" }]]);
});

it("masks the authorization code, sends it to the waiting login and clears the field", async () => {
  const id = "google_authorization_code";
  const { request, dialog, button } = setup(asking({
    agent: "antigravity",
    device_code: null,
    question: { id, params: {}, message: "Paste the authorization code.", answer_kind: "secret_text" },
  }));
  await settleFrames();
  const field = dialog().querySelector<HTMLInputElement>('input[type="password"]')!;
  expect(field).toBeTruthy();
  expect(dialog().querySelector('a[href="https://test.example/authorize"]')).not.toBeNull();
  expect(button("Send code")?.disabled).toBe(true);
  await typeInto(field, "example-code");
  await click(button("Send code"));
  expect(request).toHaveBeenCalledExactlyOnceWith({
    type: "answer_account_login_question",
    agent: "antigravity",
    account: null,
    question: id,
    answer: { secret_text: "example-code" },
  });
  expect(field.value).toBe("");
});

it("keeps the typed code in the field when sending it fails", async () => {
  const { dialog, button } = setup(asking({
    agent: "antigravity",
    question: { id: "google_authorization_code", params: {}, message: "Paste the authorization code.", answer_kind: "secret_text" },
  }), () => Promise.reject(new Error("The daemon is unreachable.")));
  await settleFrames();
  const field = dialog().querySelector<HTMLInputElement>('input[type="password"]')!;
  await typeInto(field, "example-code");
  await click(button("Send code"));
  await settleFrames();
  expect(field.value).toBe("example-code");
});

it.each([
  ["Do not keep", "Keep in the file"],
  ["Keep in the file", "Do not keep"],
])("holds %s pending while it is being sent, and takes no other answer meanwhile", async (pressed, other) => {
  // The daemon has not answered yet.
  const { request, button } = setup(asking(), () => new Promise(() => {}));
  await click(button(pressed));
  expect(button(pressed)?.getAttribute("aria-busy")).toBe("true");
  await click(button(pressed));
  await click(button(other));
  expect(request).toHaveBeenCalledOnce();
});

it("ends the login unanswered when the question is closed without an answer", async () => {
  const { request, onClose, dialog } = setup();
  await click(dialog().querySelector('button[aria-label="Close"]'));
  expect(request.mock.calls).toEqual([[{ type: "cancel_account_login", agent: "test_agent", account: null }]]);
  expect(onClose).toHaveBeenCalledWith(false, expect.any(Promise));
});

it("goes back to the running login, focus on Cancel login, once the question is answered", async () => {
  const { dialog, button, title, set } = setup();
  await click(button("Keep in the file"));
  set([asking({ question: undefined })]);
  await settleFrames();
  expect(title()).toBe("Log in");
  expect(withoutIsolates(dialog().textContent)).toContain("Confirm this code in your browser: ABCD-1234");
  expect(button("Keep in the file")).toBeUndefined();
  expect(document.activeElement).toBe(button("Cancel login"));
});

it("takes focus back for the next question, with neither answer still pending", async () => {
  const { dialog, button, set } = setup(asking(), () => new Promise(() => {}));
  await settleFrames();
  act(() => button("Keep in the file")!.focus());
  await click(button("Keep in the file"));
  set([asking({ question: { id: SHARE_USAGE, params: {}, message: "Share usage data?" } })]);
  await settleFrames();
  expect(withoutIsolates(dialog().textContent)).toContain("Share usage data with the test agent's maker?");
  expect(document.activeElement?.tagName).not.toBe("BUTTON");
  expect(dialog().contains(document.activeElement)).toBe(true);
  expect(button("Share usage data")?.getAttribute("aria-busy")).not.toBe("true");
  expect(button("Do not share")?.getAttribute("aria-busy")).not.toBe("true");
});

it("turns into the timeout alert when a login that asked ends as timed out", async () => {
  const { request, dialog, button, title, set } = setup();
  await settleFrames();
  set([asking({ status: "timed_out", question: undefined })]);
  await settleFrames();
  expect(title()).toBe("Login timed out");
  expect(withoutIsolates(dialog().textContent)).toContain("The login to Test Agent account Default timed out.");
  expect(button("Keep in the file")).toBeUndefined();
  expect(button("Log in again")).toBeDefined();
  expect(request).not.toHaveBeenCalled();
});

it("closes, claiming no success and sending nothing, when the login is cancelled elsewhere while the question is open", async () => {
  const { request, onClose, set } = setup();
  await settleFrames();
  set([asking({ status: "cancelled", question: undefined })]);
  await settleFrames();
  expect(onClose).toHaveBeenCalledWith(false);
  expect(request).not.toHaveBeenCalled();
});

it("takes an answer the login no longer waits for as no failure", async () => {
  const { button, dialog } = setup(asking(), () => Promise.reject(new CodedError("not open", "login_question_not_open", { agent: "Test Agent" })));
  await click(button("Do not keep"));
  await click(button("Keep in the file"));
  await settleFrames();
  expect(document.querySelectorAll('[role="alertdialog"]')).toHaveLength(1);
  expect(withoutIsolates(dialog().textContent)).toContain("Keep the token in the file");
});

it("offers no answer to a question it has no words for: a failure alert says so and how to log in instead, and its button cancels the login", async () => {
  const { request, dialog, button, title } = setup(asking({ question: { id: "from_a_newer_daemon", params: {}, message: "Do the new thing?" } }));
  await settleFrames();
  expect(dialog().getAttribute("role")).toBe("alertdialog");
  expect(title()).toBe("Could not show the login question");
  const text = withoutIsolates(dialog().textContent)!;
  expect(text).toContain("The login to Test Agent account Default is waiting for an answer.");
  expect(text).not.toContain("Logging in to");
  expect(text).toContain("The agent asked a question that cannot be shown here, so this login cannot continue.");
  // The remedy names the account's configuration directory, so the account logged in by hand is this one.
  expect(text).toMatch(/Cancel the login, then log in with the agent's own CLI in the environment where sessions run, using \S*\.octoboard-test-agent as its configuration directory\./);
  expect(text).not.toContain("Do the new thing?");
  expect(text).not.toContain("ABCD-1234");
  expect(dialog().querySelector('a[href="https://test.example/authorize"]')).toBeNull();
  expect(Array.from(dialog().querySelectorAll("button")).map((found) => found.textContent).filter(Boolean)).toEqual(["Cancel login"]);
  expect(document.activeElement).toBe(button("Cancel login"));
  expect(request).not.toHaveBeenCalled();
  await click(button("Cancel login"));
  expect(request.mock.calls).toEqual([[{ type: "cancel_account_login", agent: "test_agent", account: null }]]);
});
