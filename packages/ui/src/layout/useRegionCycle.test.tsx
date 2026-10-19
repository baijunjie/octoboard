// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { useRegionCycle } from "./useRegionCycle";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SHOWN = { topbar: true, rail: false, welcome: false, sidebar: false, archive: false, terminal: false, banner: false };

function Cycle({ panel, landing = false }: { panel: boolean; landing?: boolean }) {
  useRegionCycle({ shown: SHOWN, panels: { "panel-files": panel }, focusTerminal: () => {} });
  return (
    <>
      <header data-region="topbar"><button id="bar">bar</button></header>
      <section data-region="panel-files"><button id="first">first</button><button id="second" data-region-landing={landing ? "" : undefined}>second</button></section>
    </>
  );
}

function mount(panel: boolean, landing = false) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Cycle panel={panel} landing={landing} />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const f6 = (shiftKey = false) => act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "F6", shiftKey, bubbles: true })));
  return { f6, active: () => document.activeElement?.id };
}

it("visits a shown panel as a stop of its own, landing on its first control, and skips one that is not shown", () => {
  const shown = mount(true);
  document.getElementById("bar")!.focus();
  shown.f6();
  expect(shown.active()).toBe("first");
  shown.f6();
  expect(shown.active()).toBe("bar");
  shown.f6(true);
  expect(shown.active()).toBe("first");
});

it("skips a panel the pane does not show", () => {
  const hidden = mount(false);
  document.getElementById("bar")!.focus();
  hidden.f6();
  expect(hidden.active()).toBe("bar");
});

it("lands on the control a region marks as its landing spot rather than its first", () => {
  const marked = mount(true, true);
  document.getElementById("bar")!.focus();
  marked.f6();
  expect(marked.active()).toBe("second");
});

function TerminalPanelCycle() {
  useRegionCycle({ shown: SHOWN, panels: { "panel-files": true, "panel-terminal": true }, focusTerminal: () => {} });
  return (
    <>
      <header data-region="topbar"><button id="bar">bar</button></header>
      <section data-region="panel-files"><button id="files">files</button></section>
      <section data-region="panel-terminal">
        <button id="new">new terminal</button>
        <textarea id="hidden-input" className="xterm-helper-textarea" />
        <textarea id="shown-input" className="xterm-helper-textarea" data-region-landing="" />
      </section>
    </>
  );
}

it("stops at the Terminal panel as a region of its own, landing in the shown shell terminal, and leaves the terminal with F6 and Shift+F6", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<TerminalPanelCycle />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const f6 = (shiftKey = false) => {
    const event = new KeyboardEvent("keydown", { key: "F6", shiftKey, bubbles: true, cancelable: true });
    act(() => void document.activeElement!.dispatchEvent(event));
    return event;
  };
  document.getElementById("files")!.focus();
  f6();
  expect(document.activeElement?.id).toBe("shown-input");
  // The key never reaches the terminal, which would write it to the shell.
  expect(f6().defaultPrevented).toBe(true);
  expect(document.activeElement?.id).toBe("bar");
  document.getElementById("shown-input")!.focus();
  f6(true);
  expect(document.activeElement?.id).toBe("files");
});
