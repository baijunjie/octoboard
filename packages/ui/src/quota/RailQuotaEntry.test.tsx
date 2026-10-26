// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { presence, stubListMotion } from "../components/listMotionTestSupport";
import { enter, leave, mountForTooltip, openTooltip, pointer, press as pressWith, withFakeTimers } from "../components/tooltipTestSupport";
import { currentLanguage, setLanguage } from "../i18n/language";
import type { Account, AccountQuota, Agent, Availability, Session } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { MODAL_OPEN } from "../layout/useRegionCycle";
import { RailQuotaEntry } from "./RailQuotaEntry";
import { DEFAULT_GIT_WORKFLOW } from "../gitWorkflow";

// jsdom has no layout, so a scroller never reports overflow. This one reports the way the library's
// does, once when it mounts: overflowing when its list holds the marked account. Each scroller's report
// is also kept in `reports`, for a test to make one again later, as a browser's layout does once the
// panel has settled.
const overflowing = vi.hoisted(() => ({ marker: "", reports: [] as Array<(visibility: "bottom" | "none") => void> }));
vi.mock("../components/scrollFade", async (importOriginal) => {
  const original = await importOriginal<typeof import("../components/scrollFade")>();
  const { useEffect, useImperativeHandle, useRef } = await import("react");
  return {
    ...original,
    FadedScroller: ({ region, children, ref: forwarded }: { region?: import("../components/scrollFade").FadeRegion; children: React.ReactNode; ref?: React.Ref<HTMLDivElement> }) => {
      const ref = useRef<HTMLDivElement>(null);
      useImperativeHandle(forwarded, () => ref.current!);
      useEffect(() => {
        const overflows = overflowing.marker !== "" && ref.current!.textContent!.includes(overflowing.marker);
        region?.onVisibilityChange?.(overflows ? "bottom" : "none");
        if (region?.onVisibilityChange) overflowing.reports.push(region.onVisibilityChange);
      }, []);
      return <div ref={ref} {...region?.props}>{children}</div>;
    },
  };
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const MINUTE = 60_000;
const NOW = Date.UTC(2026, 9, 18, 12, 0, 0);
const OPEN_DELAY = 250;
const CLOSE_DELAY = 150;

const request = vi.fn(() => Promise.resolve({}));
const onOpenSettingsAt = vi.fn();
const language = currentLanguage();

beforeEach(() => {
  act(() => setLanguage("en"));
  request.mockClear();
  onOpenSettingsAt.mockClear();
  overflowing.reports = [];
  // jsdom has no Web Animations, which the popover's exit waits for.
  HTMLElement.prototype.animate = vi.fn(() => ({ cancel: vi.fn(), finished: Promise.resolve(), onfinish: null })) as unknown as typeof HTMLElement.prototype.animate;
});
afterEach(() => {
  delete (HTMLElement.prototype as Partial<HTMLElement>).animate;
  act(() => setLanguage(language));
});

const quota = (agent: Agent, account: string | null, plan: string): AccountQuota => ({
  agent,
  account,
  fetched_at: NOW - MINUTE,
  plan,
  windows: [{ kind: "session", label: null, used_percent: 18, resets_at: NOW + 117 * MINUTE, window_minutes: 300 }],
  unavailable: null,
});

const accounts: Account[] = [
  { id: "claude-work", agent: "claude", name: "Work", config_dir: "/home/dev/.claude-work" },
  { id: "codex-work", agent: "codex", name: "Codex Work", config_dir: "/home/dev/.codex-work" },
];

/** Mounts the entry with Claude Code and Codex available (Grok Build missing) and added (`added`, in
 * that order, `null` while the daemon has not decided the list). The
 * returned `outside` stands for whatever else is on the window, `before` for a control that held
 * focus when the entry was pressed. */
function mount(
  availability: Partial<Record<Agent, Availability>> = { claude: "available", codex: "available", grok: "unavailable" },
  added: Agent[] | null = ["claude", "codex"],
) {
  vi.setSystemTime(NOW);
  const store = createStateStore({
    agentAvailability: new Map((["claude", "codex", "grok"] as Agent[]).map((agent) => [agent, { agent, availability: availability[agent] ?? "not_determined" }])),
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: added, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts },
    accountQuotas: [quota("claude", null, "max"), quota("claude", "claude-work", "pro"), quota("codex", null, "plus")],
  });
  const daemon = { store, request } as unknown as Daemon;
  const container = mountForTooltip(
    <DaemonProvider value={daemon}>
      <button type="button" id="before">before</button>
      <RailQuotaEntry onOpenSettingsAt={onOpenSettingsAt} />
      <button type="button" id="outside">outside</button>
    </DaemonProvider>,
  );
  return {
    store,
    container,
    before: container.querySelector<HTMLButtonElement>("#before")!,
    outside: container.querySelector<HTMLButtonElement>("#outside")!,
    button: container.querySelector<HTMLButtonElement>('button[aria-label="Account usage"]')!,
  };
}

const panel = () => document.querySelector<HTMLElement>('[role="dialog"]');
const tabs = () => Array.from(document.querySelectorAll<HTMLElement>('[role="tab"]'));
/** The rail button's quota capsules, and the remaining percent each one's fill spans of its 18 px. */
const CAPSULE = '[data-slot="quota-capsule"]';
const remaining = (button: Element) => Array.from(button.querySelectorAll(CAPSULE), (capsule) =>
  `${Math.round((Number(capsule.querySelector('rect[fill="currentColor"]')?.getAttribute("width") ?? 0) / 18) * 100)}%`);

/** A press the way a mouse makes it. */
function click(element: Element) {
  act(() => {
    element.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
    element.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
    (element as HTMLElement).click();
  });
}

/** A press the way a keyboard makes it. */
function press(element: HTMLElement, key: string) {
  act(() => {
    element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
  });
}

const advance = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

/** Opens the panel the way a mouse does, by resting on the button: a mouse click opens nothing. */
function openPanel(button: Element) {
  if (!vi.isFakeTimers()) withFakeTimers();
  enter(button);
  advance(OPEN_DELAY);
}

it("floats the panel in after a short rest on the button, and out once the pointer is off both", () => {
  withFakeTimers();
  const { button } = mount();
  enter(button);
  advance(OPEN_DELAY - 1);
  expect(panel()).toBeNull();
  advance(1);
  expect(panel()).not.toBeNull();
  expect(button.getAttribute("aria-expanded")).toBe("true");

  leave(button);
  advance(CLOSE_DELAY - 1);
  expect(panel()).not.toBeNull();
  advance(1);
  expect(panel()).toBeNull();
  expect(button.getAttribute("aria-expanded")).toBe("false");
});

it("keeps the panel while the pointer crosses from the button to it, and closes when it leaves the panel", () => {
  withFakeTimers();
  const { button } = mount();
  enter(button);
  advance(OPEN_DELAY);
  leave(button);
  advance(CLOSE_DELAY - 10);
  act(() => void panel()!.dispatchEvent(pointer("pointerover")));
  advance(1000);
  expect(panel()).not.toBeNull();

  act(() => void panel()!.dispatchEvent(pointer("pointerout")));
  advance(CLOSE_DELAY);
  expect(panel()).toBeNull();
});

it("has no tooltip, on hover or on keyboard focus, while its name stays", () => {
  withFakeTimers();
  const { button, outside } = mount();
  enter(button);
  advance(OPEN_DELAY);
  openPanel(button);
  openPanel(button);
  leave(button);
  advance(CLOSE_DELAY);
  expect(panel()).toBeNull();
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    outside.focus();
  });
  act(() => button.focus());
  advance(1000);
  expect(openTooltip()).toBeNull();
  expect(button.getAttribute("aria-label")).toBe("Account usage");
});

