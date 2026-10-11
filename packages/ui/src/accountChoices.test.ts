import { describe, expect, it } from "vitest";

import { choiceGroups, choiceKey, consoleAccount, currentChoice, inheritedAccount, initialChoice, projectAccount, switchEntries } from "./accountChoices";
import { consoleOf, projectOf, sessionOf } from "./gallery/fixtures/builders";
import { format } from "./i18n/catalog";
import type { Translate } from "./i18n/catalog";
import type { Account, Agent, AgentAvailability } from "./protocol";

const availability = (entries: Partial<Record<Agent, AgentAvailability["availability"]>>): Map<Agent, AgentAvailability> =>
  new Map(
    (["claude", "codex", "grok"] as Agent[]).map((agent) => [
      agent,
      { agent, availability: entries[agent] ?? "not_determined" },
    ]),
  );

const account = (id: string, agent: Agent, name: string): Account => ({ id, agent, name, config_dir: `/home/${id}` });
const accounts = [account("w", "claude", "Work"), account("c", "codex", "Codex work")];

describe("choiceGroups", () => {
  it("puts the default account first in every agent's group, the stored accounts after it", () => {
    const groups = choiceGroups(accounts, availability({}), "Default");
    expect(groups.map((g) => [g.agent, g.entries.map((e) => e.name)])).toEqual([
      ["claude", ["Default", "Work"]],
      ["codex", ["Default", "Codex work"]],
      ["grok", ["Default"]],
    ]);
  });

  it("holds the default account alone for every agent on a first-ever start", () => {
    for (const group of choiceGroups([], availability({}), "Default")) {
      expect(group.entries.map((e) => e.account)).toEqual([null]);
    }
  });

  it.each([
    ["not yet determined", "not_determined", true],
    ["available", "available", true],
    ["unavailable", "unavailable", false],
  ] as const)("leaves a group selectable unless its agent is known to be missing (%s)", (_, state, selectable) => {
    expect(choiceGroups(accounts, availability({ claude: state }), "Default")[0].selectable).toBe(selectable);
  });
});

describe("initialChoice", () => {
  const owner = consoleOf("c-1", "Main", { codex_account_id: "c" });
  const project = (agent?: Agent) => projectOf("p-1", "c-1", "App", { default_agent: agent });

  it.each([
    ["the console's default agent, on the default account when it pins nothing", undefined, {}, { agent: "claude", account: null }],
    ["the project's agent over the console's, on the account the console holds for it", "codex", {}, { agent: "codex", account: "c" }],
    ["the first agent that may be picked when the resolved one is known to be missing", "claude", { claude: "unavailable" }, { agent: "codex", account: "c" }],
    ["the resolved agent while availability is not determined", "claude", {}, { agent: "claude", account: null }],
    ["the resolved agent when none may be picked", "claude", { claude: "unavailable", codex: "unavailable", grok: "unavailable" }, { agent: "claude", account: null }],
  ] as const)("opens on %s", (_, projectAgent, states, expected) => {
    expect(initialChoice(owner, project(projectAgent), accounts, availability(states))).toEqual(expected);
  });

  it("opens on the project's account for the agent it moved to when the first is missing", () => {
    const moved = projectOf("p-1", "c-1", "App", { codex_account_id: { kind: "account", id: "c2" } });
    const stored = [...accounts, account("c2", "codex", "Codex 2")];
    expect(initialChoice(owner, moved, stored, availability({ claude: "unavailable" }))).toEqual({ agent: "codex", account: "c2" });
  });
});

describe("inheritedAccount", () => {
  const owner = consoleOf("c-1", "Main", { claude_account_id: "w" });
  const stored = [...accounts, account("w2", "claude", "Work 2")];

  it("is the default account when neither the project nor the console pins one", () => {
    expect(inheritedAccount(consoleOf("c-1", "Main"), projectOf("p-1", "c-1", "App"), "grok", accounts)).toBeNull();
  });

  it("is the project's account over the console's", () => {
    const project = projectOf("p-1", "c-1", "App", { claude_account_id: { kind: "account", id: "w2" } });
    expect(inheritedAccount(owner, project, "claude", stored)).toBe("w2");
  });

  it("is the console's account when the project's reference is no longer stored", () => {
    const project = projectOf("p-1", "c-1", "App", { claude_account_id: { kind: "account", id: "gone" } });
    expect(inheritedAccount(owner, project, "claude", stored)).toBe("w");
  });
});

