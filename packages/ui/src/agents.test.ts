import { describe, expect, it } from "vitest";

import { accountsByAgent, agentPickerOptions, noAgentAvailable, selectableAgent } from "./agents";
import { format } from "./i18n/catalog";
import type { Translate } from "./i18n/catalog";
import type { Account, Agent, AgentAvailability } from "./protocol";

// Real English messages rather than a stub, so a wording change that breaks a placeholder is
// caught here too.
const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;

const acct = (id: string, agent: Agent, name: string): Account => ({ id, agent, name, config_dir: `/home/${id}` });

const availability = (entries: Partial<Record<Agent, AgentAvailability["availability"]>>): Map<Agent, AgentAvailability> =>
  new Map(
    (["claude", "codex", "grok"] as Agent[]).map((agent) => [
      agent,
      { agent, availability: entries[agent] ?? "not_determined" },
    ]),
  );

describe("noAgentAvailable", () => {
  it("is false while nothing has been determined yet", () => {
    expect(noAgentAvailable(availability({}))).toBe(false);
  });

  it("is false once at least one agent is available", () => {
    expect(noAgentAvailable(availability({ claude: "available", codex: "unavailable", grok: "unavailable" }))).toBe(
      false,
    );
  });

  it("is true only once every agent has been determined unavailable", () => {
    expect(
      noAgentAvailable(availability({ claude: "unavailable", codex: "unavailable", grok: "unavailable" })),
    ).toBe(true);
  });
});

describe("agentPickerOptions", () => {
  it("leaves an agent that is not yet determined, or is available, selectable", () => {
    const options = agentPickerOptions(t, availability({ claude: "available" }));
    expect(options.find((o) => o.value === "claude")).toEqual({ value: "claude", label: "Claude Code" });
    expect(options.find((o) => o.value === "codex")?.isDisabled).toBeUndefined();
  });

  it("names an unavailable agent as not installed and marks it unselectable", () => {
    const options = agentPickerOptions(t, availability({ grok: "unavailable" }));
    const grok = options.find((o) => o.value === "grok");
    expect(grok?.label).toBe("Grok Build (not installed)");
    expect(grok?.isDisabled).toBe(true);
  });
});

describe("selectableAgent", () => {
  it.each([
    ["keeps an agent that is not yet determined", {}, "claude", "claude"],
    ["keeps an available agent", { claude: "available" }, "claude", "claude"],
    ["moves off an unavailable agent to the first that may be picked", { claude: "unavailable", codex: "unavailable" }, "claude", "grok"],
    ["stays put when none may be picked", { claude: "unavailable", codex: "unavailable", grok: "unavailable" }, "codex", "codex"],
  ] as const)("%s", (_, states, wanted, expected) => {
    expect(selectableAgent(availability(states), wanted)).toBe(expected);
  });
});

describe("accountsByAgent", () => {
  it("lists every agent in the usual order, each with its own accounts in stored order", () => {
    const grouped = accountsByAgent([acct("1", "grok", "G"), acct("2", "claude", "B"), acct("3", "claude", "A")]);
    expect(grouped.map((g) => [g.agent, g.accounts.map((a) => a.id)])).toEqual([
      ["claude", ["2", "3"]],
      ["codex", []],
      ["grok", ["1"]],
    ]);
  });
});
