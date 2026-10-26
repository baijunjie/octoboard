// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { SessionContext } from "../protocol";
import { ArchivedSessionMetadata, SessionContextLabel, SessionRelativeTime } from "./SessionMetadata";
import { enter, mountForTooltip, openTooltip, trigger, withFakeTimers } from "./tooltipTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(element: React.ReactNode) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (node: React.ReactNode) => act(() => root.render(node));
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  render(element);
  return { container, render };
}

const context: SessionContext = { tokens: 190123, window: 475000, used_percent: 40 };

it("leads the compact-size badge with a ring of the share used, and names the exact count, limit, share used and what is left", () => {
  const { container } = mount(<SessionContextLabel context={context} />);
  expect(container.textContent).toBe("190.1K");
  const badge = container.querySelector('[aria-hidden="true"] > span:not(.sr-only)')!;
  expect(badge.getAttribute("data-variant")).toBe("bordered");
  expect(badge.firstElementChild?.matches(".progress-circle")).toBe(true);
  const indicator = container.querySelector<HTMLElement>('[role="img"]')!;
  expect(indicator.getAttribute("aria-label")).toBe(`Context ${isolate("190,123")} tokens, Limit: ${isolate("475,000")}, ${isolate("40%")} used, ${isolate("284,877")} left`);
  expect(indicator.tabIndex).toBe(-1);
  expect(indicator.querySelector(":scope > [aria-hidden]")?.getAttribute("aria-hidden")).toBe("true");
  expect(container.querySelector("button")).toBeNull();
  expect(indicator.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }))).toBe(false);
});

it("shows the compact size as a badge when the agent records no limit", () => {
  const { container } = mount(<SessionContextLabel context={{ tokens: 190123 }} />);
  expect(container.querySelector("svg")).toBeNull();
  const badge = container.querySelector('[aria-hidden="true"] > span:not(.sr-only)')!;
  expect(badge.textContent).toBe("190.1K");
  expect(badge.getAttribute("data-variant")).toBe("bordered");
});

/** Mounts the mark of a window with a share used and opens its card: the card, and the tone class
 * the ring and the card's meter each carry. */
function hoverCard(used_percent: number, tokens = 1000, window = 2000) {
  const runOut = withFakeTimers();
  const container = mountForTooltip(<SessionContextLabel context={{ tokens, window, used_percent }} />);
  const ring = container.querySelector<HTMLElement>(".progress-circle")!;
  enter(trigger(container));
  runOut();
  const card = openTooltip()!;
  const meter = card.querySelector<HTMLElement>(".meter")!;
  const tone = (element: Element) => Array.from(element.classList).find((name) => name.startsWith("context-tone--"));
  return { ringTone: tone(ring), meterTone: tone(meter), card };
}

it.each([
  [0, "accent"], [50, "accent"], [51, "warning"], [80, "warning"], [81, "danger"], [100, "danger"],
])("draws %i%% used in the %s tone, in the ring and the card's meter alike", (percent, tone) => {
  const { ringTone, meterTone } = hoverCard(percent);
  expect(ringTone).toBe(`context-tone--${tone}`);
  expect(meterTone).toBe(`context-tone--${tone}`);
});

it("lays the card out as the window's size against its limit, then the share used and what is left", () => {
  const { card } = hoverCard(91, 182400, 200000);
  expect(card.textContent).toBe(`Context window${isolate("182.4K")} / ${isolate("200K")}${isolate("91%")} used${isolate("17.6K")} left`);
});

it("leaves nothing left, not a negative count, once the size passes the limit", () => {
  const { card } = hoverCard(100, 210000, 200000);
  expect(card.textContent).toContain(`${isolate("0")} left`);
});