it("shows nothing for a pointer that only crosses the button", () => {
  withFakeTimers();
  const { button } = mount();
  enter(button);
  advance(OPEN_DELAY - 50);
  leave(button);
  advance(1000);
  expect(panel()).toBeNull();
});

it("opens nothing on a mouse click, and a click on a hover-opened panel's button leaves it as it is, not pinned", () => {
  withFakeTimers();
  const { button } = mount();
  click(button);
  expect(panel()).toBeNull();
  advance(1000);
  expect(panel()).toBeNull();

  enter(button);
  advance(OPEN_DELAY);
  click(button);
  expect(panel()!.getAttribute("aria-modal")).toBe("false");
  leave(button);
  advance(1000);
  expect(panel()).toBeNull();
});

it("opens on a touch or pen tap and on keyboard activation, and stays open as the pointer goes", () => {
  withFakeTimers();
  const { button } = mount();
  pressWith(button, "touch");
  expect(panel()).not.toBeNull();
  advance(1000);
  expect(panel()).not.toBeNull();
  press(panel()!, "Escape");
  expect(panel()).toBeNull();

  pressWith(button, "pen");
  expect(panel()).not.toBeNull();
  press(panel()!, "Escape");
  expect(panel()).toBeNull();

  act(() => button.focus());
  press(button, "Enter");
  expect(panel()).not.toBeNull();
  advance(1000);
  expect(panel()).not.toBeNull();
});

it("moves keyboard focus into a panel the keyboard pinned, and Escape closes it and returns focus to the button", () => {
  withFakeTimers();
  const { button } = mount();
  act(() => button.focus());
  press(button, "Enter");
  expect(panel()!.contains(document.activeElement)).toBe(true);
  expect(document.activeElement?.getAttribute("role")).toBe("tab");

  press(document.activeElement as HTMLElement, "Escape");
  expect(panel()).toBeNull();
  advance(100);
  expect(document.activeElement).toBe(button);
});

it("moves a keyboard pin's focus to Add agent when there is no tab to take it", () => {
  withFakeTimers();
  const { button } = mount(undefined, []);
  act(() => button.focus());
  press(button, "Enter");
  expect(document.activeElement?.textContent).toBe("Add agent…");
});

it("closes on a press outside, handing focus back to the button for a keyboard pin", () => {
  withFakeTimers();
  const { button, outside } = mount();
  act(() => button.focus());
  press(button, "Enter");
  pressWith(outside, "mouse");
  expect(panel()).toBeNull();
  advance(100);
  expect(document.activeElement).toBe(button);
});

it("returns a tap-opened panel's focus to whatever held it, not to the button", () => {
  withFakeTimers();
  const { before, button } = mount();
  act(() => before.focus());
  pressWith(button, "touch");
  expect(panel()!.contains(document.activeElement)).toBe(true);
  press(document.activeElement as HTMLElement, "Escape");
  expect(panel()).toBeNull();
  advance(100);
  expect(document.activeElement).toBe(before);
});

it("closes a hover-opened panel on Escape and on a press outside", () => {
  withFakeTimers();
  const { button, outside } = mount();
  enter(button);
  advance(OPEN_DELAY);
  press(document.body, "Escape");
  expect(panel()).toBeNull();

  advance(CLOSE_DELAY);
  enter(button);
  advance(OPEN_DELAY);
  act(() => void outside.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })));
  expect(panel()).toBeNull();
});

