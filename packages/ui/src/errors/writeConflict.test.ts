import { describe, expect, it } from "vitest";

import { CodedError } from "./codedError";
import { agentFieldPaths, agentPaths, baseVersions, reachedConflict, writeConflict } from "./writeConflict";

const conflict = (params: Record<string, string>) => new CodedError("write conflict", "write_conflict", params);

describe("baseVersions", () => {
  it("names the version read for each path the write changes, and no other", () => {
    expect(baseVersions({ name: 3, tags: 5, pinned: 1 }, ["name", "tags"])).toEqual({ name: 3, tags: 5 });
  });

  it("leaves a path with no version read out, so it is not checked", () => {
    expect(baseVersions({ name: 3 }, ["name", "description"])).toEqual({ name: 3 });
  });

  it("is undefined when no path is checked, which leaves the write unchecked", () => {
    expect(baseVersions({ name: 3 }, [])).toBeUndefined();
    expect(baseVersions({}, ["name"])).toBeUndefined();
    expect(baseVersions(undefined, ["name"])).toBeUndefined();
  });
});

describe("paths of a per-agent change", () => {
  it("is `<prefix>.<agent>` for each agent named", () => {
    expect(agentPaths("accounts", { claude: "a-1", codex: null })).toEqual(["accounts.claude", "accounts.codex"]);
    expect(agentPaths("accounts", undefined)).toEqual([]);
  });

  it("is `<prefix>.<agent>.<field>` for each field set under each agent", () => {
    expect(agentFieldPaths("launch", { claude: { model: "alpha" }, codex: { model: null, effort: "low" }, grok: undefined })).toEqual([
      "launch.claude.model",
      "launch.codex.model",
      "launch.codex.effort",
    ]);
    expect(agentFieldPaths("launch", undefined)).toEqual([]);
  });
});

describe("writeConflict", () => {
  it("reads the paths and the versions the daemon names", () => {
    expect(writeConflict(conflict({ fields: "name,git_workflow.delivery", versions: '{"name":7,"git_workflow.delivery":9}' }))).toEqual({
      fields: ["name", "git_workflow.delivery"],
      versions: { name: 7, "git_workflow.delivery": 9 },
    });
  });

  it("is undefined for any other failure", () => {
    expect(writeConflict(new CodedError("no", "field_required", { field: "name" }))).toBeUndefined();
    expect(writeConflict(new Error("write_conflict"))).toBeUndefined();
    expect(writeConflict("write_conflict")).toBeUndefined();
  });

  it("names no versions where the text does not read as an object of numbers", () => {
    expect(writeConflict(conflict({ fields: "name", versions: "not json" }))).toEqual({ fields: ["name"], versions: {} });
    expect(writeConflict(conflict({ fields: "name,tags", versions: '{"name":"7","tags":2}' }))).toEqual({ fields: ["name", "tags"], versions: { tags: 2 } });
    expect(writeConflict(conflict({ fields: "name", versions: "null" }))).toEqual({ fields: ["name"], versions: {} });
  });

  it("copes with params the daemon did not send", () => {
    expect(writeConflict(conflict({}))).toEqual({ fields: [], versions: {} });
  });
});

describe("reachedConflict", () => {
  const named = { fields: ["name", "tags"], versions: { name: 7, tags: 4 } };

  it("holds once every named path is at least at its named version", () => {
    expect(reachedConflict(named, { name: 7, tags: 4 })).toBe(true);
    expect(reachedConflict(named, { name: 9, tags: 4, pinned: 1 })).toBe(true);
  });

  it("does not hold while a path is behind or missing", () => {
    expect(reachedConflict(named, { name: 6, tags: 4 })).toBe(false);
    expect(reachedConflict(named, { name: 7 })).toBe(false);
    expect(reachedConflict(named, undefined)).toBe(false);
  });

  it("holds for a path the conflict named no version of", () => {
    expect(reachedConflict({ fields: ["name"], versions: {} }, {})).toBe(true);
  });
});
