// @vitest-environment jsdom
// Where focus goes as the archive view, which covers the agent terminal's area and leaves the terminal mounted
// beneath it, opens, closes and is left for a session or for Back: the terminal under the view is inert while it is
// open, so focus meant for the terminal lands on the view until the view has gone.
import { act } from "react";
import { expect, it, vi } from "vitest";

import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";

import { byLabel, deliver, fakeDaemon, mountApp, pressElement, pressMenuItem, railConsole, regionOfFocus, row, stubWindow, topBar } from "./appTestSupport";
import { withModality } from "./components/dialog/dialogTestSupport";
import { press, tap } from "./components/hint/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";
import { isolate } from "./i18n/isolate";
import type { Console, Session } from "./protocol";
import type { PlatformAdapter } from "./platform";
import { MAC_HOST } from "./test/hosts";
import { matchesWidthQuery } from "./test/viewport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => null }));
vi.mock("./project-browser/LazyProjectBrowser", () => ({ LazyProjectBrowser: () => null }));
// The agent terminal's input stands in for xterm's.
vi.mock("./terminal/TerminalController", () => import("./terminalControllerTestSupport"));

const console_ = consoleOf("c-main", "Main");
const project = projectOf("p-site", console_.id, "Website");
const live = sessionOf("s-live", console_.id, project.id, "Fix header", "idle");
const archived = sessionOf("s-old", console_.id, project.id, "Old fix", "archived");
const other = consoleOf("c-other", "Other");

const agentInput = () => document.querySelector<HTMLElement>("[data-region=terminal] textarea[aria-label='Agent terminal']");
/** The archived session's history, whose own section holds focus until its messages have loaded. */
const history = () => document.querySelector<HTMLElement>("[data-region=terminal] section");
const archiveSection = () => document.querySelector<HTMLElement>("[data-region=archive]");
/** The terminal pane's root, which is inert as a whole while the archive view covers it. */
const terminalInert = () => document.querySelector("[data-region=terminal][inert]");
const anyRing = () => document.querySelector("[data-focus-visible]") !== null;

function mount({ window, sessions = [archived, live], consoles = [console_], platform }: { window?: () => void; sessions?: Session[]; consoles?: Console[]; platform?: PlatformAdapter } = {}) {
  stubWindow();
  window?.();
  const daemon = fakeDaemon(snapshotState({ consoles, projects: [project], sessions }), (body) =>
    body.type === "read_session_history" ? { type: "session_history", id: null, messages: [{ speaker: "user", text: "Hello", turn: 1 }] } : { type: "ack", id: null });
  mountApp(daemon, platform ? { platform } : undefined);
  return daemon;
}

/** Opens the project's archive view from the sidebar's menu. */
async function openArchive() {
  await pressElement(byLabel(`Actions for project ${isolate(project.name)}`));
  await pressMenuItem("View archive");
  await pressMenuItem("View all");
  expect(archiveSection()?.parentElement?.getAttribute("data-state")).toBe("open");
}

const archiveRow = () => Array.from(document.querySelectorAll<HTMLElement>("[data-region=archive] [aria-label]")).find((element) => element.getAttribute("aria-label")!.includes(archived.title))!;
const closeButton = () => byLabel("Close", archiveSection()!)!;

/** Presses `element` with the input that sets the modality, which `element.click()` does not (react-aria reads it as virtual). */
function pressBy(modality: "pointer" | "keyboard", element: Element) {
  if (modality === "pointer") press(element, "mouse");
  else tap(element, "Enter");
}

it("keeps the terminal inert while the archive view is open, and reachable again once it closes", async () => {
  mount();
  await pressElement(row(live.title));
  expect(terminalInert()).toBeNull();
  await openArchive();
  expect(terminalInert()?.contains(agentInput())).toBe(true);
  press(closeButton(), "mouse");
  await act(async () => {});
  expect(archiveSection()).toBeNull();
  expect(terminalInert()).toBeNull();
});

it("makes the terminal reachable as soon as the view starts to leave, while its exit animation still runs", async () => {
  // With motion allowed the layer keeps the closing view on screen until its animation ends.
  mount({
    window: () => {
      vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: media !== "(prefers-reduced-motion: reduce)" && matchesWidthQuery(media, 1400), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
    },
  });
  await pressElement(row(live.title));
  await openArchive();
  press(closeButton(), "mouse");
  await act(async () => {});
  expect(archiveSection()?.parentElement?.getAttribute("data-state")).toBe("closing");
  expect(terminalInert()).toBeNull();
  expect(document.activeElement).toBe(agentInput());

  const ended = new Event("animationend", { bubbles: true });
  Object.defineProperty(ended, "animationName", { value: "archive-exit" });
  await act(async () => void archiveSection()!.dispatchEvent(ended));
  expect(archiveSection()).toBeNull();
});

