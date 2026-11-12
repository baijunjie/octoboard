// @vitest-environment jsdom
// Where focus goes when a control that held it goes away from under it, in the three children of `App` whose check
// for that runs after each of their own renders (`useFocusHandoff`): the terminal pane (with its history's owner
// line), the update notice and the connection banner. Each case drives the removal by what removes the control in
// the app, so the handoff is shown to run, not only to land in the right place once it does.
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";

import { byLabel, deliver, fakeDaemon, mountApp, pressElement, pressMenuItem, railConsole, regionOfFocus, row, stubWindow } from "./appTestSupport";
import { withModality } from "./components/dialog/dialogTestSupport";
import { press, tap } from "./components/hint/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";
import { isolate } from "./i18n/isolate";
import type { UpdaterCapability } from "./platform";
import { UpdatesProvider } from "./updates/UpdatesProvider";
import { BROWSER_HOST } from "./test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => null }));
vi.mock("./project-browser/LazyProjectBrowser", () => ({ LazyProjectBrowser: () => null }));
// The agent terminal's input stands in for xterm's.
vi.mock("./terminal/TerminalController", () => import("./terminalControllerTestSupport"));
// The shell's text input stands in for a shell terminal's view, inside the element that marks one.
vi.mock("./terminal/shell/ShellTerminalPanel", async () => {
  const { PanelHeader } = await import("./components/panel/PanelHeader");
  return {
    ShellTerminalPanel: () => (
      <>
        <PanelHeader title="Terminal" />
        <div data-shell-terminal>
          <textarea aria-label="shell input" data-region-landing />
        </div>
      </>
    ),
  };
});
vi.mock("./terminal/shell/shellTerminals", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./terminal/shell/shellTerminals")>();
  return { ...actual, startShellTerminal: vi.fn(async () => undefined) };
});

const console_ = consoleOf("c-main", "Main");
const project = projectOf("p-site", console_.id, "Website");
const live = sessionOf("s-live", console_.id, project.id, "Fix header", "idle");
const other = consoleOf("c-other", "Other");
const agentInput = () => document.querySelector("[data-region=terminal] textarea[aria-label='Agent terminal']");

it("hands focus on an archived session's owner link to its history when the owner's record is deleted", async () => {
  stubWindow();
  const owner = sessionOf("s-owner", console_.id, undefined, "Release captain", "archived");
  const archived = sessionOf("s-old", console_.id, project.id, "Old fix", "archived", { bound_to: owner.id });
  const daemon = fakeDaemon(snapshotState({ consoles: [console_], projects: [project], sessions: [owner, archived, live] }), (body) =>
    body.type === "read_session_history" ? { type: "session_history", id: null, messages: [{ speaker: "user", text: "Hello", turn: 1 }] } : { type: "ack", id: null });
  mountApp(daemon);
  await pressElement(byLabel(`Actions for project ${isolate(project.name)}`));
  await pressMenuItem("View archive");
  await pressMenuItem("Old fix");
  const link = document.querySelector<HTMLElement>("[data-region=terminal] header button");
  expect(link?.textContent).toContain("Release captain");
  act(() => link!.focus());

  // Archive clean-up deleting the owner's record takes the link away while it holds focus.
  await deliver(daemon, { type: "session_deleted", session: owner.id });
  expect(document.querySelector("[data-region=terminal] header button")).toBeNull();
  expect(document.activeElement).toBe(document.querySelector("[data-region=terminal] [role=region][tabindex]"));
});

it("hands focus on an archived session's owner link to its history when a project-session owner's record is deleted", async () => {
  stubWindow();
  // A project-session owner: no console session sits above the selection, so nothing above the history renders on
  // the deletion. Interrupted rather than archived, so the archive's preview lists the session under it.
  const owner = sessionOf("s-coordinator", console_.id, project.id, "Release coordinator", "interrupted");
  const archived = sessionOf("s-old", console_.id, project.id, "Old fix", "archived", { bound_to: owner.id });
  const daemon = fakeDaemon(snapshotState({ consoles: [console_], projects: [project], sessions: [owner, archived, live] }), (body) =>
    body.type === "read_session_history" ? { type: "session_history", id: null, messages: [{ speaker: "user", text: "Hello", turn: 1 }] } : { type: "ack", id: null });
  mountApp(daemon);
  await pressElement(byLabel(`Actions for project ${isolate(project.name)}`));
  await pressMenuItem("View archive");
  await pressMenuItem("Old fix");
  const link = document.querySelector<HTMLElement>("[data-region=terminal] header button");
  expect(link?.textContent).toContain("Release coordinator");
  act(() => link!.focus());

  await deliver(daemon, { type: "session_deleted", session: owner.id });
  expect(document.querySelector("[data-region=terminal] header button")).toBeNull();
  expect(document.activeElement).toBe(document.querySelector("[data-region=terminal] [role=region][tabindex]"));
});

