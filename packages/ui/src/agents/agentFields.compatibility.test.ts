import { expect, it } from "vitest";

import { TEST_AGENTS } from "../test/testProviders";
import { changedLaunch, launchFieldValue, readLaunch, withLaunchField } from "./agentFields";

it.each(TEST_AGENTS)("reads, edits and saves %s's switches independently of other agents and launch fields", (agent) => {
  const initial = readLaunch({ [agent]: { model: "kept", effort: "high", compatibility: { held: true, untouched: false } } }, TEST_AGENTS);
  const changed = withLaunchField(withLaunchField(initial, agent, "compatibility.held", null), agent, "compatibility.new", false);
  expect(changed[agent]).toEqual({ model: "kept", effort: "high", compatibility: { untouched: false, new: false } });
  expect(changedLaunch("update", changed, initial)).toEqual({ [agent]: { compatibility: { held: null, new: false } } });
  expect(initial[agent]!.compatibility).toEqual({ held: true, untouched: false });
  for (const other of TEST_AGENTS.filter((id) => id !== agent)) expect(changed[other]).toEqual({ model: null, effort: null, compatibility: {} });
  expect(changedLaunch("update", initial, initial)).toBeUndefined();
});

it("creates with explicit compatibility values only and preserves edits to agents no longer shown", () => {
  const initial = readLaunch(undefined, TEST_AGENTS);
  const current = TEST_AGENTS.reduce((launch, agent, index) => withLaunchField(launch, agent, "compatibility.held", index % 2 === 0), initial);
  expect(changedLaunch("create", current, initial)).toEqual(Object.fromEntries(TEST_AGENTS.map((agent, index) => [agent, { compatibility: { held: index % 2 === 0 } }])));
});

it("reads a false compatibility field as held and a missing one as follow", () => {
  const launch = { model: "m", effort: "high", compatibility: { held: false } };
  expect(launchFieldValue(launch, "compatibility", "held")).toBe(false);
  expect(launchFieldValue(launch, "compatibility", "missing")).toBeNull();
  expect(launchFieldValue(undefined, "compatibility", "held")).toBeNull();
  expect(launchFieldValue(launch, "model")).toBe("m");
  expect(launchFieldValue(launch, "effort")).toBe("high");
});
