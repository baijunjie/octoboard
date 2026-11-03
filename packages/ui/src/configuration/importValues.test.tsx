// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { format } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Translate } from "../i18n/catalog";
import type { ProviderInfo } from "../protocol";
import { TEST_AGENT_PROVIDER, TEST_PROVIDERS, UNPRESENTED_PROVIDER } from "../test/testProviders";
import { agentTagsValues, providerSettingLines } from "./importValues";

const t = ((key, params) => format("en", key, params as Record<string, string | number> | undefined)) as Translate;
const items = (values: { item: string }[]) => values.map(({ item }) => item);

describe("providerSettingLines", () => {
  it("lists every agent that has the setting under one line, each On or Off, when there is nothing to compare with", () => {
    expect(providerSettingLines(t, TEST_PROVIDERS, { codex: { claude_compatibility: false }, grok: { claude_compatibility: true } })).toEqual([
      { label: "Claude compatibility", values: [{ item: "Codex", value: "Off" }, { item: "Grok Build", value: "On" }] },
    ]);
  });

  it("lists only the agents this machine reports with a different value", () => {
    const current = { codex: { claude_compatibility: true }, grok: { claude_compatibility: true } };
    expect(items(providerSettingLines(t, TEST_PROVIDERS, { codex: { claude_compatibility: false } }, current)[0].values)).toEqual(["Codex"]);
    expect(providerSettingLines(t, TEST_PROVIDERS, { codex: { claude_compatibility: true } }, current)).toEqual([{ label: "Claude compatibility", values: [] }]);
  });

  it("ignores an agent this machine does not report, a setting the agent does not declare or exports, and a value it does not take", () => {
    expect(providerSettingLines(t, TEST_PROVIDERS, { codex: { claude_compatibility: false } }, {})).toEqual([{ label: "Claude compatibility", values: [] }]);
    expect(providerSettingLines(t, TEST_PROVIDERS, { claude: { claude_compatibility: false } }, { claude: { claude_compatibility: true } })).toEqual([]);
    expect(providerSettingLines(t, TEST_PROVIDERS, { claude: { approval_mode: "bypass" } })).toEqual([]);
    expect(providerSettingLines(t, TEST_PROVIDERS, { codex: { claude_compatibility: "yes" } })).toEqual([]);
    expect(providerSettingLines(t, [], { codex: { claude_compatibility: false } })).toEqual([]);
  });
});

describe("providerSettingLines for the test-only provider", () => {
  const exported = (provider: ProviderInfo): ProviderInfo => ({ ...provider, settings: provider.settings.map((setting) => ({ ...setting, exported: true })) });

  it("words a setting its presentation module has copy for", () => {
    expect(providerSettingLines(t, [exported(TEST_AGENT_PROVIDER)], { test_agent: { mode: "careful" } })).toEqual([
      { label: "Test mode", values: [{ item: "Test Agent", value: "Careful" }] },
    ]);
  });

  it("lists a setting of a provider served without a presentation module under its label and the setting's id", () => {
    expect(providerSettingLines(t, [exported(UNPRESENTED_PROVIDER)], { unpresented_agent: { mode: "careful" } })).toEqual([
      { label: `${isolate("Unpresented Agent")}: ${isolate("Mode")}`, values: [{ item: "Unpresented Agent", value: "Careful" }] },
    ]);
  });

  it("puts an unpresented agent beside the presented ones under the heading they give the same setting id", () => {
    const lines = providerSettingLines(t, [exported(TEST_AGENT_PROVIDER), exported(UNPRESENTED_PROVIDER)], { test_agent: { mode: "careful" }, unpresented_agent: { mode: "careful" } });
    expect(lines.map(({ label }) => label)).toEqual(["Test mode"]);
  });

  it("leaves out a setting the provider does not export", () => {
    expect(providerSettingLines(t, [TEST_AGENT_PROVIDER], { test_agent: { mode: "careful" } })).toEqual([]);
  });
});

describe("agentTagsValues", () => {
  it("lists only the agents whose tags would change, an agent missing from the current ones having no tags", () => {
    expect(items(agentTagsValues(t, "en", TEST_PROVIDERS, { codex: ["review"], grok: [], claude: ["same"] }, { grok: ["old"], claude: ["same"] }))).toEqual(["Codex", "Grok Build"]);
    expect(agentTagsValues(t, "en", TEST_PROVIDERS, { codex: [] }, {})).toEqual([]);
    expect(agentTagsValues(t, "en", TEST_PROVIDERS, { codex: ["a"] }, { codex: ["a"] })).toEqual([]);
  });

  it("ignores the tags of an agent the daemon does not serve", () => {
    expect(agentTagsValues(t, "en", TEST_PROVIDERS, { someone_new: ["x"] } as never, {})).toEqual([]);
    expect(items(agentTagsValues(t, "en", TEST_PROVIDERS, { someone_new: ["x"] } as never))).toEqual([]);
  });
});
