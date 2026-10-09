import { describe, expect, it } from "vitest";

import type { Project } from "./protocol";
import { effectiveTags, matchesFilter, tagVocabulary, withoutTags, withTag } from "./projectFiltering";

const project = (name: string, tags: string[]): Project => ({
  id: name,
  console_id: "c",
  host_id: "h",
  name,
  path: `/${name}`,
  source: "local",
  trust_consent: false,
  pinned: false,
  tags,
});

describe("tagVocabulary", () => {
  it.each([
    { projects: [], expected: [] },
    { projects: [project("a", ["web", "api"]), project("b", ["api", "ops"])], expected: ["api", "ops", "web"] },
    { projects: [project("a", ["Web"]), project("b", ["web"])], expected: ["Web"] },
  ])("lists the distinct tags in order: $expected", ({ projects, expected }) => {
    expect(tagVocabulary(projects)).toEqual(expected);
  });
});

describe("effectiveTags", () => {
  it.each([
    { selected: ["web", "gone"], vocabulary: ["api", "web"], expected: ["web"] },
    { selected: ["WEB"], vocabulary: ["web"], expected: ["web"] },
    { selected: [], vocabulary: ["web"], expected: [] },
  ])("keeps what the vocabulary still has: $expected", ({ selected, vocabulary, expected }) => {
    expect(effectiveTags(selected, vocabulary)).toEqual(expected);
  });
});

describe("matchesFilter", () => {
  const site = project("Website", ["web", "ui"]);
  it.each([
    { keyword: "", tags: [], expected: true },
    { keyword: " SITE ", tags: [], expected: true },
    { keyword: "api", tags: [], expected: false },
    { keyword: "", tags: ["web"], expected: true },
    { keyword: "", tags: ["web", "ui"], expected: true },
    { keyword: "", tags: ["web", "ops"], expected: false },
    { keyword: "site", tags: ["ops"], expected: false },
    { keyword: "site", tags: ["WEB"], expected: true },
  ])("keyword $keyword and tags $tags give $expected", ({ keyword, tags, expected }) => {
    expect(matchesFilter(site, keyword, tags)).toBe(expected);
  });
});

describe("withTag and withoutTags", () => {
  it("change one tag and keep a stored tag no project carries", () => {
    expect(withTag(["gone"], "web")).toEqual(["gone", "web"]);
    expect(withTag(["Web"], "web")).toEqual(["Web"]);
    expect(withoutTags(["gone", "Web", "api"], ["web"])).toEqual(["gone", "api"]);
  });
});
