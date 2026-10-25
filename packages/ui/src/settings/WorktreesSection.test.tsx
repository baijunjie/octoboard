// @vitest-environment jsdom
import { act, useSyncExternalStore } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import type { WorktreeSettings } from "../protocol";
import { DEFAULT_WORKTREE_SETTINGS } from "../worktreeSettings";
import { WorktreesSection } from "./WorktreesSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);
const toastError = vi.fn();
let worktrees: WorktreeSettings = DEFAULT_WORKTREE_SETTINGS;

const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};
const store = (next: WorktreeSettings) => {
  worktrees = next;
  act(() => listeners.forEach((listener) => listener()));
};

vi.mock("../store", () => ({
  useDaemon: () => ({ request, toastError }),
  useDaemonStore: (selector: (state: { settings: { worktrees: WorktreeSettings } }) => unknown) =>
    useSyncExternalStore(subscribe, () => selector({ settings: { worktrees } })),
}));

beforeEach(() => {
  request.mockClear();
  toastError.mockClear();
  request.mockImplementation(() => Promise.resolve({}) as Promise<never>);
  worktrees = DEFAULT_WORKTREE_SETTINGS;
});

function mount() {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<LanguageProvider><WorktreesSection /></LanguageProvider>));
  return container;
}

const toggle = (container: HTMLElement, name: string) =>
  Array.from(container.querySelectorAll<HTMLInputElement>("input[type=checkbox]")).find((el) => el.getAttribute("aria-label") === name)!;

it("shows the daemon's values, every control named", () => {
  worktrees = { use_worktrees: true, rebase_before_merge: true, delivery: "merge", revert_gate: true };
  const container = mount();
  expect(toggle(container, "Use worktrees for development").checked).toBe(true);
  expect(toggle(container, "Rebase before merging").checked).toBe(true);
  expect(toggle(container, "Revert gate").checked).toBe(true);
  const delivery = container.querySelector<HTMLElement>('button[aria-label="Delivery"], [aria-label="Delivery"] button');
  expect(delivery?.textContent).toContain("Merge locally");
});

it("lists the controls in order: Use worktrees, Rebase before merging, Revert gate, Delivery", () => {
  const container = mount();
  const names = Array.from(container.querySelectorAll('input[type=checkbox], button[aria-label="Delivery"], [aria-label="Delivery"] button')).map(
    (el) => el.getAttribute("aria-label") ?? el.closest("[aria-label]")?.getAttribute("aria-label"),
  );
  expect(names).toEqual(["Use worktrees for development", "Rebase before merging", "Revert gate", "Delivery"]);
});

it.each([
  { name: "Use worktrees for development", key: "use_worktrees" as const },
  { name: "Rebase before merging", key: "rebase_before_merge" as const },
  { name: "Revert gate", key: "revert_gate" as const },
])("sends only $key when its switch is turned", async ({ name, key }) => {
  const container = mount();
  await act(async () => toggle(container, name).click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", worktrees: { [key]: !DEFAULT_WORKTREE_SETTINGS[key] } });
});

it("sends only the delivery when another is chosen", async () => {
  const container = mount();
  await act(async () => container.querySelector<HTMLElement>('button[aria-label="Delivery"], [aria-label="Delivery"] button')!.click());
  const option = Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]')).find((o) => o.textContent?.includes("Open a pull request"))!;
  await act(async () => option.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", worktrees: { delivery: "pull_request" } });
});

it("says a rule change reaches later sessions, the revert gate at once, and a project's own rules first", () => {
  const text = mount().textContent!;
  expect(text).toContain("reaches only sessions begun after it");
  expect(text).toContain("takes effect at once, running sessions included");
  expect(text).toContain("a project's own rules");
  expect(text).not.toContain("installed itself");
});

it("shows only Use worktrees while worktrees are off, and brings the other three back with their values", async () => {
  worktrees = { ...DEFAULT_WORKTREE_SETTINGS, use_worktrees: false, delivery: "merge", revert_gate: true };
  const container = mount();
  expect(toggle(container, "Use worktrees for development")).toBeDefined();
  for (const name of ["Rebase before merging", "Revert gate"]) expect(Array.from(container.querySelectorAll("[aria-label]")).some((el) => el.getAttribute("aria-label") === name)).toBe(false);
  expect(container.querySelector('[aria-label="Delivery"]')).toBeNull();
  request.mockImplementation(((message: { worktrees: Partial<WorktreeSettings> }) => {
    store({ ...worktrees, ...message.worktrees });
    return Promise.resolve({});
  }) as never);
  await act(async () => toggle(container, "Use worktrees for development").click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", worktrees: { use_worktrees: true } });
  expect(toggle(container, "Rebase before merging").checked).toBe(true);
  expect(toggle(container, "Revert gate").checked).toBe(true);
  expect(container.querySelector<HTMLElement>('button[aria-label="Delivery"], [aria-label="Delivery"] button')?.textContent).toContain("Merge locally");
});

it("hands focus to Use worktrees when the row it was on is hidden from elsewhere", () => {
  const container = mount();
  act(() => toggle(container, "Revert gate").focus());
  expect(document.activeElement).toBe(toggle(container, "Revert gate"));
  store({ ...DEFAULT_WORKTREE_SETTINGS, use_worktrees: false });
  expect(document.activeElement).toBe(toggle(container, "Use worktrees for development"));
});

it("explains how a deliberate revert passes the gate in a help tip", () => {
  const container = mount();
  const tip = container.querySelector<HTMLElement>('button[aria-label="About passing the revert gate"]');
  expect(tip).not.toBeNull();
});

it("reports a failed change to the user", async () => {
  request.mockImplementation(() => Promise.reject(new Error("daemon refused")) as Promise<never>);
  const container = mount();
  await act(async () => toggle(container, "Revert gate").click());
  expect(toastError).toHaveBeenCalledOnce();
});
