// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { PlatformAdapter } from "./platform";
import { PlatformProvider } from "./platform/react";
import { FIRST_WRITE_DELAY_MS, READ_TIME_MS } from "./components/announce/StatusAnnouncer";
import { availabilityOf } from "./fixtures/fixtureProviders";
import { type Agent, type AgentAvailability } from "./protocol";
import { TEST_PROVIDERS } from "./test/testProviders";
import { fakeTimers } from "./test/clock";
import { DaemonProvider, type Daemon, type State } from "./store";
import { testDaemon } from "./test/testDaemon";
import { WelcomeScreen } from "./WelcomeScreen";
import { BROWSER_HOST } from "./test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A platform that can save and open a file, which is what puts the import action on the screen. */
const WITH_CONFIGURATION_FILE: PlatformAdapter = { kind: "browser", host: BROWSER_HOST, configurationFile: { save: vi.fn(), open: vi.fn() } };

/** Mounts the screen on a fake clock (it holds a status region, so the clock is faked before it mounts) and leaves its
 * first-write delay unrun, so a test can tell what is said before and after it. */
function mountUnsettled(onOpenDialog = vi.fn(), state: Partial<State> = {}, platform: PlatformAdapter = { kind: "browser", host: BROWSER_HOST }): { container: HTMLElement; daemon: Daemon } {
  if (!vi.isFakeTimers()) fakeTimers();
  const daemon = testDaemon({ state, request: vi.fn(), showNotice: vi.fn() });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <PlatformProvider value={platform}>
        <DaemonProvider value={daemon}>
          <WelcomeScreen onOpenDialog={onOpenDialog} />
        </DaemonProvider>
      </PlatformProvider>,
    ),
  );
  return { container, daemon };
}

/** The screen mounted with its first-write delay run out, so its status region is ready to write. */
function mount(onOpenDialog = vi.fn(), state: Partial<State> = {}, platform: PlatformAdapter = { kind: "browser", host: BROWSER_HOST }): HTMLElement {
  const { container } = mountUnsettled(onOpenDialog, state, platform);
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  return container;
}

it("opens the new-console dialog from its action", () => {
  const onOpenDialog = vi.fn();
  const container = mount(onOpenDialog);
  act(() => container.querySelector("button")!.click());
  expect(onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind: "new-console" });
});

it("is a named region of the window", () => {
  const region = mount().querySelector("[data-region=welcome]");
  expect(region?.getAttribute("aria-label")).toBeTruthy();
});

type Availability = AgentAvailability["availability"];

/** The statuses the screen words each availability with, as its rows read them. */
const STATUS_TEXT: Record<Availability, string> = { available: "Installed", unavailable: "Not detected", not_determined: "Checking…" };

/** The daemon's availability for `TEST_PROVIDERS`, the `index`th provider's status being `statusAt(index)`. */
function availabilityBy(statusAt: (index: number) => Availability): Map<Agent, AgentAvailability> {
  return availabilityOf(TEST_PROVIDERS, Object.fromEntries(TEST_PROVIDERS.map(({ id }, index) => [id, statusAt(index)])));
}

/** The rows a list of the providers reads, in the order given, each with its status. */
const rowTexts = (statusAt: (index: number) => Availability, only: (status: Availability) => boolean = () => true) =>
  TEST_PROVIDERS.flatMap(({ label }, index) => (only(statusAt(index)) ? [`${label}, ${STATUS_TEXT[statusAt(index)]}`] : []));

const texts = (rows: Iterable<Element>) => Array.from(rows).map((row) => row.textContent);
/** The disclosure's panel: the folded tools' rows sit in it, hidden from the accessibility tree while it is collapsed. */
const foldedPanel = (container: HTMLElement) => container.querySelector("[role=group]");
const notDetectedTrigger = (container: HTMLElement) => container.querySelector<HTMLButtonElement>("button[aria-expanded]");
/** The tool rows, split into the main list's and the disclosure's. */
function listedRows(container: HTMLElement): { main: HTMLElement[]; folded: HTMLElement[] } {
  const panel = foldedPanel(container);
  const rows = Array.from(container.querySelectorAll<HTMLElement>("li"));
  return { main: rows.filter((row) => !panel?.contains(row)), folded: rows.filter((row) => panel?.contains(row)) };
}
/** The region that says how many tools are folded, which sits beside the list rather than in a row. */
const announcement = (container: HTMLElement) => container.querySelector("section > [role=status]")?.textContent;

it.each<[string, (index: number) => Availability]>([
  ["all still being checked", () => "not_determined"],
  ["all not detected", () => "unavailable"],
  ["some not detected and the rest still being checked", (index) => (index % 2 === 0 ? "unavailable" : "not_determined")],
])("lists every tool with its own status, with no fold and the action enabled, while none is detected: %s", (_name, statusAt) => {
  const container = mount(vi.fn(), { agentAvailability: availabilityBy(statusAt) });
  expect(texts(container.querySelectorAll("li"))).toEqual(rowTexts(statusAt));
  expect(notDetectedTrigger(container)).toBeNull();
  expect(foldedPanel(container)).toBeNull();
  expect(announcement(container)).toBe("");
  // Unlike the sidebar's, the button is not disabled: a console's agents are only defaults.
  expect(container.querySelector("button")!.disabled).toBe(false);
});

