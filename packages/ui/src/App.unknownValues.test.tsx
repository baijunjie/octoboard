// @vitest-environment jsdom
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { App } from "./App";
import { AppRoot } from "./AppRoot";
import { PanelHeader } from "./components/panel/PanelHeader";
import { mountForTooltip } from "./components/hint/tooltipTestSupport";
import { stubWindow } from "./appTestSupport";
import { openSessionModelFromAnywhere } from "./dialogs/sessionModelLauncher";
import { agentCliStatusesOf, agentCliStatusOf, consoleOf, MACOS_MACHINE, projectOf, sessionOf, snapshotState } from "./fixtures/builders";
import type { AccountQuota, ClientRequestBody, Project } from "./protocol";
import { DEFAULT_SETTINGS, type Daemon } from "./store";
import { BROWSER_HOST } from "./test/hosts";
import { defaultDaemonReply, testDaemon } from "./test/testDaemon";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

vi.mock("./project-browser/LazyProjectBrowser", () => ({
  LazyProjectBrowser: ({ project }: { project: Project }) => <PanelHeader title={project.name} />,
}));
vi.mock("./terminal/shell/ShellTerminalPanel", () => ({ ShellTerminalPanel: () => <PanelHeader title="Terminal" /> }));
vi.mock("./terminal/TerminalController", () => ({
  TerminalController: class {
    constructor() {
      return new Proxy(this, { get: (target, key) => (key === "then" ? undefined : key in target ? Reflect.get(target, key) : () => {}) });
    }
  },
}));

const UNKNOWN = "from_a_newer_daemon";

/** The window, under the error boundary the real window has, over a snapshot holding in each enum it shows an effect of
 * either a value this build does not know or, as the contrast (`known`), one it does: so that each assertion below is
 * about the value, and an unguarded reader of one would take the whole window down. A value whose effect the window
 * does not show (the host's path style, for one) is left to the unit tests of its reader. */
function mountWithValues({ known = false }: { known?: boolean } = {}) {
  stubWindow(1400);
  const console_ = consoleOf("c-newer", "Main");
  const project = projectOf("p-site", console_.id, "Website");
  const sessions = [
    sessionOf("s-odd", console_.id, project.id, "Odd work", known ? "idle" : UNKNOWN),
    sessionOf("s-hub", console_.id, undefined, "Odd hub", "idle", { colour: known ? "teal" : UNKNOWN }),
  ];
  // The window of a kind this build does not know is used up; the session window beside it has room.
  const quota: AccountQuota = {
    agent: "claude", account: null, fetched_at: Date.now(), plan: "max", email: null, unavailable: null,
    windows: [
      { kind: known ? "weekly" : UNKNOWN, label: "Future", used_percent: 100, resets_at: null, window_minutes: null },
      { kind: "session", label: null, used_percent: 10, resets_at: null, window_minutes: 300 },
    ],
  };
  const state = {
    ...snapshotState({
      consoles: [console_],
      projects: [project],
      sessions,
      accountQuotas: [quota],
      host: MACOS_MACHINE.host,
      settings: {
        ...DEFAULT_SETTINGS,
        default_clone_dir: "/Users/dev/Projects",
        // The tab menus, which carry the CLI's update mark, belong to the agents the user has added.
        added_agents: ["claude"],
        keep_awake: true,
        keep_awake_when: known ? "always" : UNKNOWN,
        git_workflow: { ...DEFAULT_SETTINGS.git_workflow, development: known ? "branch" : UNKNOWN, delivery: known ? "merge" : UNKNOWN },
      },
    }),
    agentCliStatuses: agentCliStatusesOf([agentCliStatusOf("claude", { install_method: known ? "native" : UNKNOWN, latest_version: "9.9.9" })]),
  };
  const daemon: Daemon = testDaemon({
    state,
    request: vi.fn(async (body: ClientRequestBody) => defaultDaemonReply(body)) as Daemon["request"],
    showNotice: vi.fn(),
    reconnect: vi.fn(),
  });
  mountForTooltip(
    <AppRoot platform={{ kind: "browser", host: BROWSER_HOST }} daemon={daemon} nativeSync={false}>
      <App />
    </AppRoot>,
  );
}

const crashed = () => document.body.textContent?.includes("An unexpected error occurred") ?? false;
const press = (element: Element | null | undefined) => {
  expect(element).toBeTruthy();
  return act(async () => (element as HTMLElement).click());
};
const sessionRow = (title: string) =>
  Array.from(document.querySelectorAll<HTMLElement>("[data-region=sidebar] [role=button]")).filter((element) => element.textContent?.includes(title)).at(-1);
