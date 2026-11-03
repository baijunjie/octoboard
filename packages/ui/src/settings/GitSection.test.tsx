// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { DEFAULT_GIT_WORKFLOW } from "../git/gitWorkflow";
import { LanguageProvider } from "../i18n/react";
import type { GitWorkflowSettings } from "../protocol";
import { DEFAULT_SETTINGS, type Daemon } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { GitSection } from "./GitSection";
import { errorAlerts } from "../components/alerts/errorAlerts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

vi.mock("./CloneDirSetting", () => ({ CloneDirSetting: () => <p>Clone directory</p> }));

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);
const raiseError = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
let daemon: Daemon;
/** Puts the store's git workflow to `next`, as a change from elsewhere does. */
const store = (next: GitWorkflowSettings) => act(() => daemon.store.setState({ settings: { ...daemon.store.getState().settings, git_workflow: next } }));

beforeEach(() => {
  request.mockClear();
  raiseError.mockClear();
  request.mockImplementation(() => Promise.resolve({}) as Promise<never>);
});

function mount({ gitWorkflow = DEFAULT_GIT_WORKFLOW, autoPull = false }: { gitWorkflow?: GitWorkflowSettings; autoPull?: boolean } = {}) {
  daemon = testDaemon({ state: { settings: { ...DEFAULT_SETTINGS, git_workflow: gitWorkflow, auto_pull_repositories: autoPull } }, request });
  return mountHost(<LanguageProvider><GitSection /></LanguageProvider>, { daemon });
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
  const container = mount({ gitWorkflow: { development: "branch", squash_before_delivery: false, rebase_before_delivery: true, delivery: "merge", revert_gate: true } });
  expect(picker(container, "Where sessions develop")?.textContent).toContain("On a branch of its own");
  expect(toggle(container, "Squash commits before delivery").checked).toBe(false);
  expect(toggle(container, "Rebase before delivery").checked).toBe(true);
  expect(toggle(container, "Revert gate").checked).toBe(true);
  expect(picker(container, "Delivery")?.textContent).toContain("Merge locally");
});

it("lists the controls in order: Where sessions develop, Squash commits before delivery, Rebase before delivery, Delivery, Revert gate", () => {
  const container = mount();
  const names = Array.from(container.querySelectorAll('input[type=checkbox], button[aria-label="Where sessions develop"], button[aria-label="Delivery"], [aria-label="Where sessions develop"] button, [aria-label="Delivery"] button')).map(
    (el) => el.getAttribute("aria-label") ?? el.closest("[aria-label]")?.getAttribute("aria-label"),
  );
  expect(names.filter((name) => name !== "Automatically pull repositories")).toEqual(["Where sessions develop", "Squash commits before delivery", "Rebase before delivery", "Delivery", "Revert gate"]);
});

it.each([
  { name: "Squash commits before delivery", key: "squash_before_delivery" as const },
  { name: "Rebase before delivery", key: "rebase_before_delivery" as const },
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
  const container = mount({ gitWorkflow: { ...DEFAULT_GIT_WORKFLOW, development: "branch" } });
  expect(container.textContent).toContain("shares that directory, so this suits one session at a time");
  expect(container.textContent).toContain("reaches only sessions begun after it");
  store({ ...DEFAULT_GIT_WORKFLOW, development: "in_place" });
  expect(container.textContent).toContain("directly on the current branch");
});

it("says the revert gate takes effect at once and a project's own rules come first", () => {
  const text = mount().textContent!;
  expect(text).toContain("takes effect at once, running sessions included");
  expect(text).toContain("a project's own rules");
});

it("hides rebase and delivery on the current branch, keeps the revert gate, and brings them back with their values", async () => {
  const container = mount({ gitWorkflow: { ...DEFAULT_GIT_WORKFLOW, development: "in_place", delivery: "merge", revert_gate: true } });
  expect(picker(container, "Where sessions develop")).not.toBeNull();
  expect(toggle(container, "Revert gate").checked).toBe(true);
  expect(shown(container, "Squash commits before delivery")).toBe(false);
  expect(shown(container, "Rebase before delivery")).toBe(false);
  expect(shown(container, "Delivery")).toBe(false);
  request.mockImplementation(((message: { git_workflow: Partial<GitWorkflowSettings> }) => {
    store({ ...daemon.store.getState().settings.git_workflow, ...message.git_workflow });
    return Promise.resolve({});
  }) as never);
  await choose(container, "Where sessions develop", "In a worktree of its own");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", git_workflow: { development: "worktree" } });
  expect(toggle(container, "Squash commits before delivery").checked).toBe(false);
  expect(toggle(container, "Rebase before delivery").checked).toBe(true);
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
  expect(raiseError).toHaveBeenCalledOnce();
});

it("splits the rows into Repository, Development and Protection under h3 headings", () => {
  const container = mount();
  const groups = Array.from(container.querySelectorAll("section")).map((section) => ({
    heading: section.querySelector("h3")?.textContent,
    labelled: section.getAttribute("aria-labelledby") === section.querySelector("h3")?.id,
    has: ["Clone directory", "Where sessions develop", "Squash commits before delivery", "Rebase before delivery", "Delivery", "Revert gate"].filter((name) => section.textContent!.includes(name) || shown(section as HTMLElement, name)),
  }));
  expect(groups.map((group) => group.heading)).toEqual(["Repository", "Development", "Protection"]);
  expect(groups.every((group) => group.labelled)).toBe(true);
  expect(Array.from(container.querySelectorAll("h3")).every((heading) => heading.closest("[data-sticky-heading]") !== null)).toBe(true);
  expect(groups[0].has).toEqual(["Clone directory"]);
  expect(groups[1].has).toEqual(["Where sessions develop", "Squash commits before delivery", "Rebase before delivery", "Delivery"]);
  expect(groups[2].has).toEqual(["Revert gate"]);
});

it.each([
  { name: "what turning it on does while it is off", on: false, shown: "Once this is on, a project's branch is pulled" },
  { name: "what is happening now while it is on", on: true, shown: "A project's branch is pulled" },
])("describes automatic pulling by $name", ({ on, shown }) => {
  const container = mount({ autoPull: on });
  expect(container.textContent).toContain(shown);
  expect(container.textContent?.includes("Once this is on, a project's branch is pulled")).toBe(!on);
});
