// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { SLEEP_PATHS } from "./sleepGlyph";
import { SleepGlyph } from "./statusGlyphs";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(): SVGSVGElement {
  const container = document.createElement("div");
  act(() => createRoot(container).render(<SleepGlyph className="size-4" />));
  return container.querySelector("svg")!;
}

const classesOf = (element: Element): string[] => (element.getAttribute("class") ?? "").split(/\s+/);

it("draws exactly SLEEP_PATHS, unmoved, under reduced motion", () => {
  const svg = mount();
  const [still, drifting] = [...svg.querySelectorAll("g")];
  expect(classesOf(still)).toContain("motion-safe:hidden");
  expect(classesOf(drifting)).toContain("motion-reduce:hidden");
  expect([...still.querySelectorAll("path")].map((path) => path.getAttribute("d"))).toEqual([...SLEEP_PATHS]);
  // Nothing in what shows under reduced motion is animated or transformed.
  expect(still.querySelectorAll("[class], [style]")).toHaveLength(0);
  expect(still.getAttribute("style")).toBeNull();
});

it("applies every animation class only where motion is allowed, and clips the drifting Zs to the svg's own box", () => {
  const svg = mount();
  const names = [svg, ...svg.querySelectorAll("*")].flatMap(classesOf).filter((name) => name.includes("animate-"));
  expect(names).toEqual(["motion-safe:animate-status-scroll", "motion-safe:animate-status-scroll-second", "motion-safe:animate-status-scroll-third"]);
  expect(classesOf(svg)).toEqual(expect.arrayContaining(["contain-layout", "overflow-hidden"]));
});
