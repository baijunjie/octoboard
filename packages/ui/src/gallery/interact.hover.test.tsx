// @vitest-environment jsdom

import React, { act } from "react";
import { useHover } from "react-aria";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import type { LaunchPair } from "../agents";
import { Button } from "../components/Button";
import { change, changeViewFixture, mountRows } from "../browser/browserTestSupport";
import { ChangeList } from "../browser/ChangeList";
import { changeItems } from "../browser/changes";
import { ActivityMarker } from "../components/ActivityMarker";
import { AgentIcon } from "../components/AgentIcon";
import { AnchoredTooltip } from "../components/AnchoredTooltip";
import { SessionAgentTooltip } from "../components/SessionAgentIcon";
import { SessionContextLabel } from "../components/SessionMetadata";
import { StatusIcon } from "../components/StatusIcon";
import { TitledControl } from "../components/TitledControl";
import { leave, mountForTooltip, openTooltip } from "../components/tooltipTestSupport";
import { gitBadgeAriaLabel } from "../gitStatusLabel";
import { isolate } from "../i18n/isolate";
import { LanguageProvider } from "../i18n/react";
import type { Account, ChangeEntry } from "../protocol";
import { GitBadge } from "../sidebar/GitBadge";
import { ProjectFilterSummary } from "../sidebar/ProjectFilter";
import { RailQuotaEntry } from "../quota/RailQuotaEntry";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { gitStatusOf, sessionOf } from "./fixtures/builders";
import { createUi } from "./interact";

/** The tooltip of a Claude Code session's agent mark, drawn by the app's own `SessionAgentTooltip`: the
 * agent's name, then the account the session runs under as a badge or, given `launch`, a list of the
 * account, model and effort (as a sidebar row's and a focus card's are). */
function agentTooltip(account: string, launch?: LaunchPair): React.ReactElement {
  const stored: Account = { id: "account", agent: "claude", name: account, config_dir: "/Users/dev/.claude-work" };
  return (
    <LanguageProvider>
      <SessionAgentTooltip session={sessionOf("s-tooltip", "c", "p", "Tooltip", "idle", { account_id: stored.id })} accounts={[stored]} defaultNames={undefined} launch={launch} />
    </LanguageProvider>
  );
}

beforeEach(() => {
  vi.useFakeTimers();
  // jsdom does not load the app's styles, so supply its global tooltip delay for HeroUI.
  document.documentElement.style.setProperty("--tooltip-delay", "0.5s");
  onTestFinished(() => {
    document.documentElement.style.removeProperty("--tooltip-delay");
  });
});
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
        <AgentIcon agent="claude" tooltip={agentTooltip("Work")} />
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
            <AgentIcon agent="claude" tooltip={agentTooltip("Work")} />
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
        <AgentIcon agent="claude" tooltip={agentTooltip("Work", { model: "claude-opus-5-5", effort: "high" })} />
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
  runOut();
}

/** Runs out the timers left, for a test that ends with no pointer on anything. */
function runOut(): void {
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

it("hoverCalloutAgent puts up the agent and account of the callout's session it names", async () => {
  mountWindow();
  await settledLive(createUi(document, "en").hoverCalloutAgent("Tidy the changelog", "Claude Code", "Work"));
  expect(openTooltip()?.textContent).toBe("Claude Code, Work");
  release(document.querySelector("[role=alertdialog] li")!.children[1]);
});

it("hoverCalloutAgent fails where the tooltip is not the one it was told", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverCalloutAgent("Tidy the changelog", "Claude Code", "Home")).rejects.toThrow(
    /has no tooltip "Claude Code, Home": its last tooltip is "Claude Code, Work"/,
  );
  await vi.advanceTimersByTimeAsync(3000);
  await done;
  release(document.querySelector("[role=alertdialog] li")!.children[1]);
});

it("hoverCalloutAgent fails for an agent icon with no tooltip", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverCalloutAgent("Bare glyph", "Claude Code", "Work")).rejects.toThrow(/with an agent icon to hover/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
  expect(openTooltip()).toBeNull();
});

