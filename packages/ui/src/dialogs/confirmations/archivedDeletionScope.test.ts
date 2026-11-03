import { expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../../fixtures/builders";
import { archivedDeletionScope } from "./archivedDeletionScope";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const hub = sessionOf("s-hub", console_.id, undefined, "Hub", "archived", { role: "console" });
const lead = sessionOf("s-lead", console_.id, project.id, "Lead", "archived", { bound_to: hub.id });
const worker = sessionOf("s-worker", console_.id, project.id, "Worker", "archived", { bound_to: lead.id });
const live = sessionOf("s-live", console_.id, project.id, "Live", "idle", { bound_to: hub.id });
const all = [hub, lead, worker, live];

it("takes a console's archived console sessions with the archived sessions bound under them", () => {
  const scope = archivedDeletionScope(all, { kind: "delete-archived", console: console_ });
  expect(scope.count).toBe(1);
  expect(scope.withThem).toBe(2);
  expect(scope.targetIds.sort()).toEqual(["s-hub", "s-lead", "s-worker"]);
});

it("takes a project's archived sessions, each with those bound under it, once", () => {
  const scope = archivedDeletionScope(all, { kind: "delete-archived", console: console_, project });
  expect(scope.count).toBe(2);
  expect(scope.withThem).toBe(0);
  expect(scope.targetIds.sort()).toEqual(["s-lead", "s-worker"]);
});

it("takes the archived sessions bound to a console session, and those bound to them", () => {
  const scope = archivedDeletionScope(all, { kind: "delete-archived", console: console_, consoleSession: hub });
  expect(scope.count).toBe(2);
  expect(scope.targetIds.sort()).toEqual(["s-lead", "s-worker"]);
});
