import { describe, expect, it } from "vitest";

import { avatarFill } from "./ConsoleAvatar";

describe("avatarFill", () => {
  it("gives the same name the same fill, ignoring surrounding spaces", () => {
    expect(avatarFill("Operations")).toBe(avatarFill("Operations"));
    expect(avatarFill(" Operations ")).toBe(avatarFill("Operations"));
  });

  it("reads a name composed or decomposed, and past invisible marks, as the same", () => {
    expect(avatarFill("e\u0301tude")).toBe(avatarFill("\u00e9tude"));
    expect(avatarFill("\u200b\u00e9tude\u200e")).toBe(avatarFill("\u00e9tude"));
  });

  it("spreads names over the palette", () => {
    const names = ["Main", "Operations", "Billing", "Docs", "Work", "Personal", "Second", "Lab", "Research", "Infra"];
    expect(new Set(names.map(avatarFill)).size).toBeGreaterThanOrEqual(4);
  });
});
