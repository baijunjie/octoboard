// @vitest-environment jsdom
import React, { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { enter, leave, mountForTooltip, openTooltip, withFakeTimers } from "../components/tooltipTestSupport";
import { consoleOf, gitStatusOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import { PlatformProvider } from "../platform/react";
import type { GitStatus } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ProjectFocusView } from "./FocusView";
import { Sidebar } from "./Sidebar";
import type { SidebarHandlers } from "./types";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Reduced motion keeps the sidebar's slide animation, which jsdom has no `animate` for, from
// playing; the marquee test turns it off for itself.
let reducedMotion = true;
window.matchMedia = ((query: string) => ({ matches: reducedMotion, media: query, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;

const main = consoleOf("c-1", "Main");
const web = projectOf("p-web", main.id, "Website");
const sessions = [sessionOf("web-free", main.id, web.id, "Tidy the changelog", "idle")];
const sidebarWidth = { width: 280, min: 200, max: 480, setWidth: () => {}, persist: () => {}, reset: () => {} };

/** Where the badge sits: on a sidebar project row, whose own name folds the badge's in, or in
 * focus mode's header, where the badge names itself. */
interface Placement {
  name: string;
  /** Mounts the badge for `status` and returns its box, the tooltip's trigger, found by its state glyph. */
  mount: (status: GitStatus, glyph: string) => HTMLElement;
  /** The element carrying the badge's accessible name, whichever that is. */
  named: (container: HTMLElement) => Element | null;
}

function daemonWith(status: GitStatus): Daemon {
  return {
    store: createStateStore({ gitStatuses: new Map([[web.id, status]]) }),
    request: vi.fn(),
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
}

const handlers = () =>
  ({ onFocus: vi.fn(), onOpenDialog: vi.fn(), onOpenArchive: vi.fn(), onSelectSession: vi.fn(), onSetPinned: vi.fn(), onSwitchConsoleSession: vi.fn() }) as unknown as SidebarHandlers;

function mountOver(status: GitStatus, view: React.ReactElement): HTMLElement {
  return mountForTooltip(
    <PlatformProvider value={{ kind: "browser" }}>
      <DaemonProvider value={daemonWith(status)}>{view}</DaemonProvider>
    </PlatformProvider>,
  );
}

/** The badge's trigger: the box around the state glyph that gives way when the row is squeezed. */
const badgeOf = (container: HTMLElement, glyph: string): HTMLElement => container.querySelector(`svg.lucide-${glyph}`)!.closest<HTMLElement>(".overflow-hidden")!;

/** The shared line the branch and the counts run in, which fades and runs as a marquee together. */
const lineOf = (badge: HTMLElement): HTMLElement => badge.querySelector<HTMLElement>("[dir=ltr]")!;

/** What the badge lays out: its own children, or those of the `aria-hidden` box inside it that a
 * badge on a row wraps them in without a box of its own. */
function partsOf(badge: HTMLElement): Element[] {
  const only = badge.children.length === 1 ? badge.firstElementChild! : null;
  return Array.from(only?.classList.contains("contents") ? only.children : badge.children);
}

const PLACEMENTS: Placement[] = [
  {
    name: "a sidebar project row",
    mount: (status, glyph) =>
      badgeOf(mountOver(status, <Sidebar {...handlers()} projects={[web]} sessions={sessions} shownConsole={main} open sidebarWidth={sidebarWidth} />), glyph),
    named: (container) => container.querySelector("[role=button][data-marquee-scope]"),
  },
  {
    name: "focus mode's header",
    mount: (status, glyph) =>
      badgeOf(mountOver(status, <ProjectFocusView handlers={handlers()} console={main} project={web} sessions={sessions} owners={new Map()} />), glyph),
    named: (container) => container.querySelector("[role=img]"),
  },
];

const BRANCH = isolate("main");
// The sentences are spelled out although `gitStatusLabel.test.ts` pins the same wording: this pins
// the text the badge's tooltip shows end to end, glyph and all.
const ALL_STATES: Array<[string, Partial<GitStatus>, string, string]> = [
  ["checking", { activity: "checking" }, "refresh-cw", `Checking branch ${BRANCH}`],
  ["fast-forwarding", { activity: "syncing", behind: 2 }, "arrow-down-to-line", `Fast-forwarding branch ${BRANCH}, 2 commits behind`],
  ["on a branch", {}, "git-branch", `Branch ${BRANCH}`],
  ["detached", { branch: "a1b2c3d", detached: true }, "git-commit-horizontal", `Detached at ${isolate("a1b2c3d")}`],
  ["ahead", { ahead: 1 }, "git-branch", `Branch ${BRANCH}, 1 commit ahead`],
  ["behind", { behind: 4 }, "git-branch", `Branch ${BRANCH}, 4 commits behind`],
  ["failed", { ahead: 3, behind: 2, error: "fetch failed" }, "triangle-alert", `Branch ${BRANCH}, 3 commits ahead, 2 commits behind, Git error: ${isolate("fetch failed")}`],
];

/** Puts the pointer on `element`, runs the delay out and reads which tooltip is up; then takes it
 * off and shows nothing is left counting. */
function hoverAndRead(element: Element): string | undefined {
  const settle = withFakeTimers();
  enter(element);
  expect(openTooltip()).toBeNull();
  settle();
  expect(document.querySelectorAll('[role="tooltip"]').length).toBeLessThanOrEqual(1);
  const text = openTooltip()?.textContent ?? undefined;
  leave(element);
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
  return text;
}

/** jsdom lays nothing out, so a label is cut at its end by stubbing the overflow it measures
 * itself by (200 px of text in a box 0 wide) and by backing its scroll position with a field of
 * its own, every write to which is kept. */
function stubOverflow(): { positions: WeakMap<HTMLElement, number>; writes: (element: HTMLElement) => number[] } {
  for (const [name, value] of [
    ["scrollWidth", 200],
    ["clientWidth", 0],
  ] as const) {
    const own = Object.getOwnPropertyDescriptor(Element.prototype, name)!;
    Object.defineProperty(Element.prototype, name, { configurable: true, get: () => value });
    onTestFinished(() => void Object.defineProperty(Element.prototype, name, own));
  }
  const positions = new WeakMap<HTMLElement, number>();
  const written = new WeakMap<HTMLElement, number[]>();
  const ownScrollLeft = Object.getOwnPropertyDescriptor(Element.prototype, "scrollLeft")!;
  Object.defineProperty(Element.prototype, "scrollLeft", {
    configurable: true,
    get(this: HTMLElement) {
      return positions.get(this) ?? 0;
    },
    set(this: HTMLElement, value: number) {
      positions.set(this, value);
      written.set(this, [...(written.get(this) ?? []), value]);
    },
  });
  onTestFinished(() => void Object.defineProperty(Element.prototype, "scrollLeft", ownScrollLeft));
  return { positions, writes: (element) => written.get(element) ?? [] };
}

const STATES = ALL_STATES.filter(([name]) => name !== "failed");
const FAILED = ALL_STATES.find(([name]) => name === "failed")!;

// The text and the glyph do not vary with where the badge sits, so the states run once, on the
// sidebar row; each placement is held below to one full sentence, over the glyph and over the
// shared text line alike.
it.each(STATES)("says its whole state in one tooltip: %s", (_, extra, glyph, sentence) => {
  const badge = PLACEMENTS[0].mount(gitStatusOf(web.id, extra), glyph);
  expect(hoverAndRead(badge.querySelector(`svg.lucide-${glyph}`)!)).toBe(sentence);
});

for (const placement of PLACEMENTS) {
  it(`gives the glyph, the branch and the counts the one sentence, the error's and the branch's alone gone, on ${placement.name}`, () => {
    stubOverflow();
    const [, extra, glyph, sentence] = FAILED;
    const badge = placement.mount(gitStatusOf(web.id, { ...extra, branch: "feature/long-branch-name" }), glyph);
    const line = lineOf(badge);
    expect(line.textContent).toBe("feature/long-branch-name32");
    const full = sentence.replace(BRANCH, isolate("feature/long-branch-name"));
    expect(hoverAndRead(badge.querySelector(`svg.lucide-${glyph}`)!)).toBe(full);
    expect(hoverAndRead(line)).toBe(full);
  });

  it(`keeps the badge the part that gives way, its shared text line the one thing inside it that shrinks, on ${placement.name}`, () => {
    const badge = placement.mount(gitStatusOf(web.id, { ahead: 1, behind: 1 }), "git-branch");
    expect(badge.classList.contains("min-w-0")).toBe(true);
    expect(badge.classList.contains("overflow-hidden")).toBe(true);
    expect(badge.classList.contains("flex")).toBe(true);
    expect(badge.getAttribute("tabindex")).toBe("-1");
    const shrinking = partsOf(badge).filter((child) => !child.classList.contains("shrink-0"));
    expect(shrinking).toEqual([lineOf(badge)]);
    expect(lineOf(badge).classList.contains("min-w-0")).toBe(true);
    // The branch and both counts share that one line.
    expect(lineOf(badge).textContent).toBe("main11");
  });

  it(`shows no tooltip for the state whose visible text already says it, on ${placement.name}`, () => {
    const badge = placement.mount(gitStatusOf(web.id, { branch: null, upstream: null }), "git-branch");
    expect(badge.textContent).toBe("No commits yet");
    expect(hoverAndRead(badge)).toBeUndefined();
    expect(hoverAndRead(badge.querySelector("svg.lucide-git-branch")!)).toBeUndefined();
    expect(badge.getAttribute("tabindex")).toBeNull();
  });
}

it("names the badge in focus mode's header itself, once, with the sentence its tooltip carries", () => {
  const [, header] = PLACEMENTS;
  const badge = header.mount(gitStatusOf(web.id, { ahead: 1 }), "git-branch");
  const named = header.named(document.body);
  expect(named).toBe(badge);
  expect(named?.getAttribute("aria-label")).toBe(`Branch ${BRANCH}, 1 commit ahead`);
  expect(document.body.querySelectorAll("[role=img]")).toHaveLength(1);
  expect(badge.querySelector("[aria-label]")).toBeNull();
  expect(lineOf(badge).closest("[aria-hidden=true]")).toBeNull();
});

it("leaves the badge on a sidebar row unnamed, the row carrying its sentence in its own name", () => {
  const [row] = PLACEMENTS;
  const badge = row.mount(gitStatusOf(web.id, { ahead: 1 }), "git-branch");
  expect(badge.hasAttribute("aria-label")).toBe(false);
  expect(badge.getAttribute("role")).toBe("presentation");
  // What it shows stays out of the accessible tree, as the whole badge was, the row saying it all.
  expect(lineOf(badge).closest("[aria-hidden=true]")).not.toBeNull();
  expect(badge.querySelector("svg.lucide-git-branch")!.closest("[aria-hidden=true]")).not.toBeNull();
  expect(document.body.querySelector("[role=img]")).toBeNull();
  expect(row.named(document.body)?.getAttribute("aria-label")).toBe(`${isolate("Website")}, a session is running, Branch ${BRANCH}, 1 commit ahead`);
});

it("keeps the shared text line running as a marquee while the pointer is over the row", () => {
  reducedMotion = false;
  onTestFinished(() => void (reducedMotion = true));
  const settle = withFakeTimers();
  const { positions, writes } = stubOverflow();

  const [row] = PLACEMENTS;
  const badge = row.mount(gitStatusOf(web.id, { ahead: 1 }), "git-branch");
  const treeRow = row.named(document.body)!;
  const line = lineOf(badge);
  // Cut at its end, the line rests at its start (0); it is the marquee that carries it past that.
  const rested = writes(line).length;
  expect(positions.get(line) ?? 0).toBe(0);
  act(() => void treeRow.dispatchEvent(new MouseEvent("pointerenter")));
  act(() => void vi.advanceTimersByTime(3000));
  expect(Math.max(...writes(line).slice(rested))).toBeGreaterThan(0);

  act(() => void treeRow.dispatchEvent(new MouseEvent("pointerleave")));
  act(() => void vi.advanceTimersByTime(5000));
  // The line measures itself a frame after the last scroll, and that measure can ask for one more
  // while it settles a clipped edge; two runs leave nothing pending.
  settle();
  settle();
  expect(positions.get(line)).toBe(0);
  expect(vi.getTimerCount()).toBe(0);
});
