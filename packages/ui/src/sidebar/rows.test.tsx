// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { expect, it } from "vitest";

import { Plus } from "lucide-react";
import { InlineChip } from "../components/InlineChip";
import { RowIconButton, TreeRow } from "./rows";

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
  act(() => root.render(<RowIconButton icon={Plus} label="New session in Project" title={<>New session in <InlineChip dir="auto" wrap>Project</InlineChip></>} unavailable={unavailable} onPress={() => {}} />));
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
