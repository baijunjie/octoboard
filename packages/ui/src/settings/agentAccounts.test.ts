import { describe, expect, it } from "vitest";

import { accountOf } from "../gallery/fixtures/builders";
import type { Account } from "../protocol";
import { isAbsoluteConfigDir, nameCollision } from "./agentAccounts";

const account = (id: string, agent: Account["agent"], name: string): Account => accountOf(id, agent, name, `/home/${id}`);

describe("isAbsoluteConfigDir", () => {
  it.each([
    ["/home/me/.claude-work", true],
    ["  /home/me  ", true],
    ["~/work", true],
    ["~", true],
    ["relative/dir", false],
    ["./dir", false],
    ["~other/dir", false],
    ["", false],
    ["   ", false],
  ])("judges %j to be %s", (path, expected) => {
    expect(isAbsoluteConfigDir(path)).toBe(expected);
  });
});

describe("nameCollision", () => {
  const accounts = [account("a", "claude", "Work"), account("b", "codex", "Work"), account("c", "claude", "Home")];

  it.each([
    ["Work", "claude", { kind: "account", name: "Work" }],
    ["  work ", "claude", { kind: "account", name: "Work" }],
    ["Work", "grok", undefined],
    ["Other", "claude", undefined],
  ] as const)("compares %j within %s only, trimmed and ignoring case", (typed, agent, expected) => {
    expect(nameCollision(typed, agent, accounts, "Default")).toEqual(expected);
  });

  it("counts the default account's name in the current language as taken", () => {
    expect(nameCollision(" 默认 ", "claude", accounts, "默认")).toEqual({ kind: "default", name: "默认" });
    expect(nameCollision("Varsayılan", "codex", accounts, "varsayılan")).toEqual({ kind: "default", name: "varsayılan" });
  });

  it("reports the reserved word as typed when the default account is shown under another name", () => {
    expect(nameCollision(" default ", "claude", accounts, "Personal")).toEqual({ kind: "reserved", name: "default" });
    expect(nameCollision("Personal", "claude", accounts, "Personal")).toEqual({ kind: "default", name: "Personal" });
  });

  it("does not let the account being edited collide with itself", () => {
    expect(nameCollision("work", "claude", accounts, "Default", "a")).toBeUndefined();
    expect(nameCollision("home", "claude", accounts, "Default", "a")).toEqual({ kind: "account", name: "Home" });
  });
});
