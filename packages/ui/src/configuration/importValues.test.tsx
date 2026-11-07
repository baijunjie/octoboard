// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";

import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import type { ProviderInfo } from "../protocol";
import { TEST_AGENT_PROVIDER, TEST_PROVIDERS, UNPRESENTED_PROVIDER } from "../test/testProviders";
import { agentTagsValues, providerSettingLines } from "./importValues";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const t = ((key, params) => format("en", key, params as Record<string, string | number> | undefined)) as Translate;
const items = (values: { item: string }[]) => values.map(({ item }) => item);

describe("providerSettingLines", () => {
  it("lists every agent that has the setting under one line, each On or Off, when there is nothing to compare with", () => {
    expect(providerSettingLines(t, TEST_PROVIDERS, { codex: { claude_compatibility: false }, grok: { claude_compatibility: true } })).toMatchObject([
      { label: "Claude compatibility", values: [{ item: "Codex", value: "Off" }, { item: "Grok Build", value: "On" }] },
    ]);
  });

  it("lists only the agents this machine reports with a different value", () => {
    const current = { codex: { claude_compatibility: true }, grok: { claude_compatibility: true } };
    expect(items(providerSettingLines(t, TEST_PROVIDERS, { codex: { claude_compatibility: false } }, current)[0].values)).toEqual(["Codex"]);
    expect(providerSettingLines(t, TEST_PROVIDERS, { codex: { claude_compatibility: true } }, current)).toMatchObject([{ label: "Claude compatibility", values: [] }]);
  });

  it.each([
    ["codex", "approval_reviewer", "user", "Custom (config.toml)"],
    ["claude", "approval_mode", "manual", "Follow account configuration"],
    ["grok", "approval_mode", "ask", "Follow account configuration"],
  ])("names %s's unset current permission setting as its settings selector does", (agent, setting, value, inherited) => {
    const [line] = providerSettingLines(t, TEST_PROVIDERS, { [agent]: { [setting]: value } }, { [agent]: { [setting]: null } });
    const host = document.createElement("div");
    const root = createRoot(host);
    try {
      act(() => root.render(<>{line.values[0].value}</>));
      expect(host.textContent).toContain(inherited);
    } finally {
      act(() => root.unmount());
    }
  });

  it("ignores an agent this machine does not report, a setting the agent does not declare, and a value it does not take", () => {
    expect(providerSettingLines(t, TEST_PROVIDERS, { codex: { claude_compatibility: false } }, {})).toMatchObject([{ label: "Claude compatibility", values: [] }]);
    expect(providerSettingLines(t, TEST_PROVIDERS, { claude: { claude_compatibility: false } }, { claude: { claude_compatibility: true } })).toEqual([]);
    expect(providerSettingLines(t, TEST_PROVIDERS, { claude: { approval_mode: "invented" } })).toEqual([]);
    expect(providerSettingLines(t, TEST_PROVIDERS, { claude: { no_such_setting: true } })).toEqual([]);
    expect(providerSettingLines(t, TEST_PROVIDERS, { codex: { claude_compatibility: "yes" } })).toEqual([]);
    expect(providerSettingLines(t, [], { codex: { claude_compatibility: false } })).toEqual([]);
  });
});

describe("providerSettingLines for the test-only provider", () => {
  const exported = (provider: ProviderInfo): ProviderInfo => ({ ...provider, settings: provider.settings.map((setting) => ({ ...setting, exported: true })) });

  it("words a setting its presentation module has copy for", () => {
    expect(providerSettingLines(t, [exported(TEST_AGENT_PROVIDER)], { test_agent: { mode: "careful" } })).toMatchObject([
      { label: "Test mode", values: [{ item: "Test Agent", value: "Careful" }] },
    ]);
  });

  it("lists a setting of a provider served without a presentation module under its label and the setting's id", () => {
    const [line] = providerSettingLines(t, [exported(UNPRESENTED_PROVIDER)], { unpresented_agent: { mode: "careful" } });
    expect(line.values).toEqual([{ item: "Unpresented Agent", value: "Careful" }]);
    // Each name is isolated as a child of the sentence, not by isolate characters in a string.
    const host = document.createElement("div");
    act(() => createRoot(host).render(<>{line.label}</>));
    expect(host.innerHTML).toBe('<bdi dir="auto">Unpresented Agent</bdi>: <bdi dir="auto">Mode</bdi>');
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