it("hoverSessionAgent puts up the agent and the account, model and effort listed for the card it names", async () => {
  mountWindow();
  await settledLive(createUi(document, "en").hoverSessionAgent(row("Card"), "Claude Code", { account: "Work", model: "Opus 5.5", effort: "high" }));
  expect(openTooltip()?.querySelector("dl")?.textContent).toBe("AccountWorkModelOpus 5.5Efforthigh");
  release(document.querySelector('[aria-label^="Card"] [role=presentation]')!);
});

it("hoverSessionAgent fails where the tooltip is not the one it was told", async () => {
  mountWindow();
  await expect(settledLive(createUi(document, "en").hoverSessionAgent(row("Card"), "Claude Code", { account: "Home", model: "Opus 5.5", effort: "high" }))).rejects.toThrow(
    /has no tooltip "Claude Code \| Account: Home \| Model: Opus 5.5 \| Effort: high": its last tooltip is "Claude Code \| Account: Work \| Model: Opus 5.5 \| Effort: high"/,
  );
  release(document.querySelector('[aria-label^="Card"] [role=presentation]')!);
});

it("hoverSessionAgent fails for a card with no agent mark", async () => {
  mountWindow();
  const done = expect(createUi(document, "en").hoverSessionAgent(row("Docs"), "Claude Code", { account: "Work", model: "Opus 5.5", effort: "high" })).rejects.toThrow(/with an agent mark to hover/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
  expect(openTooltip()).toBeNull();
});

/** A change list of a modified file and a deleted one, as a change row is mounted in the app: each
 * row a collection item whose status letter is `aria-hidden` and carries the anchored tooltip. */
function mountChanges(layout: "flat" | "tree"): void {
  const index = { kind: "index", worktree: "w", blob: "b" } as const;
  const deleted: ChangeEntry = { group: "unstaged", old: { state: "present", path: "gone.ts", kind: "file", source: index }, new: { state: "absent" } };
  mountRows(
    <ChangeList
      items={changeItems([change("unstaged", "kept.ts"), deleted])}
      selected={undefined}
      label="Changes"
      onOpen={() => {}}
      listRef={() => {}}
      changeView={changeViewFixture({ layout })}
    />,
  );
}

const changeRow = (name: string) => (label: string) => label.startsWith(name);
const letterOf = (name: string): Element => document.querySelector(`[aria-label*="${name}"]`)!.querySelector("[data-variant]")!;

it.each(["flat", "tree"] as const)("hoverChangeStatus puts up the word of the status of the %s row's letter", async (layout) => {
  mountChanges(layout);
  await settledLive(createUi(document, "en").hoverChangeStatus(changeRow("gone.ts"), "deleted"));
  expect(openTooltip()?.textContent).toBe("Deleted");
  release(letterOf("gone.ts"));
});

it("hoverChangeStatus fails where the tooltip that opens is another status's", async () => {
  mountChanges("flat");
  const done = expect(createUi(document, "en").hoverChangeStatus(changeRow("kept.ts"), "deleted")).rejects.toThrow(/for deleted \(Deleted\): "Modified" is up/);
  await vi.advanceTimersByTimeAsync(3000);
  await done;
  release(letterOf("kept.ts"));
});

it("hoverChangeStatus fails for a row that is not there", async () => {
  mountChanges("flat");
  const done = expect(createUi(document, "en").hoverChangeStatus(changeRow("nothing.ts"), "modified")).rejects.toThrow(/No change row/);
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

/** The rail's quota entry over a daemon with Claude Code available, the panel a resting pointer floats in. */
function mountQuotaEntry(): void {
  // jsdom has no Web Animations, which the popover's exit waits for.
  HTMLElement.prototype.animate = vi.fn(() => ({ cancel: vi.fn(), finished: Promise.resolve(), onfinish: null })) as unknown as typeof HTMLElement.prototype.animate;
  onTestFinished(() => void delete (HTMLElement.prototype as Partial<HTMLElement>).animate);
  const store = createStateStore({ agentAvailability: new Map([["claude", { agent: "claude", availability: "available" }]]) });
  const daemon = { store, request: () => Promise.resolve({}) } as unknown as Daemon;
  mountForTooltip(
    <DaemonProvider value={daemon}>
      <RailQuotaEntry onOpenSettingsAt={() => {}} />
    </DaemonProvider>,
  );
}

it("hoverPanel floats in the panel of the control it names", async () => {
  mountQuotaEntry();
  await settledLive(createUi(document, "en").hoverPanel("Account quota", "Account quota"));
  expect(document.querySelector('[role="dialog"][aria-label="Account quota"]')).not.toBeNull();
  release(document.querySelector('[aria-label="Account quota"]')!);
});

it("hoverPanel fails for a control whose hover floats in no such panel", async () => {
  mountQuotaEntry();
  const done = expect(createUi(document, "en").hoverPanel("Account quota", "Nothing")).rejects.toThrow(/floated in no dialog matching Nothing/);
  await vi.advanceTimersByTimeAsync(8000);
  await done;
  release(document.querySelector('[aria-label="Account quota"]')!);
});

it("hoverPanel fails for a control that is not there", async () => {
  mountQuotaEntry();
  const done = expect(createUi(document, "en").hoverPanel("Nothing", "Account quota")).rejects.toThrow(/Nothing to hover/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
});

const branchStatus = gitStatusOf("p-1", { ahead: 3, behind: 2 });
const sentenceOf = (status: typeof branchStatus) => gitBadgeAriaLabel("en", createUi(document, "en").t, status);
/** The sentence as the badge's tooltip shows it: references are chips there, so no isolates. */
const tooltipSentenceOf = (status: typeof branchStatus) => sentenceOf(status).replace(/[\u2068\u2069]/g, "");

/** The badge as focus mode's header draws it: in the sidebar, and named by itself. */
function mountFocusBadge(status: typeof branchStatus): void {
  mountForTooltip(
    <div data-region="sidebar">
      <GitBadge status={status} />
    </div>,
  );
}

/** A project row as the sidebar draws it: named with the badge's sentence folded in, the badge (which
 * names nothing itself there) and then the activity marker. */
function mountProjectRow(status: typeof branchStatus): void {
  mountForTooltip(
    <div role="button" aria-label={`Website, ${sentenceOf(status)}`}>
      <span>Website</span>
      <GitBadge status={status} decorative />
      <ActivityMarker activity="running" />
    </div>,
  );
}

it("hoverGitBadge puts the pointer on the line of a row's badge and puts up the badge's sentence", async () => {
  mountProjectRow(branchStatus);
  const badge = document.querySelector("[role=button] [role=presentation]")!;
  const seen: Element[] = [];
  document.addEventListener("pointerover", (event) => seen.push(event.target as Element), { capture: true });
  await settledLive(createUi(document, "en").hoverGitBadge(branchStatus, "line", (name) => name.startsWith("Website")));
  expect(openTooltip()?.textContent).toBe(tooltipSentenceOf(branchStatus));
  // The row's badge holds its parts in a box of their own, which is not itself one of them.
  expect(seen[0]).toBe(badge.querySelector("[dir=ltr]"));
  release(badge);
});

it.each(["idle", "checking", "pulling", "pushing"] as const)("hoverGitBadge puts the pointer on the glyph or line for %s, as asked", async (activity) => {
  const status = { ...branchStatus, activity };
  const targets: Element[] = [];
  for (const over of ["glyph", "line"] as const) {
    mountFocusBadge(status);
    const badge = document.querySelector("[role=img]")!;
    document.addEventListener("pointerover", (event) => targets.push(event.target as Element), { capture: true, once: true });
    await settledLive(createUi(document, "en").hoverGitBadge(status, over));
    expect(openTooltip()?.textContent).toBe(tooltipSentenceOf(status));
    release(badge);
    document.body.replaceChildren();
  }
  expect(targets.map((target) => target.tagName.toLowerCase())).toEqual(["svg", "span"]);
  expect(targets[1]!.textContent).toBe("main");
  expect(targets[1]!.getAttribute("dir")).toBe("ltr");
  expect(targets[0]!.classList.contains(activity === "idle" ? "lucide-git-branch" : "lucide-refresh-cw")).toBe(true);
});

it.each(["checking", "pulling", "pushing"] as const)("hoverGitBadge takes the badge of the %s glyph the status draws", async (activity) => {
  const status = gitStatusOf("p-1", { activity, error: "fetch failed" });
  mountFocusBadge(status);
  await settledLive(createUi(document, "en").hoverGitBadge(status, "glyph"));
  expect(openTooltip()?.textContent).toBe(tooltipSentenceOf(status));
  release(document.querySelector("[role=img]")!);
});

it("hoverGitBadge fails for a line the badge does not have", async () => {
  const lone = gitStatusOf("p-1", { branch: null, error: "fetch failed" });
  mountFocusBadge(lone);
  await expect(createUi(document, "en").hoverGitBadge(lone, "line")).rejects.toThrow(/no line to hover/);
  expect(openTooltip()).toBeNull();
});

it("hoverGitBadge fails for a header badge that says so in text and has no tooltip", async () => {
  const empty = gitStatusOf("p-1", { branch: null, upstream: null });
  mountFocusBadge(empty);
  await expect(createUi(document, "en").hoverGitBadge(empty, "glyph")).rejects.toThrow(/carries no tooltip/);
  runOut();
});

it("hoverGitBadge fails for a row that does not name the sentence of the status it was given", async () => {
  mountProjectRow(branchStatus);
  const other = gitStatusOf("p-1", { ahead: 9 });
  await expect(createUi(document, "en").hoverGitBadge(other, "glyph", (name) => name.startsWith("Website"))).rejects.toThrow(
    /does not name the badge's sentence/,
  );
  runOut();
});

it("hoverGitBadge fails where there is no such row", async () => {
  mountProjectRow(branchStatus);
  const done = expect(createUi(document, "en").hoverGitBadge(branchStatus, "glyph", (name) => name.startsWith("Nothing"))).rejects.toThrow(/No sidebar row/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
});

it("hoverGitBadge says a row's badge has no tooltip rather than hover the activity marker beside it", async () => {
  const empty = gitStatusOf("p-1", { branch: null, upstream: null });
  mountProjectRow(empty);
  await expect(createUi(document, "en").hoverGitBadge(empty, "glyph", (name) => name.startsWith("Website"))).rejects.toThrow(
    /has a badge that carries no tooltip/,
  );
  runOut();
});

it.each(["flat", "tree"] as const)("pressChangeStatus dismisses the %s tooltip without a pointerleave", async (layout) => {
  mountChanges(layout);
  const ui = createUi(document, "en");
  await settledLive(ui.hoverChangeStatus(changeRow("gone.ts"), "deleted"));
  const events: string[] = [];
  const chip = letterOf("gone.ts");
  for (const event of ["pointerdown", "pointerup", "pointerleave", "click"]) chip.addEventListener(event, () => events.push(event));
  await settledLive(ui.pressChangeStatus(changeRow("gone.ts")));
  expect(events).toEqual(["pointerdown", "pointerup", "click"]);
  expect(document.querySelector("[data-anchored-tooltip]")).toBeNull();
  release(chip);
});

/** A session card as focus mode draws it, named by its title, with its context size among the
 * metadata. A card without one draws no count, as the real one does. */
function mountCard(context?: { tokens: number; window?: number; used_percent?: number }): void {
  mountForTooltip(
    <div role="button" aria-label="Add idempotency keys, Claude Code, Working">
      <StatusIcon status="working" decorative />
      <SessionContextLabel context={context} />
    </div>,
  );
}

it("hoverSessionContext puts up the tooltip of the card's context size", async () => {
  mountCard({ tokens: 1500, window: 475000, used_percent: 40 });
  await settledLive(createUi(document, "en").hoverSessionContext((name) => name.startsWith("Add idempotency keys")));
  expect(openTooltip()!.textContent).toBe(`Context window${isolate("1.5K")} / ${isolate("475K")}${isolate("40%")} used${isolate("473.5K")} left`);
  release(document.querySelector("[role=img]")!);
});

it("hoverSessionContext fails for a card whose context size is hidden", async () => {
  mountCard({ tokens: 0 });
  const done = expect(createUi(document, "en").hoverSessionContext((name) => name.startsWith("Add idempotency keys"))).rejects.toThrow(/with a context size to hover/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
  runOut();
});

/** The filter's conditions on a heading line, which clips them where `clipped` and fits them
 * otherwise: jsdom lays nothing out, so the box the strip measures is given its width. */
function mountFilter(clipped: boolean): void {
  for (const [name, width] of [["scrollWidth", 400], ["clientWidth", clipped ? 100 : 400]] as const) {
    const own = Object.getOwnPropertyDescriptor(Element.prototype, name)!;
    Object.defineProperty(Element.prototype, name, { configurable: true, get: () => width });
    onTestFinished(() => void Object.defineProperty(Element.prototype, name, own));
  }
  mountForTooltip(<ProjectFilterSummary filter={{ keyword: "site", tags: ["docs", "customer-facing"] }} />);
}

it("hoverFilterConditions puts up the tokens of the clipped filter", async () => {
  mountFilter(true);
  await settledLive(createUi(document, "en").hoverFilterConditions());
  expect(Array.from(openTooltip()!.querySelectorAll("[data-list-motion]"), (chip) => chip.textContent)).toEqual(["site", "docs", "customer-facing"]);
  release(document.querySelector("[role=group]")!.parentElement!);
});

it("hoverFilterConditions fails for a filter that fits the heading's line", async () => {
  mountFilter(false);
  const done = expect(createUi(document, "en").hoverFilterConditions()).rejects.toThrow(/fit the heading's line/);
  await vi.advanceTimersByTimeAsync(9000);
  await done;
  runOut();
});

/** A tab whose tooltip is the anchored shape, which the tab's own hover opens, beside one with none. */
function TabWithTip(): React.ReactElement {
  const ref = React.useRef<HTMLDivElement>(null);
  const { hoverProps, isHovered } = useHover({});
  return (
    <div role="tab" aria-label="Codex (not installed)" ref={ref} {...hoverProps}>
      Codex
      <AnchoredTooltip tooltip="Codex is not installed." anchor={ref} hovered={isHovered} />
    </div>
  );
}

function mountTabs(): void {
  mountForTooltip(
    <div role="tablist">
      <div role="tab" aria-label="Claude Code">Claude Code</div>
      <TabWithTip />
    </div>,
  );
}

it("hoverAnchored puts up the tooltip of the control it names", async () => {
  mountTabs();
  await settledLive(createUi(document, "en").hoverAnchored("Codex (not installed)", "Codex is not installed."));
  expect(openTooltip()?.textContent).toBe("Codex is not installed.");
  release(document.querySelector('[aria-label="Codex (not installed)"]')!);
});

it("hoverAnchored fails for a control with no tooltip", async () => {
  mountTabs();
  const done = expect(createUi(document, "en").hoverAnchored("Claude Code", "Codex is not installed.")).rejects.toThrow(/has no tooltip "Codex is not installed\.": its anchored tooltip is none/);
  await vi.advanceTimersByTimeAsync(9000);
  await done;
  expect(openTooltip()).toBeNull();
});

it("hoverAnchored fails where the tooltip that opens is not the one named", async () => {
  mountTabs();
  const done = expect(createUi(document, "en").hoverAnchored("Codex (not installed)", "Grok is not installed.")).rejects.toThrow(
    /has no tooltip "Grok is not installed\.": its anchored tooltip is "Codex is not installed\."/,
  );
  await vi.advanceTimersByTimeAsync(3000);
  await done;
  release(document.querySelector('[aria-label="Codex (not installed)"]')!);
});
