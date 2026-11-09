// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { framesAsTimers, settleFrames, trackMountRefocus } from "../../components/dialog/dialogTestSupport";
import { FIRST_WRITE_DELAY_MS } from "../../components/announce/StatusAnnouncer";
import { errorAlerts } from "../../components/alerts/errorAlerts";
import { isolate } from "../../i18n/isolate";
import { partSelector } from "../../primitives";
import { ErrorAlerts } from "../../shell/ErrorAlerts";
import { TestHost } from "../../test/TestHost";
import type { Agent, AgentAvailability, AgentCliStatus, AgentCliUpdate } from "../../protocol";
import { request, startDaemon } from "./agentAccountsSectionTestState";
import { buttonNamed, chooseItem, mount, mountAgentVersion, openTabMenu } from "./agentAccountsSectionTestSupport";
import { fakeTimers } from "../../test/clock";

vi.mock("../../components/icons/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

const statusOf = (patch: Partial<AgentCliStatus> = {}): AgentCliStatus => ({
  agent: "claude", installed_version: "2.1.319", install_method: "npm", latest_version: "2.1.320", latest_error: null, latest_detail: null, checked_at: 1, update: null, ...patch,
});
const updateOf = (patch: Partial<AgentCliUpdate>): AgentCliUpdate => ({ state: "running", started_at: 10, finished_at: null, error: null, detail: null, ...patch });

/** Mounts the section with the Claude Code version dialog open and `status` held, and puts later statuses to the store
 * as events do. */
async function mountWith(status?: AgentCliStatus) {
  const mounted = await mountAgentVersion("Claude Code");
  const put = (next: AgentCliStatus) => mounted.set({ agentCliStatuses: new Map<Agent, AgentCliStatus>([...mounted.store.getState().agentCliStatuses, [next.agent, next]]) });
  if (status) put(status);
  const row = () => mounted.container.querySelector("[aria-label='About updating']")?.closest<HTMLElement>(".groove-b") ?? undefined;
  const text = () => row()?.textContent ?? "";
  const update = () => buttonNamed(mounted.container, "Update");
  return { ...mounted, put, text, update, row };
}

it("says it is still checking while no status has arrived", async () => {
  const { text, update } = await mountWith();
  expect(text()).toContain("Checking the version…");
  expect(update()).toBeUndefined();
});

it("offers Update, with the latest version, when a newer one is out", async () => {
  const { text, update } = await mountWith(statusOf());
  expect(text()).toContain(`Version ${isolate("2.1.319")} is installed. Version ${isolate("2.1.320")} is available.`);
  expect(text()).toContain("Installed with npm.");
  expect(update()).toBeDefined();
});

it("says the CLI is up to date, with no Update, when the latest version is not newer", async () => {
  const { text, update } = await mountWith(statusOf({ latest_version: "2.1.319" }));
  expect(text()).toContain(`Version ${isolate("2.1.319")} is installed and up to date.`);
  expect(update()).toBeUndefined();
});

it("says only the installed version, with a mark for the reason, when the latest version could not be looked up", async () => {
  const { text, row } = await mountWith(statusOf({ latest_version: null, latest_error: "agent_cli_latest_failed", latest_detail: "getaddrinfo ENOTFOUND" }));
  expect(text()).toContain(`Version ${isolate("2.1.319")} is installed.`);
  expect(row()!.querySelector('button[aria-label="The latest version could not be looked up."]')).not.toBeNull();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("gives an install the daemon cannot tell no Update and says on the row, not in the help tip, to update it the way it was installed", async () => {
  const { text, update } = await mountWith(statusOf({ install_method: "unknown", latest_version: null }));
  expect(text()).toContain(`Version ${isolate("2.1.319")} is installed.`);
  expect(update()).toBeUndefined();
  expect(text()).toContain("cannot tell how this was installed. Update it the way you installed it.");
  expect(text()).not.toContain("Installed with");
});

it("offers Update, as a secondary button, for an install whose method is known when the latest version is not", async () => {
  const { update } = await mountWith(statusOf({ latest_version: null, latest_error: "agent_cli_latest_failed" }));
  expect(update()).toBeDefined();
});

it("says the version is unknown when the daemon could not read it", async () => {
  expect((await mountWith(statusOf({ installed_version: null }))).text()).toContain("The installed version could not be read.");
});

it("asks for the update, and stays pending while the status says it runs, which a reopened section shows too", async () => {
  const { update, put, text, container } = await mountWith(statusOf());
  await act(async () => update()!.click());
  expect(request).toHaveBeenCalledWith({ type: "update_agent_cli", agent: "claude" });
  expect(update()!.getAttribute("aria-busy")).toBe("true");
  put(statusOf({ update: updateOf({}) }));
  expect(text()).toContain("Updating…");
  expect(container.querySelectorAll("[aria-busy='true']").length).toBeGreaterThan(0);
});

it("shows a running update that another window started", async () => {
  const { text } = await mountWith(statusOf({ update: updateOf({}) }));
  expect(text()).toContain("Updating…");
});

it("opens the error alert for an update it asked for that failed, and leaves the mark", async () => {
  const { update, put, row } = await mountWith(statusOf());
  await act(async () => update()!.click());
  put(statusOf({ update: updateOf({}) }));
  put(statusOf({ update: updateOf({ state: "failed", finished_at: 20, error: "agent_cli_update_failed", detail: "EACCES: permission denied" }) }));
  const alerts = errorAlerts.getSnapshot();
  expect(alerts).toHaveLength(1);
  expect(alerts[0]!.title).toBe("Could not update the agent");
  expect(row()!.querySelector(`button[aria-label="Updating ${isolate("Claude Code")} failed."]`)).not.toBeNull();
});

it.each([
  ["agent_cli_update_failed", "EACCES: permission denied", /Updating .*Claude Code.* failed\./, "Fix the problem shown in the details, or update it the way you installed it."],
  ["agent_cli_update_timed_out", null, /Updating .*Claude Code.* did not finish within ten minutes/, "Check the install, then try again."],
])("names the agent in the alert for an update that ended as %s, with its remedy line and the details apart", async (error, detail, sentence, remedy) => {
  framesAsTimers("pointer");
  const alertsRoot = createRoot(document.body.appendChild(document.createElement("div")));
  onTestFinished(() => act(() => alertsRoot.unmount()));
  const { update, put } = await mountWith(statusOf());
  act(() => alertsRoot.render(<TestHost daemon={startDaemon()}><ErrorAlerts /></TestHost>));
  await act(async () => update()!.click());
  put(statusOf({ update: updateOf({}) }));
  put(statusOf({ update: updateOf({ state: "failed", finished_at: 20, error, detail }) }));
  await settleFrames();
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  const body = document.body.querySelector<HTMLElement>('[role="alertdialog"]')!.textContent!.replace(/[\u2066-\u2069]/g, "");
  expect(body).toMatch(sentence);
  expect(body).toContain(remedy);
  expect(body).not.toContain("{agent}");
});

it("still raises the error alert for an update it asked for when the dialog was closed before the update ended", async () => {
  framesAsTimers("pointer");
  const { update, put, dialog, store } = await mountWith(statusOf());
  await act(async () => update()!.click());
  await act(async () => buttonNamed(dialog, "Close")!.click());
  await settleFrames();
  put(statusOf({ update: updateOf({}) }));
  put(statusOf({ update: updateOf({ state: "failed", finished_at: 20, error: "agent_cli_update_failed" }) }));
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(store.getState().cliUpdatesStartedHere.size).toBe(0);
});

it("does not take the earlier failure that the status held when it asked for the failure of the update it started", async () => {
  const old = updateOf({ state: "failed", started_at: 5, finished_at: 6, error: "agent_cli_update_timed_out" });
  const { update, put } = await mountWith(statusOf({ update: old }));
  await act(async () => update()!.click());
  expect(errorAlerts.getSnapshot()).toEqual([]);
  put(statusOf({ update: updateOf({ started_at: 10 }) }));
  put(statusOf({ update: updateOf({ started_at: 10, state: "failed", finished_at: 20, error: "agent_cli_update_failed" }) }));
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
});

it("draws one mark, naming both reasons, where the latest lookup and the last update both failed", async () => {
  const { row } = await mountWith(statusOf({
    latest_version: null, latest_error: "agent_cli_latest_failed",
    update: updateOf({ state: "failed", finished_at: 20, error: "agent_cli_update_failed", detail: "EACCES" }),
  }));
  const marks = row()!.querySelectorAll("[data-status-mark]");
  expect(marks).toHaveLength(1);
  expect(marks[0]!.getAttribute("aria-label")).toBe(`The latest version could not be looked up. Updating ${isolate("Claude Code")} failed.`);
});

it("draws the agent's name in the mark's popover as a chip, as the alert does, and keeps the remedy line out of it", async () => {
  const { row } = await mountWith(statusOf({ update: updateOf({ state: "failed", finished_at: 20, error: "agent_cli_update_failed", detail: "EACCES" }) }));
  await act(async () => row()!.querySelector<HTMLElement>("[data-status-mark]")!.click());
  const popover = document.body.querySelector<HTMLElement>("[data-status-mark-popover]")!;
  expect(popover.querySelector("[data-inline-reference]")?.textContent).toBe("Claude Code");
  expect(popover.textContent).not.toContain("Fix the problem");
});

it("hands focus held on Update to the help tip when the update succeeds and the button goes", async () => {
  const { update, put, row } = await mountWith(statusOf({ update: updateOf({}) }));
  act(() => row()!.querySelector<HTMLElement>("[data-cli-update]")!.focus());
  put(statusOf({ latest_version: "2.1.320", installed_version: "2.1.320", update: updateOf({ state: "succeeded", finished_at: 20 }) }));
  expect(update()).toBeUndefined();
  expect(document.activeElement).toBe(row()!.querySelector("[aria-label='About updating']"));
});

it("hands focus held on the mark to the help tip when the failure it shows is gone and the mark goes", async () => {
  const failed = updateOf({ state: "failed", finished_at: 20, error: "agent_cli_update_failed" });
  const { put, row } = await mountWith(statusOf({ update: failed }));
  act(() => row()!.querySelector<HTMLElement>("[data-status-mark]")!.focus());
  put(statusOf({ installed_version: "2.1.320", latest_version: "2.1.320", update: failed }));
  expect(row()!.querySelector("[data-status-mark]")).toBeNull();
  expect(document.activeElement).toBe(row()!.querySelector("[aria-label='About updating']"));
});

describe("what is spoken", () => {
  /** What the row's status region says once its first-write delay has passed. */
  const spoken = async (row: HTMLElement) => {
    await act(async () => vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
    return (row.querySelector("[role=status]")?.textContent ?? "").replace(/[\u2066-\u2069]/g, "");
  };

  it("is the version for a success, or only that it finished when the version cannot be read", async () => {
    fakeTimers();
    const first = await mountWith(statusOf({ update: updateOf({}) }));
    first.put(statusOf({ installed_version: "2.1.320", latest_version: "2.1.320", update: updateOf({ state: "succeeded", finished_at: 20 }) }));
    expect(await spoken(first.row()!)).toBe("Updated to version 2.1.320.");
    const second = await mountWith(statusOf({ update: updateOf({}) }));
    second.put(statusOf({ installed_version: null, update: updateOf({ state: "succeeded", finished_at: 20 }) }));
    expect(await spoken(second.row()!)).toBe("The update finished.");
  });

  it("is the reason for a failure seen from another window, but not for one asked for here, which the alert speaks", async () => {
    fakeTimers();
    const other = await mountWith(statusOf({ update: updateOf({}) }));
    other.put(statusOf({ update: updateOf({ state: "failed", finished_at: 20, error: "agent_cli_update_timed_out" }) }));
    expect(await spoken(other.row()!)).toContain("did not finish within ten minutes");
    const here = await mountWith(statusOf());
    await act(async () => here.update()!.click());
    here.put(statusOf({ update: updateOf({}) }));
    here.put(statusOf({ update: updateOf({ state: "failed", finished_at: 20, error: "agent_cli_update_failed" }) }));
    expect(await spoken(here.row()!)).toBe("");
  });
});

it("shows only the mark for a failed update it did not ask for", async () => {
  const { row } = await mountWith(statusOf({ update: updateOf({ state: "failed", finished_at: 20, error: "agent_cli_update_timed_out" }) }));
  expect(errorAlerts.getSnapshot()).toEqual([]);
  expect(row()!.querySelector("button[aria-label^='Updating']")).not.toBeNull();
});

it("reports a refusal of the request as an error alert and does not stay pending", async () => {
  const { update } = await mountWith(statusOf());
  request.mockRejectedValueOnce(Object.assign(new Error("refused"), { code: "agent_cli_update_running", params: { agent: "claude" } }));
  await act(async () => update()!.click());
  await act(async () => {});
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(update()!.getAttribute("aria-busy")).not.toBe("true");
});

it("asks again for the versions after a fresh snapshot", async () => {
  const { store } = await mountWith();
  request.mockClear();
  act(() => store.setState({ snapshotEpoch: store.getState().snapshotEpoch + 1 }));
  expect(request).toHaveBeenCalledWith({ type: "check_agent_clis" });
});

const unavailable = (agent: Agent) => new Map<Agent, AgentAvailability>([[agent, { agent, availability: "unavailable", default_account_dir: null } as AgentAvailability]]);

describe("the update mark on an agent's menus", () => {
  const tabOf = (container: HTMLElement, name: string) => [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((el) => el.textContent === name)!;
  const actionsOf = (container: HTMLElement, name: string) => container.querySelector<HTMLElement>(`[data-tab-actions] button[aria-label^="Actions for ${isolate(name)}"]`)!;
  const withStatus = (patch?: Partial<AgentCliStatus>) => {
    const mounted = mount();
    act(() => mounted.store.setState({ agentCliStatuses: new Map<Agent, AgentCliStatus>([["claude", statusOf(patch)]]) }));
    return mounted;
  };
  const versionEntry = () => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent?.startsWith("Version"))!;
  const described = (entry: HTMLElement) => document.getElementById(entry.getAttribute("aria-describedby")!.split(" ")[0]!)?.textContent;

  it("names the actions button for the update and puts a badge beside it, not inside it, and says it at the Version entry, while the tab has no mark", async () => {
    const { container } = withStatus();
    const actions = actionsOf(container, "Claude Code");
    expect(actions.getAttribute("aria-label")).toBe(`Actions for ${isolate("Claude Code")}, update available`);
    expect(actions.querySelector("[inert]")).toBeNull();
    expect(actions.parentElement!.querySelector('[aria-hidden="true"][inert]')).not.toBeNull();
    expect(actionsOf(container, "Codex").parentElement!.querySelector("[inert]")).toBeNull();
    expect(tabOf(container, "Claude Code").getAttribute("aria-label")).toBeNull();
    await openTabMenu(container, "Claude Code");
    expect(versionEntry().textContent).toContain("2.1.319");
    expect(described(versionEntry())).toBe(`${isolate("2.1.319")}, update available`);
  });

  it.each([
    ["is up to date", { latest_version: "2.1.319" }],
    ["has no known latest version", { latest_version: null }],
    ["cannot be updated by the daemon", { install_method: "unknown" as const }],
  ])("marks nothing for a CLI that %s", async (_, patch) => {
    const { container } = withStatus(patch);
    expect(actionsOf(container, "Claude Code").getAttribute("aria-label")).toBe(`Actions for ${isolate("Claude Code")}`);
    expect(actionsOf(container, "Claude Code").parentElement!.querySelector("[inert]")).toBeNull();
    await openTabMenu(container, "Claude Code");
    expect(versionEntry().textContent).toContain("2.1.319");
    expect(versionEntry().querySelector(".bg-accent-soft-foreground")).toBeNull();
    expect(described(versionEntry())).toBe("2.1.319");
  });

  it("has no Version entry for an agent that is not installed", async () => {
    const mounted = mount();
    act(() => mounted.store.setState({ agentAvailability: unavailable("claude") }));
    await openTabMenu(mounted.container, "Claude Code");
    expect([...document.querySelectorAll<HTMLElement>("[role=menuitem]")].some((item) => item.textContent?.startsWith("Version"))).toBe(false);
  });

  describe("the dialogs the entries open", () => {
    const closeDialog = async () => {
      await act(async () => buttonNamed(document.body.querySelector<HTMLElement>('[role="dialog"]')!, "Close")!.click());
      await settleFrames();
    };
    const openFromActions = async (container: HTMLElement, entry: string) => {
      const trigger = actionsOf(container, "Claude Code");
      await act(async () => {
        trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
        trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
        trigger.click();
      });
      await chooseItem(entry);
      return trigger;
    };

    it.each([["Version", "Agent version"], ["Settings", "Agent settings"]])("give focus back to the tab whose menu opened the %s dialog when it closes", async (entry, title) => {
      framesAsTimers("pointer");
      const { container } = mount();
      await openTabMenu(container, "Claude Code");
      await chooseItem(entry);
      await trackMountRefocus()();
      expect(document.body.querySelector('[role="dialog"] h2')?.textContent).toBe(title);
      await closeDialog();
      expect(document.activeElement).toBe(tabOf(container, "Claude Code"));
    });

    it("give focus back to the actions button that opened them, the ring following the input that closed the dialog", async () => {
      // The strip places each tab's actions button from its tab's box, which jsdom has none of.
      vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
        const right = this.matches(partSelector.scrollShadow) ? 1000 : this.matches('[role="tab"]') ? 100 : 0;
        return { x: 0, y: 0, left: 0, right, top: 0, bottom: 30, width: right, height: 30, toJSON: () => ({}) };
      });
      onTestFinished(() => { vi.restoreAllMocks(); });
      framesAsTimers("pointer");
      const { container } = mount();
      const trigger = await openFromActions(container, "Version");
      await trackMountRefocus()();
      act(() => setInteractionModality("pointer"));
      await closeDialog();
      expect(document.activeElement).toBe(trigger);
      // Not forced to the keyboard's, as a handoff the user did not make is.
      expect(getInteractionModality()).not.toBe("keyboard");
    });

    it("close when the agent leaves the list, and hand focus to the section's tab with the ring, the tab and its button being gone", async () => {
      framesAsTimers("pointer");
      const { container, entry, setSettings } = mount();
      await openTabMenu(container, "Claude Code");
      await chooseItem("Settings");
      setSettings({ added_agents: ["codex", "grok"] });
      await settleFrames();
      expect(document.body.querySelector('[role="dialog"]')).toBeNull();
      expect(document.activeElement).toBe(entry);
      expect(getInteractionModality()).toBe("keyboard");
    });

    it("do not come back when the agent is added again", async () => {
      framesAsTimers("pointer");
      const { container, setSettings } = mount();
      await openTabMenu(container, "Claude Code");
      await chooseItem("Settings");
      setSettings({ added_agents: ["codex", "grok"] });
      await settleFrames();
      setSettings({ added_agents: ["claude", "codex", "grok"] });
      expect(document.body.querySelector('[role="dialog"]')).toBeNull();
    });

    it("close for the Version one when its CLI is no longer installed, handing focus to the tab whose menu opened it with the ring", async () => {
      framesAsTimers("pointer");
      const { container, set } = mount();
      await openTabMenu(container, "Claude Code");
      await chooseItem("Version");
      expect(document.body.querySelector('[role="dialog"] h2')?.textContent).toBe("Agent version");
      set({ agentAvailability: unavailable("claude") });
      await settleFrames();
      expect(document.body.querySelector('[role="dialog"]')).toBeNull();
      expect((document.activeElement as HTMLElement | null)?.getAttribute("role")).toBe("tab");
      expect(document.activeElement?.textContent).toContain("Claude Code");
      expect(getInteractionModality()).toBe("keyboard");
    });
  });
});
