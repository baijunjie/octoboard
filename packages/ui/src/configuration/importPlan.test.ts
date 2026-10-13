import { describe, expect, it } from "vitest";

import { REVIEW } from "../gallery/fixtures/configuration";
import type { ImportedConsole } from "../protocol";
import {
  applyToAll,
  availableFor,
  choicesFor,
  directoryProblem,
  initialReview,
  outcomeOf,
  resolutionsOf,
  targetOf,
  withRow,
} from "./importPlan";

const consoles = REVIEW.consoles;
const [website, searchApi, designSystem, notes] = consoles[0].projects;

it("offers a project only the choices its state allows, and pointing at a directory or leaving it out always", () => {
  expect(choicesFor(website)).toEqual(["exported_path", "default_clone_dir", "directory", "skip"]);
  expect(choicesFor(searchApi)).toEqual(["default_clone_dir", "directory", "skip"]);
  expect(choicesFor(designSystem)).toEqual(["clone", "directory", "skip"]);
  expect(choicesFor(notes)).toEqual(["directory", "skip"]);
});

it("starts every project on the daemon's default, and leaves out one that has none", () => {
  expect(initialReview(consoles).map((rows) => rows.map((row) => row.choice))).toEqual([
    ["exported_path", "default_clone_dir", "clone", "skip"],
    ["clone"],
    [],
  ]);
});

it("applies a choice to the projects that offer it and leaves every other row as it was", () => {
  const start = withRow(initialReview(consoles), 0, 0, { choice: "directory", directory: "~/web" });
  const cloned = applyToAll(consoles, start, "clone");
  expect(cloned.map((rows) => rows.map((row) => row.choice))).toEqual([
    ["directory", "default_clone_dir", "clone", "skip"],
    ["clone"],
    [],
  ]);
  // The typed directory is kept under a choice that is not the directory, so going back finds it.
  expect(applyToAll(consoles, start, "default_clone_dir")[0][0]).toEqual({ choice: "default_clone_dir", directory: "~/web" });
});

it("leaves out every project when asked to, whatever it offers", () => {
  const left = applyToAll(consoles, initialReview(consoles), "skip");
  expect(left.flat().map((row) => row.choice)).toEqual(["skip", "skip", "skip", "skip", "skip"]);
});

it("counts the projects a choice is available for", () => {
  expect(
    (["exported_path", "default_clone_dir", "clone", "skip"] as const).map((choice) => availableFor(consoles, choice)),
  ).toEqual([1, 2, 2, 5]);
});

it("replaces one row and keeps every other the same object", () => {
  const start = initialReview(consoles);
  const next = withRow(start, 0, 1, { choice: "skip", directory: "" });
  expect(next[0][1].choice).toBe("skip");
  expect(next[0][0]).toBe(start[0][0]);
  expect(next[1]).toBe(start[1]);
});

it.each([
  { directory: "", problem: "empty" },
  { directory: "relative/dir", problem: "not_absolute" },
  { directory: "~other/dir", problem: "not_absolute" },
  { directory: "/srv/web", problem: undefined },
  { directory: "~/code/web", problem: undefined },
])("judges the directory typed for a project: $directory", ({ directory, problem }) => {
  expect(directoryProblem({ choice: "directory", directory })).toBe(problem);
});

it("finds no fault with the directory of a row that does not use one", () => {
  expect(directoryProblem({ choice: "skip", directory: "relative/dir" })).toBeUndefined();
  expect(directoryProblem({ choice: "clone", directory: "" })).toBeUndefined();
});

it("shows the directory each choice would use", () => {
  expect(targetOf(website, { choice: "exported_path", directory: "" })).toBe("/Users/ana/code/website");
  expect(targetOf(website, { choice: "default_clone_dir", directory: "" })).toBe("/Users/dev/Projects/website");
  expect(targetOf(designSystem, { choice: "clone", directory: "" })).toBe("/Users/dev/Projects/design-system");
  expect(targetOf(notes, { choice: "directory", directory: "~/notes" })).toBe("~/notes");
  expect(targetOf(notes, { choice: "directory", directory: "" })).toBeUndefined();
  expect(targetOf(notes, { choice: "skip", directory: "" })).toBeUndefined();
});

it("sends one resolution for each project that is placed, by its position in the document, and none for one left out", () => {
  const review = withRow(initialReview(consoles), 0, 3, { choice: "directory", directory: "~/notes" });
  expect(resolutionsOf(review)).toEqual([
    { console: 0, project: 0, choice: { kind: "exported_path" } },
    { console: 0, project: 1, choice: { kind: "default_clone_dir" } },
    { console: 0, project: 2, choice: { kind: "clone" } },
    { console: 0, project: 3, choice: { kind: "directory", path: "~/notes" } },
    { console: 1, project: 0, choice: { kind: "clone" } },
  ]);
  expect(resolutionsOf(applyToAll(consoles, review, "skip"))).toEqual([]);
});

describe("outcomeOf", () => {
  const project = (name: string, status: ImportedConsole["projects"][number]["status"], fallbacks = 0) => ({
    name,
    status,
    via: null,
    project: null,
    error: null,
    account_fallbacks: Array.from({ length: fallbacks }, () => ({ agent: "codex" as const, name: "Personal" })),
  });
  const imported: ImportedConsole[] = [
    {
      name: "A",
      status: "created",
      console: "c1",
      error: null,
      account_fallbacks: [{ agent: "claude", name: "Work" }],
      projects: [project("a1", "created", 1), project("a2", "left_out"), project("a3", "failed")],
    },
    { name: "B", status: "failed", console: null, error: null, account_fallbacks: [], projects: [project("b1", "not_attempted")] },
  ];

  it("counts what was created, left out, failed and not tried", () => {
    expect(outcomeOf(imported)).toMatchObject({
      consolesCreated: 1,
      consolesFailed: 1,
      projectsCreated: 1,
      projectsLeftOut: 1,
      projectsFailed: 1,
      projectsNotAttempted: 1,
    });
  });

  it("collects every account that fell back with the console or project that held it", () => {
    expect(outcomeOf(imported).fallbacks).toEqual([
      { agent: "claude", name: "Work", console: "A" },
      { agent: "codex", name: "Personal", console: "A", project: "a1" },
    ]);
  });
});
