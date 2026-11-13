// @vitest-environment jsdom
// Loaded first, as `main.tsx` loads it, so a guard that swallows the window's focus events fails this test.
import "../input/focusGuard";
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { filterField, holdExitAnimations, openBy, pressInFilter, typeInFilter } from "../components/menus/menuTestSupport";
import { accountOf, consoleOf, projectOf } from "../fixtures/builders";
import type { Agent, Project } from "../protocol";
import { DEFAULT_GIT_WORKFLOW } from "../git/gitWorkflow";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { fakeTimers, runOutTimers } from "../test/clock";
import { IS_MAC } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { NewSessionMenu } from "./NewSessionMenu";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Nor `matchMedia`, which the menu's motion asks about reduced motion.
window.matchMedia ??= ((query: string) => ({ matches: false, media: query })) as unknown as typeof window.matchMedia;

const main = consoleOf("c-1", "Main");
const web = projectOf("p-web", main.id, "Website", { default_agent: "claude" });
const api = projectOf("p-api", main.id, "Search API", { default_agent: "codex" });
const docs = projectOf("p-docs", main.id, "Documentation", { default_agent: "claude" });
const target = (project: Project): NewSessionRequest => ({ kind: "new-session", console: main, project, binding: { kind: "unbound" } });

interface Mounted {
  onOpen: ReturnType<typeof vi.fn>;
  onQuickCreate: ReturnType<typeof vi.fn>;
  container: HTMLElement;
  /** Gives the mounted menu another list of projects. */
  setProjects: (projects: Project[]) => void;
}

/** Mounts the menu over `projects`; `turnedOffDefaults` are the agents whose default account is turned off, which makes
 * a quick-created session of a project on that agent impossible. The clock is faked first, as react-aria hands focus to
 * the field a frame after a pointer opens the menu. */
function mount({ projects = [web, api, docs], turnedOffDefaults = [] }: { projects?: Project[]; turnedOffDefaults?: Agent[] } = {}): Mounted {
  fakeTimers();
  const daemon = testDaemon({
    state: {
      settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/p", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [accountOf("work", "claude", "Work", "/w")], disabled_default_accounts: turnedOffDefaults },
    },
  });
  const onOpen = vi.fn();
  const onQuickCreate = vi.fn();
  const menu = (list: Project[]) => <NewSessionMenu label="New session" quickLabel="New session with defaults" projects={list} target={target} onOpen={onOpen} onQuickCreate={onQuickCreate} />;
  const container = mountHost(menu(projects), { daemon });
  onTestFinished(() => void act(() => void window.dispatchEvent(new Event("blur"))));
  return { onOpen, onQuickCreate, container, setProjects: (list) => container.render(menu(list)) };
}

const hold = () => act(() => void window.dispatchEvent(new KeyboardEvent("keydown", IS_MAC ? { metaKey: true } : { ctrlKey: true })));
const release = () => act(() => void window.dispatchEvent(new KeyboardEvent("keyup")));

function createShortcut(quick = false): void {
  act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "n", code: "KeyN", ...(IS_MAC ? { metaKey: true } : { ctrlKey: true }), shiftKey: quick, cancelable: true, bubbles: true })));
}

const trigger = (container: HTMLElement) => container.querySelector("button")!;
const items = () => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")];
const names = () => items().map((el) => el.textContent).filter((text) => !text?.includes("No matching projects"));
/** Closes the menu with Escape; the field is empty then, so the one key closes it. */
const closeMenu = async (container: HTMLElement) => {
  await pressInFilter("Escape");
  expect(trigger(container).getAttribute("aria-expanded")).toBe("false");
};

it("opens with a focused filter field named for projects and every project below it", async () => {
  const { container } = mount();
  await openBy(trigger(container), "pointer");
  expect(filterField().getAttribute("aria-label")).toBe("Project name contains");
  expect(document.activeElement).toBe(filterField());
  expect(names().sort()).toEqual(["Documentation", "Search API", "Website"]);
});

it("says no project matches while none does", async () => {
  const { container } = mount();
  await openBy(trigger(container), "pointer");
  await typeInFilter("zzz");
  expect(names()).toEqual([]);
  expect(document.querySelector("[role=menuitem]")).toBeNull();
  expect(document.querySelector("[role=dialog]")?.textContent).toContain("No matching projects");
});

