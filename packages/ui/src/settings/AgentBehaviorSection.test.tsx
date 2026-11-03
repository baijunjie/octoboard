// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { DEFAULT_SETTINGS } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { AgentBehaviorSection } from "./AgentBehaviorSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);

const settings = { ...DEFAULT_SETTINGS, auto_approve_session_requests: true, auto_approve_resume_requests: false, auto_approve_account_switches: false, auto_approve_handoffs: true, ask_questions_as_choices: true };

it("shows the session-request, resume-request, account-switch, handoff and question-asking switches in that order, each with its own value and setting", async () => {
  const container = mountHost(
    <LanguageProvider>
      <AgentBehaviorSection />
    </LanguageProvider>,
    { daemon: testDaemon({ state: { settings }, request }) },
  );
  expect(container.textContent).toContain("Choose what sessions may do without asking you first, and how they ask you.");
  const [sessions, resumes, switches, handoffs, questions] = [...container.querySelectorAll<HTMLInputElement>("input[type=checkbox]")];
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
  expect([sessions.checked, resumes.checked, switches.checked, handoffs.checked, questions.checked]).toEqual([true, false, false, true, true]);
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
  expect(request).toHaveBeenCalledTimes(4);
});
