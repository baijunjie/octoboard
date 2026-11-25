import { expect, it } from "vitest";

import { availabilityOf } from "../fixtures/fixtureProviders";
import { accountOf } from "../fixtures/builders";
import { defaultAccountConfigDir } from "../agents/providers";
import type { AgentAvailability } from "../protocol";
import { TEST_PROVIDERS } from "../test/testProviders";
import { accountConfigDir } from "./accountDir";

const found = (dir: string | null): Map<string, AgentAvailability> => new Map([["claude", { agent: "claude", availability: "available", default_account_dir: dir }]]);
const stored = [accountOf("work", "claude", "Work", "/accounts/work")];

it("is a stored account's own directory", () => {
  expect(accountConfigDir(stored, found("/home/dev/.claude"), TEST_PROVIDERS, "claude", "work")).toBe("/accounts/work");
});

it("is empty for an account that is not stored, which has no directory to name", () => {
  expect(accountConfigDir(stored, found("/home/dev/.claude"), TEST_PROVIDERS, "claude", "gone")).toBe("");
  expect(accountConfigDir([], availabilityOf(TEST_PROVIDERS), TEST_PROVIDERS, "claude", "work")).toBe("");
});

it("is the default account's directory as the daemon found it, else the provider's usual one under home", () => {
  expect(accountConfigDir(stored, found("/home/dev/.claude"), TEST_PROVIDERS, "claude", null)).toBe("/home/dev/.claude");
  expect(accountConfigDir(stored, found(null), TEST_PROVIDERS, "claude", null)).toBe("~/.claude");
  expect(defaultAccountConfigDir(found(null), TEST_PROVIDERS, "claude")).toBe("~/.claude");
});

it("is empty for a default account of a provider the daemon does not serve and has found no directory for", () => {
  expect(accountConfigDir(stored, new Map(), TEST_PROVIDERS, "not_served", null)).toBe("");
  expect(defaultAccountConfigDir(new Map(), [], "claude")).toBe("");
});