it("opens the dialog request of the project chosen from the narrowed list", async () => {
  const { container, onOpen, onQuickCreate } = mount();
  await openBy(trigger(container), "pointer");
  await typeInFilter("arch");
  await act(async () => items()[0].click());
  expect(onOpen).toHaveBeenCalledExactlyOnceWith(target(api));
  expect(onQuickCreate).not.toHaveBeenCalled();
});

it.each([
  ["new session", false],
  ["quick create", true],
])("takes focus into the field when the %s shortcut opens the menu", async (_, quick) => {
  mount();
  createShortcut(quick);
  runOutTimers();
  expect(document.querySelector("[role=menu]")).not.toBeNull();
  expect(document.activeElement).toBe(filterField());
});

it.each([
  ["Enter", async () => pressInFilter("Enter")],
  ["a click", async () => act(async () => items()[0].click())],
])("quick-creates for the project chosen from the narrowed list in a menu the quick shortcut opened: %s", async (_, choose) => {
  const { onOpen, onQuickCreate } = mount();
  createShortcut(true);
  runOutTimers();
  await typeInFilter("web");
  expect(names()).toEqual(["Website"]);
  await choose();
  expect(onQuickCreate).toHaveBeenCalledExactlyOnceWith(target(web));
  expect(onOpen).not.toHaveBeenCalled();
});

it("keeps the mode a shortcut chose after the modifier is released, with the field focused", async () => {
  const { onOpen, onQuickCreate } = mount();
  hold();
  createShortcut(true);
  runOutTimers();
  release();
  expect(document.activeElement).toBe(filterField());
  await typeInFilter("web");
  await pressInFilter("Enter");
  expect(onQuickCreate).toHaveBeenCalledExactlyOnceWith(target(web));
  expect(onOpen).not.toHaveBeenCalled();
});

it("leaves a project that cannot quick-create disabled with its reason, also once the list is narrowed to it, in a quick menu", async () => {
  const { onOpen, onQuickCreate } = mount({ turnedOffDefaults: ["codex"] });
  createShortcut(true);
  runOutTimers();
  const search = () => items().find((el) => el.textContent?.includes("Search API"))!;
  expect(search().getAttribute("aria-disabled")).toBe("true");
  expect(search().textContent).toContain("No available account");
  await typeInFilter("arch");
  expect(items()).toHaveLength(1);
  expect(search().getAttribute("aria-disabled")).toBe("true");
  expect(search().textContent).toContain("No available account");
  await act(async () => search().click());
  await pressInFilter("Enter");
  expect(onQuickCreate).not.toHaveBeenCalled();
  expect(onOpen).not.toHaveBeenCalled();
});

it("quick-creates the available project from a narrowed list that still shows one that cannot quick-create", async () => {
  const { onOpen, onQuickCreate } = mount({ turnedOffDefaults: ["codex"] });
  createShortcut(true);
  runOutTimers();
  await typeInFilter("a");
  expect(items().map((el) => el.textContent?.startsWith("Search API") ? "Search API" : el.textContent)).toEqual(["Documentation", "Search API"]);
  await act(async () => items().find((el) => el.textContent === "Documentation")!.click());
  expect(onQuickCreate).toHaveBeenCalledExactlyOnceWith(target(docs));
  expect(onOpen).not.toHaveBeenCalled();
});

it.each([
  ["Enter", async () => pressInFilter("Enter")],
  ["a click", async () => act(async () => items()[0].click())],
])("quick-creates from a menu the pointer opened with the modifier held, though it is released to type in the field: %s", async (_, choose) => {
  const { container, onOpen, onQuickCreate } = mount();
  hold();
  await openBy(trigger(container), "pointer");
  release();
  expect(trigger(container).getAttribute("aria-label")).toBe("New session with defaults");
  await typeInFilter("web");
  expect(names()).toEqual(["Website"]);
  expect(trigger(container).getAttribute("aria-label")).toBe("New session with defaults");
  await choose();
  expect(onQuickCreate).toHaveBeenCalledExactlyOnceWith(target(web));
  expect(onOpen).not.toHaveBeenCalled();
});

it("keeps a project that cannot quick-create disabled with its reason in a menu the pointer opened with the modifier held, once it is released and the list is narrowed", async () => {
  const { container, onOpen, onQuickCreate } = mount({ turnedOffDefaults: ["codex"] });
  hold();
  await openBy(trigger(container), "pointer");
  release();
  await typeInFilter("arch");
  expect(items()).toHaveLength(1);
  expect(items()[0].getAttribute("aria-disabled")).toBe("true");
  expect(items()[0].textContent).toContain("No available account");
  await act(async () => items()[0].click());
  await pressInFilter("Enter");
  expect(onQuickCreate).not.toHaveBeenCalled();
  expect(onOpen).not.toHaveBeenCalled();
});

