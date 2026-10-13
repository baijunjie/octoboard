// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { AgentIcon } from "../components/AgentIcon";
import { StatusIcon } from "../components/StatusIcon";
import { leave, mountForTooltip, openTooltip } from "../components/tooltipTestSupport";
import { createUi } from "./interact";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/** The placements the hover steps reach, built from the real glyph: the top bar, a session row with
 * a status glyph and an agent mark that has a tooltip of its own, a row with a status glyph only,
 * and a row like focus mode's card, whose glyph carries no tooltip and whose agent mark does. */
function mountWindow(): void {
  mountForTooltip(
    <>
      <header data-region="topbar">
        <StatusIcon status="archived" withTooltip />
      </header>
      <div role="button" aria-label="Fix the layout, Claude Code, Working">
        <StatusIcon status="working" decorative withTooltip />
        <AgentIcon agent="claude" tooltip="Claude Code (Work)" />
      </div>
      <div role="button" aria-label="Write the migration, Claude Code, Waiting for you">
        <StatusIcon status="waiting_user" decorative withTooltip />
      </div>
      <div role="button" aria-label="Card, Claude Code, Idle">
        <div>
          <StatusIcon status="idle" decorative />
        </div>
        <AgentIcon agent="claude" tooltip="Claude Code (Work)" />
      </div>
    </>,
  );
}

/** Runs `hover` to the end, advancing the timers it waits on. */
async function settled(hover: Promise<void>): Promise<void> {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(3000);
    await hover;
  });
}

/** Takes the pointer off what is hovered and runs out the timers left, as the tests of the glyph do,
 * so that the test shows none still pending. */
function release(target: Element): void {
  leave(target);
  act(() => void vi.runOnlyPendingTimers());
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
}

/** The glyph a row's step hovers: the row's first child, found from the row's name. */
const glyphOf = (title: string): Element => document.querySelector(`[aria-label^="${title}"]`)!.firstElementChild!;

const row = (title: string) => (name: string) => name.startsWith(title);

it("hoverTopBarStatus puts the tooltip of the top bar's glyph up", async () => {
  mountWindow();
  await settled(createUi(document, "en").hoverTopBarStatus());
  expect(openTooltip()?.textContent).toBe("Archived");
  release(document.querySelector("[data-region=topbar] [role=img]")!);
});

it("hoverRowStatus puts up the status of the row it names", async () => {
  mountWindow();
  await settled(createUi(document, "en").hoverRowStatus(row("Write the migration")));
  expect(openTooltip()?.textContent).toBe("Waiting for you");
  release(glyphOf("Write the migration"));
});

it("hoverRowStatus takes the glyph, not the agent mark's tooltip beside it", async () => {
  mountWindow();
  await settled(createUi(document, "en").hoverRowStatus(row("Fix the layout")));
  expect(openTooltip()?.textContent).toBe("Working");
  release(glyphOf("Fix the layout"));
});

it("hoverRowStatus fails for a row whose glyph has no tooltip rather than hover the agent mark", async () => {
  mountWindow();
  await expect(createUi(document, "en").hoverRowStatus(row("Card"))).rejects.toThrow(/has no tooltip on its status glyph/);
  expect(openTooltip()).toBeNull();
});

it("hoverRowStatus fails for a row that is not there", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverRowStatus("Nothing")).rejects.toThrow(/No session row/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
});
