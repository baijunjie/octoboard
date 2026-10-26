import { describe, expect, it } from "vitest";
import { daemonMessage } from "../../daemonMessage";
import { structuredErrorScenarios } from "./structuredErrors";

// A successful render alone cannot reveal a missing fixture parameter: interpolation leaves it visible.
describe("structured error fixture contracts", () => {
  it.each(["en", "zh-Hans", "ar"] as const)("fills every catalog placeholder in %s", (language) => {
    for (const scenario of structuredErrorScenarios) {
      const state = scenario.state!;
      for (const error of scenario.toasts ?? []) {
        const text = daemonMessage(language, error.code!, error.params!, typeof error.message === "string" ? error.message : "", {
          consoles: state.consoles!, projects: state.projects!, sessions: state.sessions!,
          shellTerminals: state.shellTerminals, accounts: state.settings!.accounts,
        });
        expect(text, scenario.id).not.toMatch(/\{[a-zA-Z_]+\}/);
        expect(text.trim(), scenario.id).not.toBe("");
      }
    }
  });
});
