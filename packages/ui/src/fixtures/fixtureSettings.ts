// What the fixture daemon answers for the application's settings: the preferences the daemon stores, the agents the user
// has added, the approval choices and the trusted folders.
import { FIXTURE_AGENTS, servedLabel } from "./fixtureProviders";
import { type Agent, type ClientRequestBody } from "../protocol";
import { defaultClaudeModel, SAMPLE_MODELS } from "./agentModels";
import { effectiveProviderSettings } from "./builders";
import { HOME, neverReplied, normalizedPath, normalizeTags, type Ack, type FixtureContext, type Handlers } from "./fixtureContext";

/** Keeps the window's settings in step with the user's explicit choices (`providerSettingChoices`): what the window is
 * told is the value in effect: the choice, or the setting's default where there is none. A scenario's own state carries
 * only the explicit choices, and so can anything else that replaces `settings` or `providers` as a whole, so the store
 * is watched rather than written once. */
export function keepProviderSettingsInEffect({ store, providerSettingChoices }: FixtureContext): void {
  const syncProviderSettings = () => {
    const { settings, providers } = store.getState();
    const effective = effectiveProviderSettings(providerSettingChoices.current, providers);
    if (JSON.stringify(settings.provider_settings) !== JSON.stringify(effective)) {
      store.setState({ settings: { ...settings, provider_settings: effective } });
    }
  };
  syncProviderSettings();
  store.subscribe(syncProviderSettings);
}

