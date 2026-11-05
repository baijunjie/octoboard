// @vitest-environment jsdom
// Where focus goes when a control that held it goes away from under it, in the three children of `App` whose check
// for that runs after each of their own renders (`useFocusHandoff`): the terminal pane (with its history's owner
// line), the update notice and the connection banner. Each case drives the removal by what removes the control in
// the app, so the handoff is shown to run, not only to land in the right place once it does.
import { act } from "react";
import { expect, it, vi } from "vitest";

import { byLabel, deliver, fakeDaemon, mountApp, pressElement, pressMenuItem, regionOfFocus, row, stubWindow } from "./appTestSupport";
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

const console_ = consoleOf("c-main", "Main");
const project = projectOf("p-site", console_.id, "Website");
const live = sessionOf("s-live", console_.id, project.id, "Fix header", "idle");
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

  await pressElement(retry);
  expect(daemon.reconnect).toHaveBeenCalledOnce();
  // The client reports its new attempt, and the banner drops Try again for the reconnecting line.
  await act(async () => daemon.store.setState({ connectionState: "reconnecting" }));
  expect(document.querySelector("[data-region=banner] button")).toBeNull();
  expect(document.activeElement).toBe(agentInput());
});
