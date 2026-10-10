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
const hub1 = sessionOf("s-hub-1", main.id, undefined, "Hub", "idle", { colour: "teal" });
const hub2 = sessionOf("s-hub-2", main.id, undefined, "Hub 2", "idle", { colour: "rose" });
const hub3 = sessionOf("s-hub-3", main.id, undefined, "Hub 3", "archived", { colour: "jade" });
const sessions = [
  hub1,
  hub2,
  hub3,
  sessionOf("web-mine", main.id, web.id, "Fix the layout", "working", { bound_to: hub1.id, account_id: "work" }),
  sessionOf("web-hub-2", main.id, web.id, "Update the dependencies", "idle", { bound_to: hub2.id }),
  sessionOf("web-free", main.id, web.id, "Tidy the changelog", "idle"),
  sessionOf("web-old", main.id, web.id, "An old bound spike", "archived", { bound_to: hub1.id }),
  sessionOf("api-mine", main.id, api.id, "Add idempotency keys", "idle", { bound_to: hub1.id }),
];
const owners = new Map([hub1, hub2].map((s) => [s.id, s]));

// A team in `web`: an unbound session with one of its own, and a lead session (bound to Hub) with
// one of its own, which the project's focus mode leaves to Hub's and Hub's nests under it.
const lead = sessionOf("web-lead", main.id, web.id, "Rework the gallery", "idle", { bound_to: hub1.id });
const underLead = sessionOf("web-under-lead", main.id, web.id, "Chase the flaky test", "working", { bound_to: lead.id });
const underFree = sessionOf("web-under-free", main.id, web.id, "Measure the first paint", "idle", { bound_to: "web-free" });
const withTeams = [...sessions, lead, underLead, underFree];

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
  const handlers = { onFocus: vi.fn(), onOpenDialog: vi.fn(), onOpenArchive: vi.fn(), onSelectSession: vi.fn(), onSetPinned: vi.fn(), onSwitchConsoleSession: vi.fn() } as unknown as SidebarHandlers;
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
  expect(text()).toContain("2 sessions in this project are under console sessions");

  const hub2Link = container.querySelector<HTMLElement>('button[aria-label="Enter focus mode for Hub 2, 1 session under it"]');
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
  expect(labels.filter((l) => l.includes(" session,") && !l.startsWith("Hub")).map((l) => l.split(" session,")[0]).sort()).toEqual(
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

const chips = (container: HTMLElement) =>
  Array.from(container.querySelectorAll<HTMLElement>('[role=group][aria-label="Switch console session"] button'));

it("a console session's focus mode has a strip of the console's console sessions, the one in focus marked", () => {
  const { container, handlers, unmount } = mount((h) => (
    <ConsoleSessionFocusView handlers={h} console={main} consoleSession={hub1} projects={[web, api]} sessions={sessions} />
  ));
  // Hub's own bound session is working; Hub 2's console session is merely running; Hub 3 is archived.
  expect(chips(container).map((chip) => [chip.getAttribute("aria-label"), chip.getAttribute("aria-current")])).toEqual([
    ["Hub, a session is working", "true"],
    ["Hub 2", null],
  ]);
  act(() => chips(container)[1].click());
  expect(handlers.onSwitchConsoleSession).toHaveBeenCalledWith(hub2);
  expect(handlers.onSelectSession).not.toHaveBeenCalled();
  unmount();
});

it("a console session's focus mode has no strip when it is the console's only console session", () => {
  const { container, unmount } = mount((h) => (
    <ConsoleSessionFocusView handlers={h} console={main} consoleSession={hub1} projects={[web, api]} sessions={sessions.filter((s) => s.id !== hub2.id)} />
  ));
  expect(container.querySelector('[aria-label="Switch console session"]')).toBeNull();
  unmount();
});

it("a pinned session's pin button unpins it without selecting its card", () => {
  const pinned = sessionOf("web-pinned", main.id, web.id, "Pinned chore", "idle", { pinned: true });
  const { container, handlers, unmount } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={[pinned]} owners={owners} />
  ));
  const unpin = container.querySelector<HTMLElement>('button[aria-label="Unpin Pinned chore"]');
  act(() => unpin?.click());
  expect(handlers.onSetPinned).toHaveBeenCalledWith({ session: pinned }, false);
  expect(handlers.onSelectSession).not.toHaveBeenCalled();
  unmount();
});

it("a project's focus mode nests an unbound session's own sessions under it and leaves a lead session's team to its console session", () => {
  const { container, text, unmount } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={withTeams.filter((s) => s.project_id === web.id)} owners={owners} />
  ));
  const cards = Array.from(container.querySelectorAll<HTMLElement>("[role=button][data-marquee-scope]")).filter((card) =>
    card.className.includes("flex-col"),
  );
  expect(cards.map((card) => [card.getAttribute("aria-label")?.split(" session,")[0], card.parentElement!.className.includes("ms-4")])).toEqual([
    ["Tidy the changelog", false],
    ["Measure the first paint", true],
  ]);
  expect(text()).toContain("Sessions (2)");
  // The line and the chip count the lead session and the session under it.
  expect(text()).toContain("4 sessions in this project are under console sessions");
  expect(container.querySelector('button[aria-label="Enter focus mode for Hub, 3 sessions under it"]')).not.toBeNull();
  unmount();
});

it("a console session's focus mode nests a lead session's own sessions under it", () => {
  const { container, text, unmount } = mount((h) => (
    <ConsoleSessionFocusView handlers={h} console={main} consoleSession={hub1} projects={[web, api]} sessions={withTeams} />
  ));
  const cards = Array.from(container.querySelectorAll<HTMLElement>("[role=button][data-marquee-scope]")).filter((card) =>
    card.className.includes("flex-col"),
  );
  expect(cards.map((card) => [card.getAttribute("aria-label")?.split(" session,")[0], card.parentElement!.className.includes("ms-4")])).toEqual([
    ["Fix the layout", false],
    ["Rework the gallery", false],
    ["Chase the flaky test", true],
    ["Add idempotency keys", false],
  ]);
  expect(text()).toContain("Sessions (4)");
  unmount();
});