it("does not let Escape reach anything else while the panel is open", () => {
  withFakeTimers();
  const { button } = mount();
  const seen = vi.fn();
  document.body.addEventListener("keydown", seen);
  openPanel(button);
  press(button, "Escape");
  expect(seen).not.toHaveBeenCalled();
  document.body.removeEventListener("keydown", seen);
});

it("has one tab per added agent and lists each agent's accounts with plan and quota", () => {
  withFakeTimers();
  const { button } = mount();
  openPanel(button);
  expect(tabs().map((tab) => tab.textContent)).toEqual(["Claude Code", "Codex"]);
  const text = () => document.querySelector('[role="tabpanel"]')!.textContent!.replace(/[⁨⁩]/g, "");
  expect(text()).toContain("Default");
  expect(text()).toContain("Max");
  expect(text()).toContain("Work");
  expect(text()).toContain("Pro");
  expect(text()).toContain("Session 82% remaining");
  expect(text()).not.toContain("Codex Work");

  click(tabs()[1]!);
  expect(tabs()[1]!.getAttribute("aria-selected")).toBe("true");
  expect(text()).toContain("Plus");
  expect(text()).toContain("Codex Work");
  expect(text()).not.toContain("Pro");
});

it("marks a failed reading on its account's name line, and the mark's popover opens on a tap without closing the panel", async () => {
  withFakeTimers();
  const { button, store } = mount();
  act(() => store.setState({ accountQuotas: [{ ...quota("claude", "claude-work", "pro"), unavailable: { code: "quota_timed_out", params: { agent: "Claude Code" }, message: "timed out" } }] }));
  openPanel(button);
  const mark = document.querySelector<HTMLElement>('[role="tabpanel"] button[aria-label^="Out of date"]')!;
  expect(mark).not.toBeNull();
  // A tap: a mouse click on a mark does nothing, its popover being the hover's.
  pressWith(mark, "touch");
  // The popover portals in beside modals; the panel's watch for a covering modal must not take it for one.
  await act(async () => {});
  expect(panel()).not.toBeNull();
  const reason = document.querySelector<HTMLElement>("[data-status-mark-popover]")!;
  expect(reason.textContent?.replace(/[\u2066-\u2069]/g, "")).toContain("took too long to report its usage");
  click(reason);
  expect(panel()).not.toBeNull();
  expect(document.querySelector("[data-status-mark-popover]")).not.toBeNull();
  act(() => void reason.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(document.querySelector("[data-status-mark-popover]")).toBeNull();
  expect(panel()).not.toBeNull();
});

/** The pointer leaving `from` for `to`, as a browser reports it: React raises leave along the component tree, from `relatedTarget`. */
function moveTo(from: Element, to: Element | null) {
  const event = new MouseEvent("pointerout", { bubbles: true, relatedTarget: to });
  Object.defineProperty(event, "pointerType", { value: "mouse" });
  act(() => void from.dispatchEvent(event));
}

it("keeps a hover-opened panel while the pointer crosses into a failed reading's popover, which Escape closes first, and closes it a grace after the pointer leaves", () => {
  withFakeTimers();
  const { button, store } = mount();
  act(() => store.setState({ accountQuotas: [{ ...quota("claude", "claude-work", "pro"), unavailable: { code: "quota_timed_out", params: { agent: "Claude Code" }, message: "timed out" } }] }));
  // Opened by hover alone, not pinned by a press.
  enter(button);
  act(() => void vi.advanceTimersByTime(300));
  expect(panel()).not.toBeNull();
  const mark = document.querySelector<HTMLElement>('[role="tabpanel"] button[aria-label^="Out of date"]')!;
  moveTo(button, panel());
  enter(mark);
  act(() => void vi.advanceTimersByTime(300));
  const reason = document.querySelector<HTMLElement>("[data-status-mark-popover]")!;
  expect(reason).not.toBeNull();
  expect(reason.hasAttribute("data-escape-topmost")).toBe(true);
  // Out of the panel and into the mark's popover, which stands outside it: the panel stays.
  moveTo(mark, reason.querySelector('[role="dialog"]'));
  act(() => void vi.advanceTimersByTime(1000));
  expect(panel()).not.toBeNull();
  expect(document.querySelector("[data-status-mark-popover]")).not.toBeNull();
  act(() => void document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
  expect(document.querySelector("[data-status-mark-popover]")).toBeNull();
  expect(panel()).not.toBeNull();
  // Leaving for the page closes the panel after the grace.
  moveTo(panel()!, null);
  act(() => void vi.advanceTimersByTime(1000));
  expect(panel()).toBeNull();
});

it("leaves a hover-opened panel's Escape to a terminal, to a text field with text and to an input method's composition, where focus is", () => {
  withFakeTimers();
  const { button } = mount();
  const terminal = document.body.appendChild(document.createElement("div"));
  terminal.className = "xterm";
  const helper = terminal.appendChild(document.createElement("textarea"));
  const field = document.body.appendChild(document.createElement("input"));
  field.value = "text";
  onTestFinished(() => {
    terminal.remove();
    field.remove();
  });
  enter(button);
  act(() => void vi.advanceTimersByTime(300));
  const escape = (target: Element, init: KeyboardEventInit = {}) => {
    const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true, ...init });
    act(() => void target.dispatchEvent(event));
    return event.defaultPrevented;
  };
  expect(escape(helper)).toBe(false);
  expect(escape(field)).toBe(false);
  expect(escape(document.body, { isComposing: true })).toBe(false);
  expect(panel()).not.toBeNull();
  expect(escape(document.body)).toBe(true);
  expect(panel()).toBeNull();
});