it("hands focus on the terminal's creation action to the top bar when another window creates what it offered", async () => {
  stubWindow();
  // Two projects and no console session: the terminal offers New console session, and nothing once there is one.
  const other = projectOf("p-api", console_.id, "API");
  const daemon = fakeDaemon(snapshotState({ consoles: [console_], projects: [project, other] }));
  mountApp(daemon);
  const action = Array.from(document.querySelectorAll<HTMLElement>("[data-region=terminal] button")).find((button) => button.textContent === "New console session");
  expect(action).toBeDefined();
  act(() => action!.focus());

  await deliver(daemon, { type: "session_upserted", session: sessionOf("s-new", console_.id, undefined, "Plan release", "idle") });
  expect(document.querySelector("[data-region=terminal] button")).toBeNull();
  expect(regionOfFocus()).toBe("topbar");
});

it("hands focus on the update notice's Later to the agent terminal as the notice goes", async () => {
  stubWindow();
  const updater: UpdaterCapability = { currentVersion: async () => "0.1.0", check: async () => ({ version: "0.2.0" }), install: async () => {} };
  const daemon = fakeDaemon(snapshotState({ consoles: [console_], projects: [project], sessions: [live] }));
  mountApp(daemon, { platform: { kind: "browser", host: BROWSER_HOST, updater }, wrap: (app) => <UpdatesProvider firstCheckDelayMs={0}>{app}</UpdatesProvider> });
  await pressElement(row(live.title));
  await vi.waitFor(() => expect(document.querySelector("[data-region=update]")).not.toBeNull(), { timeout: 2000 });
  const later = byLabel("Later", document.querySelector("[data-region=update]")!);
  act(() => later!.focus());

  await pressElement(later);
  expect(document.querySelector("[data-region=update]")).toBeNull();
  expect(document.activeElement).toBe(agentInput());
});

it("hands focus on the connection banner's Try again to the agent terminal once the client is retrying", async () => {
  stubWindow();
  const daemon = fakeDaemon(snapshotState({ consoles: [console_], projects: [project], sessions: [live] }));
  mountApp(daemon);
  await pressElement(row(live.title));
  await act(async () => daemon.store.setState({ connectionState: "closed" }));
  const retry = Array.from(document.querySelectorAll<HTMLElement>("[data-region=banner] button")).find((button) => button.textContent === "Try again");
  expect(retry).toBeDefined();
  act(() => retry!.focus());

  // A mouse press, not the tap an earlier test may have left, which keeps focus off the terminal.
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", bubbles: true }));
  await pressElement(retry);
  expect(daemon.reconnect).toHaveBeenCalledOnce();
  // The client reports its new attempt, and the banner drops Try again for the reconnecting line.
  await act(async () => daemon.store.setState({ connectionState: "reconnecting" }));
  expect(document.querySelector("[data-region=banner] button")).toBeNull();
  expect(document.activeElement).toBe(agentInput());
});

/** The app with an archived session's history on screen and focus in it, under `modality`. */
async function mountFocusedHistory(modality: "keyboard" | "pointer") {
  stubWindow();
  withModality(modality);
  const archived = sessionOf("s-old", console_.id, project.id, "Old fix", "archived");
  const daemon = fakeDaemon(snapshotState({ consoles: [console_, other], projects: [project], sessions: [archived, live] }), (body) =>
    body.type === "read_session_history" ? { type: "session_history", id: null, messages: [{ speaker: "user", text: "Hello", turn: 1 }] } : { type: "ack", id: null });
  mountApp(daemon);
  await pressElement(byLabel(`Actions for project ${isolate(project.name)}`));
  await pressMenuItem("View archive");
  await pressMenuItem("Old fix");
  const region = document.querySelector<HTMLElement>("[data-region=terminal] [role=region][tabindex]");
  expect(region).not.toBeNull();
  act(() => region!.focus());
  expect(document.activeElement).toBe(region);
  return { daemon, archived };
}

