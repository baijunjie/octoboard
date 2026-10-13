import { expect, it } from "vitest";

import { CODE_THEMES } from "./codeTheme";
import { diagramConfig, diagramReading } from "./diagramConfig";

it("configures a diagram so it cannot emit HTML, in either appearance", () => {
  for (const appearance of ["light", "dark"] as const) {
    const config = diagramConfig(appearance);
    expect(config.securityLevel).toBe("strict");
    expect(config.htmlLabels).toBe(false);
    expect(config.secure).toEqual(expect.arrayContaining(["htmlLabels", "securityLevel", "startOnLoad"]));
    expect(config.startOnLoad).toBe(false);
    expect(config.suppressErrorRendering).toBe(true);
    expect(config.darkMode).toBe(appearance === "dark");
    expect(config.themeVariables).toMatchObject({
      background: CODE_THEMES[appearance].background,
      primaryTextColor: CODE_THEMES[appearance].foreground,
      lineColor: CODE_THEMES[appearance].foreground,
    });
  }
});

it("reads a diagram's own title and description", () => {
  expect(diagramReading("graph TD\nA-->B")).toEqual({});
  expect(diagramReading("accTitle: Flow\naccDescr: From A to B\ngraph TD\nA-->B")).toEqual({ title: "Flow", description: "From A to B" });
  expect(diagramReading("accDescr {two lines\nof text}\ngraph TD\nA-->B").description).toBe("two lines\nof text");
});
