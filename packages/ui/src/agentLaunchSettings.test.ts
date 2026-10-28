import { describe, expect, it } from "vitest";

import { effortLevels, launchSettingOptions } from "./agentLaunchSettings";
import { format } from "./i18n/catalog";
import type { Translate } from "./i18n/catalog";
import type { AgentModel } from "./protocol";

// Real English messages rather than a stub, so a wording change that breaks a placeholder is
// caught here too.
const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;

const plain = ["low", "medium", "high"].map((value) => ({ value, label: value }));

describe("launchSettingOptions", () => {
  it("offers the list as it is, with no entry for unset, when nothing is stored", () => {
    expect(launchSettingOptions(t, plain, null, true)).toEqual(plain);
  });

  it("adds nothing when the stored value is already in the list", () => {
    expect(launchSettingOptions(t, plain, "medium", true)).toEqual(plain);
  });

  it("keeps a stored value that is no longer in the list, last and marked", () => {
    expect(launchSettingOptions(t, plain, "ultra", true)).toEqual([
      ...plain,
      { value: "ultra", label: "ultra", badge: "Not in the list" },
    ]);
  });

  it("offers the stored value alone, unmarked, while the list is not known", () => {
    expect(launchSettingOptions(t, [], "ultra", false)).toEqual([{ value: "ultra", label: "ultra" }]);
    expect(launchSettingOptions(t, plain, "ultra", false).at(-1)).toEqual({ value: "ultra", label: "ultra" });
  });

  it("marks the stored value of a list that is known and empty", () => {
    expect(launchSettingOptions(t, [], "ultra", true)).toEqual([{ value: "ultra", label: "ultra", badge: "Not in the list" }]);
  });

  it("marks mixed-script missing values without rewriting them or mutating the live list", () => {
    const current = "vendor/中文-مراجعة (custom)";
    const known = [{ value: "listed", label: "Listed model" }];
    const options = launchSettingOptions(t, known, current, true);
    expect(options.at(-1)).toEqual({ value: current, label: current, badge: "Not in the list" });
    expect(known).toEqual([{ value: "listed", label: "Listed model" }]);
    expect(options[0]).toBe(known[0]);
    expect(options.at(-1)?.value).not.toMatch(/[\u2066-\u2069]/);
    expect(launchSettingOptions(t, options, current, true)).toBe(options);
  });
});

describe("effortLevels", () => {
  const models: AgentModel[] = [
    { id: "a", name: "A", efforts: ["low", "high"] },
    { id: "b", name: "B", efforts: [] },
    { id: "d", name: "D", efforts: ["high", "max"] },
  ];

  it("offers a chosen model's own levels", () => {
    expect(effortLevels(models, "a")).toEqual({ levels: ["low", "high"], none: false });
  });

  it("says a model that takes none takes none", () => {
    expect(effortLevels(models, "b")).toEqual({ levels: [], none: true });
  });

  it("offers every listed level, once, in listed order, when nothing is chosen or the model is not listed", () => {
    const union = { levels: ["low", "high", "max"], none: false };
    expect(effortLevels(models, null)).toEqual(union);
    expect(effortLevels(models, "gone")).toEqual(union);
  });

  it("offers nothing without a list", () => {
    expect(effortLevels([], "a")).toEqual({ levels: [], none: false });
  });
});
