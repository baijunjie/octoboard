import { describe, expect, it } from "vitest";

import { accountsByAgent, agentChoiceBadge, agentPickerOptions, defaultAccountName, listedPickerAgents, noAgentState, projectSessionAgent, selectableAgent } from "./agents";
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

describe("noAgentState", () => {
  it("is undefined while nothing has been determined yet", () => {
    expect(noAgentState(availability({}), null)).toBeUndefined();
  });

  it("is undefined once at least one agent is available", () => {
    expect(noAgentState(availability({ claude: "available", codex: "unavailable", grok: "unavailable" }), null)).toBeUndefined();
  });

  it("asks to install one only once every agent has been determined unavailable", () => {
    expect(noAgentState(availability({ claude: "unavailable", codex: "unavailable", grok: "unavailable" }), null)).toBe("install");
  });

  it("counts only agents that are added and installed, asking to add one while an agent not known to be missing waits", () => {
    const states = availability({ claude: "available", codex: "unavailable", grok: "available" });
    expect(noAgentState(states, [])).toBe("add");
    expect(noAgentState(states, ["codex"])).toBe("add");
    expect(noAgentState(states, ["codex", "grok"])).toBeUndefined();
  });
});

describe("agentPickerOptions", () => {
  it("leaves an agent that is not yet determined, or is available, selectable", () => {
    const options = agentPickerOptions(t, availability({ claude: "available" }), null);
    expect(options.find((o) => o.value === "claude")).toEqual({ value: "claude", label: "Claude Code" });
    expect(options.find((o) => o.value === "codex")?.isDisabled).toBeUndefined();
  });

  it("lists every agent before availability has been determined and while the added list is undecided", () => {
    expect(agentPickerOptions(t, availability({}), null).map((o) => o.value)).toEqual(["claude", "codex", "grok"]);
  });

  it("leaves out an agent known to be missing", () => {
    const options = agentPickerOptions(t, availability({ grok: "unavailable" }), null);
    expect(options.map((o) => o.value)).toEqual(["claude", "codex"]);
  });

  it("keeps the held agent when it is missing, badged as not installed and unselectable", () => {
    const options = agentPickerOptions(t, availability({ grok: "unavailable", codex: "unavailable" }), null, "grok");
    expect(options.map((o) => o.value)).toEqual(["claude", "grok"]);
    const grok = options.find((o) => o.value === "grok");
    expect(grok?.label).toBe("Grok Build");
    expect(grok?.badge).toBe("Not installed");
    expect(grok?.isDisabled).toBe(true);
  });

  it("stops listing a missing agent once it is no longer held", () => {
    expect(agentPickerOptions(t, availability({ grok: "unavailable" }), null, "claude").map((o) => o.value)).toEqual(["claude", "codex"]);
  });

  it("lists only the added agents, in the order they were added", () => {
    expect(agentPickerOptions(t, availability({}), ["grok", "claude"]).map((o) => o.value)).toEqual(["grok", "claude"]);
  });

  it("leaves out an added agent that is not installed", () => {
    expect(agentPickerOptions(t, availability({ codex: "unavailable" }), ["codex", "claude"]).map((o) => o.value)).toEqual(["claude"]);
  });

  it("keeps a held agent that is not added, after the added ones, badged as not added and unselectable", () => {
    const options = agentPickerOptions(t, availability({ claude: "available", codex: "available" }), ["codex"], "claude");
    expect(options).toEqual([
      { value: "codex", label: "Codex" },
      { value: "claude", label: "Claude Code", badge: "Not added", isDisabled: true },
    ]);
  });

  it("names a held agent that is neither added nor installed as not installed", () => {
    const options = agentPickerOptions(t, availability({ grok: "unavailable" }), ["claude"], "grok");
    expect(options.find((o) => o.value === "grok")?.badge).toBe("Not installed");
  });

  it("offers nothing when no agent is added and none is held", () => {
    expect(agentPickerOptions(t, availability({}), [])).toEqual([]);
  });
});

describe("listedPickerAgents", () => {
  it("lists the agents that may be picked in the added order, then the others the caller keeps", () => {
    const states = availability({ codex: "unavailable" });
    expect(listedPickerAgents(states, ["grok", "codex"], (agent) => agent === "codex" || agent === "claude")).toEqual(["grok", "codex", "claude"]);
    expect(listedPickerAgents(states, ["grok", "codex"], () => false)).toEqual(["grok"]);
  });

  it("puts a kept agent that is added but missing after the ones that may be picked, not at its added position", () => {
    expect(listedPickerAgents(availability({ codex: "unavailable" }), ["codex", "claude"], (agent) => agent === "codex")).toEqual(["claude", "codex"]);
  });
});

describe("agentChoiceBadge", () => {
  it.each([
    ["an added, installed agent", {}, ["claude"], undefined],
    ["an agent while the added list is undecided", {}, null, undefined],
    ["a missing agent", { claude: "unavailable" }, ["claude"], "Not installed"],
    ["an agent that is not added", {}, ["codex"], "Not added"],
  ] as const)("badges %s", (_, states, added, expected) => {
    expect(agentChoiceBadge(t, availability(states), added as Agent[] | null, "claude")).toBe(expected);
  });
});

describe("projectSessionAgent", () => {
  it("follows the console session agent unless the console names one of its own", () => {
    expect(projectSessionAgent({ default_agent: "claude" })).toBe("claude");
    expect(projectSessionAgent({ default_agent: "claude", project_session_agent: null })).toBe("claude");
    expect(projectSessionAgent({ default_agent: "claude", project_session_agent: "grok" })).toBe("grok");
  });
});

describe("selectableAgent", () => {
  it.each([
    ["keeps an agent that is not yet determined", {}, "claude", "claude"],
    ["keeps an available agent", { claude: "available" }, "claude", "claude"],
    ["moves off an unavailable agent to the first that may be picked", { claude: "unavailable", codex: "unavailable" }, "claude", "grok"],
    ["stays put when none may be picked", { claude: "unavailable", codex: "unavailable", grok: "unavailable" }, "codex", "codex"],
  ] as const)("%s", (_, states, wanted, expected) => {
    expect(selectableAgent(availability(states), null, wanted)).toBe(expected);
  });

  it("moves off an agent that is not added to the first added one that may be picked", () => {
    expect(selectableAgent(availability({}), ["grok", "codex"], "claude")).toBe("grok");
    expect(selectableAgent(availability({ grok: "unavailable" }), ["grok", "codex"], "claude")).toBe("codex");
  });

  it("stays put when no added agent may be picked", () => {
    expect(selectableAgent(availability({}), [], "claude")).toBe("claude");
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

describe("accountsByAgent for a given agent list", () => {
  it("follows the order given and leaves out the other agents", () => {
    const grouped = accountsByAgent([acct("1", "grok", "G"), acct("2", "claude", "B")], ["grok", "claude"]);
    expect(grouped.map((g) => [g.agent, g.accounts.map((a) => a.id)])).toEqual([["grok", ["1"]], ["claude", ["2"]]]);
  });
});

describe("defaultAccountName", () => {
  it("is the name the user gave the agent's default account, else the language's word for it", () => {
    expect(defaultAccountName(t, undefined, "claude")).toBe("Default");
    expect(defaultAccountName(t, { codex: "Personal" }, "claude")).toBe("Default");
    expect(defaultAccountName(t, { claude: "  Personal " }, "claude")).toBe("Personal");
    expect(defaultAccountName(t, { claude: "  " }, "claude")).toBe("Default");
  });
});