it.each([
  ["pointer", false],
  ["keyboard", true],
] as const)("reaches the terminal when the view is closed by %s, ringed: %s", async (modality, ringed) => {
  mount();
  withModality(modality);
  await pressElement(row(live.title));
  await openArchive();
  act(() => archiveSection()!.focus());
  withModality(modality);

  pressBy(modality, closeButton());
  await act(async () => {});
  expect(archiveSection()).toBeNull();
  expect(document.activeElement).toBe(agentInput());
  expect(anyRing()).toBe(ringed);
  expect(getInteractionModality()).toBe(modality);
});

it.each([
  ["pointer", false],
  ["keyboard", true],
] as const)("reaches the selected session's history when it is chosen in the open view by %s, ringed: %s", async (modality, ringed) => {
  mount();
  withModality(modality);
  await pressElement(row(live.title));
  await openArchive();
  act(() => archiveSection()!.focus());
  withModality(modality);

  pressBy(modality, archiveRow());
  await act(async () => {});
  expect(archiveSection()).toBeNull();
  expect(regionOfFocus()).toBe("terminal");
  expect(history()?.contains(document.activeElement)).toBe(true);
  expect(anyRing()).toBe(ringed);
  expect(getInteractionModality()).toBe(modality);
});

it("reaches the live session's terminal when its sidebar row is pressed with the view open, by pointer, with no ring", async () => {
  mount();
  withModality("pointer");
  await pressElement(row(live.title));
  await openArchive();
  act(() => archiveSection()!.focus());
  withModality("pointer");

  // The session's own row sits in the sidebar, outside the view: pressing it closes the view.
  press(row(live.title)!, "mouse");
  await act(async () => {});
  expect(archiveSection()).toBeNull();
  expect(document.activeElement).toBe(agentInput());
  expect(anyRing()).toBe(false);
  expect(getInteractionModality()).toBe("pointer");
});

it("goes from a focused history straight to the archive view that Back returns to, with no stop on the top bar and no ring after a pointer press", async () => {
  mount();
  withModality("pointer");
  // No session is selected, and the archive view is open: the location Back returns to.
  await openArchive();
  press(archiveRow(), "mouse");
  await act(async () => {});
  expect(archiveSection()).toBeNull();
  expect(history()).not.toBeNull();
  act(() => history()!.focus());
  withModality("pointer");

  press(topBar("Back")!, "mouse");
  // Straight after the press, not after the view's own focus delay: that effect then finds nothing left to do.
  expect(archiveSection()?.parentElement?.getAttribute("data-state")).toBe("open");
  expect(document.activeElement).toBe(archiveSection());
  expect(anyRing()).toBe(false);
  expect(getInteractionModality()).toBe("pointer");
});

/** The desktop app on macOS, where Back and Forward are ⌘[ and ⌘]. */
const macApp: PlatformAdapter = { kind: "tauri", host: MAC_HOST, windowChrome: { leftInset: () => 0, subscribe: () => () => {} } } as never;
const shortcut = (key: "[" | "]") => act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { key, code: key === "[" ? "BracketLeft" : "BracketRight", metaKey: true, bubbles: true, cancelable: true })));

/** An archived session owned by an archived console session, so its history links to the owner. */
const owner = sessionOf("s-owner", console_.id, undefined, "Release captain", "archived");
const owned = sessionOf("s-owned", console_.id, project.id, "Owned fix", "archived", { bound_to: owner.id });

it("makes an archived session's history, its owner link and its Reopen unreachable while the archive view covers it", async () => {
  mount({ sessions: [owner, owned, live] });
  await pressElement(byLabel(`Actions for project ${isolate(project.name)}`));
  await pressMenuItem("View archive");
  await pressMenuItem("Owned fix");
  const link = document.querySelector<HTMLElement>("[data-region=terminal] header button");
  expect(link?.textContent).toContain("Release captain");
  expect(link?.closest("[inert]")).toBeNull();

  await openArchive();
  expect(terminalInert()).not.toBeNull();
  const controls = Array.from(document.querySelectorAll("[data-region=terminal] button, [data-region=terminal] [tabindex]"));
  expect(controls.length).toBeGreaterThan(1);
  expect(controls.every((control) => control.closest("[inert]") !== null)).toBe(true);
  expect(controls.some((control) => control.textContent === "Reopen")).toBe(true);
});

it("hands focus held in the live terminal straight to the archive view that opens over it, not to <body>", async () => {
  mount({ platform: macApp });
  await pressElement(row(live.title));
  await openArchive();
  await pressElement(row(live.title));
  expect(archiveSection()).toBeNull();
  act(() => agentInput()!.focus());

  // Back to the location with the view open: the terminal under it goes inert with focus in it.
  shortcut("[");
  expect(archiveSection()?.parentElement?.getAttribute("data-state")).toBe("open");
  expect(document.activeElement).toBe(archiveSection());
});

