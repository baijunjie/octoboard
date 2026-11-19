// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ClientRequestBody, ContentMatch } from "../protocol";
import { fakeTimers } from "../test/clock";
import { searchContent, type SearchedFile } from "./fixtureContentSearch";
import { createFixtureDaemon } from "./fixtureDaemon";
import { projectOf, snapshotState } from "./builders";
import type { FixtureError } from "./sampleFiles";

type SearchBody = Extract<ClientRequestBody, { type: "search_project_content" }>;

const FILES: SearchedFile[] = [
  ["src/b.ts", "const Index = 1;\nconst reindexed = index + INDEX;\n"],
  ["README.md", "The index.\n"],
  ["bin/blob.dat", "index\0index\n"],
  ["src/deep/c.md", "no hit here\nindex, indexes\n"],
];

const refused = (error: FixtureError): never => {
  throw Object.assign(new Error(error.message), error);
};

function search(body: Partial<SearchBody>, options: { incomplete?: boolean; stoppedBy?: "time" } = {}, files = FILES) {
  const events = searchContent(files, { type: "search_project_content", project: "p", search: "s", query: "index", ...body }, { incomplete: false, fail: refused, ...options });
  const lines = events.flatMap((event) => (event.type === "project_content_matches" ? event.matches : []));
  const ended = events.at(-1);
  if (ended?.type !== "project_content_search_ended") throw new Error("the search did not end");
  return { events, lines, ended };
}

const at = (lines: ContentMatch[]) => lines.map((line) => `${line.path}:${line.line}`);

describe("the fixture daemon's content search", () => {
  it("finds a literal query ignoring case, in path order, with each match's range in the line and the counts it ends with", () => {
    const { lines, ended } = search({});
    expect(at(lines)).toEqual(["README.md:1", "src/b.ts:1", "src/b.ts:2", "src/deep/c.md:2"]);
    expect(lines[2]!.ranges).toEqual([{ start: 8, end: 13 }, { start: 18, end: 23 }, { start: 26, end: 31 }]);
    expect(ended).toMatchObject({ matches: 7, files: 3, complete: true });
    expect(ended.stopped_by).toBeUndefined();
  });

  it("narrows a query by case and by whole word, and reads it as a pattern only when asked", () => {
    expect(at(search({ match_case: true }).lines)).toEqual(["README.md:1", "src/b.ts:2", "src/deep/c.md:2"]);
    expect(search({ whole_word: true }).ended.matches).toBe(5);
    expect(search({ query: "ind.x", regex: true }).ended.matches).toBe(7);
    expect(search({ query: "ind.x" }).ended.matches).toBe(0);
  });

  it("ends an empty query at once with nothing found", () => {
    expect(search({ query: "" }).events).toHaveLength(1);
  });

  it("limits the files by a mask: a glob without a slash by name, one with a slash by path, a leading ! leaving files out", () => {
    expect(at(search({ file_mask: "*.md" }).lines)).toEqual(["README.md:1", "src/deep/c.md:2"]);
    expect(at(search({ file_mask: "src/*.ts" }).lines)).toEqual(["src/b.ts:1", "src/b.ts:2"]);
    expect(at(search({ file_mask: "src/**/*.md" }).lines)).toEqual(["src/deep/c.md:2"]);
    expect(at(search({ file_mask: "!*.md" }).lines)).toEqual(["src/b.ts:1", "src/b.ts:2"]);
    expect(at(search({ file_mask: "*.md, *.ts, !README.md" }).lines)).toEqual(["src/b.ts:1", "src/b.ts:2", "src/deep/c.md:2"]);
  });

  it("refuses a pattern that does not compile with why and where, and the regex crate's explanation pointing at the fault", () => {
    expect(() => search({ query: "a(b", regex: true })).toThrow(expect.objectContaining({ code: "invalid_search_pattern", params: { cause: "unclosed_group", offset: "1", detail: "regex parse error:\n    a(b\n     ^\nerror: unclosed group" } }));
    expect(() => search({ query: "ab)", regex: true })).toThrow(expect.objectContaining({ code: "invalid_search_pattern", params: { cause: "unopened_group", offset: "2", detail: "regex parse error:\n    ab)\n      ^\nerror: unopened group" } }));
    expect(() => search({ query: "x[ab", regex: true })).toThrow(expect.objectContaining({ code: "invalid_search_pattern", params: expect.objectContaining({ cause: "unclosed_class", offset: "1" }) }));
    expect(() => search({ query: "检索(", regex: true })).toThrow(expect.objectContaining({ params: expect.objectContaining({ cause: "unclosed_group", offset: "2" }) }));
  });

  it("refuses a file mask with an unclosed class, naming the glob and why", () => {
    expect(() => search({ file_mask: "*.ts, a[" })).toThrow(expect.objectContaining({ code: "invalid_file_mask", params: { glob: "a[", cause: "unclosed_class", detail: "unclosed character class; missing ']'" } }));
  });

  it("splits a mask at every comma, so a choice in braces is not whole", () => {
    expect(() => search({ file_mask: "*.{md,ts}" })).toThrow(expect.objectContaining({ code: "invalid_file_mask", params: expect.objectContaining({ glob: "*.{md", cause: "unclosed_alternation" }) }));
  });

  describe("clipping a long line", () => {
    const long = `${"x".repeat(300)}index${"y".repeat(300)}`;
    it("keeps about 250 characters around the first match, saying where the text starts and whether its end is cut", () => {
      const [line] = search({}, {}, [["long.txt", long]]).lines;
      expect(line!.text).toHaveLength(250);
      expect(line).toMatchObject({ text_start: 300 - 83, clipped_end: true, ranges: [{ start: 300, end: 305 }] });
      expect(line!.text.slice(300 - line!.text_start, 305 - line!.text_start)).toBe("index");
    });

    it("sends a short line whole", () => {
      expect(search({}, {}, [["short.txt", "an index\n"]]).lines[0]).toMatchObject({ text: "an index", text_start: 0, clipped_end: false });
    });
  });

  it("ends incomplete, with the bound that stopped it where it has one", () => {
    expect(search({}, { incomplete: true }).ended).toMatchObject({ complete: false });
    expect(search({}, { incomplete: true }).ended.stopped_by).toBeUndefined();
    expect(search({}, { stoppedBy: "time" }).ended).toMatchObject({ complete: false, stopped_by: "time" });
  });

  it("sends the lines in batches of 500", () => {
    const text = Array.from({ length: 1200 }, () => "index").join("\n");
    const { events } = search({}, {}, [["many.txt", text]]);
    expect(events.map((event) => (event.type === "project_content_matches" ? event.matches.length : 0))).toEqual([500, 500, 200, 0]);
  });

  describe("faults it cannot tell as the daemon does", () => {
    afterEach(() => vi.restoreAllMocks());
    it("refuses a regex fault it does not read as the crate does as an unfillable fixture, not as a refusal the daemon never sends", () => {
      expect(() => search({ query: "*a", regex: true })).toThrow(/cannot tell/);
      expect(document.querySelector("[data-gallery-failure]")).not.toBeNull();
    });
  });
});

