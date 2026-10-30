import { describe, expect, it } from "vitest";

import type { Launch } from "./protocol";
import { changedAccounts, changedLaunch, changedProjectAccounts, readAccounts, readLaunch, readProjectAccounts } from "./agentFields";

const unset: Launch = { model: null, effort: null };
const launch = (claude: Launch = unset, codex: Launch = unset) => ({ claude, codex, grok: unset });

describe("reading what a dialog holds", () => {
  it("fills every agent the record leaves out with its unset value", () => {
    expect(readAccounts(undefined)).toEqual({ claude: null, codex: null, grok: null });
    expect(readLaunch(undefined)).toEqual(launch());
    expect(readProjectAccounts(undefined)).toEqual({ claude: { kind: "inherit" }, codex: { kind: "inherit" }, grok: { kind: "inherit" } });
  });
});

describe("changedAccounts", () => {
  const initial = readAccounts({ claude: "a", codex: null, grok: null });

  it("clears an account back to the default with an explicit null on update", () => {
    expect(changedAccounts("update", { ...initial, claude: null }, initial)).toEqual({ claude: null });
  });

  it("sends only the accounts that were set on create", () => {
    expect(changedAccounts("create", { claude: "a", codex: null, grok: null }, readAccounts(undefined))).toEqual({ claude: "a" });
    expect(changedAccounts("create", readAccounts(undefined), readAccounts(undefined))).toBeUndefined();
  });
});

describe("changedLaunch", () => {
  const initial = launch({ model: "opus", effort: "high" });

  it("sends only the field that changed, for the agent that changed", () => {
    expect(changedLaunch("update", launch({ model: "opus", effort: "low" }), initial)).toEqual({ claude: { effort: "low" } });
  });

  it("sends only the agents that were touched", () => {
    expect(changedLaunch("update", launch({ model: "opus", effort: "high" }, { model: "gpt-5", effort: null }), initial)).toEqual({
      codex: { model: "gpt-5" },
    });
  });

  it("clears a field with an explicit null on update", () => {
    expect(changedLaunch("update", launch({ model: null, effort: "high" }), initial)).toEqual({ claude: { model: null } });
  });

  it("sends only the fields that were set on create", () => {
    expect(changedLaunch("create", launch({ model: "beta", effort: null }), launch())).toEqual({ claude: { model: "beta" } });
    expect(changedLaunch("create", launch(), launch())).toBeUndefined();
  });
});

describe("changedProjectAccounts", () => {
  const initial = readProjectAccounts({ claude: { kind: "account", id: "a" }, codex: { kind: "inherit" }, grok: { kind: "inherit" } });

  it("puts an account back to inheriting with an explicit null on update, and leaves it out on create", () => {
    expect(changedProjectAccounts("update", { ...initial, claude: { kind: "inherit" } }, initial)).toEqual({ claude: null });
    expect(changedProjectAccounts("create", { ...initial, claude: { kind: "inherit" } }, initial)).toBeUndefined();
  });

  it("sends the default account, pinned over what the console names, as its own kind", () => {
    expect(changedProjectAccounts("update", { ...initial, claude: { kind: "default" } }, initial)).toEqual({ claude: { kind: "default" } });
    expect(changedProjectAccounts("create", { ...initial, codex: { kind: "default" } }, initial)).toEqual({ codex: { kind: "default" } });
  });

  it("compares an account by id", () => {
    expect(changedProjectAccounts("update", { ...initial, claude: { kind: "account", id: "b" } }, initial)).toEqual({ claude: { kind: "account", id: "b" } });
    expect(changedProjectAccounts("update", { ...initial, claude: { kind: "account", id: "a" } }, initial)).toBeUndefined();
  });
});
