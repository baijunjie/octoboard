// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { enter, leave, mountForTooltip, openTooltip, pointer, withFakeTimers } from "../components/tooltipTestSupport";
import { currentLanguage, setLanguage } from "../i18n/language";
import type { Account, AccountQuota, Agent, Availability, Session } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { MODAL_OPEN, useRegionCycle } from "../layout/useRegionCycle";
import { RailQuotaEntry } from "./RailQuotaEntry";

// jsdom has no layout, so a scroller never reports overflow. This one reports the way the library's
// does, once when it mounts: overflowing when its list holds the marked account. Each scroller's report
// is also kept in `reports`, for a test to make one again later, as a browser's layout does once the
// panel has settled.
const overflowing = vi.hoisted(() => ({ marker: "", reports: [] as Array<(visibility: "bottom" | "none") => void> }));
vi.mock("../components/scrollFade", async (importOriginal) => {
  const original = await importOriginal<typeof import("../components/scrollFade")>();
  const { useEffect, useRef } = await import("react");
  return {
    ...original,
    FadedScroller: ({ region, children }: { region?: import("../components/scrollFade").FadeRegion; children: React.ReactNode }) => {
      const ref = useRef<HTMLDivElement>(null);
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

/** Mounts the entry with Claude Code and Codex available (Grok Build missing, so it has no tab). The
 * returned `outside` stands for whatever else is on the window, `before` for a control that held
 * focus when the entry was pressed. */
function mount(availability: Partial<Record<Agent, Availability>> = { claude: "available", codex: "available", grok: "unavailable" }) {
  vi.setSystemTime(NOW);
  const store = createStateStore({
    agentAvailability: new Map((["claude", "codex", "grok"] as Agent[]).map((agent) => [agent, { agent, availability: availability[agent] ?? "not_determined" }])),
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "", accounts },
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
    button: container.querySelector<HTMLButtonElement>('button[aria-label="Account quota"]')!,
  };
}

const panel = () => document.querySelector<HTMLElement>('[role="dialog"]');
const tabs = () => Array.from(document.querySelectorAll<HTMLElement>('[role="tab"]'));

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

it("floats the panel in after a short rest on the button, and out once the pointer is off both", () => {
  withFakeTimers();
  const { button } = mount();
  enter(button);
  advance(OPEN_DELAY - 1);
  expect(panel()).toBeNull();
  advance(1);
  expect(panel()).not.toBeNull();
  expect(button.getAttribute("aria-expanded")).toBe("true");
  // The tooltip is held back while the panel says more.
  advance(1000);
  expect(openTooltip()).toBeNull();

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

it("shows nothing for a pointer that only crosses the button", () => {
  withFakeTimers();
  const { button } = mount();
  enter(button);
  advance(OPEN_DELAY - 50);
  leave(button);
  advance(1000);
  expect(panel()).toBeNull();
});

it("pins on a click, so that leaving does not close it", () => {
  withFakeTimers();
  const { button } = mount();
  enter(button);
  advance(OPEN_DELAY);
  click(button);
  leave(button);
  advance(1000);
  expect(panel()).not.toBeNull();

  // Another press of the button lets go of it.
  click(button);
  expect(panel()).toBeNull();
});

it("pins on a click without a prior hover (touch), and on keyboard activation", () => {
  withFakeTimers();
  const { button } = mount();
  click(button);
  expect(panel()).not.toBeNull();
  click(button);
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

it("closes on a press outside, handing focus back to the button for a keyboard pin", () => {
  withFakeTimers();
  const { button, outside } = mount();
  act(() => button.focus());
  press(button, "Enter");
  act(() => void outside.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })));
  expect(panel()).toBeNull();
  advance(100);
  expect(document.activeElement).toBe(button);
});

it("returns a pointer pin's focus to whatever held it, not to the button", () => {
  withFakeTimers();
  const { before, button } = mount();
  act(() => before.focus());
  click(button);
  expect(document.activeElement).toBe(before);
  // A press inside the panel takes focus there.
  act(() => tabs()[0]!.focus());
  press(tabs()[0]!, "Escape");
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
  click(button);
  press(button, "Escape");
  expect(seen).not.toHaveBeenCalled();
  document.body.removeEventListener("keydown", seen);
});

it("has one tab per available agent and lists each agent's accounts with plan and quota", () => {
  withFakeTimers();
  const { button } = mount();
  click(button);
  expect(tabs().map((tab) => tab.textContent)).toEqual(["Claude Code", "Codex"]);
  const text = () => document.querySelector('[role="tabpanel"]')!.textContent!.replace(/[⁨⁩]/g, "");
  expect(text()).toContain("Default");
  expect(text()).toContain("Max");
  expect(text()).toContain("Work");
  expect(text()).toContain("Pro");
  expect(text()).toContain("5-hour 82% remaining");
  expect(text()).not.toContain("Codex Work");

  click(tabs()[1]!);
  expect(tabs()[1]!.getAttribute("aria-selected")).toBe("true");
  expect(text()).toContain("Plus");
  expect(text()).toContain("Codex Work");
  expect(text()).not.toContain("Pro");
});

it("holds the panel's height across tabs, so that choosing a tab with fewer accounts moves nothing under the pointer", () => {
  withFakeTimers();
  const { button } = mount();
  click(button);
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
  click(button);
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
  click(button);
  // The lists find they overflow only after the panel has rendered without a region.
  expect(document.querySelector('[role="tabpanel"]')!.getAttribute("tabindex")).toBe("0");
  act(() => overflowing.reports.forEach((report) => report("bottom")));
  const tabPanel = document.querySelector<HTMLElement>('[role="tabpanel"]')!;
  expect(tabPanel.querySelector('[role="region"]')?.getAttribute("tabindex")).toBe("0");
  expect(tabPanel.hasAttribute("tabindex")).toBe(false);
});

it("says so when no agent is available", () => {
  withFakeTimers();
  const { button } = mount({ claude: "unavailable", codex: "unavailable", grok: "unavailable" });
  click(button);
  expect(tabs()).toHaveLength(0);
  expect(panel()!.textContent).toContain("No agent available");
});

it("asks the daemon to refresh the quotas each time the panel opens, and not when it only changes mode", () => {
  withFakeTimers();
  const { button } = mount();
  expect(request).not.toHaveBeenCalled();
  enter(button);
  advance(OPEN_DELAY);
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "refresh_account_quotas" });
  click(button);
  expect(request).toHaveBeenCalledTimes(1);

  click(button);
  expect(panel()).toBeNull();
  click(button);
  expect(request).toHaveBeenCalledTimes(2);
});

it("opens Settings' accounts section from the panel, and closes the panel", () => {
  withFakeTimers();
  const { button } = mount();
  click(button);
  const manage = Array.from(panel()!.querySelectorAll("button")).find((b) => b.textContent === "Manage accounts…")!;
  click(manage);
  expect(onOpenSettingsAt).toHaveBeenCalledExactlyOnceWith("accounts");
  expect(panel()).toBeNull();
});

it("points the button at the panel it opens", () => {
  withFakeTimers();
  const { button } = mount();
  click(button);
  expect(panel()!.id).not.toBe("");
  expect(button.getAttribute("aria-controls")).toBe(panel()!.id);
  expect(button.hasAttribute("data-pane-toggle")).toBe(false);
});

it("holds the tooltip back after Escape returns focus to the button, until focus leaves it", () => {
  withFakeTimers();
  const { button, outside } = mount();
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
  press(button, "Enter");
  press(document.activeElement as HTMLElement, "Escape");
  advance(1000);
  expect(document.activeElement).toBe(button);
  expect(openTooltip()).toBeNull();

  act(() => outside.focus());
  act(() => button.focus());
  advance(1000);
  expect(openTooltip()?.textContent).toBe("Account quota");
});

it("closes when a modal opens without a press on the entry", async () => {
  withFakeTimers();
  const { button } = mount();
  click(button);
  const modal = document.body.appendChild(document.createElement("div"));
  modal.setAttribute("role", "dialog");
  await act(async () => {});
  expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
  expect(document.querySelector('[aria-label="Account quota"][role="dialog"]')).toBeNull();
  modal.remove();
});

it("is not a modal to the window: its F6 cycle, shortcuts and floating panes go on working beside it", () => {
  withFakeTimers();
  const { button } = mount();
  click(button);
  expect(panel()).not.toBeNull();
  expect(panel()!.getAttribute("aria-modal")).toBe("false");
  expect(document.querySelector(MODAL_OPEN)).toBeNull();
});

it("marks itself the topmost Escape taker only while open", () => {
  withFakeTimers();
  const { button } = mount();
  click(button);
  expect(document.querySelector("[data-escape-topmost]")).not.toBeNull();
  click(button);
  expect(document.querySelector("[data-escape-topmost]")).toBeNull();
});

it("does not hold the tooltip back when the panel closes with focus and pointer elsewhere", () => {
  withFakeTimers();
  const { button, outside } = mount();
  click(button);
  act(() => void outside.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })));
  expect(panel()).toBeNull();
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    outside.focus();
  });
  act(() => button.focus());
  advance(1000);
  expect(openTooltip()?.textContent).toBe("Account quota");
});