const tab = (name: string) => Array.from(document.querySelectorAll<HTMLElement>('[role="tab"]')).find((element) => element.textContent === name);
const pickerNamed = (name: string) => document.querySelector<HTMLElement>(`button[aria-label="${name}"], [aria-label="${name}"] button`);
const openSettingsTab = async (name: string) => {
  await press(document.querySelector('[data-region=rail] [aria-label="Settings"]'));
  await press(tab(name));
  expect(crashed(), name).toBe(false);
  expect(tab(name)?.getAttribute("aria-selected"), name).toBe("true");
};

describe.each([
  { known: false, mode: "values this build does not know" },
  { known: true, mode: "values this build knows" },
])("the window over $mode", ({ known }) => {
  it("keeps the window up, and lists and selects every session", async () => {
    mountWithValues({ known });
    expect(crashed()).toBe(false);
    expect(document.querySelector("[data-region=sidebar]")).not.toBeNull();
    expect(sessionRow("Odd work")).toBeDefined();
    expect(sessionRow("Odd hub")).toBeDefined();
    // Selecting each of them shows its terminal and panels.
    await press(sessionRow("Odd work"));
    expect(crashed()).toBe(false);
    await press(sessionRow("Odd hub"));
    expect(crashed()).toBe(false);
  });

  it(`shows the session's status ${known ? "by its own name" : "as Status unknown"}`, () => {
    mountWithValues({ known });
    expect(sessionRow("Odd work")?.getAttribute("aria-label")?.includes("Status unknown")).toBe(!known);
    expect(sessionRow("Odd hub")?.getAttribute("aria-label")).not.toContain("Status unknown");
  });

  it(`${known ? "draws" : "draws no"} colour dot on the console session`, () => {
    mountWithValues({ known });
    expect(sessionRow("Odd hub")?.querySelector(".session-colour-dot") !== null).toBe(known);
  });

  it(`${known ? "marks" : "does not mark"} the account used up for a ${known ? "weekly" : "newer"} window at 100%, and shows only windows it knows`, async () => {
    mountWithValues({ known });
    await openSettingsTab("Agent accounts");
    const row = Array.from(document.querySelectorAll("tr")).find((candidate) => candidate.textContent?.includes("Default"));
    // Used up by the weekly window it knows; with room, by the session window alone, when the other is not one it
    // knows.
    expect(row?.textContent?.includes("Limit reached")).toBe(known);
    expect(row?.textContent?.includes("Session 90% remaining")).toBe(!known);
    expect(row?.textContent).not.toContain("Future");
  });

  it(`${known ? "offers" : "offers no"} update of a CLI installed by a method ${known ? "it knows" : "it does not know"}, on the agent's actions and in its Version dialog`, async () => {
    mountWithValues({ known });
    await openSettingsTab("Agent accounts");
    const actions = document.querySelector<HTMLElement>('[data-tab-actions] button[aria-label^="Actions for"]');
    expect(actions?.getAttribute("aria-label")?.includes("update available")).toBe(known);
    expect(actions?.parentElement?.querySelector('[aria-hidden="true"][inert]') !== null).toBe(known);
    await press(actions);
    const version = Array.from(document.querySelectorAll<HTMLElement>("[role=menuitem]")).find((item) => item.textContent?.startsWith("Version"));
    expect(version?.querySelector(".bg-accent-soft-foreground") !== null).toBe(known);
    await press(version);
    // The Version dialog stands over the Settings dialog: the one that says what is installed.
    const dialog = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).find((candidate) => candidate.textContent?.includes("is installed"));
    expect(dialog).toBeDefined();
    const update = Array.from(dialog!.querySelectorAll("button")).find((button) => button.textContent === "Update");
    expect(update !== undefined).toBe(known);
  });

  it(`shows the choice of when to prevent sleep ${known ? "as Always" : "as an empty choice"}`, async () => {
    mountWithValues({ known });
    await openSettingsTab("General");
    const picker = pickerNamed("When to prevent sleep");
    expect(picker).not.toBeNull();
    expect(picker?.textContent?.includes("Always")).toBe(known);
    expect(picker?.textContent).not.toContain(UNKNOWN);
  });

  it(`shows the Git workflow ${known ? "as chosen" : "as empty choices, rows kept"}`, async () => {
    mountWithValues({ known });
    await openSettingsTab("Git");
    const development = pickerNamed("Where sessions develop");
    const delivery = pickerNamed("Delivery");
    expect(development).not.toBeNull();
    expect(delivery).not.toBeNull();
    expect(development?.textContent?.includes("On a branch of its own")).toBe(known);
    expect(delivery?.textContent?.includes("Merge locally")).toBe(known);
    expect(document.body.textContent).not.toContain(UNKNOWN);
  });

  it(`${known ? "opens" : "does not open"} Change model for the session from outside the window`, async () => {
    mountWithValues({ known });
    await act(async () => openSessionModelFromAnywhere("s-odd"));
    expect(document.body.textContent?.includes("Change model")).toBe(known);
    expect(crashed()).toBe(false);
  });
});
