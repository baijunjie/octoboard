// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { agentTagsValues, claudeCompatibilityValues } from "./importValues";

const t = ((key, params) => format("en", key, params as Record<string, string | number> | undefined)) as Translate;
const items = (values: { item: string }[]) => values.map(({ item }) => item);

describe("claudeCompatibilityValues", () => {
  it("lists every agent that has the setting, each On or Off, when there is nothing to compare with", () => {
    expect(claudeCompatibilityValues(t, { codex: false, grok: true, claude: true })).toEqual([
      { item: "Codex", value: "Off" },
      { item: "Grok Build", value: "On" },
    ]);
  });

  it("lists only the agents this machine reports with a different value", () => {
    expect(items(claudeCompatibilityValues(t, { codex: false }, { codex: true, grok: true }))).toEqual(["Codex"]);
    expect(claudeCompatibilityValues(t, { codex: true }, { codex: true, grok: true })).toEqual([]);
  });

  it("ignores an agent this machine does not report, and one without the setting", () => {
    expect(claudeCompatibilityValues(t, { codex: false }, {})).toEqual([]);
    expect(claudeCompatibilityValues(t, { claude: false }, { claude: true })).toEqual([]);
  });
});

describe("agentTagsValues", () => {
  it("lists only the agents whose tags would change, an agent missing from the current ones having no tags", () => {
    expect(items(agentTagsValues(t, "en", { codex: ["review"], grok: [], claude: ["same"] }, { grok: ["old"], claude: ["same"] }))).toEqual(["Codex", "Grok Build"]);
    expect(agentTagsValues(t, "en", { codex: [] }, {})).toEqual([]);
    expect(agentTagsValues(t, "en", { codex: ["a"] }, { codex: ["a"] })).toEqual([]);
  });

  it("ignores the tags of an agent this build does not know", () => {
    expect(agentTagsValues(t, "en", { someone_new: ["x"] } as never, {})).toEqual([]);
    expect(items(agentTagsValues(t, "en", { someone_new: ["x"] } as never))).toEqual([]);
  });
});
