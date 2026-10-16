// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { expectIndependentMarquees, stubMarquee } from "../components/marqueeTestSupport";
import { isolate } from "../i18n/isolate";
import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { sessionColourHex } from "../sessionColours";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ConsoleSessionFocusView, ProjectFocusView } from "./FocusView";
import { projectFocusIncludeConsoleSessions } from "./projectFocusPreference";
import type { SidebarHandlers } from "./types";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The clock's own timer would count as pending work in the marquee checks, which these tests do not use it for.
vi.mock("../useNow", () => ({ useNow: () => 0 }));

// Nor `matchMedia`, which `useFlip` asks about reduced motion.
window.matchMedia ??= ((query: string) => ({ matches: false, media: query })) as unknown as typeof window.matchMedia;

const main = consoleOf("c-1", "Main");
const web = projectOf("p-web", main.id, "Website");
const api = projectOf("p-api", main.id, "Search API");
const hub1 = sessionOf("s-hub-1", main.id, undefined, "Hub", "idle", { colour: "teal" });
const hub2 = sessionOf("s-hub-2", main.id, undefined, "Hub 2", "idle", { colour: "pink" });
const hub3 = sessionOf("s-hub-3", main.id, undefined, "Hub 3", "archived", { colour: "lime" });
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
    store: createStateStore({ settings: { auto_pull_repositories: false, default_clone_dir: "/p", accounts: [{ id: "work", agent: "claude", name: "Work", config_dir: "/w" }] } }),
    request: vi.fn(),
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const handlers = { onFocus: vi.fn(), onOpenDialog: vi.fn(), onOpenArchive: vi.fn(), onSelectSession: vi.fn(), onSetPinned: vi.fn(), onSwitchConsoleSession: vi.fn() } as unknown as SidebarHandlers;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<DaemonProvider value={daemon}>{render(handlers)}</DaemonProvider>));
  const unmount = () => {
    act(() => root.unmount());
    container.remove();
  };
  onTestFinished(unmount);
  return { text: () => container.textContent ?? "", container, handlers, unmount };
}

const inProject = (id: string) => sessions.filter((s) => s.project_id === id);
const sessionLabels = (container: HTMLElement) =>
  Array.from(container.querySelectorAll("div[role=button]")).map((row) => row.getAttribute("aria-label") ?? "");

it("a project's focus mode lists only its unbound sessions, its archived parent sessions, and one line for the rest", () => {
  const { container, text, handlers, unmount } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={inProject(web.id)} owners={owners} />
  ));
  const labels = sessionLabels(container);
  expect(labels.some((l) => l.includes("Tidy the changelog"))).toBe(true);
  expect(labels.some((l) => l.includes("Fix the layout") || l.includes("Update the dependencies"))).toBe(false);
  expect(labels.some((l) => l.includes("An old bound spike"))).toBe(true);
  expect(text()).toContain("2 sessions in this project are under console sessions");
  expect(text().indexOf("2 sessions in this project")).toBeLessThan(text().indexOf("Archived (1)"));
  expect(Array.from(container.querySelectorAll(".badge__label")).map((badge) => badge.textContent)).toEqual(["1", "1"]);

  const hub2Link = container.querySelector<HTMLElement>(`button[aria-label="Enter focus mode for ${isolate("Hub 2")}, 1 session under it"]`);
  act(() => hub2Link?.click());
  expect(handlers.onFocus).toHaveBeenCalledWith({ consoleSession: hub2 });
  unmount();
});

it.each([
  ["empty", []],
  ["unbound sessions and a bound archive", inProject(web.id).filter((s) => !s.bound_to || s.status === "archived")],
  ["an unbound project session's team", [sessions.find((s) => s.id === "web-free")!, underFree]],
])("project focus hides the combined-view switch for %s", (_name, projectSessions) => {
  const { container, text } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={projectSessions} owners={owners} />
  ));
  expect(text()).not.toContain("sessions in this project are under console sessions");
  expect(container.querySelector('input[role="switch"]')).toBeNull();
});

