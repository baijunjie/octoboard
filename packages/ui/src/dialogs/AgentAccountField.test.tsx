// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished } from "vitest";

import { AgentAccountField } from "./AgentAccountField";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("hands focus from the account's mark to the account picker, with its ring, when the mark goes", async () => {
  const modality = getInteractionModality();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => Reflect.apply(setInteractionModality, undefined, [modality]));
  });
  // A synchronous update, as the daemon's store makes: the field's own hand-off runs in the same commit's effects.
  const render = (marked: boolean) => act(() => root.render(
    <AgentAccountField
      label="Agent"
      options={[{ value: "claude", label: "Claude Code" }]}
      value="claude"
      onChange={() => {}}
      account={{
        label: "Account",
        options: [{ value: "work", label: "Work" }],
        value: "work",
        onChange: () => {},
        mark: marked ? (markId) => <button type="button" data-status-mark-id={markId}>Why</button> : undefined,
      }}
    />,
  ));
  render(true);
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", bubbles: true }));
  act(() => {
    setInteractionModality("pointer");
    container.querySelector<HTMLElement>("[data-status-mark-id]")!.focus();
  });
  render(false);
  await act(async () => {});
  const [, accountTrigger] = container.querySelectorAll<HTMLElement>("[data-slot=select-trigger]");
  expect(document.activeElement).toBe(accountTrigger);
  expect(getInteractionModality()).toBe("keyboard");
  expect(accountTrigger!.hasAttribute("data-focus-visible")).toBe(true);
});
