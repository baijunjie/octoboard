import { describe, expect, it } from "vitest";

import { consoleOf, projectOf } from "../fixtures/builders";
import { detectionMarkers, providerModelDisplay } from "./providers";
import { TEST_AGENTS, TEST_PROVIDERS } from "../test/testProviders";
import { agentChoiceBadge, agentPickerOptions, defaultAccountName, inheritedSessionLaunch, listedPickerAgents, noAgentState, projectSessionAgent, projectSessionLaunch, selectableAgent } from "./agents";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { type Agent, type AgentAvailability } from "../protocol";

// Real English messages rather than a stub, so a wording change that breaks a placeholder is
// caught here too.
const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;


const availability = (entries: Partial<Record<Agent, AgentAvailability["availability"]>>): Map<Agent, AgentAvailability> =>
  new Map(
    TEST_AGENTS.map((agent) => [
      agent,
      { agent, availability: entries[agent] ?? "not_determined", default_account_dir: null },
    ]),
  );

describe("noAgentState", () => {
  it("is undefined while nothing has been determined yet", () => {
    expect(noAgentState(availability({}), null, TEST_PROVIDERS)).toBeUndefined();
  });

  it("is undefined once at least one agent is available", () => {
    expect(noAgentState(availability({ claude: "available", codex: "unavailable", grok: "unavailable" }), null, TEST_PROVIDERS)).toBeUndefined();
  });

  it("asks to install one only once every agent has been determined unavailable", () => {
    expect(noAgentState(availability({ claude: "unavailable", codex: "unavailable", grok: "unavailable", test_agent: "unavailable" }), null, TEST_PROVIDERS)).toBe("install");
  });

  it("counts only agents that are added and installed, asking to add one while an agent not known to be missing waits", () => {
    const states = availability({ claude: "available", codex: "unavailable", grok: "available", test_agent: "unavailable" });
    expect(noAgentState(states, [], TEST_PROVIDERS)).toBe("add");
    expect(noAgentState(states, ["codex"], TEST_PROVIDERS)).toBe("add");
    expect(noAgentState(states, ["codex", "grok"], TEST_PROVIDERS)).toBeUndefined();
  });
});

describe("agentPickerOptions", () => {
  it("leaves an agent that is not yet determined, or is available, selectable", () => {
    const options = agentPickerOptions(t, availability({ claude: "available" }), null, TEST_PROVIDERS);
    expect(options.find((o) => o.value === "claude")).toEqual({ value: "claude", label: "Claude Code" });
    expect(options.find((o) => o.value === "codex")?.isDisabled).toBeUndefined();
  });

  it("lists every agent before availability has been determined and while the added list is undecided", () => {
    expect(agentPickerOptions(t, availability({}), null, TEST_PROVIDERS).map((o) => o.value)).toEqual(["claude", "codex", "grok", "test_agent"]);
  });

  it("leaves out an agent known to be missing", () => {
    const options = agentPickerOptions(t, availability({ grok: "unavailable" }), null, TEST_PROVIDERS);
    expect(options.map((o) => o.value)).toEqual(["claude", "codex", "test_agent"]);
  });

  it("keeps the held agent when it is missing, badged as not installed and unselectable", () => {
    const options = agentPickerOptions(t, availability({ grok: "unavailable", codex: "unavailable" }), null, TEST_PROVIDERS, "grok");
    expect(options.map((o) => o.value)).toEqual(["claude", "test_agent", "grok"]);
    const grok = options.find((o) => o.value === "grok");
    expect(grok?.label).toBe("Grok Build");
    expect(grok?.badge).toBe("Not installed");
    expect(grok?.isDisabled).toBe(true);
  });

  it("stops listing a missing agent once it is no longer held", () => {
    expect(agentPickerOptions(t, availability({ grok: "unavailable" }), null, TEST_PROVIDERS, "claude").map((o) => o.value)).toEqual(["claude", "codex", "test_agent"]);
  });

  it("lists only the added agents, in the order they were added", () => {
    expect(agentPickerOptions(t, availability({}), ["grok", "claude"], TEST_PROVIDERS).map((o) => o.value)).toEqual(["grok", "claude"]);
  });

  it("leaves out an added agent that is not installed", () => {
    expect(agentPickerOptions(t, availability({ codex: "unavailable" }), ["codex", "claude"], TEST_PROVIDERS).map((o) => o.value)).toEqual(["claude"]);
  });

  it("keeps a held agent that is not added, after the added ones, badged as not added and unselectable", () => {
    const options = agentPickerOptions(t, availability({ claude: "available", codex: "available" }), ["codex"], TEST_PROVIDERS, "claude");
    expect(options).toEqual([
      { value: "codex", label: "Codex" },
      { value: "claude", label: "Claude Code", badge: "Not added", isDisabled: true },
    ]);
  });

  it("names a held agent that is neither added nor installed as not installed", () => {
    const options = agentPickerOptions(t, availability({ grok: "unavailable" }), ["claude"], TEST_PROVIDERS, "grok");
    expect(options.find((o) => o.value === "grok")?.badge).toBe("Not installed");
  });

  it("offers nothing when no agent is added and none is held", () => {
    expect(agentPickerOptions(t, availability({}), [], TEST_PROVIDERS)).toEqual([]);
  });
});