it("does not hold the tooltip back after a keyboard pin is closed by a press outside", () => {
  withFakeTimers();
  const { button, outside } = mount();
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
  press(button, "Enter");
  expect(panel()!.contains(document.activeElement)).toBe(true);
  act(() => void outside.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })));
  // The press lands on something that takes focus, so none returns to the button.
  act(() => outside.focus());
  advance(100);
  // The key is its own event, rendered before focus lands, as a real Tab is.
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })));
  act(() => button.focus());
  advance(1000);
  expect(openTooltip()?.textContent).toBe("Account quota");
});

it("closes when Tab takes focus out of a keyboard-pinned panel, leaving focus where Tab put it", () => {
  withFakeTimers();
  const { button, outside } = mount();
  act(() => button.focus());
  press(button, "Enter");
  expect(panel()!.contains(document.activeElement)).toBe(true);
  act(() => outside.focus());
  advance(100);
  expect(panel()).toBeNull();
  expect(document.activeElement).toBe(outside);
});

it("closes when F6 inside the panel moves focus to the first region", () => {
  withFakeTimers();
  function Cycle() {
    useRegionCycle({
      shown: { topbar: true, rail: true, welcome: false, sidebar: false, archive: false, terminal: false, aside: false, banner: false },
      focusTerminal: () => {},
    });
    return <div data-region="topbar"><button type="button" id="topbar-control">top</button></div>;
  }
  const { container, button } = mount();
  act(() => button.focus());
  press(button, "Enter");
  expect(panel()!.contains(document.activeElement)).toBe(true);
  // The cycle's own host is mounted beside the entry; focus in the panel is in no region.
  const host = container.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => act(() => root.unmount()));
  act(() => root.render(<Cycle />));
  press(document.activeElement as HTMLElement, "F6");
  advance(100);
  expect(document.activeElement?.id).toBe("topbar-control");
  expect(panel()).toBeNull();
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
  const fill = () => button().querySelector<HTMLElement>("span[style]");
  select(session.id);
  expect(button().getAttribute("aria-label")).toContain("Claude Code, ⁨Work⁩, 5-hour 82% remaining");
  expect(fill()?.style.width).toBe("82%");
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
  expect(fill()?.style.width).toBe("36%");
  select();
  expect(fill()).toBeNull();
  expect(button().getAttribute("aria-label")).toBe("Account quota");
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
  const fill = () => host.querySelector("span[style]");
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
