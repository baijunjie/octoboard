// @vitest-environment jsdom
import { act, useSyncExternalStore } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { DEFAULT_GIT_WORKFLOW } from "../gitWorkflow";
import { LanguageProvider } from "../i18n/react";
import type { GitWorkflowSettings } from "../protocol";
import { GitSection } from "./GitSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

vi.mock("./CloneDirSetting", () => ({ CloneDirSetting: () => <p>Clone directory</p> }));

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);
const toastError = vi.fn();
let gitWorkflow: GitWorkflowSettings = DEFAULT_GIT_WORKFLOW;

const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};
const store = (next: GitWorkflowSettings) => {
  gitWorkflow = next;
  act(() => listeners.forEach((listener) => listener()));
};

vi.mock("../store", () => ({
  useDaemon: () => ({ request, toastError }),
  useDaemonStore: (selector: (state: { settings: { git_workflow: GitWorkflowSettings; auto_pull_repositories: boolean } }) => unknown) =>
    useSyncExternalStore(subscribe, () => selector({ settings: { git_workflow: gitWorkflow, auto_pull_repositories: false } })),
}));

beforeEach(() => {
  request.mockClear();
  toastError.mockClear();
  request.mockImplementation(() => Promise.resolve({}) as Promise<never>);
  gitWorkflow = DEFAULT_GIT_WORKFLOW;
});

function mount() {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<LanguageProvider><GitSection /></LanguageProvider>));
  return container;
}

const toggle = (container: HTMLElement, name: string) =>
  Array.from(container.querySelectorAll<HTMLInputElement>("input[type=checkbox]")).find((el) => el.getAttribute("aria-label") === name)!;
const picker = (container: HTMLElement, name: string) =>
  container.querySelector<HTMLElement>(`button[aria-label="${name}"], [aria-label="${name}"] button`);
const shown = (container: HTMLElement, name: string) => Array.from(container.querySelectorAll("[aria-label]")).some((el) => el.getAttribute("aria-label") === name);
const choose = async (container: HTMLElement, name: string, option: string) => {
  await act(async () => picker(container, name)!.click());
  await act(async () => Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]')).find((o) => o.textContent?.includes(option))!.click());
};

it("shows the daemon's values, every control named", () => {
  gitWorkflow = { development: "branch", rebase_before_merge: true, delivery: "merge", revert_gate: true };
  const container = mount();
  expect(picker(container, "Where sessions develop")?.textContent).toContain("On a branch of its own");
  expect(toggle(container, "Rebase before merging").checked).toBe(true);
  expect(toggle(container, "Revert gate").checked).toBe(true);
  expect(picker(container, "Delivery")?.textContent).toContain("Merge locally");
});

it("lists the controls in order: Where sessions develop, Rebase before merging, Delivery, Revert gate", () => {
  const container = mount();
  const names = Array.from(container.querySelectorAll('input[type=checkbox], button[aria-label="Where sessions develop"], button[aria-label="Delivery"], [aria-label="Where sessions develop"] button, [aria-label="Delivery"] button')).map(
    (el) => el.getAttribute("aria-label") ?? el.closest("[aria-label]")?.getAttribute("aria-label"),
  );
  expect(names.filter((name) => name !== "Automatically pull repositories")).toEqual(["Where sessions develop", "Rebase before merging", "Delivery", "Revert gate"]);
});

