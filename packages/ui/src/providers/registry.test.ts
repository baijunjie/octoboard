// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { providerLabel, providerShortName } from "../agents/providers";
import { recoveryParams } from "../components/alerts/ErrorGuidance";
import { TEST_AGENTS, TEST_PROVIDERS, UNPRESENTED_PROVIDER } from "../test/testProviders";
import { CATALOGS, CORE_PARAM_NAMES, translatorFor, type MessageKey } from "../i18n/catalog";
import { GENERIC_MARK } from "./genericMark";
import { BUILT_IN, providerColour, providerMark, providerPresentation, providerReferences } from "./registry";
import { BUILT_IN_SERVED } from "./servedFixtures";

/** The brand names the modules declare as global placeholders, which no caller passes. */
const BRAND_NAMES = TEST_AGENTS.flatMap((id) => Object.keys(providerPresentation(id)!.params ?? {}));

/** The placeholders every message is filled with by itself, whoever words it: the core ones (the product names, `osName`,
 * the platform words) and the providers' brand names. A message of a provider's module may use any of them. */
const GLOBAL_PLACEHOLDERS: readonly string[] = [...CORE_PARAM_NAMES, ...BRAND_NAMES];

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

  it("tells every provider's colour from the others' in each appearance", () => {
    const coloured = BUILT_IN_SERVED.flatMap(({ id }) => {
      const colour = providerColour(id);
      return colour ? [colour] : [];
    });
    expect(coloured.length).toBeGreaterThan(0);
    for (const appearance of ["light", "dark"] as const) {
      const values = coloured.map((colour) => colour[appearance]);
      expect(new Set(values).size, appearance).toBe(values.length);
    }
  });

  it("draws every built-in provider's mark in one fill, from its own module", () => {
    for (const { id } of BUILT_IN_SERVED) {
      expect(providerMark(id), id).toBe(providerPresentation(id)!.mark);
      expect(providerMark(id), id).not.toBe(GENERIC_MARK);
      expect(providerMark(id).fill, id).toMatch(/^(currentColor|#[0-9a-fA-F]{6})$/);
    }
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
  it("are declared by at least one built-in provider, so that the checks below walk some", () => {
    expect(BUILT_IN_SERVED.some((provider) => provider.settings.length > 0)).toBe(true);
  });

  // The test-only provider's module carries English and Chinese alone (the rest fall back to English), so it is left out.
  it.each(BUILT_IN_SERVED.map(({ id }) => id))("%s has the copy of each of its settings and their choices in every language", (agent) => {
    const module = providerPresentation(agent)!;
    const served = TEST_PROVIDERS.find((provider) => provider.id === agent)!;
    const source = module.messages.en as Record<string, string>;
    // A provider that declares no setting, like the preview ones, has no copy of one either.
    if (served.settings.length === 0) {
      expect(Object.keys(source).filter((key) => key.startsWith(`provider.${agent}.setting.`)), agent).toEqual([]);
    }
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

describe("the placeholders of the settings' messages", () => {
  it.each(TEST_AGENTS)("%s words no placeholder that neither the application nor the setting's references supply", (agent) => {
    const module = providerPresentation(agent)!;
    for (const [language, messages] of Object.entries(module.messages) as [string, Record<string, string>][]) {
      for (const [key, text] of Object.entries(messages)) {
        const setting = key.match(new RegExp(`^provider\\.${agent}\\.setting\\.([a-z_]+)\\.`))?.[1];
        if (setting === undefined) continue;
        const supplied = new Set(["agent", ...GLOBAL_PLACEHOLDERS, ...Object.keys(module.settingReferences?.[setting] ?? {})]);
        for (const [, name] of text.matchAll(/\{([A-Za-z][A-Za-z0-9_]*)\}/g)) {
          expect(supplied.has(name!), `${language} ${key} uses {${name}}`).toBe(true);
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
      // The remedy is drawn with the params of the code its key names, so a record names its own code's remedy alone.
      expect(recovery, `${code} names the remedy of another code`).toBe(`${code}.remedy`);
      for (const [language, messages] of Object.entries(module.messages) as [string, Record<string, string>][]) {
        expect(typeof messages[recovery] === "string" && messages[recovery].trim() !== "", `${language} ${recovery}`).toBe(true);
      }
    }
  });
});

describe("the sign-in hints the providers declare", () => {
  it.each(TEST_AGENTS)("%s words a declared signInHint in every language it carries, and in every catalog", (agent) => {
    const module = providerPresentation(agent)!;
    const key = `provider.${agent}.signInHint`;
    const declared = Object.hasOwn(module.messages.en, key);
    for (const [language, messages] of Object.entries(module.messages) as [string, Record<string, string>][]) {
      expect(Object.hasOwn(messages, key), `${agent} ${language}`).toBe(declared);
    }
    for (const [language, catalog] of Object.entries(CATALOGS)) {
      const text = (catalog as Record<string, string>)[key];
      expect(typeof text === "string" && text.trim().length > 0, `${language} ${key}`).toBe(declared);
    }
  });

  it("sets the commands a hint names apart as references its message fills, and words no other placeholder than the directory the window fills", () => {
    for (const agent of TEST_AGENTS) {
      const module = providerPresentation(agent)!;
      const key = `provider.${agent}.signInHint`;
      const references = Object.keys(module.references?.signInHint ?? {});
      for (const [language, messages] of Object.entries(module.messages) as [string, Record<string, string>][]) {
        const placeholders = [...(messages[key] ?? "").matchAll(/\{([A-Za-z][A-Za-z0-9_]*)\}/g)].map((match) => match[1]!);
        for (const name of placeholders) expect([...references, "dir", "appName"], `${language} ${key} uses {${name}}`).toContain(name);
      }
    }
    expect(providerReferences("test_agent", "signInHint")).toEqual({ command: "test-agent login" });
  });

  it("has a generic sentence in every catalog for a provider without one, naming no agent and no command", () => {
    for (const [language, catalog] of Object.entries(CATALOGS)) {
      const text = (catalog as Record<string, string>)["settings.accounts.signInHint"];
      expect(typeof text === "string" && text.trim().length > 0, language).toBe(true);
      // It names the app and the account's directory, which the window fills, and nothing else: no agent, no command.
      expect([...text.matchAll(/\{([A-Za-z][A-Za-z0-9_]*)\}/g)].map((match) => match[1]).sort(), language).toEqual(["appName", "dir"]);
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

/** What a built-in module declares of one code: the words of its record the remedy's placeholders are read against. */
interface CodeDeclaration {
  readonly recovery?: string;
  readonly codeParams?: readonly string[];
  readonly uiParams?: Readonly<Record<string, string>>;
}

describe("the remedies the built-in providers' codes carry", () => {
  const sharedParams = Object.keys(recoveryParams(translatorFor("en")));
  const placeholders = (text: string) => [...text.matchAll(/\{([A-Za-z][A-Za-z0-9_]*)\}/g)].map((match) => match[1]!);
  const declared = (module: (typeof BUILT_IN)[number]) => Object.entries((module.codes ?? {}) as Record<string, CodeDeclaration>);
  const referencesOf = (module: (typeof BUILT_IN)[number]) => Object.keys((module as { recoveryReferences?: Readonly<Record<string, string>> }).recoveryReferences ?? {});

  it("are walked: at least one built-in module words a remedy, and the shared recovery params are the application's", () => {
    expect(BUILT_IN.some((module) => Object.keys(module.messages.en).some((key) => key.endsWith(".remedy")))).toBe(true);
    expect(sharedParams).toContain("appName");
  });

  it.each(BUILT_IN.map((module) => module.id))("%s words each remedy with no placeholder but the agent, its own references, a shared recovery param, a global placeholder or a param its code declares, in every language", (agent) => {
    const module = BUILT_IN.find((candidate) => candidate.id === agent)!;
    for (const [language, messages] of Object.entries(module.messages) as [string, Record<string, string>][]) {
      for (const [key, text] of Object.entries(messages)) {
        if (!key.endsWith(".remedy")) continue;
        // The renderer reads the code from the key, so the record is the one the key names.
        const record = declared(module).find(([code]) => `${code}.remedy` === key)?.[1];
        const supplied = new Set(["agent", ...GLOBAL_PLACEHOLDERS, ...referencesOf(module), ...sharedParams, ...(record?.codeParams ?? []), ...Object.keys(record?.uiParams ?? {})]);
        for (const name of placeholders(text)) expect(supplied.has(name), `${language} ${key} uses {${name}}`).toBe(true);
      }
    }
  });

  it.each(BUILT_IN.map((module) => module.id))("%s names no param of a code like one of its own references, a shared recovery param or a global placeholder, which the remedy would fill with the module's own or the global value", (agent) => {
    const module = BUILT_IN.find((candidate) => candidate.id === agent)!;
    const shadowing = new Set([...referencesOf(module), ...sharedParams, ...GLOBAL_PLACEHOLDERS]);
    for (const [code, record] of declared(module)) {
      for (const name of [...(record.codeParams ?? []), ...Object.keys(record.uiParams ?? {})]) expect(shadowing.has(name), `${code} declares the param ${name}`).toBe(false);
    }
  });

  it.each(BUILT_IN.map((module) => module.id))("%s lists no code param named `agent`, and a UI param named `agent` only with the module's own label, which the remedy's {agent} resolves to", (agent) => {
    const module = BUILT_IN.find((candidate) => candidate.id === agent)!;
    const label = (module.messages.en as Record<string, string>)[`provider.${agent}.label`];
    expect(label, `${agent} label`).toEqual(expect.any(String));
    for (const [code, record] of declared(module)) {
      expect(record.codeParams ?? [], `${code} lists agent as a code param`).not.toContain("agent");
      if (record.uiParams && Object.hasOwn(record.uiParams, "agent")) expect(record.uiParams.agent, `${code} gives agent a value other than the module's label`).toBe(label);
    }
  });
});