it("removing the last live console-bound session hides the switch, restores focus and preserves its preference", () => {
  projectFocusIncludeConsoleSessions.set(true);
  onTestFinished(() => projectFocusIncludeConsoleSessions.set(false));
  let archive!: () => void;
  function View({ handlers }: { handlers: SidebarHandlers }) {
    const [bound, setBound] = useState(lead);
    archive = () => setBound({ ...lead, status: "archived" });
    return <ProjectFocusView handlers={handlers} console={main} project={web} sessions={[bound]} owners={owners} />;
  }
  const { container } = mount((handlers) => <View handlers={handlers} />);
  const toggle = container.querySelector<HTMLInputElement>('input[role="switch"]')!;
  expect(toggle.checked).toBe(true);
  act(() => toggle.focus());
  expect(document.activeElement).toBe(toggle);
  act(archive);
  expect(container.querySelector('input[role="switch"]')).toBeNull();
  expect(document.activeElement?.getAttribute("aria-label")).toBe("Leave focus mode");
  expect(localStorage.getItem(PREFERENCE_KEYS.projectFocusIncludeConsoleSessions)).toBe("true");
});

it("a console session's focus mode shows only its projects and sessions without inline account names", () => {
  const { container, text, handlers, unmount } = mount((h) => (
    <ConsoleSessionFocusView handlers={h} console={main} consoleSession={hub1} projects={[web, api]} sessions={sessions} />
  ));
  const labels = sessionLabels(container);
  expect(labels.filter((l) => l.includes(" session,") && !l.startsWith(isolate("Hub"))).map((l) => l.split(" session,")[0]).sort()).toEqual(
    ["Add idempotency keys", "An old bound spike", "Fix the layout"].map(isolate).sort(),
  );
  expect(text()).toContain("Claude Code");
  expect(text()).not.toContain("(Work)");
  expect(text()).toContain("Website");
  expect(text()).toContain("Search API");

  const newInProject = container.querySelector<HTMLElement>(`button[aria-label="New session in ${isolate("Website")}"]`);
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
    [`${isolate("Hub")}, a session is working`, "true"],
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
  const unpin = container.querySelector<HTMLElement>(`button[aria-label="Unpin ${isolate("Pinned chore")}"]`);
  act(() => unpin?.click());
  expect(handlers.onSetPinned).toHaveBeenCalledWith({ session: pinned }, false);
  expect(handlers.onSelectSession).not.toHaveBeenCalled();
  unmount();
});

it("a project's focus mode nests an unbound session's own sessions under it and leaves a lead session's team to its console session", () => {
  const { container, text, unmount } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={withTeams.filter((s) => s.project_id === web.id)} owners={owners} />
  ));
  const cards = Array.from(container.querySelectorAll<HTMLElement>("div[role=button]")).filter((card) =>
    card.className.includes("flex-col"),
  );
  expect(cards.map((card) => [card.getAttribute("aria-label")?.split(" session,")[0], card.parentElement!.className.includes("ms-[17px]")])).toEqual([
    [isolate("Tidy the changelog"), false],
    [isolate("Measure the first paint"), true],
  ]);
  expect(text()).toContain("Sessions (2)");
  // The line and the chip count the lead session and the session under it.
  expect(text()).toContain("4 sessions in this project are under console sessions");
  expect(container.querySelector(`button[aria-label="Enter focus mode for ${isolate("Hub")}, 3 sessions under it"]`)).not.toBeNull();
  unmount();
});

it("a console session's focus mode nests a lead session's own sessions under it", () => {
  const { container, text, unmount } = mount((h) => (
    <ConsoleSessionFocusView handlers={h} console={main} consoleSession={hub1} projects={[web, api]} sessions={withTeams} />
  ));
  const cards = Array.from(container.querySelectorAll<HTMLElement>("div[role=button]")).filter((card) =>
    card.className.includes("flex-col"),
  );
  expect(cards.map((card) => [card.getAttribute("aria-label")?.split(" session,")[0], card.parentElement!.className.includes("ms-[17px]")])).toEqual([
    [isolate("Rework the gallery"), false],
    [isolate("Chase the flaky test"), true],
    [isolate("Fix the layout"), false],
    [isolate("Add idempotency keys"), false],
  ]);
  expect(text()).toContain("Sessions (4)");
  unmount();
});

