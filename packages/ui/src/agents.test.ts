import { describe, expect, it } from "vitest";

import { agentPickerOptions, noAgentAvailable } from "./agents";
import { format } from "./i18n/catalog";
import type { Translate } from "./i18n/catalog";
import type { Agent, AgentAvailability } from "./protocol";

// Real English messages rather than a stub, so a wording change that breaks a placeholder is
// caught here too.
const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;

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
