// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { errorText } from "../../../errors/errorText";
import { DaemonProvider, type Daemon } from "../../../store";
import { fakeTimers } from "../../../test/clock";
import { daemonError } from "../../../test/daemonError";
import { SEARCH_DEBOUNCE_MS } from "../useFileSearch";
import { contentMatch, contentSearchDaemon } from "./contentSearchTestSupport";
import { DEFAULT_CONTENT_SEARCH_OPTIONS, useContentSearch, type ContentSearch, type ContentSearchOptions } from "./useContentSearch";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

interface Props {
  query: string;
  options?: ContentSearchOptions;
  directory?: string;
}

function mount(daemon: Daemon) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const seen: { result: ContentSearch; retry: () => Promise<void> } = { result: { state: "idle" }, retry: async () => {} };
  function Search({ query, options = DEFAULT_CONTENT_SEARCH_OPTIONS, directory }: Props) {
    Object.assign(seen, useContentSearch({ project: "p", query, options, directory }));
    return null;
  }
  const render = (props: Props) => act(async () => root.render(<DaemonProvider value={daemon}><Search {...props} /></DaemonProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { seen, render };
}

const wait = (ms: number) => act(async () => { vi.advanceTimersByTime(ms); });

it("sends the query and its options once typing settles, and appends the streamed batches until the search ends", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  const { seen, render } = mount(fake.daemon);
  const options = { ...DEFAULT_CONTENT_SEARCH_OPTIONS, matchCase: true, fileMask: "*.ts" };
  await render({ query: "fo", options, directory: "src" });
  await render({ query: "foo", options, directory: "src" });
  await wait(SEARCH_DEBOUNCE_MS - 1);
  expect(fake.searches).toHaveLength(0);
  expect(seen.result).toMatchObject({ state: "searching", matches: [] });
  await wait(1);
  expect(fake.searches).toEqual([
    { type: "search_project_content", project: "p", search: expect.any(String), query: "foo", slot: "content-search", match_case: true, file_mask: "*.ts", directory: "src" },
  ]);
  const first = contentMatch("src/a.ts", 3, "const foo = foo;", "foo");
  first.ranges.push({ start: 12, end: 15 });
  await act(async () => fake.matches([first]));
  expect(seen.result).toEqual({ state: "searching", matches: [first], matchCount: 2, fileCount: 1, complete: true });
  const second = contentMatch("src/b.ts", 9, "foo()", "foo");
  await act(async () => fake.matches([second]));
  expect(seen.result).toMatchObject({ state: "searching", matches: [first, second], matchCount: 3, fileCount: 2 });
  await act(async () => fake.end({ matches: 3, files: 2, complete: false, stopped_by: "time" }));
  expect(seen.result).toEqual({ state: "done", matches: [first, second], matchCount: 3, fileCount: 2, complete: false, stoppedBy: "time" });
});

it("reads a bound a newer daemon names as an incomplete search with no bound named", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  const { seen, render } = mount(fake.daemon);
  await render({ query: "foo" });
  await wait(SEARCH_DEBOUNCE_MS);
  await act(async () => fake.end({ matches: 0, files: 0, complete: false, stopped_by: "a_newer_bound" }));
  expect(seen.result).toEqual({ state: "done", matches: [], matchCount: 0, fileCount: 0, complete: false });
});

it("takes no event of a search it has moved on from, and searches at once when a toggle changes", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  const { seen, render } = mount(fake.daemon);
  await render({ query: "foo" });
  await wait(SEARCH_DEBOUNCE_MS);
  const old = fake.latest().search;
  await render({ query: "foo", options: { ...DEFAULT_CONTENT_SEARCH_OPTIONS, regex: true } });
  // A toggle is not a keystroke: no debounce is waited out.
  await wait(0);
  expect(fake.searches).toHaveLength(2);
  expect(fake.latest()).toMatchObject({ regex: true });
  expect(fake.latest().search).not.toBe(old);
  const current = contentMatch("now.ts", 1, "foo", "foo");
  await act(async () => fake.matches([current]));
  await act(async () => fake.matches([contentMatch("stale.ts", 1, "foo", "foo")], old));
  await act(async () => fake.end({ matches: 9, files: 9 }, old));
  expect(seen.result).toMatchObject({ state: "searching", matches: [current] });
  await act(async () => fake.end({ matches: 1, files: 1 }));
  expect(seen.result).toMatchObject({ state: "done", matches: [current], matchCount: 1, complete: true });
});

it("searches nothing for an empty query", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  const { seen, render } = mount(fake.daemon);
  await render({ query: "" });
  await wait(SEARCH_DEBOUNCE_MS);
  expect(seen.result).toEqual({ state: "idle" });
  expect(fake.searches).toHaveLength(0);
});

it.each([
  ["query", daemonError("invalid_search_pattern", { cause: "unclosed_group", offset: "0", detail: "unclosed group" }), "The regular expression is not valid: the parenthesis at character 1 is never closed."],
  ["fileMask", daemonError("invalid_file_mask", { glob: "[", cause: "unclosed_class", detail: "unclosed class" }), "is not valid: the bracket is never closed."],
] as const)("tells the field the daemon refused (%s) from a failure to show, worded from why", async (field, refusal, worded) => {
  fakeTimers();
  const fake = contentSearchDaemon({ refuse: () => refusal });
  const { seen, render } = mount(fake.daemon);
  await render({ query: "(" });
  await wait(SEARCH_DEBOUNCE_MS);
  expect(seen.result).toMatchObject({ state: "invalid", field });
  expect(seen.result.state === "invalid" && errorText(seen.result.error)).toContain(worded);
  expect(seen.result.state === "invalid" && errorText(seen.result.error)).not.toContain(refusal.params.detail);
});

