// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { AgentBehaviorSection } from "./AgentBehaviorSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);

vi.mock("../store", () => ({
  useDaemon: () => ({ request, toastError: vi.fn() }),
  useDaemonStore: (selector: (state: { settings: Record<string, boolean> }) => unknown) =>
    selector({ settings: { auto_approve_session_requests: true, auto_approve_resume_requests: false, auto_approve_account_switches: false, auto_approve_handoffs: true, ask_questions_as_choices: true, reply_in_interface_language: false } }),
}));

it("shows the session-request, resume-request, account-switch, handoff, question-asking and language switches in that order, each with its own value and setting", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <LanguageProvider>
        <AgentBehaviorSection />
      </LanguageProvider>,
    ),
  );
  expect(container.textContent).toContain("Choose what sessions may do without asking you first, and how they ask and talk to you.");
  const [sessions, resumes, switches, handoffs, questions, language] = [...container.querySelectorAll<HTMLInputElement>("input[type=checkbox]")];
  expect(container.textContent!.indexOf("Automatically approve session requests")).toBeLessThan(
    container.textContent!.indexOf("Automatically approve resume requests"),
  );
  expect(container.textContent!.indexOf("Automatically approve resume requests")).toBeLessThan(
    container.textContent!.indexOf("Automatically approve account switches"),
  );
  expect(container.textContent!.indexOf("Automatically approve account switches")).toBeLessThan(
    container.textContent!.indexOf("Automatically approve handoffs"),
  );
  expect(container.textContent!.indexOf("Automatically approve handoffs")).toBeLessThan(
    container.textContent!.indexOf("Ask questions as choices"),
  );
  expect(container.textContent!.indexOf("Ask questions as choices")).toBeLessThan(
    container.textContent!.indexOf("Talk to me in the interface language"),
  );
  expect([sessions.checked, resumes.checked, switches.checked, handoffs.checked, questions.checked, language.checked]).toEqual([true, false, false, true, true, false]);
  await act(async () => {
    resumes.click();
  });
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", auto_approve_resume_requests: true });
  await act(async () => {
    switches.click();
  });
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", auto_approve_account_switches: true });
  await act(async () => {
    handoffs.click();
  });
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", auto_approve_handoffs: false });
  await act(async () => {
    questions.click();
  });
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", ask_questions_as_choices: false });
  await act(async () => {
    language.click();
  });
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", reply_in_interface_language: true });
  expect(request).toHaveBeenCalledTimes(5);
});
