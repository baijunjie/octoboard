// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ConsoleSessionFocusView, ProjectFocusView } from "./FocusView";
import type { SidebarHandlers } from "./types";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom has no ResizeObserver; FadeOverflow and HeroUI's ScrollShadow each need one to mount.
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;
// Nor `matchMedia`, which `useFlip` asks about reduced motion.
window.matchMedia ??= ((query: string) => ({ matches: false, media: query })) as unknown as typeof window.matchMedia;

const main = consoleOf("c-1", "Main");
const web = projectOf("p-web", main.id, "Website");
const api = projectOf("p-api", main.id, "Search API");
const hub1 = sessionOf("s-hub-1", main.id, undefined, "Hub 1", "idle", { colour: "teal" });
const hub2 = sessionOf("s-hub-2", main.id, undefined, "Hub 2", "idle", { colour: "rose" });
const sessions = [
  hub1,
  hub2,
  sessionOf("web-mine", main.id, web.id, "Fix the layout", "working", { bound_to: hub1.id, account_id: "work" }),
  sessionOf("web-hub-2", main.id, web.id, "Update the dependencies", "idle", { bound_to: hub2.id }),
  sessionOf("web-free", main.id, web.id, "Tidy the changelog", "idle"),
  sessionOf("web-old", main.id, web.id, "An old bound spike", "archived", { bound_to: hub1.id }),
  sessionOf("api-mine", main.id, api.id, "Add idempotency keys", "idle", { bound_to: hub1.id }),
];
const owners = new Map([hub1, hub2].map((s) => [s.id, s]));

function mount(render: (handlers: SidebarHandlers) => React.ReactElement): { text: () => string; container: HTMLElement; handlers: SidebarHandlers; unmount: () => void } {
  const daemon: Daemon = {
    store: createStateStore({ settings: { auto_sync_repositories: false, default_clone_dir: "/p", accounts: [{ id: "work", agent: "claude", name: "Work", config_dir: "/w" }] } }),
    request: vi.fn(),
    toastError: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const handlers = { onFocus: vi.fn(), onOpenDialog: vi.fn(), onOpenArchive: vi.fn(), onSelectSession: vi.fn() } as unknown as SidebarHandlers;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<DaemonProvider value={daemon}>{render(handlers)}</DaemonProvider>));
  return { text: () => container.textContent ?? "", container, handlers, unmount: () => (act(() => root.unmount()), container.remove()) };
}

const inProject = (id: string) => sessions.filter((s) => s.project_id === id);
const sessionLabels = (container: HTMLElement) =>
  Array.from(container.querySelectorAll("[role=button][data-marquee-scope]")).map((row) => row.getAttribute("aria-label") ?? "");

it("a project's focus mode lists only its unbound sessions, its archive in full, and one line for the rest", () => {
  const { container, text, handlers, unmount } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={inProject(web.id)} owners={owners} />
  ));
  const labels = sessionLabels(container);
  expect(labels.some((l) => l.includes("Tidy the changelog"))).toBe(true);
  expect(labels.some((l) => l.includes("Fix the layout") || l.includes("Update the dependencies"))).toBe(false);
  expect(labels.some((l) => l.includes("An old bound spike"))).toBe(true);
  expect(text()).toContain("2 sessions in this project are bound to Hub 1 and Hub 2");

  const hub2Link = container.querySelector<HTMLElement>('button[aria-label="Enter focus mode for Hub 2"]');
  act(() => hub2Link?.click());
  expect(handlers.onFocus).toHaveBeenCalledWith({ consoleSession: hub2 });
  unmount();
});

it("a project's focus mode has no line when nothing is bound here", () => {
  const { text, unmount } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={inProject(web.id).filter((s) => !s.bound_to || s.status === "archived")} owners={owners} />
  ));
  expect(text()).not.toContain("bound to");
  unmount();
});

it("a console session's focus mode shows only its projects and sessions, naming each session's account", () => {
  const { container, text, handlers, unmount } = mount((h) => (
    <ConsoleSessionFocusView handlers={h} console={main} consoleSession={hub1} projects={[web, api]} sessions={sessions} />
  ));
  const labels = sessionLabels(container);
  expect(labels.filter((l) => l.includes(" session,") && !l.startsWith("Hub 1")).map((l) => l.split(" session,")[0]).sort()).toEqual(
    ["Add idempotency keys", "An old bound spike", "Fix the layout"].sort(),
  );
  expect(text()).toContain("Claude Code (Work)");
  expect(text()).toContain("Website");
  expect(text()).toContain("Search API");

  const newInProject = container.querySelector<HTMLElement>('button[aria-label="New session in Website"]');
  act(() => newInProject?.click());
  expect(handlers.onOpenDialog).toHaveBeenCalledWith(expect.objectContaining({ kind: "new-session", binding: { kind: "bound", to: hub1 } }));
  unmount();
});

it("a console session's focus mode offers neither a project's own focus mode nor the project filter", () => {
  const { container, unmount } = mount((h) => (
    <ConsoleSessionFocusView handlers={h} console={main} consoleSession={hub2} projects={[web, api]} sessions={sessions} />
  ));
  expect(container.querySelector('[aria-label*="Filter"]')).toBeNull();
  unmount();
});