it("works the share out from the counts, as a whole percent, when the agent gives a limit without one", () => {
  const { container } = mount(<SessionContextLabel context={{ tokens: 100600, window: 200000 }} />);
  expect(container.querySelector('[role="img"]')?.getAttribute("aria-label")).toContain(`${isolate("50%")} used`);
  expect(container.querySelector(".progress-circle")!.classList).toContain("context-tone--accent");
});

it("draws the card of a size without a limit as its name and size alone", () => {
  const runOut = withFakeTimers();
  const container = mountForTooltip(<SessionContextLabel context={{ tokens: 190123 }} />);
  enter(trigger(container));
  runOut();
  const card = openTooltip()!;
  expect(card.textContent).toBe(`Context window${isolate("190.1K")}`);
  expect(card.querySelector(".meter")).toBeNull();
});

it("names the context size alone when the agent records no limit", () => {
  const { container } = mount(<SessionContextLabel context={{ tokens: 190123 }} />);
  expect(container.querySelector('[role="img"]')?.getAttribute("aria-label")).toBe(`Context ${isolate("190,123")} tokens`);
});

it("omits an unknown or zero context size until a positive one arrives", () => {
  const { container, render } = mount(<SessionContextLabel />);
  expect(container.textContent).toBe("");
  render(<SessionContextLabel context={{ tokens: 0 }} />);
  expect(container.innerHTML).toBe("");
  render(<SessionContextLabel context={{ tokens: 1 }} />);
  expect(container.textContent).toBe("1");
  expect(container.querySelector('[role="img"]')?.getAttribute("aria-label")).toBe(`Context ${isolate("1")} token`);
});

it("shows both absolute timestamps and never invents a missing archive timestamp", () => {
  const session = sessionOf("old", "console", "project", "A session", "archived", { started_at: 1700000000000, ended_at: 1700000600000 });
  const { container, render } = mount(<ArchivedSessionMetadata session={session} />);
  expect(Array.from(container.querySelectorAll("time")).map((time) => time.dateTime)).toEqual(["2023-11-14T22:23:20.000Z", "2023-11-14T22:13:20.000Z"]);
  expect(container.textContent).toContain("Created");
  expect(container.textContent).toContain("Archived");
  render(<ArchivedSessionMetadata session={{ ...session, ended_at: null }} />);
  expect(container.querySelectorAll("time")).toHaveLength(1);
  expect(container.textContent).not.toContain("Archived");
});

it("shows how long ago an archived session was archived, and writes out its archive and creation dates", () => {
  const runOut = withFakeTimers();
  const session = { started_at: 1700000000000, ended_at: 1700000600000 };
  vi.setSystemTime(session.ended_at + 5 * 60_000);
  const container = mountForTooltip(<SessionRelativeTime session={session} />);
  const time = container.querySelector("time")!;
  expect(time.textContent).toBe("5 minutes ago");
  expect(time.dateTime).toBe(new Date(session.ended_at).toISOString());
  const label = trigger(container).getAttribute("aria-label")!;
  expect(label).toMatch(/^Archived .+, Created .+$/);
  enter(trigger(container));
  runOut();
  const lines = Array.from(openTooltip()!.querySelectorAll("span"), (line) => line.textContent);
  expect(lines).toHaveLength(2);
  expect(lines[0]).toMatch(/^Archived /);
  expect(lines[1]).toMatch(/^Created /);
});

it("shows how long ago a session that is not archived was created, and writes out its creation date alone", () => {
  const runOut = withFakeTimers();
  const session = { started_at: 1700000000000, ended_at: null };
  vi.setSystemTime(session.started_at + 3 * 60 * 60_000);
  const container = mountForTooltip(<SessionRelativeTime session={session} />);
  expect(container.querySelector("time")!.textContent).toBe("3 hours ago");
  expect(trigger(container).getAttribute("aria-label")).toMatch(/^Created /);
  expect(trigger(container).getAttribute("aria-label")).not.toContain("Archived");
  enter(trigger(container));
  runOut();
  expect(openTooltip()!.textContent).toMatch(/^Created /);
});
