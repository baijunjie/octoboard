// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { QR_QUIET_ZONE, QrCode, qrPath } from "./QrCode";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ROWS = ["1101", "0010", "1111", "0100"];

/** The modules a path of `M x y h w v1 h-w z` runs covers, as "x,y" keys with the quiet zone taken off. */
function modulesOf(path: string): Set<string> {
  const modules = new Set<string>();
  for (const [, x, y, width] of path.matchAll(/M(\d+) (\d+)h(\d+)v1h-\d+z/g)) {
    for (let i = 0; i < Number(width); i++) modules.add(`${Number(x) + i - QR_QUIET_ZONE},${Number(y) - QR_QUIET_ZONE}`);
  }
  return modules;
}

it("draws exactly the dark modules in one path", () => {
  const expected = new Set<string>();
  ROWS.forEach((row, y) => [...row].forEach((module, x) => module === "1" && expected.add(`${x},${y}`)));
  expect(modulesOf(qrPath(ROWS))).toEqual(expected);
  expect(qrPath(["000"])).toBe("");
});

it("is a labelled image with a white square, a quiet zone of four modules and crisp edges", async () => {
  const container = document.createElement("div");
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); });
  await act(async () => root.render(<QrCode rows={ROWS} label="QR code of the connection details" />));
  const svg = container.querySelector("svg")!;
  expect(svg.getAttribute("role")).toBe("img");
  expect(svg.getAttribute("aria-label")).toBe("QR code of the connection details");
  expect(svg.getAttribute("viewBox")).toBe("0 0 12 12");
  expect(svg.getAttribute("shape-rendering")).toBe("crispEdges");
  expect(svg.querySelector("rect")!.getAttribute("fill")).toBe("#fff");
  expect(svg.querySelectorAll("path")).toHaveLength(1);
  // The first dark module of the first row starts after the quiet zone.
  expect(svg.querySelector("path")!.getAttribute("d")).toMatch(/^M4 4h2v1h-2z/);
});
