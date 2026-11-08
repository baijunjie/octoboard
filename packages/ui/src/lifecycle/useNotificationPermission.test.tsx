// @vitest-environment jsdom
// Loaded first, as `main.tsx` loads it, so a guard that swallows the window's focus events fails this test.
import "../input/focusGuard";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { NotificationPermissionStatus, PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import { BROWSER_HOST } from "../test/hosts";
import { useNotificationPermission } from "./useNotificationPermission";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Probe(): string {
  return useNotificationPermission().status ?? "none";
}

/** Mounts the hook over a page that asks for the permission itself, whose answer the test changes behind it with
 * the returned `answer`. */
async function mount() {
  let status: NotificationPermissionStatus = "undecided";
  const platform: PlatformAdapter = {
    kind: "browser",
    host: BROWSER_HOST,
    notifications: {
      ensurePermission: vi.fn(() => Promise.resolve(false)),
      notify: vi.fn(() => Promise.resolve()),
      permissionPrompt: { status: () => status, request: () => Promise.resolve() },
    },
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(<PlatformProvider value={platform}><Probe /></PlatformProvider>));
  return { container, answer: (next: NotificationPermissionStatus) => void (status = next) };
}

it("reads the permission again when the window regains focus", async () => {
  const { container, answer } = await mount();
  answer("granted");
  expect(container.textContent).toBe("undecided");
  act(() => void window.dispatchEvent(new Event("focus")));
  expect(container.textContent).toBe("granted");
});
