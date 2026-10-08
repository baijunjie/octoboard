import { describe, expect, it } from "vitest";

import { abbreviateHome } from "./pathDisplay";

describe("abbreviateHome", () => {
  it("writes the home directory itself and anything below it with a leading ~", () => {
    expect(abbreviateHome("/Users/dev", "/Users/dev")).toBe("~");
    expect(abbreviateHome("/Users/dev/.claude", "/Users/dev")).toBe("~/.claude");
    expect(abbreviateHome("/Users/dev/a/b", "/Users/dev")).toBe("~/a/b");
  });

  it("leaves a path that only shares the home directory's text as a prefix alone", () => {
    expect(abbreviateHome("/Users/dev\\foo", "/Users/dev")).toBe("/Users/dev\\foo");
    expect(abbreviateHome("/Users/devx", "/Users/dev")).toBe("/Users/devx");
    expect(abbreviateHome("/Users/devx/code", "/Users/dev")).toBe("/Users/devx/code");
    expect(abbreviateHome("/opt/tools", "/Users/dev")).toBe("/opt/tools");
  });

  it("tolerates a trailing separator on the home directory", () => {
    expect(abbreviateHome("/Users/dev/code", "/Users/dev/")).toBe("~/code");
    expect(abbreviateHome("/Users/dev", "/Users/dev/")).toBe("~");
  });

  it("returns the path unchanged when the home directory is unknown or a root", () => {
    expect(abbreviateHome("/Users/dev/code", null)).toBe("/Users/dev/code");
    expect(abbreviateHome("/Users/dev/code", undefined)).toBe("/Users/dev/code");
    expect(abbreviateHome("/Users/dev/code", "")).toBe("/Users/dev/code");
    expect(abbreviateHome("/Users/dev/code", "/")).toBe("/Users/dev/code");
  });
});