it("hands focus held in an archived session's history straight to the archive view that opens over it, not to <body>", async () => {
  mount({ platform: macApp });
  await openArchive();
  press(archiveRow(), "mouse");
  await act(async () => {});
  act(() => history()!.focus());
  shortcut("[");
  expect(archiveSection()?.parentElement?.getAttribute("data-state")).toBe("open");
  // Forward to the history again: the view it leaves held focus, which goes on to the history it uncovers.
  shortcut("]");
  expect(archiveSection()).toBeNull();
  expect(history()?.contains(document.activeElement)).toBe(true);
  shortcut("[");
  expect(document.activeElement).toBe(archiveSection());
});

it("hands focus on, with no ring after a pointer press, when Back leaves the view it was in for a location without it", async () => {
  mount();
  withModality("pointer");
  await pressElement(row(live.title));
  await openArchive();
  act(() => archiveSection()!.focus());
  withModality("pointer");

  press(topBar("Back")!, "mouse");
  await act(async () => {});
  expect(archiveSection()).toBeNull();
  expect(document.activeElement).not.toBe(document.body);
  expect(document.activeElement).toBe(agentInput());
  expect(anyRing()).toBe(false);
  expect(getInteractionModality()).toBe("pointer");
});

const liveOther = sessionOf("s-other", other.id, undefined, "Other work", "idle");

it("hands focus to the terminal of the session a rail press shows, when it switches console from the view, with no ring after a pointer press", async () => {
  mount({ consoles: [console_, other], sessions: [archived, live, liveOther] });
  withModality("pointer");
  await pressElement(railConsole(other.name));
  await pressElement(row(liveOther.title));
  await pressElement(railConsole(console_.name));
  await openArchive();
  act(() => archiveSection()!.focus());
  withModality("pointer");

  // The rail's press leaves focus in the view, which the other console's location has no part of.
  press(railConsole(other.name)!, "mouse");
  await act(async () => {});
  expect(railConsole(other.name)?.getAttribute("aria-current")).toBe("true");
  expect(archiveSection()).toBeNull();
  expect(agentInput()).not.toBeNull();
  expect(document.activeElement).toBe(agentInput());
  expect(anyRing()).toBe(false);
  expect(getInteractionModality()).toBe("pointer");
});

it.each([
  ["no session shown", false],
  ["a live session shown", true],
] as const)("hands focus to the empty state's action, never <body> or the closing view, when a rail press switches from the view over %s to a console with no session", async (_case, selected) => {
  mount({ consoles: [console_, other] });
  withModality("pointer");
  if (selected) await pressElement(row(live.title));
  await openArchive();
  act(() => archiveSection()!.focus());
  withModality("pointer");

  press(railConsole(other.name)!, "mouse");
  await act(async () => {});
  expect(railConsole(other.name)?.getAttribute("aria-current")).toBe("true");
  expect(archiveSection()).toBeNull();
  expect(document.activeElement).not.toBe(document.body);
  expect(regionOfFocus()).toBe("terminal");
  expect(document.activeElement?.tagName).toBe("BUTTON");
  expect(anyRing()).toBe(false);
  expect(getInteractionModality()).toBe("pointer");
});

it("forces the ring when the view closes by itself, as what it lists is gone, with focus in it", async () => {
  const daemon = mount();
  withModality("pointer");
  await pressElement(row(live.title));
  await openArchive();
  act(() => archiveSection()!.focus());
  withModality("pointer");

  await deliver(daemon, { type: "project_deleted", project: project.id });
  expect(archiveSection()).toBeNull();
  expect(document.activeElement).not.toBe(document.body);
  expect(getInteractionModality()).toBe("keyboard");
});

it.each([
  ["Close by pointer", "pointer"],
  ["Escape", "keyboard"],
] as const)("hands focus to the empty state's action when %s ends the view with no session selected, ringed only after a key", async (_how, modality) => {
  mount();
  withModality(modality);
  await openArchive();
  act(() => archiveSection()!.focus());
  withModality(modality);

  if (modality === "pointer") press(closeButton(), "mouse");
  else act(() => void archiveSection()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
  await act(async () => {});
  expect(archiveSection()).toBeNull();
  expect(document.activeElement).not.toBe(document.body);
  expect(regionOfFocus()).toBe("terminal");
  expect(document.activeElement?.tagName).toBe("BUTTON");
  expect(anyRing()).toBe(modality === "keyboard");
  expect(getInteractionModality()).toBe(modality);
});
