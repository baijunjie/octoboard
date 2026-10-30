// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { defaultStack, type PanelKind, type PanelStacks } from "./stackState";
import { useFloatingPanel, type FloatingPanel } from "./useFloatingPanel";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const STACKS: PanelStacks = { project: { open: ["files", "terminal"], shares: {} }, report: defaultStack("report") };

/** The hook with a card for the panel it floats, and a control outside it to hold focus. */
function mount(props: { floats: boolean; drawerOpen?: boolean; owner?: "project" | "report"; loaded?: boolean; touch?: boolean; textLanding?: boolean; unavailable?: PanelKind[] }) {
  const focusTerminal = vi.fn();
  let latest!: FloatingPanel;
  // Each test's interaction: a tap, or a mouse's press.
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: props.touch ? "touch" : "mouse", bubbles: true }));
  function Harness({ floats, drawerOpen = false, owner = "project", loaded = true, unavailable }: typeof props) {
    latest = useFloatingPanel({ floats, owner, stacks: STACKS, drawerOpen, focusTerminal, unavailable });
    return (
      <>
        <button type="button" id="outside">outside</button>
        {latest.kind && (
          <section id={`panel-${latest.kind}`}>
            {loaded && props.textLanding && <textarea aria-label="shell" data-region-landing />}
            {loaded && <button type="button">inside {latest.kind}</button>}
          </section>
        )}
      </>
    );
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (next: typeof props) => act(() => root.render(<Harness {...next} />));
  render(props);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { render, panel: () => latest, focusTerminal, focusedText: () => document.activeElement?.textContent };
}

it("floats nothing while the window docks the pane, and the stack's top panel once it narrows", () => {
  const test = mount({ floats: false });
  expect(test.panel().kind).toBeUndefined();
  test.render({ floats: true });
  expect(test.panel().kind).toBe("terminal");
});

it("floats the panel opened last in the docked pane when the window narrows, if it is still open", () => {
  const test = mount({ floats: false });
  act(() => test.panel().noteOpened("project", "files"));
  test.render({ floats: true });
  expect(test.panel().kind).toBe("files");
});

it("closes the panels of every owner kind when the drawer opens, and floats none while it is open", () => {
  const test = mount({ floats: true });
  expect(test.panel().kind).toBe("terminal");
  test.render({ floats: true, drawerOpen: true });
  expect(test.panel().kind).toBeUndefined();
  test.render({ floats: true, drawerOpen: true, owner: "report" });
  expect(test.panel().kind).toBeUndefined();
  test.render({ floats: true, owner: "report" });
  expect(test.panel().kind).toBeUndefined();
  test.render({ floats: true });
  expect(test.panel().kind).toBeUndefined();
});

it("remembers the panel that was on screen when the drawer opens, one seeded as the window narrowed included", () => {
  const test = mount({ floats: true });
  expect(test.panel().lastOpened("project")).toBeUndefined();
  test.render({ floats: true, drawerOpen: true });
  expect(test.panel().kind).toBeUndefined();
  expect(test.panel().lastOpened("project")).toBe("terminal");
});

it("sends focus into the panel on every open, the same kind floating already included, and nowhere on a later change", () => {
  const test = mount({ floats: true });
  act(() => document.getElementById("outside")!.focus());
  act(() => test.panel().open("project", "files"));
  expect(test.focusedText()).toBe("inside files");
  act(() => document.getElementById("outside")!.focus());
  act(() => test.panel().open("project", "files"));
  expect(test.focusedText()).toBe("inside files");

  // A close and a kind floated by the window narrowing again ask for nothing.
  act(() => test.panel().close());
  act(() => document.getElementById("outside")!.focus());
  test.render({ floats: false });
  test.render({ floats: true });
  expect(test.panel().kind).toBe("files");
  expect(test.focusedText()).toBe("outside");
});

it("hands focus left nowhere to the terminal when the panel closes", () => {
  const test = mount({ floats: true });
  act(() => test.panel().open("project", "terminal"));
  expect(test.focusedText()).toBe("inside terminal");
  act(() => test.panel().close());
  expect(test.focusTerminal).toHaveBeenCalledTimes(1);
});

it("waits for a lazily loaded panel's content to send focus into it, even with focus fallen to <body> meanwhile", async () => {
  const test = mount({ floats: true, loaded: false });
  act(() => document.getElementById("outside")!.focus());
  act(() => test.panel().open("project", "terminal"));
  // The control that held focus is covered (made inert) by the panel, and focus falls to <body>.
  act(() => (document.activeElement as HTMLElement).blur());
  expect(document.activeElement).toBe(document.body);
  test.render({ floats: true, loaded: true });
  // The content's arrival is reported to the waiting observer as a microtask.
  await act(async () => {});
  expect(test.focusedText()).toBe("inside terminal");
});

it("leaves focus the user moved elsewhere while a lazily loaded panel's content was coming", async () => {
  const test = mount({ floats: true, loaded: false });
  act(() => test.panel().open("project", "terminal"));
  act(() => document.getElementById("outside")!.focus());
  test.render({ floats: true, loaded: true });
  await act(async () => {});
  expect(test.focusedText()).toBe("outside");
});

it("lands focus after a tap on the panel's first control that takes no text, never on its text landing", () => {
  const test = mount({ floats: true, touch: true, textLanding: true });
  act(() => test.panel().open("project", "terminal"));
  expect(test.focusedText()).toBe("inside terminal");
});

it("lands focus with a pointer or keyboard on the panel's landing, a field that takes text included", () => {
  const test = mount({ floats: true, textLanding: true });
  act(() => test.panel().open("project", "terminal"));
  expect(document.activeElement?.getAttribute("aria-label")).toBe("shell");
});

it("lands focus that a closing panel leaves nowhere on the top bar when the terminal cannot take it", () => {
  const topbar = document.body.appendChild(document.createElement("div"));
  topbar.dataset.region = "topbar";
  const back = topbar.appendChild(document.createElement("button"));
  onTestFinished(() => topbar.remove());
  const test = mount({ floats: true });
  act(() => test.panel().open("project", "terminal"));
  act(() => (document.activeElement as HTMLElement).blur());
  act(() => test.panel().close());
  expect(test.focusTerminal).toHaveBeenCalledTimes(1);
  expect(document.activeElement).toBe(back);
});

it("floats no panel of a kind that has none just now, and does not float it again when its panel is back", () => {
  const test = mount({ floats: true });
  act(() => test.panel().open("project", "report"));
  expect(test.panel().kind).toBe("report");
  test.render({ floats: true, unavailable: ["report"] });
  expect(test.panel().kind).toBeUndefined();
  test.render({ floats: true });
  expect(test.panel().kind).toBeUndefined();
  // Opened again by the user, it floats.
  act(() => test.panel().open("project", "report"));
  expect(test.panel().kind).toBe("report");
});
