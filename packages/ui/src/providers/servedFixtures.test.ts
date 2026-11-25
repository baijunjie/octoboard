import { afterEach, describe, expect, it, vi } from "vitest";

import { FIXTURE_PROVIDERS } from "../fixtures/fixtureProviders";
import { TEST_PROVIDERS } from "../test/testProviders";
import { BUILT_IN } from "./registry";
import { BUILT_IN_SERVED, TEST_AGENT_SERVED } from "./servedFixtures";

describe("the served-info fixtures of the built-in providers", () => {
  it("are the registry's modules' own, in the registry's order", () => {
    expect(BUILT_IN_SERVED.map(({ id }) => id)).toEqual(BUILT_IN.map(({ id }) => id));
    expect(BUILT_IN_SERVED.length).toBeGreaterThan(0);
  });

  it("serve the test-only provider apart from the built-in ones", () => {
    expect(TEST_AGENT_SERVED.id).toBe("test_agent");
    expect(BUILT_IN_SERVED.map(({ id }) => id)).not.toContain(TEST_AGENT_SERVED.id);
  });

  it("make up the test list with the test-only provider last, and the fixture daemon's list of released providers", () => {
    expect(TEST_PROVIDERS).toEqual([...BUILT_IN_SERVED, TEST_AGENT_SERVED]);
    expect(FIXTURE_PROVIDERS.length).toBeGreaterThan(0);
    for (const provider of FIXTURE_PROVIDERS) expect(BUILT_IN_SERVED).toContain(provider);
  });
});

describe("a registered module with no served-info fixture", () => {
  afterEach(() => {
    vi.doUnmock("./registry");
    vi.resetModules();
  });

  it("fails the import, naming the module and the file it lacks", async () => {
    vi.resetModules();
    vi.doMock("./registry", async (importOriginal) => {
      const actual = await importOriginal<typeof import("./registry")>();
      return { ...actual, BUILT_IN: [...actual.BUILT_IN, { id: "ghost_agent" }] };
    });
    await expect(import("./servedFixtures")).rejects.toThrow(/ghost_agent.*providers\/ghost_agent\.fixture\.ts/);
  });
});
