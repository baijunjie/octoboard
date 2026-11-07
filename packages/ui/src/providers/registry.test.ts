import { describe, expect, it } from "vitest";

import { FIXTURE_AGENTS } from "../fixtures/fixtureProviders";
import { providerLabel, providerShortName } from "../agents/providers";
import { TEST_AGENTS, TEST_PROVIDERS, UNPRESENTED_PROVIDER } from "../test/testProviders";
import { CATALOGS, CORE_PARAM_NAMES, type MessageKey } from "../i18n/catalog";
import { GENERIC_MARK } from "./genericMark";
import { providerColour, providerMark, providerPresentation } from "./registry";

describe("the providers' presentation modules", () => {
  it.each(TEST_AGENTS)("%s has a module whose messages are in every catalog", (agent) => {
    const module = providerPresentation(agent);
    expect(module?.id).toBe(agent);
    expect(module?.mark.d).not.toBe("");
    for (const [language, catalog] of Object.entries(CATALOGS)) {
      for (const key of [`provider.${agent}.label`, `provider.${agent}.shortName`] as MessageKey[]) {
        const message = catalog[key];
        expect(typeof message === "string" && message !== "", `${language} ${key}`).toBe(true);
      }
    }
  });

  it("has no module for an id it does not know, which draws the generic mark in the neutral foreground", () => {
    expect(providerPresentation("unknown")).toBeUndefined();
    expect(providerPresentation("toString")).toBeUndefined();
    expect(providerMark("unknown")).toBe(GENERIC_MARK);
    expect(providerColour("unknown")).toBeUndefined();
  });

  it("gives Claude and Codex a colour in each appearance and Grok none", () => {
    expect(providerColour("claude")).toEqual({ light: "#c2562f", dark: "#ea835d" });
    expect(providerColour("codex")).toEqual({ light: "#2a6df0", dark: "#6ea0ff" });
    expect(providerColour("grok")).toBeUndefined();
  });
});

describe("a served provider without a presentation module", () => {
  it("is worded by its served label and draws the generic mark", () => {
    const t = ((key: string) => key) as never;
    expect(providerPresentation(UNPRESENTED_PROVIDER.id)).toBeUndefined();
    expect(providerLabel(t, [UNPRESENTED_PROVIDER], UNPRESENTED_PROVIDER.id)).toBe("Unpresented Agent");
    expect(providerShortName(t, [UNPRESENTED_PROVIDER], UNPRESENTED_PROVIDER.id)).toBe("Unpresented");
    expect(providerLabel(t, [], UNPRESENTED_PROVIDER.id)).toBe("unpresented_agent");
    expect(providerMark(UNPRESENTED_PROVIDER.id)).toBe(GENERIC_MARK);
  });
});

describe("the test-only provider's presentation module", () => {
  it("is named Test Agent, short Test, and carries no colour", () => {
    expect(providerLabel(((key: string) => CATALOGS.en![key as MessageKey]) as never, TEST_PROVIDERS, "test_agent")).toBe("Test Agent");
    expect(providerColour("test_agent")).toBeUndefined();
    expect(providerPresentation("test_agent")?.codes).toBeUndefined();
  });
});

describe("the settings the providers declare", () => {
  it.each(FIXTURE_AGENTS)("%s has the copy of each of its settings and their choices in every language", (agent) => {
    const module = providerPresentation(agent)!;
    const served = TEST_PROVIDERS.find((provider) => provider.id === agent)!;
    const source = module.messages.en as Record<string, string>;
    expect(served.settings.length).toBeGreaterThan(0);
    for (const descriptor of served.settings) {
      const prefix = `provider.${agent}.setting.${descriptor.id}.`;
      const preferred = (name: string, fallback: string) => source[prefix + name] !== undefined ? name : fallback;
      const names = new Set([
        "heading", "helpLabel", "label", "saveFailedTitle",
        preferred("helpTiming", "description"),
        ...Object.keys(source).filter((key) => key.startsWith(prefix)).map((key) => key.slice(prefix.length)),
      ]);
      if (descriptor.kind.type === "choice") {
        for (const choice of descriptor.kind.choices) {
          names.add(`choice.${choice.value}`);
          if (choice.confirm) {
            names.add(`confirm.${choice.value}.title`);
            names.add(preferred(`confirm.${choice.value}.consequence`, `confirm.${choice.value}.message`));
          }
        }
      }
      for (const language of Object.keys(CATALOGS)) {
        const messages = (module.messages as Record<string, Record<string, string>>)[language];
        expect(messages, `${agent} ${language}`).toBeDefined();
        for (const name of names) {
          const text = messages[prefix + name];
          expect(typeof text === "string" && text.trim().length > 0, `${language} ${prefix}${name}`).toBe(true);
        }
      }
    }
  });
});

describe("the codes the providers declare", () => {
  it.each(TEST_AGENTS)("%s words each remedy it names under its own id, in every language", (agent) => {
    const module = providerPresentation(agent)!;
    for (const [code, record] of Object.entries(module.codes ?? {})) {
      const { recovery } = record as { recovery?: string };
      if (!recovery?.startsWith("provider.")) continue;
      expect(recovery.startsWith(`provider.${module.id}.`), `${code} names ${recovery}`).toBe(true);
      for (const [language, messages] of Object.entries(module.messages) as [string, Record<string, string>][]) {
        expect(messages[recovery], `${language} ${recovery}`).toEqual(expect.any(String));
      }
    }
  });
});

describe("the brand-name placeholders the modules declare", () => {
  it("are unique across the modules and clash with no core placeholder (`CORE_PARAM_NAMES`)", () => {
    const modules = TEST_AGENTS.map((agent) => providerPresentation(agent)!);
    const names = modules.flatMap((module) => Object.keys(module.params ?? {}));
    expect(names.length).toBeGreaterThan(0);
    expect(new Set(names).size).toBe(names.length);
    for (const name of names) expect(CORE_PARAM_NAMES, name).not.toContain(name);
  });
});