it("project focus uses a clickable empty row beside its archive instead of a second empty heading", () => {
  const { container, handlers, unmount } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={inProject(web.id).filter((s) => s.status === "archived")} owners={owners} />
  ));
  expect(Array.from(container.querySelectorAll("h2,h3")).some((heading) => heading.textContent === "No sessions yet")).toBe(false);
  const row = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes("No sessions yet"));
  expect(row).toBeDefined();
  act(() => row?.click());
  expect(handlers.onOpenDialog).toHaveBeenCalledWith({ kind: "new-session", console: main, project: web, binding: { kind: "unbound" } });
  unmount();
});

it.each([false, true])("console focus keeps its empty nested list compact (projects present: %s)", (hasProjects) => {
  const { container, handlers, unmount } = mount((h) => (
    <ConsoleSessionFocusView handlers={h} console={main} consoleSession={hub1} projects={hasProjects ? [web] : []} sessions={[hub1]} />
  ));
  expect(Array.from(container.querySelectorAll("h2,h3")).map((heading) => heading.textContent)).toEqual(["Hub", "Sessions (0)"]);
  if (!hasProjects) {
    const row = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes("No projects yet"));
    expect(row).toBeDefined();
    act(() => row?.click());
    expect(handlers.onOpenDialog).toHaveBeenCalledWith({ kind: "new-project", console: main });
  }
  unmount();
});

it("combining project sessions replaces the console summary with cards carrying their root console colours", () => {
  onTestFinished(() => projectFocusIncludeConsoleSessions.set(false));
  const archivedChild = { ...underLead, id: "archived-child", title: "An archived child of the live lead", status: "archived" as const };
  const { container, text, handlers } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={[...withTeams.filter((s) => s.project_id === web.id), archivedChild]} owners={owners} />
  ));
  const toggle = container.querySelector<HTMLInputElement>('input[role="switch"]')!;
  expect(toggle.checked).toBe(false);
  expect(text()).not.toContain(archivedChild.title);
  act(() => toggle.click());
  expect(toggle.checked).toBe(true);
  expect(localStorage.getItem(PREFERENCE_KEYS.projectFocusIncludeConsoleSessions)).toBe("true");
  expect(text()).not.toContain("sessions in this project are under console sessions");
  expect(text()).toContain("Sessions (6)");
  const card = (title: string) => Array.from(container.querySelectorAll<HTMLElement>('div[role="button"]')).find((row) => row.getAttribute("aria-label")?.startsWith(isolate(title)))!;
  for (const session of [lead, underLead]) {
    expect(card(session.title).querySelector(`[style*="--session-colour-light: ${sessionColourHex("teal", "light")}"]`)).not.toBeNull();
    expect(card(session.title).getAttribute("aria-label")).toContain(isolate(hub1.title));
  }
  expect(card(underLead.title).getAttribute("aria-label")).toContain(isolate(lead.title));
  expect(card(underLead.title).parentElement!.className).toContain("ms-[17px]");
  expect(card("Tidy the changelog").querySelector(".session-colour-dot")).toBeNull();
  expect(text()).toContain("An old bound spike");
  expect(text()).not.toContain(archivedChild.title);
  act(() => card(underLead.title).click());
  expect(handlers.onSelectSession).toHaveBeenCalledWith(underLead);
  act(() => toggle.click());
  expect(text()).toContain("4 sessions in this project are under console sessions");
  expect(text()).not.toContain(archivedChild.title);
  expect(localStorage.getItem(PREFERENCE_KEYS.projectFocusIncludeConsoleSessions)).toBe("false");
});

it.each(["project", "console"] as const)("%s focus previews ten archived parents, counts children and opens the full archive", (view) => {
  const parents = Array.from({ length: 10 }, (_, index) => sessionOf(`root-${index}`, main.id, web.id, `Archived parent ${index}`, "archived", { ended_at: index, bound_to: hub1.id }));
  const child = sessionOf("child", main.id, web.id, "Hidden archived child", "archived", { ended_at: 10, bound_to: parents[9].id });
  const context = [hub1, ...parents, child];
  const { container, text, handlers } = mount((h) => view === "project" ? (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={context.filter((s) => s.project_id === web.id)} owners={owners} />
  ) : (
    <ConsoleSessionFocusView handlers={h} console={main} consoleSession={hub1} projects={[web]} sessions={context} />
  ));
  expect(text()).toContain("Archived (11)");
  const rows = Array.from(container.querySelectorAll<HTMLElement>('div[role="button"]'))
    .filter((row) => row.getAttribute("aria-label")?.includes("Archived parent"));
  expect(rows.map((row) => row.textContent?.match(/Archived parent \d/)?.[0])).toEqual(Array.from({ length: 10 }, (_, index) => `Archived parent ${9 - index}`));
  expect(text()).not.toContain(child.title);
  act(() => rows[0].click());
  expect(handlers.onSelectSession).toHaveBeenCalledWith(parents[9]);
  const viewAll = container.querySelector<HTMLElement>('[role="button"][aria-label="View all"]')!;
  act(() => viewAll.click());
  expect(handlers.onOpenArchive).toHaveBeenCalledWith(view === "project" ? { console: main.id, project: web.id } : { console: main.id, consoleSession: hub1.id });
});

