import { expect, it } from "vitest";

import { AGENT_OPTIONS } from "../agents";
import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Agent, AgentAvailability } from "../protocol";
import { terminalEmptyState } from "./emptyState";

const console = consoleOf("c", "Main");
const project = projectOf("p", console.id, "Website");
const other = projectOf("other", console.id, "API");
const archive = sessionOf("old", console.id, project.id, "Old work", "archived");
const live = sessionOf("live", console.id, project.id, "Work", "interrupted");
const owner = sessionOf("owner", console.id, undefined, "Owner", "idle");
const pending = new Map<Agent, AgentAvailability>();
const unavailable = new Map(AGENT_OPTIONS.map(({ value }) => [value, { agent: value, availability: "unavailable" as const }]));

it("does not treat pre-snapshot empty collections as a bare console", () => {
  expect(terminalEmptyState(false, console, undefined, [], [], pending)).toBeUndefined();
});

it.each([
  { projects: [project], sessions: [live, { ...owner, status: "archived" as const }], kind: "new-console-session" },
  { projects: [project], sessions: [owner, archive, live], kind: "new-session" },
  { projects: [project, other], sessions: [owner], kind: undefined },
  { projects: [], sessions: [], kind: "new-console-session" },
  { projects: [], sessions: [owner], kind: "add-project" },
])("offers $kind in normal mode based on console sessions and project count", ({ projects, sessions, kind }) => {
  const state = terminalEmptyState(true, console, undefined, projects, sessions, pending);
  expect(state?.kind).toBe(kind);
  if (state?.kind === "new-session") expect(state).toEqual({ kind, project, binding: { kind: "choose" } });
});

it("project focus creates an unbound session even when another owner's team is live", () => {
  const lead = { ...live, bound_to: owner.id };
  const child = sessionOf("child", console.id, project.id, "Child", "working", { bound_to: lead.id });
  expect(terminalEmptyState(true, console, { project }, [project], [archive, owner, lead, child], pending))
    .toEqual({ kind: "new-session", project, binding: { kind: "unbound" } });
  expect(terminalEmptyState(true, console, { project }, [project], [live], pending)).toBeUndefined();
});

it("a console session in focus is itself selectable even without bound sessions", () => {
  expect(terminalEmptyState(true, console, { consoleSession: owner }, [], [owner], pending)).toBeUndefined();
});

it("known missing agents explain recovery while undetermined availability still offers creation", () => {
  expect(terminalEmptyState(true, console, { project }, [project], [], unavailable)).toEqual({ kind: "no-agents" });
  expect(terminalEmptyState(true, console, undefined, [project, other], [], unavailable)).toEqual({ kind: "no-agents" });
  expect(terminalEmptyState(true, console, undefined, [], [owner], unavailable)).toEqual({ kind: "add-project" });
  expect(terminalEmptyState(true, console, undefined, [project], [owner], unavailable)).toEqual({ kind: "no-agents" });
  expect(terminalEmptyState(true, console, undefined, [project, other], [owner], unavailable)).toBeUndefined();
});