describe("a project on the agent's default account", () => {
  const owner = consoleOf("c-1", "Main", { claude_account_id: "w", codex_account_id: "c" });
  const onDefault = projectOf("p-1", "c-1", "App", { claude_account_id: { kind: "default" } });

  it("resolves to the default account without falling through to the console's", () => {
    expect(inheritedAccount(owner, onDefault, "claude", accounts)).toBeNull();
  });

  it("is said for its own agent only", () => {
    expect(inheritedAccount(owner, onDefault, "codex", accounts)).toBe("c");
  });

  it("opens the session dialog on the default account while the console pins one", () => {
    const project = projectOf("p-1", "c-1", "App", { default_agent: "claude", claude_account_id: { kind: "default" } });
    expect(initialChoice(owner, project, accounts, availability({}))).toEqual({ agent: "claude", account: null });
  });

  it("is told from inheriting by projectAccount", () => {
    expect(projectAccount(onDefault, "claude", accounts)).toEqual({ kind: "default" });
    expect(projectAccount(projectOf("p-1", "c-1", "App", { claude_account_id: { kind: "inherit" } }), "claude", accounts)).toEqual({ kind: "inherit" });
  });

  it("reads a pinned account that is not stored as inheriting", () => {
    const gone = projectOf("p-1", "c-1", "App", { claude_account_id: { kind: "account", id: "gone" } });
    expect(projectAccount(gone, "claude", accounts)).toEqual({ kind: "inherit" });
  });
});

describe("consoleAccount", () => {
  it("reads a reference to an account that is not stored, or is another agent's, as the default account", () => {
    const owner = consoleOf("c-1", "Main", { claude_account_id: "w", codex_account_id: "w", grok_account_id: "gone" });
    expect(consoleAccount(owner, "claude", accounts)).toBe("w");
    expect(consoleAccount(owner, "codex", accounts)).toBeNull();
    expect(consoleAccount(owner, "grok", accounts)).toBeNull();
  });
});

describe("currentChoice", () => {
  const initial = { agent: "claude", account: null } as const;
  const picked = { agent: "codex", account: "c" } as const;

  it.each([
    ["the user's pick", {}, picked],
    ["the initial entry when the pick's agent has since been found missing", { codex: "unavailable" }, initial],
  ] as const)("shows %s", (_, states, expected) => {
    expect(currentChoice(picked, initial, choiceGroups(accounts, availability(states), "Default"))).toEqual(expected);
  });

  it("shows the initial entry when the pick's account is no longer stored, or there is no pick", () => {
    const groups = choiceGroups([], availability({}), "Default");
    expect(currentChoice(picked, initial, groups)).toEqual(initial);
    expect(currentChoice(undefined, initial, groups)).toEqual(initial);
  });
});

it("gives each agent's entries keys no other agent's can share", () => {
  expect(choiceKey({ agent: "claude", account: null })).not.toBe(choiceKey({ agent: "codex", account: null }));
});

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;

describe("switchEntries", () => {
  const session = (extra = {}) => sessionOf("s", "c-1", "p-1", "Title", "idle", extra);
  const summary = (entries: ReturnType<typeof switchEntries>) =>
    entries.map((entry) => `${entry.name}${entry.current ? " (current)" : ""}`);

  it.each([
    ["the default account first, then the agent's stored accounts, marking the one it is on", {}, ["Default (current)", "Work"]],
    ["a stored account as current", { account_id: "w", config_dir: "/home/w" }, ["Default", "Work (current)"]],
    ["a removed account as current, named by the directory the session recorded", { account_id: "gone", config_dir: "/home/old" }, ["Default", "Work", "/home/old (current)"]],
    ["a removed account that recorded no directory as nothing to name", { account_id: "gone" }, ["Default", "Work"]],
    ["only the session's own agent's accounts", { agent: "grok" }, ["Default (current)"]],
  ] as const)("lists %s", (_, extra, expected) => {
    expect(summary(switchEntries(session(extra), accounts, t))).toEqual(expected);
  });
});
