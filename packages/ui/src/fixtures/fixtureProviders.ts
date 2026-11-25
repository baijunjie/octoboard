// The providers the fixture daemon serves in its `snapshot`: what the real daemon's registry describes, so the install
// links, config-directory placeholders and default agent a scenario shows are the real app's.
import type { Agent, AgentAvailability, DeliveryQuirks, ModelDisplay, ProviderInfo } from "../protocol";
import { BUILT_IN_SERVED } from "../providers/servedFixtures";

const NO_QUIRKS: DeliveryQuirks = { confirms_by_reported_prompt: false, holds_report_for_completion_notice: false, resume_instruction_as_launch_prompt: false };

const PLAIN_MODELS: ModelDisplay = { family_version: false, id_prefix: null, aliases: [], retired: [] };

/** The providers released with the application, in the registry's order: what the fixture daemon, the gallery and the
 * website preview serve. A provider's module may be registered before it is released (`BUILT_IN`); releasing it appends
 * its id here. */
const RELEASED = ["claude", "codex", "grok", "antigravity", "copilot", "opencode", "qwen", "pi"];

/** Every released provider's descriptor data, in the registry's order, constant like the real daemon's while it runs:
 * the served-info fixture beside each module (`providers/<id>.fixture.ts`) for the ids in `RELEASED`. The tests' list,
 * which holds every registered module, is `TEST_PROVIDERS`. */
export const FIXTURE_PROVIDERS: ProviderInfo[] = RELEASED.map((id) => {
  const served = BUILT_IN_SERVED.find((provider) => provider.id === id);
  if (!served) throw new Error(`The released provider ${id} has no registered served-info fixture`);
  return served;
});

/** A provider a newer daemon could serve that this build has no presentation module for (it is not among
 * `FIXTURE_PROVIDERS`, which only the registered ones are, so a scenario serves it beside them): the UI names it by this
 * label and draws the generic mark, and follows these capabilities and window naming as it does for any provider. It
 * declares one exported choice setting this build has no words for, which the accounts settings leave out and an
 * import review names by the agent and the setting's id. */
export const UNPRESENTED_PROVIDER: ProviderInfo = {
  id: "acme_agent",
  label: "Acme Agent",
  short_name: "Acme",
  install_url: "https://example.com/acme-agent/install",
  default_config_dir: ".acme",
  detection_markers: [".acme"],
  capabilities: { preallocates_session_id: true, hook_script: false, status_line: false, reset_credits: false, native_app_scheme: null, per_session_trust: false, config_sharing: false, owned_accounts: true, login_flow: true, mcp_inventory: false, ends_command_groups_on_exit: false, interactions: { permission: false, question: false }, message_images: false, slash_commands: false, delivery: NO_QUIRKS },
  quota_windows: { kinds: ["session", "weekly"], session_window: "session", weekly_all_models: false },
  model_display: PLAIN_MODELS,
  native_memory: "absent",
  settings: [{ id: "review_mode", kind: { type: "choice", choices: [{ value: "careful", confirm: false }, { value: "fast", confirm: false }] }, exported: true, shown_to_agents: false, compatibility_with: null }],
};

/** A provider a newer daemon could serve whose capabilities say it has the default account only
 * (`capabilities.owned_accounts: false`). Tests use it to verify that the capability works for an unknown provider. */
export const DEFAULT_ACCOUNT_ONLY_PROVIDER: ProviderInfo = {
  ...UNPRESENTED_PROVIDER,
  id: "solo_agent",
  label: "Solo Agent",
  short_name: "Solo",
  install_url: "https://example.com/solo-agent/install",
  default_config_dir: ".solo",
  detection_markers: [".solo"],
  capabilities: { ...UNPRESENTED_PROVIDER.capabilities, owned_accounts: false },
  settings: [],
};

/** A provider a newer daemon could serve that Octoboard reads no plan quota for (`quota_windows.kinds` is empty) and
 * cannot sign an account in without a terminal (`capabilities.login_flow: false`). It also supports owned accounts,
 * so scenarios can show several sign-in readings and the generic sign-in guidance for a provider with no presentation. */
export const NO_QUOTA_PROVIDER: ProviderInfo = {
  ...UNPRESENTED_PROVIDER,
  id: "terminal_agent",
  label: "Terminal Agent",
  short_name: "Terminal",
  install_url: "https://example.com/terminal-agent/install",
  default_config_dir: ".terminal-agent",
  detection_markers: [".terminal-agent"],
  capabilities: { ...UNPRESENTED_PROVIDER.capabilities, login_flow: false },
  quota_windows: { kinds: [], session_window: "session", weekly_all_models: false },
  settings: [],
};

/** The ids of `FIXTURE_PROVIDERS`: every agent the fixture daemon serves, in its order. */
export const FIXTURE_AGENTS: Agent[] = FIXTURE_PROVIDERS.map(({ id }) => id);

/** The availability a daemon reports: every one of `providers`' agents, in the registry's order, `not_determined` (as
 * before its one-time determination lands) unless `states` says otherwise. */
export function availabilityOf(providers: readonly ProviderInfo[], states: Partial<Record<Agent, AgentAvailability["availability"]>> = {}): Map<Agent, AgentAvailability> {
  return new Map(providers.map(({ id }) => [id, { agent: id, availability: states[id] ?? "not_determined", default_account_dir: null }]));
}

/** The name of `agent` the daemon writes into its own text (an error's parameters and fallback message), from its
 * descriptor. */
export function servedLabel(agent: Agent): string {
  const provider = FIXTURE_PROVIDERS.find(({ id }) => id === agent);
  if (!provider) throw new Error(`The fixture providers have no ${agent}`);
  return provider.label;
}