it("leaves Escape to a terminal even with a mark's popover hover-opened in the hover-opened panel: neither closes", () => {
  withFakeTimers();
  const { button, store } = mount();
  act(() => store.setState({ accountQuotas: [{ ...quota("claude", "claude-work", "pro"), unavailable: { code: "quota_timed_out", params: { agent: "Claude Code" }, message: "timed out" } }] }));
  const terminal = document.body.appendChild(document.createElement("div"));
  terminal.className = "xterm";
  const helper = terminal.appendChild(document.createElement("textarea"));
  onTestFinished(() => terminal.remove());
  openPanel(button);
  act(() => helper.focus());
  enter(document.querySelector<HTMLElement>('[role="tabpanel"] button[aria-label^="Out of date"]')!);
  advance(300);
  expect(document.querySelector("[data-status-mark-popover][data-escape-topmost]")).not.toBeNull();
  const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  act(() => void helper.dispatchEvent(event));
  expect(event.defaultPrevented).toBe(false);
  expect(panel()).not.toBeNull();
  expect(document.querySelector("[data-status-mark-popover]")).not.toBeNull();
});

it("recedes a used-up account, naming it used up, and draws it as before once its window resets", () => {
  withFakeTimers();
  const { button, store } = mount();
  const used = quota("claude", "claude-work", "pro");
  used.windows = [{ ...used.windows[0]!, used_percent: 100, resets_at: NOW + 10 * MINUTE }];
  act(() => store.setState({ accountQuotas: [quota("claude", null, "max"), used] }));
  openPanel(button);
  const rows = () => Array.from(document.querySelectorAll<HTMLElement>('[role="tabpanel"]:not([data-inert="true"]) .groove-b'));
  const [usable, usedUp] = rows();
  expect(usable!.textContent).not.toContain("Limit reached");
  expect(usable!.querySelector(".quota-meter--claude")).not.toBeNull();
  expect(usedUp!.textContent).toContain("Limit reached");
  expect(usedUp!.querySelector("div.min-w-0.text-app-muted")?.textContent).toBe("Work");
  expect(usable!.querySelector("div.min-w-0.text-app-muted")).toBeNull();
  expect(usedUp!.querySelector(".quota-meter--used-up")).not.toBeNull();
  expect(usedUp!.querySelector(".quota-meter--claude")).toBeNull();

  advance(10 * MINUTE + 1000);
  expect(rows()[1]!.textContent).not.toContain("Limit reached");
  expect(rows()[1]!.querySelector(".quota-meter--claude")).not.toBeNull();
});

it("holds the panel's height across tabs, so that choosing a tab with fewer accounts moves nothing under the pointer", () => {
  withFakeTimers();
  const { button } = mount();
  openPanel(button);
  // jsdom has no layout, so the guarantee is checked in its structure: every tab's panel stands in
  // one grid cell with the same classes (so the same padding), and the unselected ones are hidden.
  const panels = () => Array.from(document.querySelectorAll<HTMLElement>('[data-slot="tabs-panel"]'));
  const clean = (element: Element) => element.textContent!.replace(/[⁨⁩]/g, "");
  const check = (selected: number) => {
    expect(panels()).toHaveLength(2);
    const [first] = panels();
    for (const [index, element] of panels().entries()) {
      expect(element.parentElement).toBe(first!.parentElement);
      expect(element.className).toBe(first!.className);
      expect(element.classList.contains("col-start-1") && element.classList.contains("row-start-1")).toBe(true);
      expect(element.classList.contains("p-0") && element.classList.contains("m-0")).toBe(true);
      const isSelected = index === selected;
      expect(element.hasAttribute("inert")).toBe(!isSelected);
      expect(element.getAttribute("data-inert")).toBe(isSelected ? null : "true");
      expect(element.getAttribute("role")).toBe(isSelected ? "tabpanel" : null);
    }
    expect(document.querySelectorAll('[role="tabpanel"]')).toHaveLength(1);
  };
  expect(panels()[0]!.className).toContain("data-[inert=true]:invisible");
  check(0);
  expect(clean(panels()[1]!)).toContain("Codex Work");

  click(tabs()[1]!);
  check(1);
});

it("makes the list a scroll region only on a tab whose own list overflows", () => {
  withFakeTimers();
  overflowing.marker = "Codex Work";
  onTestFinished(() => void (overflowing.marker = ""));
  const { button } = mount();
  openPanel(button);
  const regions = () => Array.from(document.querySelectorAll<HTMLElement>('[role="region"]'));
  // The hidden panels' scrollers have reported too, but their regions are inert.
  const reachable = () => regions().filter((region) => !region.closest("[inert]"));
  expect(reachable()).toHaveLength(0);

  click(tabs()[1]!);
  expect(reachable()).toHaveLength(1);
  expect(reachable()[0]!.getAttribute("aria-label")).toBe("Accounts");

  click(tabs()[0]!);
  expect(reachable()).toHaveLength(0);
});

