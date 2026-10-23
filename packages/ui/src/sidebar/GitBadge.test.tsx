// @vitest-environment jsdom
import React from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { expectIndependentMarquees, stubMarquee } from "../components/marqueeTestSupport";
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

// The clock's own timer would count as pending work in the marquee checks, which these tests do not use it for.
vi.mock("../useNow", () => ({ useNow: () => 0 }));

// Reduced motion keeps the sidebar's slide animation, which jsdom has no `animate` for, from
// playing; the marquee test turns it off for itself.
window.matchMedia = ((query: string) => ({ matches: true, media: query, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;

const main = consoleOf("c-1", "Main");
const web = projectOf("p-web", main.id, "Website");
const sessions = [sessionOf("web-free", main.id, web.id, "Tidy the changelog", "idle")];
const sidebarWidth = { width: 280, min: 280, max: 480, setWidth: () => {}, persist: () => {}, reset: () => {} };

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
    store: createStateStore({ gitStatuses: new Map([[web.id, status]]), sessions: new Map(sessions.map((session) => [session.id, session])) }),
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
const badgeOf = (container: HTMLElement, glyph: string): HTMLElement => container.querySelector(`svg.lucide-${glyph}`)!.closest<HTMLElement>(".grid")!;

/** The branch name alone, which fades and runs as a marquee. */
const lineOf = (badge: HTMLElement): HTMLElement => badge.querySelector<HTMLElement>(".overflow-hidden")!;

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
      badgeOf(mountOver(status, <Sidebar {...handlers()} projects={[web]} shownConsole={main} open sidebarWidth={sidebarWidth} />), glyph),
    named: (container) => container.querySelector("div[role=button]"),
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
  ["pulling", { activity: "pulling", behind: 2 }, "refresh-cw", `Pulling branch ${BRANCH}, 2 commits behind`],
  ["pushing", { activity: "pushing", ahead: 3 }, "refresh-cw", `Pushing branch ${BRANCH}, 3 commits ahead`],
  ["on a branch", {}, "git-branch", `Branch ${BRANCH}`],
  ["detached", { branch: "a1b2c3d", detached: true }, "git-commit-horizontal", `Detached at ${isolate("a1b2c3d")}`],
  ["ahead", { ahead: 1 }, "git-branch", `Branch ${BRANCH}, 1 commit ahead`],
  ["behind", { behind: 4 }, "git-branch", `Branch ${BRANCH}, 4 commits behind`],
  ["failed", { ahead: 3, behind: 2, error: "fetch failed" }, "circle-alert", `Branch ${BRANCH}, 3 commits ahead, 2 commits behind, Git error: ${isolate("fetch failed")}`],
];

/** Puts the pointer on `element`, runs the delay out and reads which tooltip is up; then takes it
 * off and shows nothing is left counting. */
function hoverAndRead(element: Element): string | undefined {
  const settle = withFakeTimers();
  enter(element);
  expect(openTooltip()).toBeNull();
  settle();
  expect(document.querySelectorAll('[role="tooltip"]').length).toBeLessThanOrEqual(1);
  const tooltip = openTooltip();
  const text = tooltip?.textContent ?? undefined;
  if (tooltip) {
    const branch = tooltip.querySelector("[data-inline-chip]");
    expect(branch?.getAttribute("dir")).toBe("ltr");
    expect(branch?.getAttribute("data-size")).toBe("xs");
    expect(branch?.querySelector("code")).not.toBeNull();
    if (text?.includes("Git error:")) expect(tooltip.querySelector("pre code")?.textContent).toBe("fetch failed");
  }
  leave(element);
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
  return text;
}

/** Gives clipped labels measurable overflow under jsdom, which lays nothing out. */
function stubOverflow(): void {
  for (const [name, value] of [
    ["scrollWidth", 200],
    ["clientWidth", 0],
  ] as const) {
    const own = Object.getOwnPropertyDescriptor(Element.prototype, name)!;
    Object.defineProperty(Element.prototype, name, { configurable: true, get: () => value });
    onTestFinished(() => void Object.defineProperty(Element.prototype, name, own));
  }
}

const STATES = ALL_STATES.filter(([name]) => name !== "failed");
const FAILED = ALL_STATES.find(([name]) => name === "failed")!;

// The text and the glyph do not vary with where the badge sits, so the states run once, on the
// sidebar row; each placement is held below to one full sentence, over the glyph and over the
// branch text and counts alike.
it.each(STATES)("says its whole state in one tooltip: %s", (_, extra, glyph, sentence) => {
  const badge = PLACEMENTS[0].mount(gitStatusOf(web.id, extra), glyph);
  expect(hoverAndRead(badge.querySelector(`svg.lucide-${glyph}`)!)).toBe(sentence.replace(/[\u2068\u2069]/g, ""));
});

for (const placement of PLACEMENTS) {
  it(`gives the glyph, the branch and the counts the one sentence, the error's and the branch's alone gone, on ${placement.name}`, () => {
    stubOverflow();
    const [, extra, glyph, sentence] = FAILED;
    const badge = placement.mount(gitStatusOf(web.id, { ...extra, branch: "feature/long-branch-name" }), glyph);
    const line = lineOf(badge);
    expect(line.textContent).toBe("feature/long-branch-name");
    const full = sentence.replace(BRANCH, "feature/long-branch-name").replace(/[\u2068\u2069]/g, "");
    expect(hoverAndRead(badge.querySelector(`svg.lucide-${glyph}`)!)).toBe(full);
    expect(hoverAndRead(line)).toBe(full);
    expect(hoverAndRead(badge.querySelector(".col-start-3")!)).toBe(full);
  });

  it(`keeps the badge the part that gives way, its branch the only part that fades, on ${placement.name}`, () => {
    const badge = placement.mount(gitStatusOf(web.id, { ahead: 1, behind: 1 }), "git-branch");
    expect(badge.classList.contains("grid")).toBe(true);
    expect(badge.classList.contains("overflow-hidden")).toBe(false);
    expect(badge.getAttribute("tabindex")).toBe("-1");
    const shrinking = partsOf(badge).filter((child) => !child.classList.contains("shrink-0"));
    expect(shrinking).toEqual([lineOf(badge)]);
    expect(lineOf(badge).classList.contains("min-w-0")).toBe(true);
    expect(lineOf(badge).textContent).toBe("main");
    const counts = badge.querySelector(".col-start-3")!;
    expect(counts.textContent).toBe("11");
    expect(lineOf(badge).contains(counts)).toBe(false);
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

it.each(PLACEMENTS)("runs only the hovered project name or branch in $name, leaving buttons and glyphs still", (placement) => {
  stubMarquee();
  const badge = placement.mount(gitStatusOf(web.id, { ahead: 1 }), "git-branch");
  const surface = placement.name === "a sidebar project row"
    ? badge.closest<HTMLElement>('div[role="button"]')!
    : badge.parentElement!;
  const name = Array.from(surface.querySelectorAll<HTMLElement>(".overflow-hidden"))
    .find((element) => element.textContent === web.name)!;
  const branch = lineOf(badge);
  expect(name).toBeDefined();
  const labels = [name, branch];
  const consoleName = Array.from(surface.querySelectorAll<HTMLElement>(".overflow-hidden"))
    .find((element) => element.textContent === main.name);
  if (consoleName) labels.unshift(consoleName);
  expectIndependentMarquees(labels, [surface, badge.querySelector("svg")!, badge.querySelector(".col-start-3")!, ...surface.querySelectorAll("button")]);
});