it.each([
  ["Enter", async () => pressInFilter("Enter")],
  ["a click", async () => act(async () => items()[0].click())],
])("opens the dialog from a menu the pointer opened without the modifier, though the modifier is pressed while it is open: %s", async (_, choose) => {
  const { container, onOpen, onQuickCreate } = mount({ turnedOffDefaults: ["codex"] });
  await openBy(trigger(container), "pointer");
  hold();
  expect(trigger(container).getAttribute("aria-label")).toBe("New session");
  await typeInFilter("arch");
  expect(items()).toHaveLength(1);
  expect(items()[0].getAttribute("aria-disabled")).not.toBe("true");
  await choose();
  expect(onOpen).toHaveBeenCalledExactlyOnceWith(target(api));
  expect(onQuickCreate).not.toHaveBeenCalled();
});

it("takes the modifier's state at each opening: quick when it is held then, normal when it is not", async () => {
  const { container, onOpen, onQuickCreate } = mount();
  hold();
  await openBy(trigger(container), "pointer");
  release();
  await closeMenu(container);
  expect(trigger(container).getAttribute("aria-label")).toBe("New session");

  await openBy(trigger(container), "pointer");
  await typeInFilter("web");
  await pressInFilter("Enter");
  expect(onOpen).toHaveBeenCalledExactlyOnceWith(target(web));
  expect(onQuickCreate).not.toHaveBeenCalled();

  hold();
  await openBy(trigger(container), "pointer");
  release();
  await typeInFilter("doc");
  await pressInFilter("Enter");
  expect(onQuickCreate).toHaveBeenCalledExactlyOnceWith(target(docs));
  expect(onOpen).toHaveBeenCalledOnce();
});

it("takes the held modifier again at an opening after a shortcut-opened menu closed", async () => {
  const { container, onOpen, onQuickCreate } = mount();
  createShortcut(true);
  runOutTimers();
  await closeMenu(container);
  await openBy(trigger(container), "pointer");
  await typeInFilter("web");
  await pressInFilter("Enter");
  expect(onOpen).toHaveBeenCalledExactlyOnceWith(target(web));
  expect(onQuickCreate).not.toHaveBeenCalled();
});

it("is a disabled \"+\" with no project, so there is no menu to filter", () => {
  const { container } = mount({ projects: [] });
  expect(container.querySelector<HTMLButtonElement>("button")?.disabled).toBe(true);
});

it("takes the modifier of an opening that comes while the menu before it is still closing, not that menu's form", async () => {
  const { container, onOpen, onQuickCreate } = mount();
  hold();
  await openBy(trigger(container), "pointer");
  release();
  expect(trigger(container).getAttribute("aria-label")).toBe("New session with defaults");
  // Escape, with the popover kept on the screen as an exit animation would, when the menu is opened again.
  holdExitAnimations();
  await act(async () => {
    filterField().dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    filterField().dispatchEvent(new KeyboardEvent("keyup", { key: "Escape", bubbles: true, cancelable: true }));
  });
  expect(trigger(container).getAttribute("aria-expanded")).toBe("false");
  expect(document.querySelector("[role=menu]")).not.toBeNull();
  await openBy(trigger(container), "pointer");
  expect(trigger(container).getAttribute("aria-expanded")).toBe("true");
  expect(trigger(container).getAttribute("aria-label")).toBe("New session");
  await typeInFilter("web");
  await pressInFilter("Enter");
  expect(onOpen).toHaveBeenCalledExactlyOnceWith(target(web));
  expect(onQuickCreate).not.toHaveBeenCalled();
});

it("is not stuck in the form it had when it turns into the disabled \"+\" while open and back into the menu", async () => {
  const { container, onOpen, onQuickCreate, setProjects } = mount();
  hold();
  await openBy(trigger(container), "pointer");
  release();
  expect(trigger(container).getAttribute("aria-label")).toBe("New session with defaults");
  setProjects([]);
  runOutTimers();
  expect(trigger(container).disabled).toBe(true);
  setProjects([web, api, docs]);
  expect(trigger(container).getAttribute("aria-label")).toBe("New session");
  await openBy(trigger(container), "pointer");
  await typeInFilter("web");
  await pressInFilter("Enter");
  expect(onOpen).toHaveBeenCalledExactlyOnceWith(target(web));
  expect(onQuickCreate).not.toHaveBeenCalled();
});
