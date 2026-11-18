// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";

import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import type { ProviderInfo, SettingKind } from "../protocol";
import { TEST_AGENT_PROVIDER, TEST_PROVIDERS, UNPRESENTED_PROVIDER } from "../test/testProviders";
import { agentTagsValues, gitWorkflowValues, providerSettingLines } from "./importValues";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const t = ((key, params) => format("en", key, params as Record<string, string | number> | undefined)) as Translate;

describe("providerSettingLines", () => {
  it("names a setting by its heading alone and holds the agent's value, On or Off, when there is nothing to compare with", () => {
    expect(providerSettingLines(t, TEST_PROVIDERS, "codex", { claude_compatibility: false })).toMatchObject([
      { label: "Claude compatibility", values: [{ value: "Off" }] },
    ]);
    expect(providerSettingLines(t, TEST_PROVIDERS, "grok", { claude_compatibility: true })[0].values).toEqual([{ value: "On" }]);
  });

  it("holds a value only where this machine reports a different one", () => {
    const current = { codex: { claude_compatibility: true }, grok: { claude_compatibility: true } };
    expect(providerSettingLines(t, TEST_PROVIDERS, "codex", { claude_compatibility: false }, current)[0].values).toHaveLength(1);
    expect(providerSettingLines(t, TEST_PROVIDERS, "codex", { claude_compatibility: true }, current)).toMatchObject([{ label: "Claude compatibility", values: [] }]);
  });

  it.each([
    ["codex", "approval_reviewer", "user", "Custom (config.toml)"],
    ["claude", "approval_mode", "manual", "Follow account configuration"],
    ["grok", "approval_mode", "ask", "Follow account configuration"],
  ] as const)("names %s's unset current permission setting as its settings selector does", (agent, setting, value, inherited) => {
    const [line] = providerSettingLines(t, TEST_PROVIDERS, agent, { [setting]: value }, { [agent]: { [setting]: null } });
    const host = document.createElement("div");
    const root = createRoot(host);
    try {
      act(() => root.render(<>{line.values[0].value}</>));
      expect(host.textContent).toContain(inherited);
    } finally {
      act(() => root.unmount());
    }
  });

  it("ignores a setting the agent does not declare and a value it does not take, and counts one this machine does not report as unchanged", () => {
    expect(providerSettingLines(t, TEST_PROVIDERS, "codex", { claude_compatibility: false }, {})).toMatchObject([{ label: "Claude compatibility", values: [] }]);
    expect(providerSettingLines(t, TEST_PROVIDERS, "claude", { claude_compatibility: false }, { claude: { claude_compatibility: true } })).toEqual([]);
    expect(providerSettingLines(t, TEST_PROVIDERS, "claude", { approval_mode: "invented" })).toEqual([]);
    expect(providerSettingLines(t, TEST_PROVIDERS, "claude", { no_such_setting: true })).toEqual([]);
    expect(providerSettingLines(t, TEST_PROVIDERS, "codex", { claude_compatibility: "yes" })).toEqual([]);
    expect(providerSettingLines(t, TEST_PROVIDERS, "codex", undefined)).toEqual([]);
    expect(providerSettingLines(t, [], "codex", { claude_compatibility: false })).toEqual([]);
  });
});

describe("providerSettingLines for the test-only provider", () => {
  const exported = (provider: ProviderInfo): ProviderInfo => ({ ...provider, settings: provider.settings.map((setting) => ({ ...setting, exported: true })) });

  it("words a setting its presentation module has copy for", () => {
    expect(providerSettingLines(t, [exported(TEST_AGENT_PROVIDER)], "test_agent", { mode: "careful" })).toMatchObject([
      { label: "Test mode", values: [{ value: "Careful" }] },
    ]);
  });

  it("names a setting of a provider served without a presentation module by the setting's id alone", () => {
    expect(providerSettingLines(t, [exported(UNPRESENTED_PROVIDER)], "unpresented_agent", { mode: "careful" })).toMatchObject([
      { label: "Mode", values: [{ value: "Careful" }] },
    ]);
  });

  it("leaves out a setting the provider does not export", () => {
    expect(providerSettingLines(t, [TEST_AGENT_PROVIDER], "test_agent", { mode: "careful" })).toEqual([]);
  });
});

describe("agentTagsValues", () => {
  it("holds the agent's tags, only where they would change, an agent missing from the current ones having no tags", () => {
    expect(agentTagsValues(t, "en", "codex", { codex: ["review"] })).toHaveLength(1);
    expect(agentTagsValues(t, "en", "codex", { codex: ["review"], grok: [], claude: ["same"] }, { grok: ["old"], claude: ["same"] })).toHaveLength(1);
    expect(agentTagsValues(t, "en", "grok", { codex: ["review"], grok: [], claude: ["same"] }, { grok: ["old"], claude: ["same"] })).toHaveLength(1);
    expect(agentTagsValues(t, "en", "claude", { codex: ["review"], grok: [], claude: ["same"] }, { grok: ["old"], claude: ["same"] })).toEqual([]);
    expect(agentTagsValues(t, "en", "codex", { codex: [] }, {})).toEqual([]);
  });

  it("holds nothing for an agent the tags do not name", () => {
    expect(agentTagsValues(t, "en", "grok", { codex: ["a"] })).toEqual([]);
  });
});

describe("what a newer daemon sends that this build does not know", () => {
  const unknownKind = { type: "from_a_newer_daemon" } as unknown as SettingKind;
  const withKind = (kind: SettingKind): ProviderInfo => ({ ...TEST_AGENT_PROVIDER, settings: TEST_AGENT_PROVIDER.settings.map((setting) => ({ ...setting, exported: true, kind })) });

  it("holds no line for a provider setting of a kind it does not know", () => {
    expect(providerSettingLines(t, [withKind(unknownKind)], "test_agent", { mode: "careful" })).toEqual([]);
  });

  it("leaves out a git workflow item whose wanted choice it does not know, and shows one whose current choice it does not know alone", () => {
    const workflow = { development: "from_a_newer_daemon", squash_before_delivery: true, rebase_before_delivery: true, delivery: "carrier_pigeon", revert_gate: true };
    const text = (value: React.ReactNode) => {
      const host = document.createElement("div");
      const root = createRoot(host);
      try {
        act(() => root.render(<>{value}</>));
        return host.textContent;
      } finally {
        act(() => root.unmount());
      }
    };
    // Wanted unknown: no item, whether or not there is a current value.
    expect(gitWorkflowValues(t, workflow).map((value) => value.item)).not.toContain("Where sessions develop");
    expect(gitWorkflowValues(t, workflow).map((value) => value.item)).not.toContain("Delivery");
    expect(gitWorkflowValues(t, workflow, { ...workflow, development: "branch", delivery: "merge" }).map((value) => value.item)).not.toContain("Where sessions develop");
    // Current unknown: the wanted choice alone, without "(now …)".
    const against = gitWorkflowValues(t, { ...workflow, development: "branch", delivery: "merge" }, workflow);
    expect(against.map((value) => value.item)).toEqual(["Where sessions develop", "Delivery"]);
    for (const value of against) expect(text(value.value)).not.toMatch(/now/i);
    // Both known: the change with what it is now.
    const known = gitWorkflowValues(t, { ...workflow, development: "branch", delivery: "merge" }, { ...workflow, development: "worktree", delivery: "ask" });
    expect(text(known[0]!.value)).toMatch(/now/i);
  });
});