it("counts where a refused pattern fails in the characters of the query it was sent with, an emoji as one", async () => {
  fakeTimers();
  // The daemon's offset counts UTF-16 units: "😀" is two, so the "(" of "😀a(" is at offset 3, the third character. The
  // store words the refusal before the query is known, as the fourth.
  const refusal = daemonError("invalid_search_pattern", { cause: "unclosed_group", offset: "3", detail: "unclosed group" });
  expect(refusal.message).toContain("character 4");
  const fake = contentSearchDaemon({ refuse: () => refusal });
  const { seen, render } = mount(fake.daemon);
  await render({ query: "😀a(" });
  await wait(SEARCH_DEBOUNCE_MS);
  expect(seen.result.state === "invalid" && errorText(seen.result.error)).toBe("The regular expression is not valid: the parenthesis at character 3 is never closed.");
});

it("shows any other refusal as a failure, and searches again on retry and after a reconnect", async () => {
  fakeTimers();
  let refused = true;
  const fake = contentSearchDaemon({ refuse: () => (refused ? daemonError("unknown_project", { project: "p" }) : undefined) });
  const { seen, render } = mount(fake.daemon);
  await render({ query: "foo" });
  await wait(SEARCH_DEBOUNCE_MS);
  expect(seen.result).toMatchObject({ state: "error" });
  refused = false;
  await act(async () => seen.retry());
  expect(fake.searches).toHaveLength(2);
  expect(seen.result).toMatchObject({ state: "searching", matches: [] });
  // The daemon keeps nothing of a search across connections.
  await act(async () => fake.daemon.store.setState({ snapshotEpoch: 2 }));
  expect(fake.searches).toHaveLength(3);
  expect(fake.latest().search).not.toBe(fake.searches[1].search);
});

it("sends each switch the user turned on, leaves the ones off and a blank file mask out", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  const { render } = mount(fake.daemon);
  await render({ query: "foo", options: { matchCase: false, wholeWord: true, regex: true, fileMask: "  ", includeIgnored: true } });
  await wait(SEARCH_DEBOUNCE_MS);
  expect(fake.latest()).toEqual({ type: "search_project_content", project: "p", search: expect.any(String), query: "foo", slot: "content-search", whole_word: true, regex: true, include_ignored: true });
});

it("waits out the debounce for a typed file mask, as for the query, but not for the include-ignored toggle", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  const { render } = mount(fake.daemon);
  await render({ query: "foo" });
  await wait(SEARCH_DEBOUNCE_MS);
  expect(fake.searches).toHaveLength(1);
  await render({ query: "foo", options: { ...DEFAULT_CONTENT_SEARCH_OPTIONS, fileMask: "*.t" } });
  await wait(SEARCH_DEBOUNCE_MS - 1);
  expect(fake.searches).toHaveLength(1);
  await wait(1);
  expect(fake.searches).toHaveLength(2);
  expect(fake.latest()).toMatchObject({ file_mask: "*.t" });
  await render({ query: "foo", options: { ...DEFAULT_CONTENT_SEARCH_OPTIONS, fileMask: "*.t", includeIgnored: true } });
  await wait(0);
  expect(fake.searches).toHaveLength(3);
  expect(fake.latest()).toMatchObject({ include_ignored: true });
});

it("starts over from nothing found when the query changes, and takes no event of the search before", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  const { seen, render } = mount(fake.daemon);
  await render({ query: "foo" });
  await wait(SEARCH_DEBOUNCE_MS);
  const first = fake.latest().search;
  await act(async () => fake.matches([contentMatch("a.ts", 1, "foo", "foo")]));
  await render({ query: "foob" });
  await wait(SEARCH_DEBOUNCE_MS);
  expect(fake.latest().search).not.toBe(first);
  expect(seen.result).toMatchObject({ state: "searching", matches: [], matchCount: 0, fileCount: 0 });
  await act(async () => fake.matches([contentMatch("late.ts", 1, "foo", "foo")], first));
  expect(seen.result).toMatchObject({ matches: [] });
});

it("counts each match of a line, and each file once, as batches arrive", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  const { seen, render } = mount(fake.daemon);
  await render({ query: "foo" });
  await wait(SEARCH_DEBOUNCE_MS);
  const two = contentMatch("a.ts", 1, "foo foo", "foo");
  two.ranges.push({ start: 4, end: 7 });
  await act(async () => fake.matches([two, contentMatch("a.ts", 2, "foo", "foo")]));
  expect(seen.result).toMatchObject({ matchCount: 3, fileCount: 1 });
  await act(async () => fake.matches([contentMatch("b.ts", 1, "foo", "foo")]));
  expect(seen.result).toMatchObject({ matchCount: 4, fileCount: 2 });
});

it("reports a search that could not read some files as incomplete with no bound named", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  const { seen, render } = mount(fake.daemon);
  await render({ query: "foo" });
  await wait(SEARCH_DEBOUNCE_MS);
  await act(async () => fake.end({ matches: 0, files: 0, complete: false }));
  expect(seen.result).toEqual({ state: "done", matches: [], matchCount: 0, fileCount: 0, complete: false });
});
