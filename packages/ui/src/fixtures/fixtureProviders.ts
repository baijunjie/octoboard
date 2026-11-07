// The providers the fixture daemon serves in its `snapshot`: what the real daemon's registry describes, so the install
// links, config-directory placeholders and default agent a scenario shows are the real app's.
import type { Agent, AgentAvailability, Capabilities, DeliveryQuirks, ModelDisplay, ProviderInfo, QuotaWindowNaming, SettingDescriptor } from "../protocol";

const NO_QUIRKS: DeliveryQuirks = { confirms_by_reported_prompt: false, holds_report_for_completion_notice: false, resume_instruction_as_launch_prompt: false };

const CAPABILITIES: Record<Agent, Capabilities> = {
  claude: {
    preallocates_session_id: true,
    hook_script: false,
    status_line: true,
    reset_credits: false,
    per_session_trust: false,
    config_sharing: true,
    delivery: { confirms_by_reported_prompt: true, holds_report_for_completion_notice: true, resume_instruction_as_launch_prompt: true },
  },
  codex: { preallocates_session_id: false, hook_script: true, status_line: false, reset_credits: true, per_session_trust: false, config_sharing: true, delivery: NO_QUIRKS },
  grok: { preallocates_session_id: true, hook_script: true, status_line: false, reset_credits: false, per_session_trust: true, config_sharing: true, delivery: NO_QUIRKS },
};

const PLAIN_MODELS: ModelDisplay = { family_version: false, id_prefix: null, aliases: [], retired: [] };

const QUOTA_WINDOWS: Record<Agent, QuotaWindowNaming> = {
  claude: { kinds: ["session", "weekly", "weekly_model"], session_window: "session", weekly_all_models: true },
  codex: { kinds: ["session", "weekly"], session_window: "five_hour", weekly_all_models: false },
  grok: { kinds: ["credit_period"], session_window: "five_hour", weekly_all_models: false },
};

const MODEL_DISPLAY: Record<Agent, ModelDisplay> = {
  claude: { family_version: true, id_prefix: "claude-", aliases: ["fable", "opus", "sonnet", "haiku"], retired: ["best"] },
  codex: PLAIN_MODELS,
  grok: PLAIN_MODELS,
};

const claudeCompatibility = (kind: Extract<SettingDescriptor["kind"], { type: "switch" }>): SettingDescriptor => ({ id: "claude_compatibility", kind, exported: true, shown_to_agents: true, compatibility_with: "claude" });

/** The settings each provider declares, as the daemon's registry does: the permission approval of every agent
 * (a choice an export carries, and no agent reads) and the Claude compatibility switch of Codex (on while Claude Code is
 * among the added agents) and Grok Build (on). */
const SETTINGS: Record<Agent, SettingDescriptor[]> = {
  claude: [
    { id: "approval_mode", kind: { type: "choice", choices: [{ value: "auto", confirm: false }, { value: "manual", confirm: false }, { value: "accept_edits", confirm: false }, { value: "plan", confirm: false }, { value: "bypass", confirm: true }] }, exported: true, shown_to_agents: false, compatibility_with: null },
  ],
  codex: [
    { id: "approval_reviewer", kind: { type: "choice", choices: [{ value: "user", confirm: false }, { value: "auto_review", confirm: false }, { value: "full_access", confirm: true }] }, exported: true, shown_to_agents: false, compatibility_with: null },
    claudeCompatibility({ type: "switch", default: { while_added: "claude" } }),
  ],
  grok: [
    { id: "approval_mode", kind: { type: "choice", choices: [{ value: "ask", confirm: false }, { value: "auto", confirm: false }, { value: "always_approve", confirm: true }] }, exported: true, shown_to_agents: false, compatibility_with: null },
    claudeCompatibility({ type: "switch", default: "on" }),
  ],
};

/** Every registered provider's descriptor data, in the registry's order (claude, codex, grok), constant like the real
 * daemon's while it runs. */
export const FIXTURE_PROVIDERS: ProviderInfo[] = [
  { id: "claude", label: "Claude Code", short_name: "Claude", install_url: "https://code.claude.com/docs/en/setup", default_config_dir: ".claude", capabilities: CAPABILITIES.claude, quota_windows: QUOTA_WINDOWS.claude, model_display: MODEL_DISPLAY.claude, settings: SETTINGS.claude },
  { id: "codex", label: "Codex", short_name: "Codex", install_url: "https://github.com/openai/codex", default_config_dir: ".codex", capabilities: CAPABILITIES.codex, quota_windows: QUOTA_WINDOWS.codex, model_display: MODEL_DISPLAY.codex, settings: SETTINGS.codex },
  { id: "grok", label: "Grok Build", short_name: "Grok", install_url: "https://docs.x.ai/build/overview", default_config_dir: ".grok", capabilities: CAPABILITIES.grok, quota_windows: QUOTA_WINDOWS.grok, model_display: MODEL_DISPLAY.grok, settings: SETTINGS.grok },
];

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
  capabilities: { preallocates_session_id: true, hook_script: false, status_line: false, reset_credits: false, per_session_trust: false, config_sharing: false, delivery: NO_QUIRKS },
  quota_windows: { kinds: ["session", "weekly"], session_window: "session", weekly_all_models: false },
  model_display: PLAIN_MODELS,
  settings: [{ id: "review_mode", kind: { type: "choice", choices: [{ value: "careful", confirm: false }, { value: "fast", confirm: false }] }, exported: true, shown_to_agents: false, compatibility_with: null }],
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
