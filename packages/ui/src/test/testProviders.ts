import type { Agent, ProviderInfo } from "../protocol";
import { FIXTURE_PROVIDERS } from "../fixtures/fixtureProviders";
import { BUILT_IN_SERVED, TEST_AGENT_SERVED } from "../providers/servedFixtures";

/** The test-only provider as the daemon serves it (`providers/testAgent.fixture.ts`). */
export const TEST_AGENT_PROVIDER: ProviderInfo = TEST_AGENT_SERVED;

/** A provider a daemon serves that the UI has no presentation module for, with the test-only provider's setting: what
 * the settings screens show of a provider without copy for its settings (the provider under its served label, no
 * setting). It is not among `TEST_PROVIDERS`; a test serves it beside them. */
export const UNPRESENTED_PROVIDER: ProviderInfo = {
  ...TEST_AGENT_PROVIDER,
  id: "unpresented_agent",
  label: "Unpresented Agent",
  short_name: "Unpresented",
  default_config_dir: ".unpresented_agent",
  detection_markers: [".unpresented_agent"],
};

/** The providers a test serves in a `snapshot`, in registry order: every registered module's served info, released or not
 * (`BUILT_IN_SERVED`), and, last, the test-only provider, so every test that serves these runs it through. The fixture
 * daemon and the gallery serve the released ones alone (`FIXTURE_PROVIDERS`). */
export const TEST_PROVIDERS: ProviderInfo[] = [...BUILT_IN_SERVED, TEST_AGENT_PROVIDER];

/** The ids of `TEST_PROVIDERS`: every agent a test that serves them has, in registry order. */
export const TEST_AGENTS: Agent[] = TEST_PROVIDERS.map(({ id }) => id);

/** The providers released with the application (`FIXTURE_PROVIDERS`) and the test-only one: the served list of a test that
 * spells out its agents' names, order and markers, which stays what the released set is while preview providers are
 * registered and follows it when one is released. Only a test that walks every registered provider (`TEST_PROVIDERS`)
 * follows the registry. */
export const BASELINE_PROVIDERS: ProviderInfo[] = [...FIXTURE_PROVIDERS, TEST_AGENT_PROVIDER];

/** The ids of `BASELINE_PROVIDERS`, in registry order. */
export const BASELINE_AGENTS: Agent[] = BASELINE_PROVIDERS.map(({ id }) => id);
