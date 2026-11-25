import type { ProviderInfo } from "../protocol";
import { BUILT_IN } from "./registry";
import { served as testAgentServed } from "./testAgent.fixture";

const FIXTURES = import.meta.glob<ProviderInfo>("./*.fixture.ts", { eager: true, import: "served" });

/** What the daemon serves for each built-in provider, in the registry's order: the `served` fixture beside its module
 * (`<id>.fixture.ts`), found by the module's id, so a provider is added by its module, its fixture and its `BUILT_IN`
 * line. A registered module with no fixture fails here. For the fixture daemon and the tests, never production. */
export const BUILT_IN_SERVED: ProviderInfo[] = BUILT_IN.map(({ id }) => {
  const served = FIXTURES[`./${id}.fixture.ts`];
  if (!served) throw new Error(`The provider ${id} has no served-info fixture (providers/${id}.fixture.ts)`);
  return served;
});

/** What the daemon serves for the test-only provider, whose module the registry holds under the test runner alone. */
export const TEST_AGENT_SERVED: ProviderInfo = testAgentServed;