/** The replies about settings. */
export function createSettingsReplies({ scenario, store, fail, providerSettingChoices }: FixtureContext): Handlers {
  const changeAgentList = (body: Extract<ClientRequestBody, { type: "remove_agent" | "reorder_agents" }>): Ack => {
    if (scenario.agentListError) return fail(scenario.agentListError);
    const current = store.getState();
    const added = current.settings.added_agents;
    if (added === null) return fail({ code: "added_agents_undecided", params: {}, message: "the added agents cannot be changed until it is known which agents are installed" });
    if (body.type === "remove_agent") {
      if (added.includes(body.agent)) store.setState({ settings: { ...current.settings, added_agents: added.filter((agent) => agent !== body.agent) } });
    } else {
      if (body.agents.length !== added.length || !added.every((agent) => body.agents.filter((given) => given === agent).length === 1)) {
        return fail({ code: "agent_order_invalid", params: {}, message: "the new order must name each added agent exactly once" });
      }
      store.setState({ settings: { ...current.settings, added_agents: body.agents } });
    }
    return { type: "ack", id: null };
  };

  return {
    update_settings: (body) => {
      const unknownAgent = Object.keys(body.agent_tags ?? {}).find((name) => !FIXTURE_AGENTS.includes(name as Agent));
      if (unknownAgent !== undefined) {
        const message = `\`${unknownAgent}\` is not an agent`;
        return fail({ code: "internal_error", params: { detail: message }, message });
      }
      if (body.agent_tags !== undefined && scenario.agentTagsError) return fail(scenario.agentTagsError);
      const current = store.getState();
      let cloneDir = current.settings.default_clone_dir;
      if (body.default_clone_dir !== undefined) {
        const given = body.default_clone_dir;
        if (given.trim() === "") cloneDir = `${HOME}/Projects`;
        else {
          if (!given.startsWith("/") && given !== "~" && !given.startsWith("~/")) return fail({ code: "path_not_absolute", params: { path: given }, message: `\`${given}\` is not an absolute path; give the full path or start it with \`~/\`` });
          cloneDir = normalizedPath(given);
        }
      }
      // Normalised as the daemon does: trimmed, empty ones dropped, case-insensitive repeats dropped keeping the first spelling.
      const agentTags = { ...current.settings.agent_tags };
      for (const [agent, tags] of Object.entries(body.agent_tags ?? {}) as [Agent, string[]][]) {
        const kept = normalizeTags(tags);
        if (kept.length > 0) agentTags[agent] = kept;
        else delete agentTags[agent];
      }
      store.setState({ settings: { ...current.settings, agent_tags: agentTags, auto_close_idle_shell_terminals: body.auto_close_idle_shell_terminals ?? current.settings.auto_close_idle_shell_terminals, resume_sessions_on_start: body.resume_sessions_on_start ?? current.settings.resume_sessions_on_start, auto_pull_repositories: body.auto_pull_repositories ?? current.settings.auto_pull_repositories, auto_delete_unresumable_archives: body.auto_delete_unresumable_archives ?? current.settings.auto_delete_unresumable_archives, ...(body.auto_approve_session_requests !== undefined && { auto_approve_session_requests: body.auto_approve_session_requests }), ...(body.auto_approve_resume_requests !== undefined && { auto_approve_resume_requests: body.auto_approve_resume_requests }), ...(body.auto_approve_account_switches !== undefined && { auto_approve_account_switches: body.auto_approve_account_switches }), ...(body.auto_approve_handoffs !== undefined && { auto_approve_handoffs: body.auto_approve_handoffs }), ...(body.ask_questions_as_choices !== undefined && { ask_questions_as_choices: body.ask_questions_as_choices }), ...(body.keep_sessions_in_own_project !== undefined && { keep_sessions_in_own_project: body.keep_sessions_in_own_project }), ...(body.interface_language !== undefined && { interface_language: body.interface_language }), ...(body.keep_awake !== undefined && { keep_awake: body.keep_awake }), ...(body.keep_awake_when !== undefined && { keep_awake_when: body.keep_awake_when }), ...(body.auto_select_account !== undefined && { auto_select_account: body.auto_select_account }), ...(body.auto_switch_on_usage_limit !== undefined && { auto_switch_on_usage_limit: body.auto_switch_on_usage_limit }), ...(body.auto_start_quota_windows !== undefined && { auto_start_quota_windows: body.auto_start_quota_windows }), ...(body.git_workflow !== undefined && { git_workflow: { ...current.settings.git_workflow, ...body.git_workflow } }), default_clone_dir: cloneDir } });
      return { type: "ack", id: null };
    },
    set_provider_setting: (body) => {
      // The daemon refuses a setting the provider does not declare or a value it does not take before it writes anything;
      // a null clears the choice.
      const descriptor = store.getState().providers.find(({ id }) => id === body.agent)?.settings.find(({ id }) => id === body.setting);
      const unreadable = (detail: string) => fail({ code: "unreadable_request", params: { detail }, message: `unreadable request: ${detail}` });
      if (!descriptor) return unreadable(`${body.agent} has no setting \`${body.setting}\``);
      const takes = body.value === null || (descriptor.kind.type === "switch" ? typeof body.value === "boolean" : descriptor.kind.choices.some(({ value }) => value === body.value));
      if (!takes) return unreadable(`${body.agent}'s setting \`${body.setting}\` does not take ${JSON.stringify(body.value)}`);
      if (scenario.accountApprovalError) return fail(scenario.accountApprovalError);
      const kept = Object.entries(providerSettingChoices.current[body.agent] ?? {}).filter(([setting]) => setting !== body.setting);
      providerSettingChoices.current = { ...providerSettingChoices.current, [body.agent]: Object.fromEntries(body.value === null ? kept : [...kept, [body.setting, body.value]]) };
      const current = store.getState();
      store.setState({ settings: { ...current.settings, provider_settings: effectiveProviderSettings(providerSettingChoices.current, current.providers) } });
      return { type: "ack", id: null };
    },
    add_agent: (body) => {
      if (scenario.agentAdd === "pending") return neverReplied();
      if (scenario.agentListError) return fail(scenario.agentListError);
      const current = store.getState();
      const added = current.settings.added_agents;
      const state = current.agentAvailability.get(body.agent)?.availability;
      if (state === "unavailable") return fail({ code: "agent_not_installed", params: { agent: servedLabel(body.agent) }, message: `${servedLabel(body.agent)} is not installed on this machine` });
      if (state !== "available") return fail({ code: "agent_availability_pending", params: { agent: servedLabel(body.agent) }, message: `${servedLabel(body.agent)} cannot be added until it is known which agents are installed` });
      if (added?.includes(body.agent)) return fail({ code: "agent_already_added", params: { agent: servedLabel(body.agent) }, message: `${servedLabel(body.agent)} is already added` });
      store.setState({ settings: { ...current.settings, added_agents: [...(added ?? []), body.agent] } });
      return { type: "ack", id: null };
    },
    remove_agent: changeAgentList,
    reorder_agents: changeAgentList,
    remove_trusted_directory: (body) => {
      const path = normalizedPath(body.path);
      store.setState((current) => ({ trustedDirectories: current.trustedDirectories.filter((given) => given !== path) }));
      return { type: "ack", id: null };
    },
    list_agent_models: (body) => {
      const listed = scenario.agentModels?.[body.agent];
      if (listed === "pending") return neverReplied();
      const { models, unavailable = null } = listed ?? { models: SAMPLE_MODELS[body.agent] };
      const defaultModel = body.agent === "claude" ? defaultClaudeModel(models) : undefined;
      return { type: "agent_models", id: null, agent: body.agent, account: body.account ?? null, models, ...(defaultModel && { default_model: defaultModel, default_model_source: "best_available" as const }), unavailable };
    },
  };
}
