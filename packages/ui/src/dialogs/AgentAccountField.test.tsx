// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { describe, expect, it, onTestFinished } from "vitest";

import { fakeTimers } from "../test/clock";
import { mountHost } from "../test/mountHost";
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
  // A synchronous update, as the daemon's store makes: the field's own handoff runs in the same commit's effects.
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
  // The last input was a key, whose ring the handoff follows; the modality is put back to the pointer's before it.
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
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

describe("the warning under the pair", () => {
  const warning = { title: "Account Work was last read as logged out", message: "The session opens all the same.", remedy: "If it asks, log in." };
  const mountField = (props: Partial<React.ComponentProps<typeof AgentAccountField>>) => {
    fakeTimers();
    return mountHost(
      <AgentAccountField
        label="Agent"
        options={[{ value: "claude", label: "Claude Code" }]}
        value="claude"
        onChange={() => {}}
        account={{ label: "Account", options: [{ value: "work", label: "Work" }], value: "work", onChange: () => {} }}
        {...props}
      />,
    );
  };
  const triggers = (container: HTMLElement) => [...container.querySelectorAll<HTMLElement>("[data-slot=select-trigger]")];
  const warningOf = (container: HTMLElement) => container.querySelector<HTMLElement>('[data-slot="alert-root"]');

  it("stands under the pickers, which it describes, without marking them invalid", () => {
    const container = mountField({ warning });
    const alert = warningOf(container)!;
    // The alert carries its id on the element that wraps it, which is what the pickers point to.
    const warningId = alert.closest("[id]")!.id;
    expect(warningId).not.toBe("");
    expect(alert.textContent).toContain(warning.title);
    expect(alert.textContent).toContain(warning.message);
    expect(alert.textContent).toContain(warning.remedy);
    for (const trigger of triggers(container)) {
      expect(trigger.getAttribute("aria-describedby")?.split(" ")).toContain(warningId);
      expect(trigger.getAttribute("aria-invalid")).not.toBe("true");
    }
  });

  it("gives way to the error where there is one, which is the refusal", () => {
    const container = mountField({ warning, errorMessage: "Cannot be used." });
    expect(container.textContent).toContain("Cannot be used.");
    expect(container.textContent).not.toContain(warning.title);
    expect(warningOf(container)).toBeNull();
  });

  it("is absent without one", () => {
    expect(warningOf(mountField({}))).toBeNull();
  });
});