it("is no tab stop of its own in front of a list that has become a scroll region, so Tab goes from the tabs to the list", () => {
  withFakeTimers();
  const { button } = mount();
  openPanel(button);
  // The lists find they overflow only after the panel has rendered without a region.
  expect(document.querySelector('[role="tabpanel"]')!.getAttribute("tabindex")).toBe("0");
  act(() => overflowing.reports.forEach((report) => report("bottom")));
  const tabPanel = document.querySelector<HTMLElement>('[role="tabpanel"]')!;
  expect(tabPanel.querySelector('[role="region"]')?.getAttribute("tabindex")).toBe("0");
  expect(tabPanel.hasAttribute("tabindex")).toBe(false);
});

it("lists the agents that are added and installed in the order they were added, and no other", () => {
  withFakeTimers();
  const { button } = mount({ claude: "available", codex: "available", grok: "available" }, ["grok", "codex"]);
  openPanel(button);
  expect(tabs().map((tab) => tab.textContent)).toEqual(["Grok Build", "Codex"]);
});

it("leaves out an added agent that is not installed", () => {
  withFakeTimers();
  const { button } = mount({ claude: "available", codex: "available", grok: "unavailable" }, ["grok", "codex"]);
  openPanel(button);
  expect(tabs().map((tab) => tab.textContent)).toEqual(["Codex"]);
});

it("lists every installed agent while the daemon has not decided the added list", () => {
  withFakeTimers();
  const { button } = mount(undefined, null);
  openPanel(button);
  expect(tabs().map((tab) => tab.textContent)).toEqual(["Claude Code", "Codex"]);
});

it("says so, pointing to Settings, when agents are installed but none is added", () => {
  withFakeTimers();
  const { button } = mount(undefined, []);
  openPanel(button);
  expect(tabs()).toHaveLength(0);
  expect(panel()!.textContent).toContain("No agent added");
  expect(panel()!.textContent).toContain("Add an agent to see its usage here.");
  // The footer link would repeat the button.
  expect(panel()!.textContent).not.toContain("Manage accounts…");
  const add = Array.from(panel()!.querySelectorAll("button")).find((b) => b.textContent === "Add agent…");
  click(add!);
  expect(onOpenSettingsAt).toHaveBeenCalledWith("accounts", { addAgent: true });
  expect(panel()).toBeNull();
});

it("says that no agent is available when the added ones are not installed and nothing else is", () => {
  withFakeTimers();
  const { button } = mount({ claude: "unavailable", codex: "unavailable", grok: "unavailable" }, ["claude"]);
  openPanel(button);
  expect(tabs()).toHaveLength(0);
  expect(panel()!.textContent).toContain("No agent available");
  expect(panel()!.textContent).toContain("Manage accounts…");
});

it("asks the daemon to refresh the quotas each time the panel opens, and not when it only changes mode", () => {
  withFakeTimers();
  const { button } = mount();
  expect(request).not.toHaveBeenCalled();
  enter(button);
  advance(OPEN_DELAY);
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "refresh_account_quotas" });
  // Turning into the press form is not another opening.
  pressWith(button, "touch");
  expect(request).toHaveBeenCalledTimes(1);

  press(panel()!, "Escape");
  expect(panel()).toBeNull();
  // The pointer that rested on the button has to leave and come back.
  leave(button);
  act(() => void document.body.dispatchEvent(pointer("pointermove")));
  openPanel(button);
  expect(request).toHaveBeenCalledTimes(2);
});

it("opens Settings' accounts section from the panel, and closes the panel", () => {
  withFakeTimers();
  const { button } = mount();
  openPanel(button);
  const manage = Array.from(panel()!.querySelectorAll("button")).find((b) => b.textContent === "Manage accounts…")!;
  click(manage);
  expect(onOpenSettingsAt).toHaveBeenCalledExactlyOnceWith("accounts");
  expect(panel()).toBeNull();
});

it("points the button at the panel it opens", () => {
  withFakeTimers();
  const { button } = mount();
  openPanel(button);
  expect(panel()!.id).not.toBe("");
  expect(button.getAttribute("aria-controls")).toBe(panel()!.id);
  expect(button.hasAttribute("data-pane-toggle")).toBe(false);
});

it("closes when a modal opens without a press on the entry", async () => {
  withFakeTimers();
  const { button } = mount();
  openPanel(button);
  const modal = document.body.appendChild(document.createElement("div"));
  modal.setAttribute("role", "dialog");
  await act(async () => {});
  expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
  expect(document.querySelector('[aria-label="Account usage"][role="dialog"]')).toBeNull();
  modal.remove();
});

it("closes a keyboard-opened (modal) panel when another modal opens and takes focus", async () => {
  withFakeTimers();
  const { button } = mount();
  act(() => button.focus());
  press(button, "Enter");
  expect(panel()!.contains(document.activeElement)).toBe(true);
  const modal = document.body.appendChild(document.createElement("div"));
  modal.setAttribute("role", "dialog");
  const inside = modal.appendChild(document.createElement("button"));
  act(() => inside.focus());
  await act(async () => {});
  expect(document.querySelector('[aria-label="Account usage"][role="dialog"]')).toBeNull();
  modal.remove();
});

it("is not a modal to the window: its F6 cycle, shortcuts and floating panes go on working beside it", () => {
  withFakeTimers();
  const { button } = mount();
  openPanel(button);
  expect(panel()).not.toBeNull();
  expect(panel()!.getAttribute("aria-modal")).toBe("false");
  expect(document.querySelector(MODAL_OPEN)).toBeNull();
});

it("marks itself the topmost Escape taker only while open", () => {
  withFakeTimers();
  const { button } = mount();
  openPanel(button);
  expect(document.querySelector("[data-escape-topmost]")).not.toBeNull();
  press(document.body, "Escape");
  expect(document.querySelector("[data-escape-topmost]")).toBeNull();
});

