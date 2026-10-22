// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { RowControls } from "./RowControls";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function clipOf(element: React.ReactElement): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(element));
  return container.querySelector<HTMLElement>("button")!.parentElement!.parentElement!;
}

it("always shows hover-revealed row actions on a device that cannot hover, and keeps them clipped for a mouse", () => {
  const clip = clipOf(<RowControls><button type="button">Actions</button></RowControls>);
  expect(clip.className).toContain("max-w-0 overflow-hidden opacity-0");
  expect(clip.className).toContain("[@media(hover:none)]:max-w-none [@media(hover:none)]:overflow-visible [@media(hover:none)]:opacity-100");
  expect(clip.className).toContain("group-hover:opacity-100");
});

it("shows reserved row actions on a device that cannot hover", () => {
  const clip = clipOf(<RowControls reserve><button type="button">Actions</button></RowControls>);
  expect(clip.className).toContain("opacity-0");
  expect(clip.className).toContain("[@media(hover:none)]:opacity-100");
});