it.each([
  ["pointer", false],
  ["keyboard", true],
] as const)("hands focus on from an archived session's history, without forcing the ring, when %s selects a console with no session to show, ringed only after a key", async (modality, ringed) => {
  await mountFocusedHistory(modality);

  // The rail's press leaves focus where it is, and the console remembers no session, so none is selected. The press
  // is made by the input that sets the modality, which `element.click()` does not (react-aria reads it as virtual).
  const avatar = railConsole(other.name)!;
  if (modality === "pointer") press(avatar, "mouse");
  else tap(avatar, "Enter");
  await act(async () => {});
  expect(railConsole(other.name)?.getAttribute("aria-current")).toBe("true");
  expect(document.querySelector("[data-region=terminal] [role=region][tabindex]")).toBeNull();
  // The empty state's creation action, where focus meant for the terminal goes with no session on screen.
  expect(regionOfFocus()).toBe("terminal");
  expect(document.activeElement?.textContent).toBe("New console session");
  expect(document.activeElement).not.toBe(document.body);
  expect(document.activeElement?.hasAttribute("data-focus-visible")).toBe(ringed);
  expect(getInteractionModality()).toBe(modality);
});

/** The last input before the daemon takes the focused thing away: a mouse press, or a hardware keyboard's key. */
const lastInput = (input: "mouse" | "key") => (input === "mouse" ? press(document.body, "mouse") : void act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }))));

it.each([
  ["mouse", false, "pointer"],
  ["key", true, "keyboard"],
] as const)("hands focus on from the archived session on screen to the empty state's action when its record is deleted, ringed only when the last input was a %s", async (input, ringed, modality) => {
  const { daemon, archived } = await mountFocusedHistory("pointer");
  lastInput(input);

  await deliver(daemon, { type: "session_deleted", session: archived.id });
  expect(document.querySelector("[data-region=terminal] [role=region][tabindex]")).toBeNull();
  // The same place as from a live terminal: the empty state's action, which is offered here.
  expect(regionOfFocus()).toBe("terminal");
  expect(document.activeElement?.textContent).toBe("New console session");
  expect(document.activeElement?.hasAttribute("data-focus-visible")).toBe(ringed);
  expect(getInteractionModality()).toBe(modality);
});

it.each([
  ["pointer", false],
  ["keyboard", true],
] as const)("hands focus on from a live session's terminal, without forcing the ring, when %s selects a console with no session to show, ringed only after a key", async (modality, ringed) => {
  stubWindow();
  const daemon = fakeDaemon(snapshotState({ consoles: [console_, other], projects: [project], sessions: [live] }));
  mountApp(daemon);
  await pressElement(row(live.title));
  const input = agentInput() as HTMLElement | null;
  expect(input).not.toBeNull();
  act(() => input!.focus());
  expect(document.activeElement).toBe(input);

  // The press is made by the input that sets the modality, as in the history case above.
  const avatar = railConsole(other.name)!;
  if (modality === "pointer") press(avatar, "mouse");
  else tap(avatar, "Enter");
  await act(async () => {});
  expect(railConsole(other.name)?.getAttribute("aria-current")).toBe("true");
  expect(regionOfFocus()).toBe("terminal");
  expect(document.activeElement?.textContent).toBe("New console session");
  expect(document.activeElement?.hasAttribute("data-focus-visible")).toBe(ringed);
  expect(getInteractionModality()).toBe(modality);
});

it.each([
  ["mouse", false, "pointer"],
  ["key", true, "keyboard"],
] as const)("hands focus on from a live session's terminal to the empty state's action when its record is deleted, ringed only when the last input was a %s", async (input, ringed, modality) => {
  stubWindow();
  const daemon = fakeDaemon(snapshotState({ consoles: [console_], projects: [project], sessions: [live] }));
  mountApp(daemon);
  await pressElement(row(live.title));
  act(() => (agentInput() as HTMLElement).focus());
  lastInput(input);

  await deliver(daemon, { type: "session_deleted", session: live.id });
  expect(agentInput()?.closest("[inert]")).not.toBeNull();
  expect(regionOfFocus()).toBe("terminal");
  expect(document.activeElement?.textContent).toBe("New console session");
  expect(document.activeElement?.hasAttribute("data-focus-visible")).toBe(ringed);
  expect(getInteractionModality()).toBe(modality);
});

