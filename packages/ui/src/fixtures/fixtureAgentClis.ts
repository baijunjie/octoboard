// What the fixture daemon answers about the versions of the agents' CLIs: the statuses a window starts with, and the
// update of one.
import type { Agent, AgentAvailability, AgentCliStatus } from "../protocol";
import { agentCliStatusesOf, agentCliStatusOf } from "./builders";
import type { Fixture } from "./fixture";
import type { FixtureContext, Handlers } from "./fixtureContext";
import { servedLabel } from "./fixtureProviders";

/** How long an update runs before it ends, for the window to show it running first. */
export const UPDATE_DURATION_MS = 600;

/** Each registered agent's installed version and how it was installed, as a machine that has them all current has them;
 * a provider served beyond these is treated as one more installed through its own installer. */
const INSTALLED: Record<string, Pick<AgentCliStatus, "installed_version" | "install_method">> = {
  claude: { installed_version: "2.1.319", install_method: "native" },
  codex: { installed_version: "0.130.0", install_method: "npm" },
  grok: { installed_version: "0.1.45", install_method: "native" },
};

/** What the daemon holds once it has checked every agent that is installed, each one up to date: the statuses a window
 * starts with unless the scenario gives its own, so that the Version row of an agent's tab never stays at "Checking the
 * version…" in a scenario that is not about it. */
export function defaultAgentCliStatuses(availability: ReadonlyMap<Agent, AgentAvailability>): Map<Agent, AgentCliStatus> {
  return agentCliStatusesOf([...availability.values()]
    .filter((entry) => entry.availability === "available")
    .map(({ agent }) => {
      const { installed_version, install_method } = INSTALLED[agent] ?? { installed_version: "1.0.0", install_method: "native" };
      return agentCliStatusOf(agent, { installed_version, install_method, latest_version: installed_version });
    }));
}

/** `state` with every installed agent's CLI current, but for the one `status` is of. */
export const withCliStatus = (state: Fixture["state"], status: AgentCliStatus): Fixture["state"] => ({
  ...state,
  agentCliStatuses: new Map(defaultAgentCliStatuses(state!.agentAvailability!)).set(status.agent, status),
});

/** The replies about the agents' CLIs. A `check_agent_clis` is acknowledged and nothing more: the statuses are the
 * scenario's, so a window that has none keeps waiting for them. */
export function createAgentCliReplies({ scenario, store, fail }: FixtureContext): Handlers {
  return {
    update_agent_cli: (body) => {
      const current = store.getState();
      const agent = servedLabel(body.agent);
      if (current.settings.added_agents && !current.settings.added_agents.includes(body.agent)) {
        return fail({ code: "agent_not_added", params: { agent }, message: `${agent} is not added. Add it in Settings → Agent accounts to update its CLI.` });
      }
      if (current.agentAvailability.get(body.agent)?.availability !== "available") {
        return fail({ code: "agent_not_installed", params: { agent }, message: `${agent} is not installed on this machine` });
      }
      const status = current.agentCliStatuses.get(body.agent);
      if (status?.update?.state === "running") return fail({ code: "agent_cli_update_running", params: { agent }, message: `${agent} is already being updated` });
      if (!status || status.install_method === "unknown") {
        return fail({ code: "agent_cli_update_unsupported", params: { agent }, message: `${agent} cannot be updated: no command is known to update it` });
      }
      if (scenario.agentCliUpdateError) return fail(scenario.agentCliUpdateError);
      const started_at = Date.now();
      const put = (next: AgentCliStatus) => store.setState((state) => ({ agentCliStatuses: new Map(state.agentCliStatuses).set(body.agent, next) }));
      put({ ...status, update: { state: "running", started_at, finished_at: null, error: null, detail: null } });
      setTimeout(() => {
        const failure = scenario.agentCliUpdateFailure;
        const finished_at = Date.now();
        // A successful update leaves the CLI at the latest version; a failed one leaves it as it was.
        put(failure
          ? { ...status, checked_at: finished_at, update: { state: "failed", started_at, finished_at, error: failure.error, detail: failure.detail } }
          : { ...status, installed_version: status.latest_version ?? status.installed_version, checked_at: finished_at, update: { state: "succeeded", started_at, finished_at, error: null, detail: null } });
      }, UPDATE_DURATION_MS);
      return { type: "ack", id: null };
    },
  };
}
