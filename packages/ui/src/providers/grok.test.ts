import { describe, expect, it } from "vitest";

import { grok } from "./grok";
import { providerPresentation } from "./registry";

const placeholders = (text: string) => [...new Set([...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]))].sort();

describe("the Grok Build presentation module's degraded-launch wording", () => {
  const leader = "provider.grok.leader_not_overridden";
  const sandbox = "provider.grok.sandbox_unsupported";

  it("is the registered module of its id", () => {
    expect(providerPresentation("grok")).toBe(grok);
  });

  it("declares the leader-mode notice as a daemon code with the agent's name, and no remedy of its own", () => {
    expect(grok.codes[leader]).toMatchObject({ origin: "daemon" });
    expect(Object.keys(grok.messages.en)).not.toContain(`${leader}.remedy`);
    for (const [language, messages] of Object.entries(grok.messages)) {
      expect(placeholders(messages[leader]), language).toEqual(["agent"]);
    }
  });

  // The sandbox no longer refuses the launch: the session starts without the app's additions, so the cause says only
  // what clashes, and the remedy says what turning the sandbox off brings.
  it("words the sandbox as the cause of a launch without the additions, never as one that was not launched", () => {
    const cause = grok.messages.en[sandbox];
    expect(cause).not.toMatch(/not launched/i);
    expect(placeholders(cause)).toEqual(["agent", "appName", "profile", "source"]);
    const remedy = grok.messages.en["provider.grok.sandbox_unsupported.remedy"];
    expect(remedy).toContain("turn the sandbox off");
    expect(remedy).toContain("{appName}");
  });
});
