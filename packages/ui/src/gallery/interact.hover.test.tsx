// @vitest-environment jsdom
import { Button } from "@heroui/react";
import { act } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { ActivityMarker } from "../components/ActivityMarker";
import { AgentIcon } from "../components/AgentIcon";
import { StatusIcon } from "../components/StatusIcon";
import { TitledControl } from "../components/TitledControl";
import { leave, mountForTooltip, openTooltip } from "../components/tooltipTestSupport";
import { createUi } from "./interact";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/** The placements the hover steps reach, built from the real glyph: the top bar, a session row with
 * a status glyph and an agent mark that has a tooltip of its own, a row with a status glyph only, a
 * row like focus mode's card, whose glyph carries no tooltip and whose agent mark does, and the
 * activity marker's two hosts: a row with a status glyph before the marker, as a project row has its
 * git badge, and a chip with the marker alone. "Docs" is named as a row with an activity but draws no
 * marker, as an expanded project row does, which is what the step's failure case needs. A confirmation
 * dialog's callout lists sessions, one with a tooltip on its glyph and on its agent icon and one
 * with neither; unlike the rows above, that glyph is the named `role="img"` rather than a
 * presentational one. Two rail-style avatars carry a tooltip of their own: one opens on hover, one
 * only on focus, as while the hidden sidebar can float in. */
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
      <div role="button" aria-label="Website, a session is running">
        <span>Website</span>
        <StatusIcon status="idle" decorative withTooltip />
        <ActivityMarker activity="running" />
      </div>
      <div role="button" aria-label="Hub 2, a session is working">
        <span>Hub 2</span>
        <ActivityMarker activity="working" />
      </div>
      <div role="button" aria-label="Docs, a session is running">
        <span>Docs</span>
      </div>
      <div role="alertdialog">
        <ul>
          <li>
            <StatusIcon status="interrupted" withTooltip />
            <AgentIcon agent="claude" tooltip="Work" />
            <span className="sr-only">Claude Code</span>
            <span>Tidy the changelog</span>
          </li>
          <li>
            <StatusIcon status="archived" />
            <AgentIcon agent="claude" />
            <span className="sr-only">Claude Code</span>
            <span>Bare glyph</span>
          </li>
        </ul>
      </div>
      <TitledControl title="Operations, a session is working" placement="end">
        <Button isIconOnly aria-label="Operations, a session is working" />
      </TitledControl>
      <TitledControl title="Billing" placement="end" focusOnly>
        <Button isIconOnly aria-label="Billing" />
      </TitledControl>
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

/** Runs `hover` to the end without `act`, so that what the pointer opened is on the page while the
 * step is still running, as in a browser: a step that looks at the tooltip it opened needs that. */
async function settledLive(hover: Promise<void>): Promise<void> {
  const done = hover.then(
    () => undefined,
    (error: unknown) => error,
  );
  await vi.advanceTimersByTimeAsync(3000);
  const error = await done;
  if (error) throw error;
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

it("hoverActivityMarker puts up the activity of the chip it names", async () => {
  mountWindow();
  await settledLive(createUi(document, "en").hoverActivityMarker((name) => name.startsWith("Hub 2")));
  expect(openTooltip()?.textContent).toBe("A session is working");
  release(document.querySelector('[aria-label^="Hub 2"] [role=presentation]')!);
});

it("hoverActivityMarker takes the marker, not the trigger before it", async () => {
  mountWindow();
  await settledLive(createUi(document, "en").hoverActivityMarker((name) => name.startsWith("Website")));
  expect(openTooltip()?.textContent).toBe("A session is running");
  release(Array.from(document.querySelectorAll('[aria-label^="Website"] [role=presentation]')).at(-1)!);
});

it("hoverActivityMarker fails for a control with no marker rather than hover another tooltip", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverActivityMarker((name) => name.startsWith("Docs"))).rejects.toThrow(/No row or chip/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
  expect(openTooltip()).toBeNull();
});

it("hoverActivityMarker fails where the last tooltip is not an activity's", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverActivityMarker((name) => name.startsWith("Write the migration"))).rejects.toThrow(
    /has no activity marker.*Waiting for you/,
  );
  await vi.advanceTimersByTimeAsync(3000);
  await done;
  release(glyphOf("Write the migration"));
});

it("hoverCalloutStatus puts up the status of the callout's session it names", async () => {
  mountWindow();
  await settledLive(createUi(document, "en").hoverCalloutStatus("Tidy the changelog"));
  expect(openTooltip()?.textContent).toBe("Interrupted");
  release(document.querySelector("[role=alertdialog] li")!.firstElementChild!);
});

it("hoverCalloutStatus fails for a session whose glyph has no tooltip", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverCalloutStatus("Bare glyph")).rejects.toThrow(/has no tooltip: its last tooltip is none/);
  await vi.advanceTimersByTimeAsync(3000);
  await done;
  expect(openTooltip()).toBeNull();
});

it("hoverCalloutStatus fails for a session that is not listed", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverCalloutStatus("Nothing")).rejects.toThrow(/callout list/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
  expect(openTooltip()).toBeNull();
});

it("hoverCalloutAgent puts up the account of the callout's session it names", async () => {
  mountWindow();
  await settledLive(createUi(document, "en").hoverCalloutAgent("Tidy the changelog", "Work"));
  expect(openTooltip()?.textContent).toBe("Work");
  release(document.querySelector("[role=alertdialog] li")!.children[1]);
});

it("hoverCalloutAgent fails where the tooltip is not the one it was told", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverCalloutAgent("Tidy the changelog", "Home")).rejects.toThrow(
    /has no tooltip "Home": its last tooltip is "Work"/,
  );
  await vi.advanceTimersByTimeAsync(3000);
  await done;
  release(document.querySelector("[role=alertdialog] li")!.children[1]);
});

it("hoverCalloutAgent fails for an agent icon with no tooltip", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverCalloutAgent("Bare glyph", "Work")).rejects.toThrow(/with an agent icon to hover/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
  expect(openTooltip()).toBeNull();
});

it("hoverTitledControl puts up the tooltip of the control it names", async () => {
  mountWindow();
  await settledLive(createUi(document, "en").hoverTitledControl((name) => name.startsWith("Operations")));
  expect(openTooltip()?.textContent).toBe("Operations, a session is working");
  release(document.querySelector('[aria-label^="Operations"]')!);
});

it("hoverTitledControl fails for a control whose tooltip stays shut under the pointer", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverTitledControl("Billing")).rejects.toThrow(/has no tooltip of its name: its last tooltip is none/);
  await vi.advanceTimersByTimeAsync(3000);
  await done;
  expect(openTooltip()).toBeNull();
});

it("hoverTitledControl fails for a control that is not there", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverTitledControl("Nothing")).rejects.toThrow(/Nothing to hover/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
});
