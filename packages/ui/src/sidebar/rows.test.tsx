// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { expect, it, vi } from "vitest";

import { Plus } from "lucide-react";
import { InlineReference } from "../i18n/InlineReference";
import { isolate } from "../i18n/isolate";
import { LockButton, PinButton, RowIconButton, TreeRow } from "./rows";

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

// The pin and lock marks: each is the way to take its mark off, and takes itself away when pressed.
it.each([
  ["PinButton", "Unpin", undefined],
  ["PinButton with a shortcut", "Unpin", "P"],
  ["LockButton", "Unlock", "L"],
] as const)("%s names what it removes the mark from, hands focus to the row and does not press the row", (kind, verb, key) => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const rowClicked = vi.fn();
  const pressed = vi.fn();
  const row = React.createRef<HTMLDivElement>();
  const button = () =>
    kind === "LockButton" ? (
      <LockButton name="Chore" onUnlock={pressed} returnFocusTo={row} />
    ) : (
      <PinButton name="Chore" shortcut={kind !== "PinButton"} onUnpin={pressed} returnFocusTo={row} />
    );
  act(() =>
    root.render(
      <div ref={row} tabIndex={0} onClick={rowClicked}>
        {button()}
      </div>,
    ),
  );
  const mark = container.querySelector("button")!;
  expect(mark.getAttribute("aria-label")).toBe(`${verb} ${isolate("Chore")}`);
  expect(mark.querySelectorAll("svg")).toHaveLength(2);

  act(() => setInteractionModality("keyboard"));
  act(() => mark.focus());
  // The shortcut's keys follow the verb in the tooltip, in the platform's modifiers.
  const tooltip = document.querySelector('[role="tooltip"]')?.textContent;
  expect(tooltip?.startsWith(verb)).toBe(true);
  if (key) expect(tooltip?.endsWith(key)).toBe(true);
  else expect(tooltip).toBe(verb);

  act(() => mark.click());
  expect(pressed).toHaveBeenCalledTimes(1);
  expect(rowClicked).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(row.current);
  act(() => root.unmount());
  container.remove();
});
