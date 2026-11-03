// @vitest-environment jsdom
import { act, useSyncExternalStore } from "react";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished, vi } from "vitest";

import type { AgentModels } from "./useAgentModels";
import { openAdvanced, pickerNamed, render } from "./consoleDialogTestSupport";

// The list of models, set by each test, as the daemon's answers would leave it.
const FAILED: AgentModels = { status: "ready", models: [], unavailable: { code: "models_timed_out", params: { agent: "Claude Code" }, message: "x" }, defaultModel: null, defaultModelSource: null };
const LISTED: AgentModels = { status: "ready", models: [{ id: "alpha", name: "Alpha", efforts: [] }], unavailable: null, defaultModel: null, defaultModelSource: null };
const LOADING: AgentModels = { status: "loading", models: [], unavailable: null, defaultModel: null, defaultModelSource: null };
let models = FAILED;
const listeners = new Set<() => void>();
const setModels = (next: AgentModels) => act(() => {
  models = next;
  for (const listener of listeners) listener();
});
vi.mock("./useAgentModels", () => ({
  useAgentModels: () => useSyncExternalStore((listener) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  }, () => models),
}));

const marks = () => document.body.querySelectorAll<HTMLElement>("button[data-status-mark][aria-label='Could not load the models']");
const selectedTab = () => document.body.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');

/** The dialog with the Advanced section open on a failed list, focus on the mark after a mouse press. */
async function openOnFailure() {
  models = FAILED;
  const modality = getInteractionModality();
  onTestFinished(() => act(() => Reflect.apply(setInteractionModality, undefined, [modality])));
  render();
  await openAdvanced();
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", bubbles: true }));
  act(() => {
    setInteractionModality("pointer");
    marks()[0]!.focus();
  });
  expect(document.activeElement).toBe(marks()[0]);
}

it("hands focus from the Model mark to the console's Model trigger, with its ring, when the list comes", async () => {
  await openOnFailure();
  setModels(LISTED);
  await act(async () => {});
  expect(marks()).toHaveLength(0);
  const trigger = pickerNamed("Model");
  expect(document.activeElement).toBe(trigger);
  expect(getInteractionModality()).toBe("keyboard");
  expect(trigger.hasAttribute("data-focus-visible")).toBe(true);
});

it("hands focus from the Model mark to the selected agent tab when the render that takes it locks the trigger", async () => {
  await openOnFailure();
  setModels(LOADING);
  await act(async () => {});
  expect(marks()).toHaveLength(0);
  expect((pickerNamed("Model") as HTMLButtonElement).disabled).toBe(true);
  expect(document.activeElement).toBe(selectedTab());
});