describe("the fixture daemon answering a content search", () => {
  const project = projectOf("p", "c", "Project");
  const daemon = (scenario: Parameters<typeof createFixtureDaemon>[0] = {}) => createFixtureDaemon({ state: snapshotState({ projects: [project] }), files: { "": { dir: true }, "a.txt": { text: "index\n" }, "ignored.txt": { text: "index\n", ignored: true }, sub: { dir: true }, "sub/b.txt": { text: "index\n" } }, ...scenario });
  const ask = (body: Partial<SearchBody>) => ({ type: "search_project_content", project: project.id, search: "s", query: "index", ...body }) as SearchBody;

  it("acknowledges the search, then streams its lines to the listener of that search and ends it", async () => {
    fakeTimers();
    const fixture = daemon();
    const events: string[] = [];
    fixture.onContentSearch("s", (event) => events.push(`${event.type}${event.type === "project_content_matches" ? `:${event.matches.map((m) => m.path)}` : `:${event.matches}/${event.files}`}`));
    expect(await fixture.request(ask({}))).toEqual({ type: "ack", id: null });
    expect(events).toEqual([]);
    await vi.runAllTimersAsync();
    expect(events).toEqual(["project_content_matches:a.txt,sub/b.txt", "project_content_search_ended:2/2"]);
  });

  it("searches the files Git ignores only when asked, and only under the directory it is limited to", async () => {
    fakeTimers();
    const fixture = daemon();
    const ended: number[] = [];
    fixture.onContentSearch("s", (event) => event.type === "project_content_search_ended" && ended.push(event.files));
    await fixture.request(ask({ include_ignored: true }));
    await fixture.request(ask({ directory: "sub" }));
    await vi.runAllTimersAsync();
    expect(ended).toEqual([3, 1]);
    await expect(fixture.request(ask({ directory: "a.txt" }))).rejects.toMatchObject({ code: "unsupported_file_type" });
    await expect(fixture.request(ask({ directory: "nope" }))).rejects.toMatchObject({ code: "file_not_found" });
  });

  it("sends nothing after the ack for a search the scenario keeps running, and refuses with the scenario's search error", async () => {
    fakeTimers();
    const fixture = daemon({ contentSearch: "searching" });
    const listener = vi.fn();
    fixture.onContentSearch("s", listener);
    await fixture.request(ask({}));
    await vi.runAllTimersAsync();
    expect(listener).not.toHaveBeenCalled();
    const failing = daemon({ searchError: { code: "source_unavailable", params: { path: "" }, message: "gone" } });
    await expect(failing.request(ask({}))).rejects.toMatchObject({ code: "source_unavailable" });
  });
});
