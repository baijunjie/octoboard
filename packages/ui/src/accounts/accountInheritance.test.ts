import { describe, expect, it } from "vitest";

import { consoleAccount, inheritedAccount, projectAccount } from "./accountInheritance";
import { consoleOf, projectOf } from "../fixtures/builders";
import { availability, account, accounts, initialChoice } from "./accountChoicesTestSupport";

describe("inheritedAccount", () => {
  const owner = consoleOf("c-1", "Main", { accounts: { claude: "w" } });
  const stored = [...accounts, account("w2", "claude", "Work 2")];

  it("is the default account when neither the project nor the console pins one", () => {
    expect(inheritedAccount(consoleOf("c-1", "Main"), projectOf("p-1", "c-1", "App"), "grok", accounts)).toBeNull();
  });

  it("is the project's account over the console's", () => {
    const project = projectOf("p-1", "c-1", "App", { accounts: { claude: { kind: "account", id: "w2" } } });
    expect(inheritedAccount(owner, project, "claude", stored)).toBe("w2");
  });

  it("is the console's account when the project's reference is no longer stored", () => {
    const project = projectOf("p-1", "c-1", "App", { accounts: { claude: { kind: "account", id: "gone" } } });
    expect(inheritedAccount(owner, project, "claude", stored)).toBe("w");
  });
});

describe("a project on the agent's default account", () => {
  const owner = consoleOf("c-1", "Main", { accounts: { claude: "w", codex: "c" } });
  const onDefault = projectOf("p-1", "c-1", "App", { accounts: { claude: { kind: "default" } } });

  it("resolves to the default account without falling through to the console's", () => {
    expect(inheritedAccount(owner, onDefault, "claude", accounts)).toBeNull();
  });

  it("is said for its own agent only", () => {
    expect(inheritedAccount(owner, onDefault, "codex", accounts)).toBe("c");
  });

  it("opens the session dialog on the default account while the console pins one", () => {
    const project = projectOf("p-1", "c-1", "App", { default_agent: "claude", accounts: { claude: { kind: "default" } } });
    expect(initialChoice(owner, project, accounts, availability({}), null)).toEqual({ agent: "claude", account: null });
  });

  it("is told from inheriting by projectAccount", () => {
    expect(projectAccount(onDefault, "claude", accounts)).toEqual({ kind: "default" });
    expect(projectAccount(projectOf("p-1", "c-1", "App", { accounts: { claude: { kind: "inherit" } } }), "claude", accounts)).toEqual({ kind: "inherit" });
  });

  it("reads a pinned account that is not stored as inheriting", () => {
    const gone = projectOf("p-1", "c-1", "App", { accounts: { claude: { kind: "account", id: "gone" } } });
    expect(projectAccount(gone, "claude", accounts)).toEqual({ kind: "inherit" });
  });
});

describe("consoleAccount", () => {
  it("reads a reference to an account that is not stored, or is another agent's, as the default account", () => {
    const owner = consoleOf("c-1", "Main", { accounts: { claude: "w", codex: "w", grok: "gone" } });
    expect(consoleAccount(owner, "claude", accounts)).toBe("w");
    expect(consoleAccount(owner, "codex", accounts)).toBeNull();
    expect(consoleAccount(owner, "grok", accounts)).toBeNull();
  });
});