it.each(["idle", "archived"] as const)("project focus hides a child and keeps its archive reachable while its parent is %s", (status) => {
  const parent = sessionOf("parent", main.id, web.id, "Parent", status);
  const child = sessionOf("child", main.id, web.id, "Hidden child", "archived", { bound_to: parent.id });
  const { container, text, handlers } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={[parent, child]} owners={owners} />
  ));
  expect(text()).toContain(`Archived (${status === "archived" ? 2 : 1})`);
  expect(text()).not.toContain(child.title);
  expect(text()).not.toContain("No archived sessions");
  const viewAll = container.querySelector<HTMLElement>('[role="button"][aria-label="View all"]')!;
  act(() => viewAll.click());
  expect(handlers.onOpenArchive).toHaveBeenCalledWith({ console: main.id, project: web.id });
});

it.each([10, 11])("project focus offers View all for an archive of %s parents exactly when the preview omits one", (count) => {
  const archived = Array.from({ length: count }, (_, index) => sessionOf(`root-${index}`, main.id, web.id, `Parent ${index}`, "archived"));
  const { container } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={archived} owners={owners} />
  ));
  expect(container.querySelector('[role="button"][aria-label="View all"]') !== null).toBe(count > 10);
});

it.each(["idle", "archived"] as const)("runs only the hovered text of an %s focus session, leaving its icons and menu still", (status) => {
  stubMarquee();
  const session = sessionOf("hover-session", main.id, web.id, "A long session title", status, { account_id: "work" });
  const { container } = mount((h) => (
    <ProjectFocusView handlers={h} console={main} project={web} sessions={[session]} owners={owners} />
  ));
  const row = Array.from(container.querySelectorAll<HTMLElement>('div[role="button"]'))
    .find((element) => element.getAttribute("aria-label")?.startsWith(isolate(session.title)))!;
  const labels = Array.from(row.querySelectorAll<HTMLElement>(".overflow-hidden.whitespace-nowrap"));
  expect(labels).toHaveLength(status === "idle" ? 2 : 1);
  expectIndependentMarquees(labels, [row, ...row.querySelectorAll("button, svg")]);
});

it("updates recorded usage on active cards and keeps archived previews compact without dates", () => {
  let update!: () => void;
  const zeroUsage = { input_tokens: 0, output_tokens: 0, total_tokens: 0 };
  const active = sessionOf("live-usage", main.id, web.id, "Live usage", "working", { usage: zeroUsage });
  const archived = sessionOf("old-usage", main.id, web.id, "Old usage", "archived", { ended_at: 1000, usage: zeroUsage });
  function View({ handlers }: { handlers: SidebarHandlers }) {
    const [session, setSession] = useState(active);
    update = () => setSession({ ...active, usage: { input_tokens: 1000, output_tokens: 200, total_tokens: 1200 } });
    return <ProjectFocusView handlers={handlers} console={main} project={web} sessions={[session, archived]} owners={owners} />;
  }
  const { container } = mount((handlers) => <View handlers={handlers} />);
  const row = (title: string) => Array.from(container.querySelectorAll<HTMLElement>('div[role="button"]')).find((element) => element.getAttribute("aria-label")?.startsWith(isolate(title)))!;
  expect(row(active.title).getAttribute("aria-describedby")).toBeNull();
  expect(row(archived.title).querySelector("time")).toBeNull();
  expect(row(archived.title).getAttribute("aria-describedby")).toBeNull();
  act(update);
  const description = document.getElementById(row(active.title).getAttribute("aria-describedby")!);
  expect(description?.querySelector('[role="img"]')?.getAttribute("aria-label")).toContain(`Tokens: ${isolate("1,200")}`);
});
