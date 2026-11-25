// The one composition behind the helpers that give the test-only provider `test_agent` a declaration for a single test
// file (`acknowledgedNoticeProvider.ts`, `remedyProvider.ts`, `loginQuestionProvider.ts`, `sessionNoteProvider.ts`): the
// registry's own values pass through, and what the helper declares is added where the real registry would hold it had
// the provider's module declared it.
import type { CodeRecord } from "../errors/codes";
import type { ProviderPresentation } from "../providers/types";

export type Registry = typeof import("../providers/registry");

/** What a helper adds to the test-only provider. */
export interface TestAgentDeclarations {
  /** Catalog entries, the same text in every language. */
  readonly messages?: Readonly<Record<string, string>>;
  /** The UI's records of the provider's codes, by code. */
  readonly codes?: Readonly<Record<string, CodeRecord>>;
  /** Brand-name placeholders the module declares as global ones. */
  readonly params?: Readonly<Record<string, string>>;
  /** The literals a message names, by message name, as `references` of a module. */
  readonly references?: ProviderPresentation["references"];
  /** The literals its recovery messages name. */
  readonly recoveryReferences?: Readonly<Record<string, string>>;
}

/** `registry` with `declarations` added to the test-only provider: its presentation module gains them (its messages in
 * every language the module carries), and the merged catalogs, code records and placeholders gain them too. */
export function withTestAgent(registry: Registry, declarations: TestAgentDeclarations): Registry {
  const { messages = {}, codes, params, references, recoveryReferences } = declarations;
  const presentation: Registry["providerPresentation"] = (id) => {
    const module = registry.providerPresentation(id);
    if (id !== "test_agent" || !module) return module;
    return {
      ...module,
      ...(codes && { codes: { ...module.codes, ...codes } }),
      ...(params && { params: { ...module.params, ...params } }),
      ...(references && { references: { ...module.references, ...references } }),
      ...(recoveryReferences && { recoveryReferences: { ...module.recoveryReferences, ...recoveryReferences } }),
      messages: Object.fromEntries(Object.entries(module.messages).map(([language, own]) => [language, { ...own, ...messages }])) as ProviderPresentation["messages"],
    };
  };
  return {
    ...registry,
    providerPresentation: presentation,
    // The registry's own readers of a module call its `providerPresentation` from inside the module, which the mock does
    // not reach, so the one a declaration here feeds reads the presentation above, as the real one reads the module's.
    providerReferences: (id, name) => presentation(id)?.references?.[name] ?? {},
    PROVIDER_PARAMS: { ...registry.PROVIDER_PARAMS, ...params } as Registry["PROVIDER_PARAMS"],
    PROVIDER_CODES: { ...registry.PROVIDER_CODES, ...codes } as Registry["PROVIDER_CODES"],
    PROVIDER_MESSAGES: Object.fromEntries(
      Object.entries(registry.PROVIDER_MESSAGES).map(([language, own]) => [language, { ...own, ...messages }]),
    ) as unknown as Registry["PROVIDER_MESSAGES"],
  };
}
