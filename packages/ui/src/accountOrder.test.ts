import { describe, expect, it } from "vitest";

import { accountOf } from "./gallery/fixtures/builders";
import { orderedAccounts } from "./accountOrder";
import type { Account } from "./protocol";

const work: Account = accountOf("work", "claude", "Work", "/work");
const home: Account = accountOf("home", "claude", "Home", "/home");
const other: Account = accountOf("other", "codex", "Other", "/other");
const accounts = [work, other, home];

describe("orderedAccounts", () => {
  it("keeps the default account first without a saved order", () => {
    expect(orderedAccounts("claude", accounts, undefined)).toEqual([null, work, home]);
    expect(orderedAccounts("grok", accounts, undefined)).toEqual([null]);
  });

  it("moves the default account along with named accounts", () => {
    expect(orderedAccounts("claude", accounts, { claude: ["home", null, "work"] })).toEqual([home, null, work]);
  });

  it("ignores stale, foreign and duplicate references and appends new accounts", () => {
    const order = { claude: ["home", "gone", "other", "home", null, null] };
    expect(orderedAccounts("claude", accounts, order)).toEqual([home, null, work]);
    expect(accounts).toEqual([work, other, home]);
    expect(order.claude).toEqual(["home", "gone", "other", "home", null, null]);
  });

  it("appends the default account when the saved order omits it", () => {
    expect(orderedAccounts("claude", accounts, { claude: ["home"] })).toEqual([home, null, work]);
  });
});
