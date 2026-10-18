// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { holdsText, usePaneToggles, type PaneToggles } from "./usePaneToggles";

afterEach(() => {
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

function field(html: string): Element {
  document.body.innerHTML = html;
  return document.body.firstElementChild!;
}

it("leaves Escape to a text field while it holds text, and to the pane once it is empty", () => {
  const input = field('<input type="search" value="abc">') as HTMLInputElement;
  expect(holdsText(input)).toBe(true);
  input.value = "";
  expect(holdsText(input)).toBe(false);
});

it("leaves Escape to the pane for xterm's hidden textarea, even with text in it", () => {
  expect(holdsText(field('<textarea class="xterm-helper-textarea">x</textarea>'))).toBe(false);
});

it.each([
  ['<input type="checkbox">', "a checkbox, whose value is always 'on'"],
  ['<input type="range" value="5">', "a range"],
  ['<input type="text" value="abc" readonly>', "a read-only field"],
])("leaves Escape to the pane for %s (%s)", (html) => {
  expect(holdsText(field(html))).toBe(false);
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Mounts the hook in a window below or at and above the `docked` breakpoint, returning its latest
 * value and a way to re-render it with other panes present. The `matchMedia` stub is deliberately
 * query-blind (it answers `matches: !narrow` to every query), so the wide case does not exercise
 * the reduced-motion path. */
function mountToggles(narrow: boolean) {
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: !narrow,
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
  let latest!: PaneToggles;
  function Probe({ panes }: { panes: { hasSidebar: boolean; hasAside: boolean } }) {
    latest = usePaneToggles({ ...panes, focusTerminal: () => {} });
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    document.documentElement.style.removeProperty("--breakpoint-docked");
    // Hiding a docked pane persists the choice, which the next test would start from.
    localStorage.clear();
  });
  const render = (panes: { hasSidebar: boolean; hasAside: boolean }) =>
    act(() => root.render(createElement(Probe, { panes })));
  render({ hasSidebar: true, hasAside: true });
  return { render, panes: () => latest };
}

it.each([
  { pane: "sidebar", gone: { hasSidebar: false, hasAside: true } },
  { pane: "aside", gone: { hasSidebar: true, hasAside: false } },
] as const)("closes the $pane drawer and stops floating it once the pane no longer exists", ({ pane, gone }) => {
  const open = (p: PaneToggles) => (pane === "sidebar" ? p.sidebarOpen : p.asideOpen);
  // The sidebar floats in for a hovered console, so only whether it may is public; the aside's
  // `reveal` floats it in where it may, so whether it did shows the same.
  const floats = (p: PaneToggles) => (pane === "sidebar" ? p.sidebarPeek.peekable : p.asidePeek.active);
  const toggle = (p: PaneToggles) => (pane === "sidebar" ? p.toggleSidebar() : p.toggleAside());

  const narrow = mountToggles(true);
  act(() => toggle(narrow.panes()));
  expect(open(narrow.panes())).toBe(true);
  narrow.render(gone);
  expect(open(narrow.panes())).toBe(false);

  // At and above the breakpoint a hidden docked pane may float in, until it is gone.
  const wide = mountToggles(false);
  act(() => toggle(wide.panes()));
  if (pane === "aside") act(() => wide.panes().asidePeek.reveal());
  expect(floats(wide.panes())).toBe(true);
  wide.render(gone);
  expect(floats(wide.panes())).toBe(false);
});

it("leaves Escape to a floating panel over an open drawer, and closes the drawer on the next one", () => {
  const { panes } = mountToggles(true);
  act(() => panes().toggleSidebar());
  expect(panes().sidebarOpen).toBe(true);
  const scope = document.body.appendChild(document.createElement("div"));
  scope.setAttribute("data-escape-scope", "");
  const escape = () => act(() => void scope.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  const panel = document.body.appendChild(document.createElement("div"));
  panel.setAttribute("data-escape-topmost", "");
  escape();
  expect(panes().sidebarOpen).toBe(true);
  panel.remove();
  escape();
  expect(panes().sidebarOpen).toBe(false);
  scope.remove();
});