it("follows the selected session's actual agent and account, account switches and live quota updates", () => {
  withFakeTimers();
  const { store, container } = mount();
  const host = container.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => act(() => root.unmount()));
  const session: Session = {
    id: "selected", agent: "claude", account_id: "claude-work", config_dir: accounts[0]!.config_dir, console_id: "console", host_id: "local",
    role: "console", origin: "console", title: "Selected", status: "idle", has_conversation: true,
    pinned: false, locked: false, started_at: NOW,
  };
  act(() => store.setState({ sessions: new Map([[session.id, session]]) }));
  const select = (id?: string) => act(() => root.render(
    <DaemonProvider value={{ store, request } as unknown as Daemon}>
      <RailQuotaEntry selectedSessionId={id} onOpenSettingsAt={onOpenSettingsAt} />
    </DaemonProvider>,
  ));
  const button = () => host.querySelector("button")!;
  const fill = () => button().querySelector(CAPSULE);
  select(session.id);
  expect(button().getAttribute("aria-label")).toContain("Claude Code, ⁨Work⁩, Session 82% remaining");
  expect(remaining(button())).toEqual(["82%"]);
  act(() => store.setState({ settings: { ...store.getState().settings, accounts: [{ ...accounts[0]!, config_dir: "/home/dev/.claude-other" }] } }));
  expect(fill()).toBeNull();
  act(() => store.setState({ settings: { ...store.getState().settings, accounts: [] } }));
  expect(fill()).toBeNull();
  act(() => store.setState({ sessions: new Map([[session.id, { ...session, account_id: null }]]) }));
  expect(fill()).toBeNull();

  act(() => store.setState({ sessions: new Map([[session.id, { ...session, agent: "codex", account_id: null, config_dir: null }]]) }));
  expect(button().getAttribute("aria-label")).toContain("Codex, ⁨Default⁩");
  const weekly = quota("codex", null, "plus");
  weekly.windows = [{ ...weekly.windows[0]!, kind: "weekly", used_percent: 64 }];
  act(() => store.setState({ accountQuotas: [weekly] }));
  expect(button().getAttribute("aria-label")).toContain("Weekly 36% remaining");
  expect(remaining(button())).toEqual(["36%"]);
  const both = quota("codex", null, "plus");
  both.windows = [{ ...both.windows[0]!, used_percent: 18 }, { ...both.windows[0]!, kind: "weekly", used_percent: 64 }];
  act(() => store.setState({ accountQuotas: [both] }));
  expect(button().getAttribute("aria-label")).toContain("5h 82% remaining, Weekly 36% remaining");
  expect(remaining(button())).toEqual(["82%", "36%"]);
  select();
  expect(fill()).toBeNull();
  expect(button().getAttribute("aria-label")).toBe("Account usage");
});

it("opens on the tab of the agent whose quota the button shows, each time, and on the first tab with none", () => {
  withFakeTimers();
  const { store, container } = mount();
  const host = container.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => act(() => root.unmount()));
  const session: Session = {
    id: "selected", agent: "codex", account_id: null, config_dir: null, console_id: "console", host_id: "local",
    role: "console", origin: "console", title: "Selected", status: "idle", has_conversation: true,
    pinned: false, locked: false, started_at: NOW,
  };
  act(() => store.setState({ sessions: new Map([[session.id, session]]) }));
  const select = (id?: string) => act(() => root.render(
    <DaemonProvider value={{ store, request } as unknown as Daemon}>
      <RailQuotaEntry selectedSessionId={id} onOpenSettingsAt={onOpenSettingsAt} />
    </DaemonProvider>,
  ));
  const selectedTab = () => tabs().find((tab) => tab.getAttribute("aria-selected") === "true")?.textContent;
  const entry = () => host.querySelector("button")!;
  select(session.id);
  openPanel(entry());
  expect(selectedTab()).toBe("Codex");
  // Free to switch within one opening, and the next opening starts from the shown agent again.
  click(tabs()[0]!);
  expect(selectedTab()).toBe("Claude Code");
  leave(entry());
  advance(1000);
  expect(panel()).toBeNull();
  openPanel(entry());
  expect(selectedTab()).toBe("Codex");
  leave(entry());
  advance(1000);

  select();
  openPanel(entry());
  expect(selectedTab()).toBe("Claude Code");
});