it.each<[string, (index: number) => Availability, string, number]>([
  ["one not detected", (index) => (index === 1 ? "unavailable" : "available"), "1 tool not detected", 1],
  ["two not detected and one still being checked", (index) => (index === 0 ? "available" : index < 3 ? "unavailable" : "not_determined"), "2 tools not detected", 2],
])("once a tool is detected, folds the tools not detected under a collapsed button and is silent about it: %s", (_name, statusAt, label, count) => {
  const container = mount(vi.fn(), { agentAvailability: availabilityBy(statusAt) });
  const panel = foldedPanel(container)!;
  const { main, folded } = listedRows(container);
  const rows = [...main, ...folded];
  // The detected and still-checking tools stay, in registry order; the rest are folded, in the same order.
  expect(texts(main)).toEqual(rowTexts(statusAt, (status) => status !== "unavailable"));
  expect(texts(folded)).toEqual(rowTexts(statusAt, (status) => status === "unavailable"));
  expect(folded).toHaveLength(count);
  expect(rows.every((row) => row.querySelector("svg[aria-hidden=true]"))).toBe(true);
  // Rows in the main list are status regions; folded rows are not, as nothing about them changes while folded.
  expect(main.map((row) => row.querySelector("[role=status]")?.textContent)).toEqual(texts(main));
  expect(folded.every((row) => row.querySelector("[role=status]") === null)).toBe(true);
  // A fold already there when the screen mounts is on the screen, not news, so nothing is said.
  expect(announcement(container)).toBe("");

  const trigger = notDetectedTrigger(container)!;
  expect(trigger.textContent).toBe(label);
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  expect(panel.getAttribute("aria-hidden")).toBe("true");
  act(() => trigger.click());
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  expect(panel.getAttribute("aria-hidden")).toBe("false");
});

it("says the count of tools not detected only once it arrives after the screen is up, and stops after the read time", () => {
  const { container, daemon } = mountUnsettled(vi.fn(), { agentAvailability: availabilityBy(() => "not_determined") });
  /** Lands `statusAt` as the daemon's detection and expects the lists and the trigger to follow it. */
  const land = (statusAt: (index: number) => Availability, count: number) => {
    act(() => daemon.store.setState({ agentAvailability: availabilityBy(statusAt) }));
    const { main, folded } = listedRows(container);
    expect(texts(main)).toEqual(rowTexts(statusAt, (status) => status !== "unavailable"));
    expect(texts(folded)).toEqual(rowTexts(statusAt, (status) => status === "unavailable"));
    expect(folded).toHaveLength(count);
  };

  // The first tool is installed, the second is not detected, the rest are still being checked.
  land((index) => (index === 0 ? "available" : index === 1 ? "unavailable" : "not_determined"), 1);
  expect(announcement(container)).toBe("");
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  expect(announcement(container)).toBe("1 tool not detected");
  expect(notDetectedTrigger(container)!.textContent).toBe("1 tool not detected");
  act(() => void vi.advanceTimersByTime(READ_TIME_MS));
  expect(announcement(container)).toBe("");

  // Another tool turns out not to be there: it leaves the main list for the fold and the new count is said.
  land((index) => (index === 0 ? "available" : index < 3 ? "unavailable" : "not_determined"), 2);
  expect(announcement(container)).toBe("2 tools not detected");
  expect(notDetectedTrigger(container)!.textContent).toBe("2 tools not detected");

  // A tool in the fold is detected after all: it moves up to the main list in registry order and the count falls.
  land((index) => (index < 2 ? "available" : index === 2 ? "unavailable" : "not_determined"), 1);
  expect(announcement(container)).toBe("1 tool not detected");
});

it("says every count of tools not detected once it differs from the one at mount, including a return to it", () => {
  const oneFolded = (index: number): Availability => (index === 1 ? "unavailable" : "available");
  const twoFolded = (index: number): Availability => (index === 1 || index === 2 ? "unavailable" : "available");
  const { container, daemon } = mountUnsettled(vi.fn(), { agentAvailability: availabilityBy(oneFolded) });
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  // The fold the screen mounted with is on the screen, not news.
  expect(announcement(container)).toBe("");
  act(() => daemon.store.setState({ agentAvailability: availabilityBy(twoFolded) }));
  expect(announcement(container)).toBe("2 tools not detected");
  // Back to the count it mounted with: that is a change too, so it is said.
  act(() => daemon.store.setState({ agentAvailability: availabilityBy(oneFolded) }));
  expect(announcement(container)).toBe("1 tool not detected");
});

it("offers importing a configuration only where the platform can open a file", () => {
  const onOpenDialog = vi.fn();
  const buttons = (container: HTMLElement) => Array.from(container.querySelectorAll("button"));
  expect(buttons(mount())).toHaveLength(1);
  const container = mount(onOpenDialog, {}, WITH_CONFIGURATION_FILE);
  expect(buttons(container)).toHaveLength(2);
  act(() => buttons(container)[1].click());
  expect(onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind: "import-configuration" });
});