it.each([
  { name: "Rebase before merging", key: "rebase_before_merge" as const },
  { name: "Revert gate", key: "revert_gate" as const },
])("sends only $key when its switch is turned", async ({ name, key }) => {
  const container = mount();
  await act(async () => toggle(container, name).click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", git_workflow: { [key]: !DEFAULT_GIT_WORKFLOW[key] } });
});

it("sends only the development when another is chosen", async () => {
  const container = mount();
  await choose(container, "Where sessions develop", "On the current branch");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", git_workflow: { development: "in_place" } });
});

it("sends only the delivery when another is chosen", async () => {
  const container = mount();
  await choose(container, "Delivery", "Open a pull request");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", git_workflow: { delivery: "pull_request" } });
});

it("describes the rule each development value gives, for the branch variant naming the shared directory", () => {
  gitWorkflow = { ...DEFAULT_GIT_WORKFLOW, development: "branch" };
  const container = mount();
  expect(container.textContent).toContain("shares that directory, so this suits one session at a time");
  expect(container.textContent).toContain("reaches only sessions begun after it");
  act(() => store({ ...gitWorkflow, development: "in_place" }));
  expect(container.textContent).toContain("directly on the current branch");
});

it("says the revert gate takes effect at once and a project's own rules come first", () => {
  const text = mount().textContent!;
  expect(text).toContain("takes effect at once, running sessions included");
  expect(text).toContain("a project's own rules");
});

it("hides rebase and delivery on the current branch, keeps the revert gate, and brings them back with their values", async () => {
  gitWorkflow = { ...DEFAULT_GIT_WORKFLOW, development: "in_place", delivery: "merge", revert_gate: true };
  const container = mount();
  expect(picker(container, "Where sessions develop")).not.toBeNull();
  expect(toggle(container, "Revert gate").checked).toBe(true);
  expect(shown(container, "Rebase before merging")).toBe(false);
  expect(shown(container, "Delivery")).toBe(false);
  request.mockImplementation(((message: { git_workflow: Partial<GitWorkflowSettings> }) => {
    store({ ...gitWorkflow, ...message.git_workflow });
    return Promise.resolve({});
  }) as never);
  await choose(container, "Where sessions develop", "In a worktree of its own");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", git_workflow: { development: "worktree" } });
  expect(toggle(container, "Rebase before merging").checked).toBe(true);
  expect(picker(container, "Delivery")?.textContent).toContain("Merge locally");
});

it("hands focus to the development picker when the row it was on is hidden from elsewhere", () => {
  const container = mount();
  act(() => picker(container, "Delivery")!.focus());
  expect(document.activeElement).toBe(picker(container, "Delivery"));
  store({ ...DEFAULT_GIT_WORKFLOW, development: "in_place" });
  expect(document.activeElement).toBe(picker(container, "Where sessions develop"));
});

it("explains how a deliberate revert passes the gate in a help tip", () => {
  const container = mount();
  expect(container.querySelector<HTMLElement>('button[aria-label="About passing the revert gate"]')).not.toBeNull();
});

it("reports a failed change to the user", async () => {
  request.mockImplementation(() => Promise.reject(new Error("daemon refused")) as Promise<never>);
  const container = mount();
  await act(async () => toggle(container, "Revert gate").click());
  expect(toastError).toHaveBeenCalledOnce();
});

it("splits the rows into Repository, Development and Protection under h3 headings", () => {
  const container = mount();
  const groups = Array.from(container.querySelectorAll("section")).map((section) => ({
    heading: section.querySelector("h3")?.textContent,
    labelled: section.getAttribute("aria-labelledby") === section.querySelector("h3")?.id,
    has: ["Clone directory", "Where sessions develop", "Rebase before merging", "Delivery", "Revert gate"].filter((name) => section.textContent!.includes(name) || shown(section as HTMLElement, name)),
  }));
  expect(groups.map((group) => group.heading)).toEqual(["Repository", "Development", "Protection"]);
  expect(groups.every((group) => group.labelled)).toBe(true);
  expect(Array.from(container.querySelectorAll("h3")).every((heading) => heading.hasAttribute("data-sticky-heading"))).toBe(true);
  expect(groups[0].has).toEqual(["Clone directory"]);
  expect(groups[1].has).toEqual(["Where sessions develop", "Rebase before merging", "Delivery"]);
  expect(groups[2].has).toEqual(["Revert gate"]);
});
