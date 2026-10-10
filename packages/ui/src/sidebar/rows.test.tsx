// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { expect, it } from "vitest";

import { TreeRow } from "./rows";

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
