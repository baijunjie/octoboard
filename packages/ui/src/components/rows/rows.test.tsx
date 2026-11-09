// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { Plus } from "lucide-react";
import { InlineReference } from "../../i18n/InlineReference";
import { Button } from "../../primitives";
import { RowIconButton } from "./RowIconButton";
import { accessibilityActionElement } from "./rowPressTestSupport";
import { TreeRow } from "./TreeRow";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Pins the row's ring to react-aria's input modality rather than CSS `:focus-visible`.
it.each([
  ["keyboard", true],
  ["pointer", false],
] as const)("a row focused by script shows its ring only after keyboard input (%s → %s)", (modality, ring) => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <TreeRow ariaLabel="row" onActivate={() => {}}>
        row
      </TreeRow>,
    ),
  );
  const row = container.querySelector<HTMLElement>("[role=button]")!;
  act(() => setInteractionModality(modality));
  act(() => row.focus());
  expect(row.hasAttribute("data-focus-visible")).toBe(ring);
  act(() => root.unmount());
  container.remove();
});


it.each([undefined, "Quota is used up"])("preserves tooltip name suppression and the unavailable reason (%s)", (unavailable) => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<RowIconButton icon={Plus} label="New session in Project" title={<>New session in <InlineReference dir="auto" wrap>Project</InlineReference></>} unavailable={unavailable} onPress={() => {}} />));
  const button = container.querySelector("button")!;
  act(() => setInteractionModality("keyboard"));
  act(() => button.focus());
  const tooltip = document.querySelector('[role="tooltip"]');
  expect(tooltip?.textContent).toBe(unavailable ?? "New session in Project");
  expect(button.getAttribute("aria-label")).toBe("New session in Project");
  const description = button.getAttribute("aria-describedby");
  if (unavailable) {
    expect(document.getElementById(description!)?.textContent).toBe(unavailable);
    expect(description).not.toBe(tooltip?.id);
  } else expect(description).toBeFalsy();
  act(() => root.unmount());
  container.remove();
});

function mountRow(onActivate: () => void, onChildPress: () => void) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <TreeRow ariaLabel="row" onActivate={onActivate}>
        <Button onPress={onChildPress}>Child action</Button>
      </TreeRow>,
    ),
  );
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return container.querySelector<HTMLElement>("[role=button]")!;
}

it("answers an accessibility press with its own target, activating the row and none of its children", () => {
  const onActivate = vi.fn();
  const onChildPress = vi.fn();
  const row = mountRow(onActivate, onChildPress);
  const target = accessibilityActionElement(row)!;
  expect(target).not.toBeNull();
  expect(target.parentElement).toBe(row);
  act(() => target.click());
  expect(onActivate).toHaveBeenCalledTimes(1);
  expect(onChildPress).not.toHaveBeenCalled();
});

it("keeps the row's press target out of the tab order and the accessibility tree", () => {
  const row = mountRow(() => {}, () => {});
  const target = accessibilityActionElement(row)!;
  expect(target.hidden).toBe(true);
  expect(target.getAttribute("aria-hidden")).toBe("true");
  expect(target.tabIndex).toBe(-1);
  expect(getComputedStyle(target).display).toBe("none");
});