it("opens the shown agent's list scrolled to the selected session's account, with room for the fade", () => {
  withFakeTimers();
  const { store, container } = mount();
  // jsdom has no layout: a row's offset is its place among its siblings, 100 px each, and a scroller keeps what is set.
  const scrolled = new WeakMap<Element, number>();
  const offset = vi.spyOn(HTMLElement.prototype, "offsetTop", "get").mockImplementation(function (this: HTMLElement) {
    return Array.from(this.parentElement?.children ?? []).indexOf(this) * 100;
  });
  const top = vi.spyOn(HTMLElement.prototype, "scrollTop", "set").mockImplementation(function (this: HTMLElement, value: number) {
    scrolled.set(this, value);
  });
  onTestFinished(() => {
    offset.mockRestore();
    top.mockRestore();
  });
  const host = container.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => act(() => root.unmount()));
  const session: Session = {
    id: "selected", agent: "claude", account_id: "claude-work", config_dir: accounts[0]!.config_dir, console_id: "console", host_id: "local",
    role: "console", origin: "console", title: "Selected", status: "idle", has_conversation: true,
    pinned: false, locked: false, started_at: NOW,
  };
  act(() => store.setState({ sessions: new Map([[session.id, session]]) }));
  const select = (id?: string) => act(() => root.render(
    <DaemonProvider value={{ store, request } as unknown as Daemon}>
      <RailQuotaEntry selectedSessionId={id} onOpenSettingsAt={onOpenSettingsAt} />
    </DaemonProvider>,
  ));
  /** The scroll position the shown agent's list was given when the panel mounted. */
  const positions = () => Array.from(document.querySelectorAll('[role="tabpanel"]'), (tabpanel) =>
    Array.from(tabpanel.querySelectorAll("div"), (div) => scrolled.get(div)).find((value) => value !== undefined));
  const entry = () => host.querySelector("button")!;
  select(session.id);
  openPanel(entry());
  // Claude's list holds the default account, then Work: Work is the second row, 100 px down, less the 24 px fade.
  expect(positions()).toEqual([76]);
  leave(entry());
  advance(1000);
  expect(panel()).toBeNull();

  // The default account is the first row, which needs no scrolling.
  act(() => store.setState({ sessions: new Map([[session.id, { ...session, account_id: null, config_dir: null }]]) }));
  openPanel(entry());
  expect(positions()).toEqual([0]);
});

it("draws the button's window full, and says so, once its reset time passes while mounted", () => {
  withFakeTimers();
  const { store, container } = mount();
  const host = container.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => act(() => root.unmount()));
  const session: Session = {
    id: "selected", agent: "codex", account_id: null, config_dir: null, console_id: "console", host_id: "local",
    role: "console", origin: "console", title: "Selected", status: "idle", has_conversation: true,
    pinned: false, locked: false, started_at: NOW,
  };
  const used = quota("codex", null, "plus");
  used.windows = [{ ...used.windows[0]!, used_percent: 100, resets_at: NOW + 10 * MINUTE }];
  act(() => store.setState({ sessions: new Map([[session.id, session]]), accountQuotas: [used] }));
  act(() => root.render(
    <DaemonProvider value={{ store, request } as unknown as Daemon}>
      <RailQuotaEntry selectedSessionId={session.id} onOpenSettingsAt={onOpenSettingsAt} />
    </DaemonProvider>,
  ));
  const button = host.querySelector("button")!;
  expect(remaining(button)).toEqual(["0%"]);
  expect(button.getAttribute("aria-label")).toContain("5h 0% remaining");
  advance(10 * MINUTE);
  expect(remaining(button)).toEqual(["100%"]);
  expect(button.getAttribute("aria-label")).toContain("5h 100% remaining");
});

it("hides the mini bar for unread, failed, stale or unavailable quota instead of showing another account", () => {
  withFakeTimers();
  const { store, container } = mount();
  const host = container.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => act(() => root.unmount()));
  const session: Session = {
    id: "selected", agent: "claude", account_id: "claude-work", console_id: "console", host_id: "local",
    role: "console", origin: "console", title: "Selected", status: "idle", has_conversation: true,
    pinned: false, locked: false, started_at: NOW,
  };
  act(() => store.setState({ sessions: new Map([[session.id, session]]), accountQuotas: [] }));
  act(() => root.render(<DaemonProvider value={{ store, request } as unknown as Daemon}>
    <RailQuotaEntry selectedSessionId={session.id} onOpenSettingsAt={onOpenSettingsAt} />
  </DaemonProvider>));
  const fill = () => host.querySelector(CAPSULE);
  expect(fill()).toBeNull();
  const reading = quota("claude", "claude-work", "max");
  act(() => store.setState({ accountQuotas: [reading] }));
  expect(fill()).not.toBeNull();
  act(() => store.setState({ accountQuotas: [{ ...reading, unavailable: { code: "quota_timed_out", params: {}, message: "timeout" } }] }));
  expect(fill()).toBeNull();
  act(() => store.setState({ accountQuotas: [{ ...reading, windows: [] }] }));
  expect(fill()).toBeNull();
  act(() => store.setState({ accountQuotas: [reading], agentAvailability: new Map() }));
  expect(fill()).toBeNull();
});

it("keeps the mini bar of a session whose agent was removed", () => {
  withFakeTimers();
  const { store, container } = mount();
  const host = container.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => act(() => root.unmount()));
  const session: Session = {
    id: "selected", agent: "claude", account_id: "claude-work", console_id: "console", host_id: "local",
    role: "console", origin: "console", title: "Selected", status: "idle", has_conversation: true,
    pinned: false, locked: false, started_at: NOW,
  };
  act(() => store.setState({ sessions: new Map([[session.id, session]]) }));
  act(() => root.render(<DaemonProvider value={{ store, request } as unknown as Daemon}>
    <RailQuotaEntry selectedSessionId={session.id} onOpenSettingsAt={onOpenSettingsAt} />
  </DaemonProvider>));
  const fill = () => host.querySelector(CAPSULE);
  expect(fill()).not.toBeNull();
  act(() => store.setState({ settings: { ...store.getState().settings, added_agents: ["codex"] } }));
  expect(fill()).not.toBeNull();
});

