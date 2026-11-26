import { describe, expect, it } from "vitest";

import { codex } from "./codex";
import { providerPresentation } from "./registry";

const placeholders = (text: string) => [...new Set([...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]))].sort();

// A launch that fell back a tier says what it left out; each is a standing notice the daemon raises, with nothing for
// the user to do, so none carries a remedy.
describe("the Codex notices of a launch that left something out", () => {
  const hooks = "provider.codex.hooks_left_out";
  const setting = "provider.codex.setting_left_out";
  const permissions = "provider.codex.permissions_left_out";

  it("is the registered module of its id", () => {
    expect(providerPresentation("codex")).toBe(codex);
  });

  it("declares each as a daemon code, the two that name a setting taking its key from the daemon, and none with a remedy", () => {
    expect(codex.codes[hooks]).toEqual({ origin: "daemon" });
    expect(codex.codes[setting]).toEqual({ origin: "daemon", codeParams: ["key"] });
    expect(codex.codes[permissions]).toEqual({ origin: "daemon", codeParams: ["key"] });
    for (const key of [hooks, setting, permissions]) {
      expect(Object.keys(codex.messages.en), key).not.toContain(`${key}.remedy`);
    }
  });

  it("words each in every language with the placeholders its code supplies", () => {
    for (const [language, messages] of Object.entries(codex.messages)) {
      expect(placeholders(messages[hooks]), `${language} hooks`).toEqual(["appName", "codex"]);
      expect(placeholders(messages[setting]), `${language} setting`).toEqual(["appName", "codex", "key"]);
      expect(placeholders(messages[permissions]), `${language} permissions`).toEqual(["appName", "codex", "key"]);
    }
  });

  // Raised by a launch (which then starts bare) and by the runs of the account's CLI alike (sign-in, quota, models, MCP
  // sign-in), so the remedy says only what holds on every route: reinstall, then try again.
  it("words the missing native program's remedy as a reinstall and a retry, true on every route that raises it", () => {
    const remedy = "provider.codex.native_program_missing.recovery";
    for (const [language, messages] of Object.entries(codex.messages)) {
      expect(placeholders(messages[remedy]), language).toEqual(["codex"]);
    }
    expect(codex.messages.en[remedy]).toBe("Reinstall {codex}, then try again.");
  });
});
