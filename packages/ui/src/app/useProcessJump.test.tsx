// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import type { ProcessJump } from "../process/processJump";
import { DaemonProvider } from "../store";
import { testDaemon } from "../test/testDaemon";
import { useProcessJump } from "./useProcessJump";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const first = consoleOf("c1", "First");
const second = consoleOf("c2", "Second");
const project = projectOf("p", second.id, "App");
const hub = sessionOf("hub", first.id, undefined, "Hub", "idle", { kind: "console" });

async function mount(asideFloats = false) {
  const switchConsole = vi.fn();
  const openProjectPanel = vi.fn();
  const showPanel = vi.fn();
  const daemon = testDaemon({ state: { consoles: new Map([[first.id, first], [second.id, second]]), projects: new Map([[project.id, project]]), sessions: new Map([[hub.id, hub]]) } });
  let jump!: (target: ProcessJump) => boolean;
  function Probe() { jump = useProcessJump({ asideFloats, switchConsole, openProjectPanel, showPanel }); return null; }
  const element = document.body.appendChild(document.createElement("div"));
  const root = createRoot(element);
  await act(async () => root.render(<DaemonProvider value={daemon}><Probe /></DaemonProvider>));
  const unmount = () => act(() => root.unmount());
  onTestFinished(() => { unmount(); element.remove(); });
  return { jump: async (target: ProcessJump) => {
    let landed = false;
    await act(async () => { landed = jump(target); await Promise.resolve(); });
    return landed;
  }, unmount, switchConsole, openProjectPanel, showPanel };
}

it("makes a project's console current and opens its Processes panel", async () => {
  const jump = await mount();
  await jump.jump({ project: project.id });
  expect(jump.switchConsole).toHaveBeenCalledWith(second.id);
  expect(jump.openProjectPanel).toHaveBeenCalledWith(project, "processes");
  expect(jump.showPanel).not.toHaveBeenCalled();
});

it("makes a console session's console current and opens that console session's Processes panel", async () => {
  const jump = await mount();
  await jump.jump({ consoleSession: hub.id });
  expect(jump.switchConsole).toHaveBeenCalledWith(first.id);
  expect(jump.showPanel).toHaveBeenCalledWith({ kind: "report", consoleSession: hub.id }, "processes");
  expect(jump.openProjectPanel).not.toHaveBeenCalled();
});

it("does nothing, and says so, for a project or console session that is gone", async () => {
  const jump = await mount();
  expect(await jump.jump({ project: "gone" })).toBe(false);
  expect(await jump.jump({ consoleSession: "gone" })).toBe(false);
  expect(jump.switchConsole).not.toHaveBeenCalled();
  expect(await jump.jump({ project: project.id })).toBe(true);
});

function addPanel() {
  const card = document.body.appendChild(document.createElement("section"));
  card.id = "panel-processes";
  const control = card.appendChild(document.createElement("button"));
  onTestFinished(() => card.remove());
  return { card, control };
}

it("sends focus into the Processes panel that is already shown", async () => {
  const jump = await mount();
  const { control } = addPanel();
  await jump.jump({ project: project.id });
  expect(document.activeElement).toBe(control);
});

it("sends focus into the Processes panel once it has rendered after the console switch", async () => {
  const jump = await mount();
  await jump.jump({ consoleSession: hub.id });
  expect(document.activeElement).toBe(document.body);
  let control!: HTMLButtonElement;
  await act(async () => { control = addPanel().control; await Promise.resolve(); });
  expect(document.activeElement).toBe(control);
});

it("leaves focus where the user moved it while the panel was coming", async () => {
  const jump = await mount();
  await jump.jump({ project: project.id });
  const elsewhere = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => elsewhere.remove());
  elsewhere.focus();
  await act(async () => { addPanel(); await Promise.resolve(); });
  expect(document.activeElement).toBe(elsewhere);
});

it("leaves the landing to the floating panel while the aside floats", async () => {
  const jump = await mount(true);
  addPanel();
  await jump.jump({ project: project.id });
  expect(document.activeElement).toBe(document.body);
});

it("waits for no panel after the unmount, and ends the wait of an earlier jump when a newer one comes", async () => {
  const waits = vi.spyOn(MutationObserver.prototype, "observe");
  const ends = vi.spyOn(MutationObserver.prototype, "disconnect");
  onTestFinished(() => { waits.mockRestore(); ends.mockRestore(); });
  const jump = await mount();
  await jump.jump({ project: project.id });
  await jump.jump({ consoleSession: hub.id });
  expect(waits).toHaveBeenCalledTimes(2);
  expect(ends).toHaveBeenCalledTimes(1);
  await jump.unmount();
  expect(ends).toHaveBeenCalledTimes(2);
  await act(async () => { addPanel(); await Promise.resolve(); });
  expect(document.activeElement).toBe(document.body);
});