it("hands focus on from a live session's terminal to the top bar, with no ring, when its record is deleted after a tap", async () => {
  stubWindow();
  const daemon = fakeDaemon(snapshotState({ consoles: [console_], projects: [project], sessions: [live] }));
  mountApp(daemon);
  await pressElement(row(live.title));
  act(() => (agentInput() as HTMLElement).focus());
  withModality("pointer");
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "touch" }));
  onTestFinished(() => void document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse" })));

  await deliver(daemon, { type: "session_deleted", session: live.id });
  expect(regionOfFocus()).toBe("topbar");
  expect(document.activeElement?.hasAttribute("data-focus-visible")).toBe(false);
  expect(getInteractionModality()).toBe("pointer");
});

it("hands focus on from a live session's terminal to the top bar, not the empty state's action, when a tap selects a console with no session to show", async () => {
  stubWindow();
  withModality("pointer");
  const daemon = fakeDaemon(snapshotState({ consoles: [console_, other], projects: [project], sessions: [live] }));
  mountApp(daemon);
  await pressElement(row(live.title));
  act(() => (agentInput() as HTMLElement).focus());

  press(railConsole(other.name)!, "touch");
  await act(async () => {});
  expect(railConsole(other.name)?.getAttribute("aria-current")).toBe("true");
  expect(regionOfFocus()).toBe("topbar");
  expect(document.activeElement?.hasAttribute("data-focus-visible")).toBe(false);
  expect(getInteractionModality()).toBe("pointer");
});

/** The docked Terminal panel opened over a selected session, with focus in the shell input after a press on it. */
async function mountFocusedShell() {
  stubWindow();
  const daemon = fakeDaemon(snapshotState({ consoles: [console_], projects: [project], sessions: [live] }));
  mountApp(daemon);
  await pressElement(row(live.title));
  await pressElement(byLabel("Show terminal panel", document.querySelector("[data-region=topbar]")!));
  const shell = document.querySelector<HTMLElement>("#panel-terminal [aria-label='shell input']");
  expect(shell).not.toBeNull();
  return shell!;
}

/** Whether the agent terminal's surface, the element that draws its ring, shows keyboard focus. */
const agentRinged = () => agentInput()?.closest("[data-focus-visible]") != null;

it.each([
  ["the top bar's Terminal toggle", () => byLabel("Hide terminal panel", document.querySelector("[data-region=topbar]")!)],
  ["the card header's Close button", () => byLabel("Close panel", document.getElementById("panel-terminal")!)],
] as const)("hands focus from the docked shell terminal to the agent terminal with no ring when %s closes the panel with a mouse press", async (_, closer) => {
  const shell = await mountFocusedShell();
  press(shell, "mouse");
  act(() => shell.focus());
  expect(document.activeElement).toBe(shell);

  press(closer()!, "mouse");
  await act(async () => {});
  expect(document.getElementById("panel-terminal")).toBeNull();
  expect(document.activeElement).toBe(agentInput());
  expect(agentRinged()).toBe(false);
});

it.each([
  // Focus never leaves the shell: a navigation key pressed in it (Enter typed into a text field is not a hardware
  // keyboard's mark), then the toggle activated without a pointer (a script or a screen reader), which leaves the
  // last input as that key.
  ["the top bar's Terminal toggle, activated without a pointer after a key", (shell: HTMLElement) => {
    act(() => void shell.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })));
    act(() => byLabel("Hide terminal panel", document.querySelector("[data-region=topbar]")!)!.click());
  }],
  ["the card header's Close button, pressed by a key", () => {
    const close = byLabel("Close panel", document.getElementById("panel-terminal")!)!;
    act(() => close.focus());
    tap(close, "Enter");
  }],
] as const)("hands focus from the docked shell terminal to the agent terminal with the ring when %s closes the panel", async (_, closeBy) => {
  const shell = await mountFocusedShell();
  press(shell, "mouse");
  act(() => shell.focus());
  expect(document.activeElement).toBe(shell);

  closeBy(shell);
  await act(async () => {});
  expect(document.getElementById("panel-terminal")).toBeNull();
  expect(document.activeElement).toBe(agentInput());
  expect(agentRinged()).toBe(true);
});
