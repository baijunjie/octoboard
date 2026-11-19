import { expect, it } from "vitest";

import { consoleOf } from "../../fixtures/builders";
import { PROJECT } from "../../fixtures/memoryBuilders";
import { scopeMatches, targetKey, targetRequest } from "./memoryTarget";

const console_ = { kind: "console", console: consoleOf("c-1", "Main") } as const;
const project = { kind: "project", project: PROJECT } as const;

it("names the memory in a request by the console or by the project", () => {
  expect(targetRequest(console_)).toEqual({ console: "c-1" });
  expect(targetRequest(project)).toEqual({ project: "p-1" });
});

it("matches a console by its id and a project by its host and path, never across the two", () => {
  expect(scopeMatches(console_, { kind: "console", console_id: "c-1" })).toBe(true);
  expect(scopeMatches(console_, { kind: "console", console_id: "c-2" })).toBe(false);
  expect(scopeMatches(console_, { kind: "place", host_id: "local", path: "/work/app" })).toBe(false);
  expect(scopeMatches(project, { kind: "place", host_id: "local", path: "/work/app" })).toBe(true);
  expect(scopeMatches(project, { kind: "place", host_id: "other", path: "/work/app" })).toBe(false);
  expect(scopeMatches(project, { kind: "place", host_id: "local", path: "/work/App" })).toBe(false);
  expect(scopeMatches(project, { kind: "console", console_id: "c-1" })).toBe(false);
});

it("matches no memory against a scope of a kind it does not know", () => {
  const unknown = { kind: "cluster" } as never;
  expect(scopeMatches(console_, unknown)).toBe(false);
  expect(scopeMatches(project, unknown)).toBe(false);
});

it("keys a project by its place, so two projects at one place share it", () => {
  expect(targetKey(project)).toBe(targetKey({ kind: "project", project: { ...PROJECT, id: "p-2" } }));
  expect(targetKey(project)).not.toBe(targetKey(console_));
});
