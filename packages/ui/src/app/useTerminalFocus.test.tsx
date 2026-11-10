// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { useTerminalFocus } from "./useTerminalFocus";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => document.body.replaceChildren());

/** The hook's `focusTerminal`, with the terminal pane's handle and the agent area it reads attached the way `App`
 * attaches them, beside a top bar whose control is where a tap's focus falls back to. The area is built by hand:
 * `archive` is the archive layer's state in it (`closing` while its exit runs), and `inert` the maximized or floating
 * panel's covering of it. */
async function mountArea({ archive, inert = false, panel = false }: { archive?: "open" | "closing"; inert?: boolean; panel?: boolean } = {}) {
  const topBarButton = document.body.appendChild(document.createElement("div")).appendChild(document.createElement("button"));
  topBarButton.parentElement!.dataset.region = "topbar";
  const area = document.body.appendChild(document.createElement("div"));
  if (inert) area.setAttribute("inert", "");
  let section: HTMLElement | undefined;
  if (archive) {
    const layer = area.appendChild(document.createElement("div"));
    layer.dataset.state = archive;
    section = layer.appendChild(document.createElement("section"));
    section.dataset.region = "archive";
    section.tabIndex = -1;
  }
  let panelButton: HTMLElement | undefined;
  if (panel) {
    const card = document.body.appendChild(document.createElement("div"));
    card.dataset.panelMaximized = "";
    panelButton = card.appendChild(document.createElement("button"));
  }
  let hook!: ReturnType<typeof useTerminalFocus>;
  function Probe() {
    hook = useTerminalFocus();
    return null;
  }
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  await act(async () => root.render(<Probe />));
  onTestFinished(() => act(() => root.unmount()));
  const terminal = { focus: vi.fn(), handOff: vi.fn() };
  const welcome = { focus: vi.fn() };
  hook.terminalRef.current = terminal;
  hook.welcomeRef.current = welcome;
  hook.agentAreaRef.current = area;
  return { focusTerminal: hook.focusTerminal, terminal, welcome, hook, topBarButton, section, panelButton };
}

/** The last press was a finger's, restored after the test. */
function afterTap(): void {
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "touch" }));
  onTestFinished(() => void document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse" })));
}

it("focuses the terminal pane when nothing covers it", async () => {
  const { focusTerminal, terminal } = await mountArea();
  focusTerminal();
  expect(terminal.focus).toHaveBeenCalledOnce();
});

it("focuses the welcome screen where there is no terminal pane", async () => {
  const { focusTerminal, hook, welcome } = await mountArea();
  hook.terminalRef.current = null;
  focusTerminal();
  expect(welcome.focus).toHaveBeenCalledOnce();
});

it("lands focus meant for the terminal on the open archive view's root, without scrolling to it", async () => {
  const { focusTerminal, terminal, section } = await mountArea({ archive: "open" });
  const focus = vi.spyOn(section!, "focus");
  focusTerminal();
  expect(document.activeElement).toBe(section);
  expect(focus).toHaveBeenCalledWith({ preventScroll: true });
  expect(terminal.focus).not.toHaveBeenCalled();
});

it("lands a tap's focus meant for the terminal on the open archive view's root too, not the top bar", async () => {
  const { focusTerminal, terminal, section } = await mountArea({ archive: "open" });
  afterTap();
  focusTerminal();
  expect(document.activeElement).toBe(section);
  expect(terminal.focus).not.toHaveBeenCalled();
});

it("lands on the terminal once the archive view is only on its way out", async () => {
  const { focusTerminal, terminal, section } = await mountArea({ archive: "closing" });
  focusTerminal();
  expect(terminal.focus).toHaveBeenCalledOnce();
  expect(document.activeElement).not.toBe(section);
});

it("lands a tap's focus on the top bar when no archive view is open", async () => {
  const { focusTerminal, terminal, topBarButton } = await mountArea();
  afterTap();
  focusTerminal();
  expect(document.activeElement).toBe(topBarButton);
  expect(terminal.focus).not.toHaveBeenCalled();
});

it("lets a covering panel win over the archive view, which an inert agent area does not offer", async () => {
  const { focusTerminal, terminal, section, panelButton } = await mountArea({ archive: "open", inert: true, panel: true });
  focusTerminal();
  expect(document.activeElement).toBe(panelButton);
  expect(document.activeElement).not.toBe(section);
  expect(terminal.focus).not.toHaveBeenCalled();
});

it("does not land on the archive view while the agent area is inert", async () => {
  const { focusTerminal, section } = await mountArea({ archive: "open", inert: true });
  focusTerminal();
  expect(document.activeElement).not.toBe(section);
});

it("hands off to the terminal pane's own handoff from <body>, which the focus meant for the terminal leaves alone with no session", async () => {
  const { hook, terminal } = await mountArea();
  expect(document.activeElement).toBe(document.body);
  hook.handOffToTerminal();
  expect(terminal.handOff).toHaveBeenCalledOnce();
});

it("hands off to the terminal pane's own handoff from the closing archive view, whose focus is about to go", async () => {
  const { hook, terminal, section } = await mountArea({ archive: "closing" });
  section!.focus();
  expect(document.activeElement).toBe(section);
  hook.handOffToTerminal();
  expect(terminal.handOff).toHaveBeenCalledOnce();
});

it("does not hand off when focus is already somewhere in the window, outside the archive view", async () => {
  const { hook, terminal, topBarButton } = await mountArea();
  terminal.focus.mockImplementation(() => topBarButton.focus());
  hook.handOffToTerminal();
  expect(document.activeElement).toBe(topBarButton);
  expect(terminal.handOff).not.toHaveBeenCalled();
});

it("hands focus on to the top bar from <body> where there is no terminal pane", async () => {
  const { hook, topBarButton } = await mountArea();
  hook.terminalRef.current = null;
  hook.handOffToTerminal();
  expect(document.activeElement).toBe(topBarButton);
});
