// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { AgentPermissionsSection } from "./AgentPermissionsSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);

vi.mock("../store", () => ({
  useDaemon: () => ({ request, toastError: vi.fn() }),
  useDaemonStore: (selector: (state: { settings: Record<string, boolean> }) => unknown) =>
    selector({ settings: { auto_approve_session_requests: true, auto_approve_resume_requests: false, auto_approve_account_switches: false, auto_approve_handoffs: true } }),
}));

it("shows the session-request, resume-request, account-switch and handoff switches in that order, each with its own value and setting", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <LanguageProvider>
        <AgentPermissionsSection />
      </LanguageProvider>,
    ),
  );
  expect(container.textContent).toContain("Choose what sessions may do without asking you first.");
  const [sessions, resumes, switches, handoffs] = [...container.querySelectorAll<HTMLInputElement>("input[type=checkbox]")];
  expect(container.textContent!.indexOf("Automatically approve session requests")).toBeLessThan(
    container.textContent!.indexOf("Automatically approve resume requests"),
  );
  expect(container.textContent!.indexOf("Automatically approve resume requests")).toBeLessThan(
    container.textContent!.indexOf("Automatically approve account switches"),
  );
  expect(container.textContent!.indexOf("Automatically approve account switches")).toBeLessThan(
    container.textContent!.indexOf("Automatically approve handoffs"),
  );
  expect([sessions.checked, resumes.checked, switches.checked, handoffs.checked]).toEqual([true, false, false, true]);
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
  expect(request).toHaveBeenCalledTimes(3);
});
