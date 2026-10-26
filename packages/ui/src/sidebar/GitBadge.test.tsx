// @vitest-environment jsdom
import React, { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { appConfig } from "../appConfig";
import { expectIndependentMarquees, stubMarquee } from "../components/marqueeTestSupport";
import { enter, leave, mountForTooltip, openPopover, openTooltip, press, tap, withFakeTimers } from "../components/tooltipTestSupport";
import { consoleOf, gitStatusOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import { PlatformProvider } from "../platform/react";
import type { GitStatus } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";
import { ProjectFocusView } from "./FocusView";
import { GitBadge } from "./GitBadge";
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
  /** Mounts the placement for `status` and returns its container. */
  render: (status: GitStatus) => HTMLElement;
  /** Mounts the badge for `status` and returns its box, the tooltip's trigger, found by its state glyph. */
  mount: (status: GitStatus, glyph: string) => HTMLElement;
  /** The element carrying the badge's accessible name, whichever that is. */
  named: (container: HTMLElement) => Element | null;
}

/** The store of the placement mounted last, so a test can change the status under it. */
let store: ReturnType<typeof createStateStore>;

function daemonWith(status: GitStatus): Daemon {
  store = createStateStore({ gitStatuses: new Map([[web.id, status]]), sessions: new Map(sessions.map((session) => [session.id, session])) } satisfies Partial<State>);
  return {
    store,
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

/** A failed check: the mark stands in for the idle glyph, so the badge's box is found by its branch. */
const RAW = "fatal: unable to access 'https://example.com/repo.git/': Could not resolve host";
const FAILED: Partial<GitStatus> = { ahead: 3, behind: 2, error: RAW };

/** The failure mark's button. */
const markOf = (container: HTMLElement): HTMLElement => container.querySelector<HTMLElement>("[data-status-mark]")!;

/** The badge's box beside the failure mark, the tooltip's trigger, which has no state glyph while nothing is in flight. */
const factsOf = (container: HTMLElement): HTMLElement => markOf(container).parentElement!.nextElementSibling as HTMLElement;

/** The branch name alone, which fades and runs as a marquee. */
const lineOf = (badge: HTMLElement): HTMLElement => badge.querySelector<HTMLElement>(".overflow-hidden")!;

/** What the badge lays out: its own children, or those of the `aria-hidden` box inside it that a
 * badge on a row wraps them in without a box of its own. */
function partsOf(badge: HTMLElement): Element[] {
  const only = badge.children.length === 1 ? badge.firstElementChild! : null;
  return Array.from(only?.classList.contains("contents") ? only.children : badge.children);
}

const renderRow = (status: GitStatus) => mountOver(status, <Sidebar {...handlers()} projects={[web]} shownConsole={main} open sidebarWidth={sidebarWidth} />);
const renderHeader = (status: GitStatus) => mountOver(status, <ProjectFocusView handlers={handlers()} console={main} project={web} sessions={sessions} owners={new Map()} />);

const PLACEMENTS: Placement[] = [
  {
    name: "a sidebar project row",
    render: renderRow,
    mount: (status, glyph) => badgeOf(renderRow(status), glyph),
    named: (container) => container.querySelector("div[role=button]"),
  },
  {
    name: "focus mode's header",
    render: renderHeader,
    mount: (status, glyph) => badgeOf(renderHeader(status), glyph),
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
    const branch = tooltip.querySelector("[data-inline-reference]");
    expect(branch?.getAttribute("dir")).toBe("ltr");
    expect(branch?.getAttribute("data-size")).toBe("xs");
    expect(branch?.querySelector("code")).not.toBeNull();
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

// The text and the glyph do not vary with where the badge sits, so the states run once, on the
// sidebar row; each placement is held below to one full sentence, over the branch text and the
// counts alike.
it.each(ALL_STATES)("says its whole state in one tooltip: %s", (_, extra, glyph, sentence) => {
  const badge = PLACEMENTS[0].mount(gitStatusOf(web.id, extra), glyph);
  expect(hoverAndRead(badge.querySelector(`svg.lucide-${glyph}`)!)).toBe(sentence.replace(/[\u2068\u2069]/g, ""));
});

for (const placement of PLACEMENTS) {
  it(`gives the branch and the counts the one sentence, with no word of the failure, on ${placement.name}`, () => {
    stubOverflow();
    const container = placement.render(gitStatusOf(web.id, { ...FAILED, branch: "feature/long-branch-name" }));
    const badge = factsOf(container);
    const line = lineOf(badge);
    expect(line.textContent).toBe("feature/long-branch-name");
    const full = "Branch feature/long-branch-name, 3 commits ahead, 2 commits behind";
    expect(hoverAndRead(line)).toBe(full);
    expect(hoverAndRead(badge.querySelector(".col-start-2")!)).toBe(full);
  });

  it(`keeps Git's own text out of every name and tooltip, behind the failure mark's popover alone, on ${placement.name}`, () => {
    const container = placement.render(gitStatusOf(web.id, FAILED));
    expect(document.body.textContent).not.toContain(RAW);
    for (const element of document.body.querySelectorAll("[aria-label]")) expect(element.getAttribute("aria-label")).not.toContain(RAW);
    const mark = markOf(container);
    expect(mark.getAttribute("aria-label")).toBe("Git check failed");
    // The mark is no tooltip's trigger: hovering it opens its popover, never a tooltip.
    expect(hoverAndRead(mark)).toBeUndefined();
    act(() => mark.focus());
    tap(mark, "Enter");
    const popover = openPopover()!;
    expect(popover.getAttribute("aria-label")).toBe("Git check failed");
    expect(popover.contains(document.activeElement)).toBe(true);
    expect(Array.from(popover.querySelectorAll("p"), (p) => p.textContent)).toEqual([
      "Git reported an error the last time this project’s repository was checked or pulled.",
      `${appConfig.name} checks it again on its own. If the error does not go away, resolve it in the repository with Git.`,
    ]);
    // Git's text comes last, on a diagnostic block of its own.
    const block = popover.querySelector("pre")!;
    expect(block.textContent).toBe(RAW);
    expect(Array.from(popover.querySelectorAll("p, pre")).pop()).toBe(block);
  });

  it(`makes the failure mark a tab stop of its own, handing focus on when a check starting takes it away, on ${placement.name}`, () => {
    const container = placement.render(gitStatusOf(web.id, FAILED));
    const mark = markOf(container);
    expect(mark.tabIndex).toBe(0);
    expect(mark.closest("[aria-hidden=true]")).toBeNull();
    act(() => mark.focus());
    act(() => store.setState({ gitStatuses: new Map([[web.id, gitStatusOf(web.id, { ...FAILED, activity: "checking" })]]) }));
    expect(container.querySelector("[data-status-mark]")).toBeNull();
    expect(container.querySelector("svg.lucide-refresh-cw")).not.toBeNull();
    const holder = placement.name === "a sidebar project row" ? placement.named(container) : container.querySelector("[data-focus-exit]");
    expect(document.activeElement).toBe(holder);
  });

  it(`shows the failure mark again only when the check that hid it fails again, on ${placement.name}`, () => {
    const container = placement.render(gitStatusOf(web.id, { ...FAILED, activity: "checking" }));
    expect(container.querySelector("[data-status-mark]")).toBeNull();
    act(() => store.setState({ gitStatuses: new Map([[web.id, gitStatusOf(web.id, { ahead: 3, behind: 2 })]]) }));
    expect(container.querySelector("[data-status-mark]")).toBeNull();
    act(() => store.setState({ gitStatuses: new Map([[web.id, gitStatusOf(web.id, FAILED)]]) }));
    expect(markOf(container)).not.toBeNull();
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

it("names the touch popover of the badge in focus mode's header by a generic word, not by the sentence it holds", () => {
  const badge = PLACEMENTS[1].mount(gitStatusOf(web.id, { ahead: 1 }), "git-branch");
  press(badge, "touch");
  const popover = openPopover();
  expect(popover?.textContent).toBe("Branch main, 1 commit ahead");
  expect(popover?.getAttribute("aria-label")).toBe("Hint");
  expect(popover?.hasAttribute("aria-labelledby")).toBe(false);
});

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

it("keeps a press on the failure mark from folding the project, the row naming the failure in its own words", () => {
  const container = PLACEMENTS[0].render(gitStatusOf(web.id, FAILED));
  const row = PLACEMENTS[0].named(container)!;
  expect(row.getAttribute("aria-label")).toBe(`${isolate("Website")}, a session is running, Branch ${BRANCH}, 3 commits ahead, 2 commits behind, Git check failed`);
  const expanded = row.getAttribute("aria-expanded");
  press(markOf(container), "mouse");
  expect(row.getAttribute("aria-expanded")).toBe(expanded);
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

it("opens the badge's tooltip content in a popover on a tap only where the caller leaves it on", () => {
  const status = gitStatusOf(web.id, { branch: "feature/x" });
  const alone = mountForTooltip(<GitBadge status={status} touchPopover />);
  press(alone.querySelector<HTMLElement>('[role="img"]')!);
  expect(openPopover()?.textContent).toContain("feature/x");
  press(document.body);
  const inRow = mountForTooltip(<GitBadge status={status} decorative touchPopover={false} />);
  press(inRow.firstElementChild!);
  expect(openPopover()).toBeNull();
});
