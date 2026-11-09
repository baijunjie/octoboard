import { FIXTURE_PROVIDERS } from "../fixtures/fixtureProviders";
import type { Agent, ProviderInfo } from "../protocol";

/** The test-only provider, as the daemon's registry describes it (`apps/daemon/src/providers/test_provider.rs`): no
 * capability or compatibility of its own, a single session window, and one choice setting, `mode`, that is neither
 * exported nor shown to agents (`reckless` asks for confirmation). Its presentation module is registered under the test
 * runner (`providers/registry.ts`). */
export const TEST_AGENT_PROVIDER: ProviderInfo = {
  id: "test_agent",
  label: "Test Agent",
  short_name: "Test",
  install_url: "https://example.com/test-agent",
  default_config_dir: ".octoboard-test-agent",
  detection_markers: [".test-agent"],
  capabilities: {
    preallocates_session_id: false,
    hook_script: false,
    status_line: false,
    reset_credits: false,
    per_session_trust: false,
    config_sharing: false,
    mcp_inventory: false,
    delivery: { confirms_by_reported_prompt: false, holds_report_for_completion_notice: false, resume_instruction_as_launch_prompt: false },
  },
  quota_windows: { kinds: ["session"], session_window: "session", weekly_all_models: false },
  model_display: { family_version: false, id_prefix: null, aliases: [], retired: [] },
  settings: [
    { id: "mode", kind: { type: "choice", choices: [{ value: "careful", confirm: false }, { value: "reckless", confirm: true }] }, exported: false, shown_to_agents: false, compatibility_with: null },
  ],
};

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

/** The providers a daemon serves in its `snapshot`, in its registry order: the fixture daemon's (`FIXTURE_PROVIDERS`, so
 * the two cannot drift) and, last, the test-only provider, so every test that serves these runs it through. */
export const TEST_PROVIDERS: ProviderInfo[] = [...FIXTURE_PROVIDERS, TEST_AGENT_PROVIDER];

/** The ids of `TEST_PROVIDERS`: every agent a test that serves them has, in registry order. */
export const TEST_AGENTS: Agent[] = TEST_PROVIDERS.map(({ id }) => id);