describe("listedPickerAgents", () => {
  it("lists the agents that may be picked in the added order, then the others the caller keeps", () => {
    const states = availability({ codex: "unavailable" });
    expect(listedPickerAgents(states, ["grok", "codex"], TEST_PROVIDERS, (agent) => agent === "codex" || agent === "claude")).toEqual(["grok", "codex", "claude"]);
    expect(listedPickerAgents(states, ["grok", "codex"], TEST_PROVIDERS, () => false)).toEqual(["grok"]);
  });

  it("puts a kept agent that is added but missing after the ones that may be picked, not at its added position", () => {
    expect(listedPickerAgents(availability({ codex: "unavailable" }), ["codex", "claude"], TEST_PROVIDERS, (agent) => agent === "codex")).toEqual(["claude", "codex"]);
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
    expect(projectSessionAgent({ default_agent: "claude", project_session_agent: null })).toBe("claude");
    expect(projectSessionAgent({ default_agent: "claude", project_session_agent: null })).toBe("claude");
    expect(projectSessionAgent({ default_agent: "claude", project_session_agent: "grok" })).toBe("grok");
  });
});

describe("selectableAgent", () => {
  it.each([
    ["keeps an agent that is not yet determined", {}, "claude", "claude"],
    ["keeps an available agent", { claude: "available" }, "claude", "claude"],
    ["moves off an unavailable agent to the first that may be picked", { claude: "unavailable", codex: "unavailable" }, "claude", "grok"],
    ["stays put when none may be picked", { claude: "unavailable", codex: "unavailable", grok: "unavailable", test_agent: "unavailable" }, "codex", "codex"],
  ] as const)("%s", (_, states, wanted, expected) => {
    expect(selectableAgent(availability(states), null, TEST_PROVIDERS, wanted)).toBe(expected);
  });

  it("moves off an agent that is not added to the first added one that may be picked", () => {
    expect(selectableAgent(availability({}), ["grok", "codex"], TEST_PROVIDERS, "claude")).toBe("grok");
    expect(selectableAgent(availability({ grok: "unavailable" }), ["grok", "codex"], TEST_PROVIDERS, "claude")).toBe("codex");
  });

  it("stays put when no added agent may be picked", () => {
    expect(selectableAgent(availability({}), [], TEST_PROVIDERS, "claude")).toBe("claude");
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

describe("the launch settings a session inherits", () => {
  const display = (agent: "claude" | "codex") => providerModelDisplay(TEST_PROVIDERS, agent);

  it("falls through a project's stored retired `best` model to the console's, as the daemon reads it", () => {
    const owner = consoleOf("c", "Console", { project_session_launch: { claude: { model: "sonnet" }, codex: { model: "best" } } });
    const project = projectOf("p", "c", "Project", { launch: { claude: { model: "best", effort: "low" }, codex: { model: "best" } } });
    expect(projectSessionLaunch(owner, project, "claude", display("claude"))).toEqual({ model: "sonnet", effort: "low", compatibility: {} });
    expect(inheritedSessionLaunch(owner, project, "claude", display("claude"))).toEqual({ model: "sonnet", effort: "low", compatibility: {} });
    // Another agent's model of that name is a model.
    expect(projectSessionLaunch(owner, project, "codex", display("codex")).model).toBe("best");
  });

  it("reads a console's stored `best` as unset for a console session", () => {
    const owner = consoleOf("c", "Console", { launch: { claude: { model: "best", effort: "high" }, codex: { model: "best" } } });
    expect(inheritedSessionLaunch(owner, undefined, "claude", display("claude"))).toEqual({ model: null, effort: "high", compatibility: {} });
    expect(inheritedSessionLaunch(owner, undefined, "codex", display("codex")).model).toBe("best");
  });
});

describe("detectionMarkers", () => {
  it("lists the served names in the registry's order, each once", () => {
    expect(detectionMarkers(TEST_PROVIDERS)).toEqual(["CLAUDE.md", ".claude", ".codex", "AGENTS.md", ".agents", ".grok", ".test-agent"]);
  });

  it("is empty while the daemon has not served the providers", () => {
    expect(detectionMarkers([])).toEqual([]);
  });
});
