import { claude } from "./claude";
import { codex } from "./codex";
import { GENERIC_MARK } from "./genericMark";
import { grok } from "./grok";
import { testAgent } from "./testAgent";
import type { ProviderColour, ProviderMark, ProviderMessageName, ProviderPresentation } from "./types";

/** The modules of the providers this build ships. The catalogs' keys and the code records are typed from these. */
const BUILT_IN = [claude, codex, grok] as const;

/** The presentation modules, in the order the providers are registered: one line registers a provider. The last line is
 * the test-only provider, present under the test runner alone. */
const MODULES: readonly ProviderPresentation[] = [
  ...BUILT_IN,
  ...(import.meta.env.MODE === "test" ? [testAgent] : []),
];

type UnionToIntersection<U> = (U extends unknown ? (variable: U) => void : never) extends (merged: infer I) => void ? I : never;

type BuiltInMessages<Language extends keyof ProviderPresentation["messages"]> = UnionToIntersection<(typeof BUILT_IN)[number]["messages"][Language]>;

/** The presentation module of the provider `id`, or `undefined` for an id with none (a provider the daemon serves that
 * this build does not know). */
export function providerPresentation(id: string): ProviderPresentation | undefined {
  return MODULES.find((module) => module.id === id);
}

/** The mark of the provider `id`: its module's, or the generic one. */
export function providerMark(id: string): ProviderMark {
  return providerPresentation(id)?.mark ?? GENERIC_MARK;
}

/** The colour of the provider `id` in each appearance, `undefined` for one that draws in the neutral foreground. */
export function providerColour(id: string): ProviderColour | undefined {
  return providerPresentation(id)?.colour;
}

/** The literals the provider `id`'s message `name` sets apart as references, by placeholder. */
export function providerReferences(id: string, name: ProviderMessageName): Readonly<Record<string, string>> {
  return providerPresentation(id)?.references?.[name] ?? {};
}

/** The display name the provider `id`'s module gives the plan `plan`, `undefined` for a plan it names nothing special. */
export function providerPlanName(id: string, plan: string): string | undefined {
  const names = providerPresentation(id)?.planNames;
  return names && Object.hasOwn(names, plan) ? names[plan] : undefined;
}

/** The UI's record of every code the providers declare, by code (`provider.<id>.<code>`), for the code registry, which
 * holds them to its `CodeRecord`. */
export const PROVIDER_CODES = Object.assign({}, ...MODULES.map((module) => module.codes)) as UnionToIntersection<NonNullable<(typeof BUILT_IN)[number]["codes"]>>;

/** The literals the description of the setting `setting` of provider `id` sets apart as references, by placeholder. */
export function settingReferences(id: string, setting: string): Readonly<Record<string, string>> {
  return providerPresentation(id)?.settingReferences?.[setting] ?? {};
}

/** The brand-name placeholders the modules declare (`params`), for the catalog to offer every message. */
export const PROVIDER_PARAMS = Object.assign({}, ...MODULES.map((module) => module.params)) as UnionToIntersection<NonNullable<(typeof BUILT_IN)[number]["params"]>>;

function mergedMessages<Language extends keyof ProviderPresentation["messages"]>(language: Language): BuiltInMessages<Language> {
  return Object.assign({}, ...MODULES.map((module) => module.messages[language]));
}

/** Every module's messages in one language, for the catalog of that language to spread in. */
export const PROVIDER_MESSAGES = {
  en: mergedMessages("en"),
  "zh-Hans": mergedMessages("zh-Hans"),
} as const;
