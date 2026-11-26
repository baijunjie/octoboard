// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { useRegionCycle } from "./useRegionCycle";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SHOWN = { topbar: true, rail: false, devices: false, tasks: false, welcome: false, sidebar: false, archive: false, notices: false, uploads: false, terminal: false, banner: false, update: false };

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

it("visits a shown panel as a stop of its own, landing on its first control, forwards", () => {
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

function BannerCycle({ update, banner }: { update: boolean; banner: boolean }) {
  useRegionCycle({ shown: { ...SHOWN, update, banner }, focusTerminal: () => {} });
  return (
    <>
      <header data-region="topbar"><button id="bar">bar</button></header>
      <section data-region="update"><button id="install">install</button></section>
      <section data-region="banner"><button id="retry">retry</button></section>
    </>
  );
}

it("visits the update notice, then the connection banner, while they are shown", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<BannerCycle update banner />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const f6 = () => act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "F6", bubbles: true })));
  document.getElementById("bar")!.focus();
  f6();
  expect(document.activeElement?.id).toBe("install");
  f6();
  expect(document.activeElement?.id).toBe("retry");
  f6();
  expect(document.activeElement?.id).toBe("bar");
  act(() => root.render(<BannerCycle update={false} banner />));
  f6();
  expect(document.activeElement?.id).toBe("retry");
});

function UploadsCycle({ uploads }: { uploads: boolean }) {
  useRegionCycle({
    shown: { ...SHOWN, terminal: true, uploads },
    focusTerminal: () => document.getElementById("terminal-input")?.focus(),
  });
  return (
    <>
      <header data-region="topbar"><button id="bar">bar</button></header>
      {uploads && <div data-region="uploads"><button id="cancel-uploads">Cancel</button></div>}
      <section data-region="terminal"><textarea id="terminal-input" /></section>
    </>
  );
}

function mountUploads(uploads: boolean) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<UploadsCycle uploads={uploads} />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const f6 = (shiftKey = false) => act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "F6", shiftKey, bubbles: true })));
  return { f6, active: () => document.activeElement?.id };
}

it("visits the uploads notice just before the terminal while it is shown, in both directions", () => {
  const cycle = mountUploads(true);
  document.getElementById("bar")!.focus();
  cycle.f6();
  expect(cycle.active()).toBe("cancel-uploads");
  cycle.f6();
  expect(cycle.active()).toBe("terminal-input");
  cycle.f6(true);
  expect(cycle.active()).toBe("cancel-uploads");
  cycle.f6(true);
  expect(cycle.active()).toBe("bar");
});

it("skips the uploads notice where it is not shown", () => {
  const cycle = mountUploads(false);
  document.getElementById("bar")!.focus();
  cycle.f6();
  expect(cycle.active()).toBe("terminal-input");
});

function NoticesCycle({ notices, uploads }: { notices: boolean; uploads: boolean }) {
  useRegionCycle({
    shown: { ...SHOWN, terminal: true, notices, uploads },
    focusTerminal: () => document.getElementById("terminal-input")?.focus(),
  });
  return (
    <>
      <header data-region="topbar"><button id="bar">bar</button></header>
      {notices && <div data-region="notices"><button id="details">Details</button><button id="dismiss">Dismiss</button></div>}
      {uploads && <div data-region="uploads"><button id="cancel-uploads">Cancel</button></div>}
      <section data-region="terminal"><textarea id="terminal-input" /></section>
    </>
  );
}

function mountNotices(notices: boolean, uploads: boolean) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<NoticesCycle notices={notices} uploads={uploads} />));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const f6 = (shiftKey = false) => act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "F6", shiftKey, bubbles: true })));
  return { f6, active: () => document.activeElement?.id };
}

it("visits the standing notices' banner before the uploads notice and the terminal, in both directions", () => {
  const cycle = mountNotices(true, true);
  document.getElementById("bar")!.focus();
  cycle.f6();
  expect(cycle.active()).toBe("details");
  cycle.f6();
  expect(cycle.active()).toBe("cancel-uploads");
  cycle.f6();
  expect(cycle.active()).toBe("terminal-input");
  cycle.f6(true);
  expect(cycle.active()).toBe("cancel-uploads");
  cycle.f6(true);
  expect(cycle.active()).toBe("details");
  cycle.f6(true);
  expect(cycle.active()).toBe("bar");
});

it("skips the standing notices' banner where it is not shown", () => {
  const cycle = mountNotices(false, false);
  document.getElementById("bar")!.focus();
  cycle.f6();
  expect(cycle.active()).toBe("terminal-input");
});
