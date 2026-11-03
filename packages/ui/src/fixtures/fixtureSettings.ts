// What the fixture daemon answers for the application's settings: the preferences the daemon stores, the agents the user
// has added, the approval choices and the trusted folders.
import { AGENT_LABEL } from "../agents/agents";
import type { Agent, RequestBody } from "../protocol";
import { hasClaudeCompatibility } from "../agents/claudeCompatibility";
import { SAMPLE_MODELS } from "./agentModels";
import { ALL_AGENTS, effectiveClaudeCompatibility } from "./builders";
import { HOME, neverReplied, normalizedPath, normalizeTags, type Ack, type FixtureContext, type Handlers } from "./fixtureContext";

/** Keeps the window's settings in step with the user's explicit choices (`claudeCompatibilityChoices`): what the window is
 * told is the value in effect, which follows the added agents while Codex's is unset. */
export function keepClaudeCompatibilityInEffect({ store, claudeCompatibilityChoices }: FixtureContext): void {
  const syncClaudeCompatibility = () => {
    const { settings } = store.getState();
    const effective = effectiveClaudeCompatibility(claudeCompatibilityChoices.current, settings.added_agents);
    if (ALL_AGENTS.some((agent) => hasClaudeCompatibility(agent) && settings.claude_compatibility?.[agent] !== effective[agent])) {
      store.setState({ settings: { ...settings, claude_compatibility: effective } });
    }
  };
  syncClaudeCompatibility();
  store.subscribe(syncClaudeCompatibility);
}

/** The replies about settings. */
export function createSettingsReplies({ scenario, store, fail, claudeCompatibilityChoices }: FixtureContext): Handlers {
  const changeAgentList = (body: Extract<RequestBody, { type: "remove_agent" | "reorder_agents" }>): Ack => {
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
      // The daemon refuses a name that has no such setting (Claude Code, which the others are made compatible with) before
      // it writes anything, whichever other field the request carries.
      const unsupported = Object.keys(body.claude_compatibility ?? {}).find((name) => !hasClaudeCompatibility(name));
      if (unsupported !== undefined) {
        const message = `\`${unsupported}\` has no Claude compatibility setting; it applies to codex and grok`;
        return fail({ code: "internal_error", params: { detail: message }, message });
      }
      const unknownAgent = Object.keys(body.agent_tags ?? {}).find((name) => !ALL_AGENTS.includes(name as Agent));
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
      if (body.claude_compatibility !== undefined) claudeCompatibilityChoices.current = { ...claudeCompatibilityChoices.current, ...body.claude_compatibility };
      // Normalised as the daemon does: trimmed, empty ones dropped, case-insensitive repeats dropped keeping the first spelling.
      const agentTags = { ...current.settings.agent_tags };
      for (const [agent, tags] of Object.entries(body.agent_tags ?? {}) as [Agent, string[]][]) {
        const kept = normalizeTags(tags);
        if (kept.length > 0) agentTags[agent] = kept;
        else delete agentTags[agent];
      }
      store.setState({ settings: { ...current.settings, agent_tags: agentTags, auto_pull_repositories: body.auto_pull_repositories ?? current.settings.auto_pull_repositories, auto_delete_unresumable_archives: body.auto_delete_unresumable_archives ?? current.settings.auto_delete_unresumable_archives, ...(body.auto_approve_session_requests !== undefined && { auto_approve_session_requests: body.auto_approve_session_requests }), ...(body.auto_approve_resume_requests !== undefined && { auto_approve_resume_requests: body.auto_approve_resume_requests }), ...(body.auto_approve_account_switches !== undefined && { auto_approve_account_switches: body.auto_approve_account_switches }), ...(body.auto_approve_handoffs !== undefined && { auto_approve_handoffs: body.auto_approve_handoffs }), ...(body.ask_questions_as_choices !== undefined && { ask_questions_as_choices: body.ask_questions_as_choices }), ...(body.interface_language !== undefined && { interface_language: body.interface_language }), ...(body.auto_select_account !== undefined && { auto_select_account: body.auto_select_account }), ...(body.auto_switch_on_usage_limit !== undefined && { auto_switch_on_usage_limit: body.auto_switch_on_usage_limit }), ...(body.sidebar_order_by_status !== undefined && { sidebar_order_by_status: body.sidebar_order_by_status }), ...(body.auto_start_quota_windows !== undefined && { auto_start_quota_windows: body.auto_start_quota_windows }), ...(body.git_workflow !== undefined && { git_workflow: { ...current.settings.git_workflow, ...body.git_workflow } }), default_clone_dir: cloneDir } });
      return { type: "ack", id: null };
    },
    set_codex_approval_reviewer: (body) => {
      if (scenario.accountApprovalError) return fail(scenario.accountApprovalError);
      store.setState({ settings: { ...store.getState().settings, codex_approval_reviewer: body.reviewer } });
      return { type: "ack", id: null };
    },
    set_claude_approval_mode: (body) => {
      if (scenario.accountApprovalError) return fail(scenario.accountApprovalError);
      store.setState({ settings: { ...store.getState().settings, claude_approval_mode: body.mode } });
      return { type: "ack", id: null };
    },
    add_agent: (body) => {
      if (scenario.agentListError) return fail(scenario.agentListError);
      const current = store.getState();
      const added = current.settings.added_agents;
      const state = current.agentAvailability.get(body.agent)?.availability;
      if (state === "unavailable") return fail({ code: "agent_not_installed", params: { agent: AGENT_LABEL[body.agent] }, message: `${AGENT_LABEL[body.agent]} is not installed on this machine` });
      if (state !== "available") return fail({ code: "agent_availability_pending", params: { agent: AGENT_LABEL[body.agent] }, message: `${AGENT_LABEL[body.agent]} cannot be added until it is known which agents are installed` });
      if (added?.includes(body.agent)) return fail({ code: "agent_already_added", params: { agent: AGENT_LABEL[body.agent] }, message: `${AGENT_LABEL[body.agent]} is already added` });
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
      return { type: "agent_models", id: null, agent: body.agent, account: body.account ?? null, models, unavailable };
    },
  };
}
