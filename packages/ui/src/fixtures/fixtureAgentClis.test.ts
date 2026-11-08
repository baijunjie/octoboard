// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Agent, AgentAvailability } from "../protocol";
import { agentCliStatusesOf, agentCliStatusOf, agentCliUpdateOf, snapshotState } from "./builders";
import type { State } from "../store";
import type { Fixture } from "./fixture";
import { createFixtureDaemon } from "./fixtureDaemon";

const available = (agent: Agent, availability: AgentAvailability["availability"]): [Agent, AgentAvailability] => [agent, { agent, availability, default_account_dir: null }];
const update = { type: "update_agent_cli", agent: "claude" } as const;
const installed = () => ({ ...snapshotState({}), agentAvailability: new Map([available("claude", "available"), available("codex", "available"), available("grok", "available")]) });

function daemonWith(fixture: Partial<Fixture> = {}, state: Partial<State> = installed()) {
  const daemon = createFixtureDaemon({ state, ...fixture });
  const status = () => daemon.store.getState().agentCliStatuses.get("claude");
  return { daemon, status };
}

afterEach(() => vi.useRealTimers());

describe("the fixture daemon's agent CLI statuses", () => {
  it("starts with an up-to-date status for each installed agent and none for an agent that is not", () => {
    const { daemon } = daemonWith();
    expect([...daemon.store.getState().agentCliStatuses.keys()]).toEqual(["claude", "codex", "grok"]);
    daemon.store.getState().agentCliStatuses.forEach((status) => expect(status.latest_version).toBe(status.installed_version));
    const withMissingCodex = createFixtureDaemon({ state: { agentAvailability: new Map([available("claude", "available"), available("codex", "unavailable"), available("grok", "not_determined")]) } });
    expect([...withMissingCodex.store.getState().agentCliStatuses.keys()]).toEqual(["claude"]);
  });

  it("keeps the statuses a scenario gives, an empty map included", () => {
    expect(createFixtureDaemon({ state: { agentCliStatuses: new Map() } }).store.getState().agentCliStatuses.size).toBe(0);
  });

  it("only acknowledges a check, so a window with no status keeps waiting for one", async () => {
    const daemon = createFixtureDaemon({ state: { agentCliStatuses: new Map() } });
    await expect(daemon.request({ type: "check_agent_clis" })).resolves.toMatchObject({ type: "ack" });
    expect(daemon.store.getState().agentCliStatuses.size).toBe(0);
  });
});

describe("the fixture daemon's update of an agent CLI", () => {
  const outdated = agentCliStatusOf("claude", { install_method: "npm", latest_version: "2.1.320" });
  const stateWith = (...statuses: ReturnType<typeof agentCliStatusOf>[]) => ({ ...installed(), agentCliStatuses: agentCliStatusesOf(statuses) });

  it("marks the update running, then leaves the CLI at its latest version", async () => {
    vi.useFakeTimers();
    const { daemon, status } = daemonWith({}, stateWith(outdated));
    await daemon.request(update);
    expect(status()).toMatchObject({ installed_version: "2.1.319", update: { state: "running", finished_at: null } });
    await vi.advanceTimersByTimeAsync(1000);
    expect(status()).toMatchObject({ installed_version: "2.1.320", latest_version: "2.1.320", update: { state: "succeeded", error: null, detail: null, finished_at: expect.any(Number) } });
  });

  it("ends as the scenario says when it fails, the CLI as it was", async () => {
    vi.useFakeTimers();
    const { daemon, status } = daemonWith({ agentCliUpdateFailure: { error: "agent_cli_update_failed", detail: "EACCES" } }, stateWith(outdated));
    await daemon.request(update);
    await vi.advanceTimersByTimeAsync(1000);
    expect(status()).toMatchObject({ installed_version: "2.1.319", update: { state: "failed", error: "agent_cli_update_failed", detail: "EACCES" } });
  });

  it("refuses an update that is running, an install it cannot tell, a CLI it never checked, and what the scenario refuses", async () => {
    await expect(daemonWith({}, stateWith({ ...outdated, update: agentCliUpdateOf() })).daemon.request(update)).rejects.toMatchObject({ code: "agent_cli_update_running", params: { agent: "Claude Code" } });
    await expect(daemonWith({}, stateWith(agentCliStatusOf("claude", { install_method: "unknown", latest_version: null }))).daemon.request(update)).rejects.toMatchObject({ code: "agent_cli_update_unsupported" });
    await expect(daemonWith({}, { ...installed(), agentCliStatuses: new Map() }).daemon.request(update)).rejects.toMatchObject({ code: "agent_cli_update_unsupported" });
    const refused = { code: "agent_cli_update_unsupported", params: { agent: "Claude Code" }, message: "no command" };
    const { daemon, status } = daemonWith({ agentCliUpdateError: refused }, stateWith(outdated));
    await expect(daemon.request(update)).rejects.toMatchObject({ code: "agent_cli_update_unsupported" });
    expect(status()?.update).toBeNull();
  });

  it("refuses an agent that is not installed or not added", async () => {
    const notInstalled = { ...installed(), agentAvailability: new Map([available("claude", "unavailable")]) };
    await expect(daemonWith({}, notInstalled).daemon.request(update)).rejects.toMatchObject({ code: "agent_not_installed" });
    const notAdded = { ...stateWith(outdated), settings: { ...installed().settings!, added_agents: ["codex" as Agent] } };
    await expect(daemonWith({}, notAdded).daemon.request(update)).rejects.toMatchObject({ code: "agent_not_added" });
  });
});
