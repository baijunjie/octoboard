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
    selector({ settings: { auto_approve_session_requests: true, auto_approve_resume_requests: false } }),
}));

it("shows the session-request switch above the resume-request one, each with its own value and setting", async () => {
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
  const [sessions, resumes] = [...container.querySelectorAll<HTMLInputElement>("input[type=checkbox]")];
  expect(container.textContent!.indexOf("Automatically approve session requests")).toBeLessThan(
    container.textContent!.indexOf("Automatically approve resume requests"),
  );
  expect([sessions.checked, resumes.checked]).toEqual([true, false]);
  await act(async () => {
    resumes.click();
  });
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", auto_approve_resume_requests: true });
});