it("moves the accounts and agent tabs that come and go while the panel is open, and nothing as it opens", async () => {
  withFakeTimers();
  const motion = stubListMotion();
  const { button, store } = mount();
  openPanel(button);
  expect(motion.played()).toEqual([]);
  const setAccounts = (next: Account[]) => act(() => store.setState((state) => ({ settings: { ...state.settings, accounts: next } })));
  const withSide = [...accounts, { id: "claude-side", agent: "claude" as const, name: "Side", config_dir: "/home/dev/.claude-side" }];
  setAccounts(withSide);
  const row = (id: string) => document.querySelector(`[data-list-motion="${id}"]`);
  expect(motion.played().map(({ element, frames }) => [element, presence(frames)])).toEqual([[row("claude-side"), "enter"]]);
  motion.clear();
  setAccounts(withSide.filter((account) => account.id !== "claude-work"));
  expect(motion.played().map(({ frames }) => presence(frames))).toEqual(["exit"]);
  const copy = panel()!.querySelector("[data-list-motion-exit]")!;
  expect(copy.getAttribute("aria-hidden")).toBe("true");
  expect(copy.hasAttribute("inert")).toBe(true);
  expect(row("claude-work")).toBeNull();
  motion.clear();
  // Codex stops being installed: its tab leaves the strip.
  act(() => store.setState((state) => ({ agentAvailability: new Map(state.agentAvailability).set("codex", { agent: "codex", availability: "unavailable" }) })));
  // The tabs are rendered from the collection in a commit of their own, which the motion observes.
  await act(async () => {});
  expect(motion.played().map(({ frames }) => presence(frames))).toEqual(["exit"]);
  expect(tabs().filter((tab) => !tab.closest("[inert]")).map((tab) => tab.textContent)).toEqual(["Claude Code"]);
});

it("drops a removed account at once, with no copy, while reduced motion is asked for", () => {
  withFakeTimers();
  const motion = stubListMotion();
  const { button, store } = mount();
  openPanel(button);
  motion.reduce();
  act(() => store.setState((state) => ({ settings: { ...state.settings, accounts: accounts.filter((account) => account.id !== "claude-work") } })));
  expect(motion.played()).toEqual([]);
  expect(panel()!.querySelector("[data-list-motion-exit]")).toBeNull();
});

/** Mounts the entry with a Claude Code session on `claude-work` selected, in a host of its own. */
function mountWithSession(accountId: string | null = "claude-work") {
  const { store, container } = mount();
  const host = container.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => act(() => root.unmount()));
  const session: Session = {
    id: "selected", agent: "claude", account_id: accountId, config_dir: null, console_id: "console", host_id: "local",
    role: "console", origin: "console", title: "Selected", status: "idle", has_conversation: true,
    pinned: false, locked: false, started_at: NOW,
  };
  act(() => store.setState({ sessions: new Map([[session.id, session]]) }));
  act(() => root.render(
    <DaemonProvider value={{ store, request } as unknown as Daemon}>
      <RailQuotaEntry selectedSessionId={session.id} onOpenSettingsAt={onOpenSettingsAt} />
    </DaemonProvider>,
  ));
  return { button: host.querySelector("button")! };
}

/** The ids of the panel's account rows that are dimmed, across every agent's tab. */
const dimmedRows = () => Array.from(document.querySelectorAll("[data-list-motion][data-dimmed]"), (row) => row.getAttribute("data-list-motion"));

it("dims every account but the one the button shows while the mouse is on the button, and lifts it only once the mouse enters the panel", () => {
  withFakeTimers();
  const { button } = mountWithSession();
  enter(button);
  expect(panel()).toBeNull();
  advance(OPEN_DELAY);
  expect(dimmedRows()).toEqual(["default:claude", "default:codex", "codex-work"]);
  // The cue is a veil: the rows stay exposed to assistive technology.
  expect(panel()!.querySelectorAll("[data-list-motion][aria-hidden], [data-list-motion][inert]")).toHaveLength(0);

  // The gap between the button and the panel keeps it, until the mouse is in the panel.
  leave(button);
  advance(CLOSE_DELAY - 10);
  expect(dimmedRows()).toEqual(["default:claude", "default:codex", "codex-work"]);
  // The mouse comes into the panel onto one of its rows, entering the panel and its contents at once.
  act(() => void panel()!.querySelector("[data-list-motion]")!.dispatchEvent(pointer("pointerover")));
  expect(dimmedRows()).toEqual([]);
  advance(1000);
  expect(panel()).not.toBeNull();
});

it("dims the other accounts for the default account too, and not at all when the button shows no account", () => {
  withFakeTimers();
  const { button } = mountWithSession(null);
  enter(button);
  advance(OPEN_DELAY);
  expect(dimmedRows()).toEqual(["claude-work", "default:codex", "codex-work"]);

  const bare = mount();
  leave(button);
  advance(CLOSE_DELAY);
  expect(panel()).toBeNull();
  enter(bare.button);
  advance(OPEN_DELAY);
  expect(panel()).not.toBeNull();
  expect(dimmedRows()).toEqual([]);
});

it("leaves the accounts alone for keyboard focus and a keyboard pin", () => {
  withFakeTimers();
  const { button } = mountWithSession();
  act(() => button.focus());
  press(button, "Enter");
  advance(OPEN_DELAY);
  expect(panel()).not.toBeNull();
  expect(dimmedRows()).toEqual([]);
});

it("leaves the accounts alone for a keyboard opening after the mouse crossed the button without opening the panel", () => {
  withFakeTimers();
  const { button } = mountWithSession();
  enter(button);
  advance(OPEN_DELAY - 10);
  leave(button);
  advance(1000);
  expect(panel()).toBeNull();
  act(() => button.focus());
  press(button, "Enter");
  expect(panel()).not.toBeNull();
  expect(dimmedRows()).toEqual([]);
});
