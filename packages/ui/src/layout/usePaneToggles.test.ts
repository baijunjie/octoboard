// @vitest-environment jsdom
import { setInteractionModality } from "react-aria";
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { usePaneToggles, type PaneToggles } from "./usePaneToggles";

afterEach(() => {
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Mounts the hook in a window below or at and above the `docked` breakpoint, returning its latest
 * value and a way to re-render it with other panes present. The `matchMedia` stub answers
 * `matches: !narrow` to every query but the one for a device that cannot hover (`noHover`), so the
 * wide case does not exercise the reduced-motion path. */
function mountToggles(narrow: boolean, focusTerminal: () => void = () => {}, noHover = false, mobile = narrow) {
  document.documentElement.style.setProperty("--breakpoint-docked", "1394px");
  document.documentElement.style.setProperty("--breakpoint-phone", "594px");
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query === "(hover: none)" ? noHover : query === "(min-width: 594px)" ? !mobile : !narrow,
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
  let latest!: PaneToggles;
  function Probe({ panes }: { panes: { hasSidebar: boolean; asidePeekable: boolean } }) {
    latest = usePaneToggles({ ...panes, focusTerminal });
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
  const render = (panes: { hasSidebar: boolean; asidePeekable: boolean }) =>
    act(() => root.render(createElement(Probe, { panes })));
  render({ hasSidebar: true, asidePeekable: true });
  return { render, panes: () => latest };
}

it("closes the sidebar drawer and stops floating it once the sidebar no longer exists", () => {
  const gone = { hasSidebar: false, asidePeekable: true };
  const narrow = mountToggles(true);
  act(() => narrow.panes().toggleSidebar());
  expect(narrow.panes().sidebarOpen).toBe(true);
  narrow.render(gone);
  expect(narrow.panes().sidebarOpen).toBe(false);

  // At and above the breakpoint a hidden docked sidebar may float in, until it is gone. Only
  // whether it may is public, as it floats in for a hovered console.
  const wide = mountToggles(false);
  act(() => wide.panes().toggleSidebar());
  expect(wide.panes().sidebarPeek.peekable).toBe(true);
  wide.render(gone);
  expect(wide.panes().sidebarPeek.peekable).toBe(false);
});

it.each([true, false])("floats the aside in on request while it may, in a narrow window too (narrow: %s), and stops once it may not", (narrow) => {
  const { render, panes } = mountToggles(narrow);
  act(() => panes().asidePeek.reveal());
  expect(panes().asidePeek.active).toBe(true);
  render({ hasSidebar: true, asidePeekable: false });
  expect(panes().asidePeek.active).toBe(false);
  act(() => panes().asidePeek.reveal());
  expect(panes().asidePeek.active).toBe(false);
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

it.each([
  { modality: "pointer", dismissed: true },
  { modality: "keyboard", dismissed: false },
] as const)("lets the floating aside go when the pointer leaves with focus a $modality put inside: dismissed $dismissed", ({ modality, dismissed }) => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { panes } = mountToggles(false);
  const pane = document.body.appendChild(document.createElement("div"));
  pane.setAttribute("data-pane", "aside");
  pane.innerHTML = '<button type="button">tab</button>';
  onTestFinished(() => pane.remove());
  act(() => panes().asidePeek.reveal());
  expect(panes().asidePeek.active).toBe(true);
  act(() => setInteractionModality(modality));
  pane.querySelector("button")!.focus();
  act(() => panes().asidePeek.leave());
  act(() => void vi.advanceTimersByTime(1000));
  expect(panes().asidePeek.active).toBe(!dismissed);
});

it("closes the sidebar's open drawer when the aside floats in, instead of floating over it", () => {
  const { panes } = mountToggles(true);
  act(() => panes().toggleSidebar());
  expect(panes().sidebarOpen).toBe(true);
  act(() => panes().asidePeek.reveal());
  expect(panes().sidebarOpen).toBe(false);
  expect(panes().asidePeek.active).toBe(true);
  // Dismissing the aside's float alone leaves the sidebar's state as it is.
  act(() => panes().toggleSidebar());
  expect(panes().sidebarOpen).toBe(true);
  expect(panes().asidePeek.active).toBe(false);
  act(() => panes().asidePeek.hide());
  expect(panes().sidebarOpen).toBe(true);
});

it("leaves Escape in a shell terminal of the floating aside to the shell, and dismisses the pane from the agent terminal", () => {
  const { panes } = mountToggles(false);
  act(() => panes().asidePeek.reveal());
  const aside = document.body.appendChild(document.createElement("div"));
  aside.setAttribute("data-escape-scope", "");
  aside.innerHTML = '<div data-shell-terminal><textarea class="xterm-helper-textarea"></textarea></div>';
  const agent = document.body.appendChild(document.createElement("div"));
  agent.setAttribute("data-escape-scope", "");
  agent.innerHTML = '<textarea class="xterm-helper-textarea"></textarea>';
  onTestFinished(() => {
    aside.remove();
    agent.remove();
  });
  const escape = (target: Element) => {
    const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true });
    act(() => void target.dispatchEvent(event));
    return event;
  };
  const shellKey = vi.fn();
  aside.querySelector("textarea")!.addEventListener("keydown", shellKey);
  escape(aside.querySelector("textarea")!);
  expect(shellKey).toHaveBeenCalledOnce();
  expect(panes().asidePeek.active).toBe(true);
  escape(agent.querySelector("textarea")!);
  expect(panes().asidePeek.active).toBe(false);
});

it("leaves focus in a docked card when the aside stops being peekable as a column replaces its float", () => {
  const focusTerminal = vi.fn();
  const { render, panes } = mountToggles(false, focusTerminal);
  const pane = document.body.appendChild(document.createElement("div"));
  pane.setAttribute("data-pane", "aside");
  pane.innerHTML = '<button type="button">in the restored card</button>';
  act(() => panes().asidePeek.reveal());
  pane.querySelector("button")!.focus();
  render({ hasSidebar: true, asidePeekable: false });
  expect(panes().asidePeek.active).toBe(false);
  expect(focusTerminal).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(pane.querySelector("button"));
});

it("hands focus in the docked sidebar to the terminal when the window narrows it into the closed drawer", () => {
  document.documentElement.style.setProperty("--breakpoint-docked", "1394px");
  document.documentElement.style.setProperty("--breakpoint-phone", "594px");
  let narrow = false;
  const listeners = new Set<() => void>();
  vi.stubGlobal("matchMedia", (query: string) => ({
    get matches() {
      return !narrow;
    },
    media: query,
    addEventListener: (_: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_: string, listener: () => void) => listeners.delete(listener),
  }));
  const sidebar = document.body.appendChild(document.createElement("nav"));
  sidebar.setAttribute("data-pane", "sidebar");
  const row = sidebar.appendChild(document.createElement("button"));
  const focusTerminal = vi.fn();
  function Probe() {
    usePaneToggles({ hasSidebar: true, asidePeekable: false, focusTerminal });
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    sidebar.remove();
    document.documentElement.style.removeProperty("--breakpoint-docked");
    localStorage.clear();
  });
  act(() => root.render(createElement(Probe)));
  row.focus();
  expect(focusTerminal).not.toHaveBeenCalled();
  act(() => {
    narrow = true;
    for (const listener of [...listeners]) listener();
  });
  expect(focusTerminal).toHaveBeenCalledTimes(1);
});

it.each([true, false])("floats neither pane in on a device that cannot hover (narrow: %s)", (narrow) => {
  const { panes } = mountToggles(narrow, () => {}, true);
  act(() => panes().asidePeek.reveal());
  expect(panes().asidePeek.active).toBe(false);
  if (!narrow) {
    act(() => panes().toggleSidebar());
    expect(panes().sidebarPeek.peekable).toBe(false);
  }
});

it("floats the sidebar on medium screens while retaining its wide-screen docking preference", () => {
  const { panes } = mountToggles(true, () => {}, false, false);
  expect(panes().sidebarDocked).toBe(true);
  expect(panes().sidebarPeek.peekable).toBe(true);
  act(() => panes().sidebarPeek.reveal());
  expect(panes().sidebarPeek.active).toBe(true);
  act(() => panes().asidePeek.reveal());
  expect(panes().sidebarPeek.active).toBe(false);
  expect(panes().sidebarDocked).toBe(true);
});

it("opens the floating group on a press on a touch-only medium screen", () => {
  const { panes } = mountToggles(true, () => {}, true, false);
  act(() => panes().asidePeek.reveal());
  expect(panes().asidePeek.active).toBe(false);
  act(() => panes().asidePeek.reveal(true));
  expect(panes().asidePeek.active).toBe(true);
  act(() => panes().dismissOverlays());
  expect(panes().asidePeek.active).toBe(false);
});

it("keeps a medium-screen sidebar revealed when its button is pressed after hovering", () => {
  const { panes } = mountToggles(true, () => {}, false, false);
  act(() => panes().sidebarPeek.reveal());
  expect(panes().sidebarShown).toBe(true);
  act(() => panes().toggleSidebar());
  act(() => panes().toggleSidebar());
  expect(panes().sidebarPeek.active).toBe(true);
  expect(panes().sidebarShown).toBe(true);
  expect(panes().sidebarDocked).toBe(true);
});

it("keeps a touch-only medium-screen sidebar open on repeated presses", () => {
  const { panes } = mountToggles(true, () => {}, true, false);
  act(() => panes().toggleSidebar());
  act(() => panes().toggleSidebar());
  expect(panes().sidebarOpen).toBe(true);
  expect(panes().sidebarShown).toBe(true);
  expect(panes().sidebarDocked).toBe(true);
});
