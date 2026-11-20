// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { ContentMatch } from "../../../protocol";
import { ThemeProvider } from "../../../preferences/theme";
import { DaemonProvider, type Daemon } from "../../../store";
import { fakeTimers } from "../../../test/clock";
import { daemonError } from "../../../test/daemonError";
import { type } from "../../../test/dom";
import { SEARCH_DEBOUNCE_MS } from "../useFileSearch";
import { ContentSearchDialog } from "./ContentSearchDialog";
import { contentMatch, contentSearchDaemon, modalPage } from "./contentSearchTestSupport";
import { DEFAULT_CONTENT_SEARCH_OPTIONS, type ContentSearchOptions } from "./useContentSearch";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(daemon: Daemon, { directory, narrow = false }: { directory?: string; narrow?: boolean } = {}) {
  modalPage({ narrow });
  const onOpen = vi.fn<(match: ContentMatch) => void>();
  const onClose = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  function Host() {
    const [query, setQuery] = useState("");
    const [options, setOptions] = useState<ContentSearchOptions>(DEFAULT_CONTENT_SEARCH_OPTIONS);
    return <ContentSearchDialog project="p" projectName="demo" directory={directory} query={query} options={options} onQueryChange={setQuery} onOptionsChange={setOptions} onOpen={onOpen} onClose={onClose} />;
  }
  act(() => root.render(<ThemeProvider><DaemonProvider value={daemon}><Host /></DaemonProvider></ThemeProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { onOpen, onClose };
}

const query = () => document.querySelector<HTMLInputElement>('input[aria-label="Search file contents"]')!;
const status = () => document.querySelector<HTMLElement>("[data-content-search-status]")!;
const rows = () => [...document.querySelectorAll<HTMLElement>("[data-content-search-result]")];
const search = async (text: string) => {
  type(query(), text);
  await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
};
const key = (target: HTMLElement, k: string) => act(() => {
  target.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));
  target.dispatchEvent(new KeyboardEvent("keyup", { key: k, bubbles: true, cancelable: true }));
});

it("lists the streamed lines with their matches marked by the daemon's ranges, and counts matches and files", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon);
  expect(document.querySelector("[role=dialog]")?.getAttribute("aria-labelledby")).toBeTruthy();
  expect(query()).not.toBeNull();
  await search("foo");
  // Before the first result the list's place alone says the search is running: no count, no second spinner.
  expect(document.body.textContent).toContain("Searching file contents…");
  expect(status().textContent).toBe("");
  expect(status().querySelector("[data-content-search-spinner]")).toBeNull();
  const a = contentMatch("src/a.ts", 12, "  return foo(foo);", "foo");
  a.ranges.push({ start: 13, end: 16 });
  await act(async () => fake.matches([a, contentMatch("lib/b.ts", 3, "foo", "foo")]));
  expect(rows()).toHaveLength(2);
  expect(status().textContent).toBe("3 matches in 2 files");
  expect(document.body.textContent).not.toContain("Searching file contents…");
  // The count says the search goes on with a spinner of its own, until the search ends.
  expect(status().querySelector("[data-content-search-spinner]")).not.toBeNull();
  // The marks are the daemon's ranges, with the line's indentation left out.
  expect([...rows()[0].querySelectorAll("mark")].map((mark) => mark.textContent)).toEqual(["foo", "foo"]);
  expect(rows()[0].textContent).toBe("return foo(foo);a.ts:12");
  expect(rows()[0].getAttribute("aria-label")).toBe("return foo(foo);, src/a.ts, line 12");
  await act(async () => fake.end({ matches: 3, files: 2 }));
  expect(status().textContent).toBe("3 matches in 2 files");
  expect(status().querySelector("[data-content-search-spinner]")).toBeNull();
  expect(status().querySelector("[data-status-mark-id], [aria-haspopup]")).toBeNull();
});

const announced = () => [...document.querySelectorAll("[role=status]")].map((element) => element.textContent).join("");

it("says a search found nothing, and marks one that ended incomplete with why", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon);
  await search("nothing");
  await act(async () => fake.end({ matches: 0, files: 0 }));
  expect(document.body.textContent).toContain("No matches");
  expect(status().textContent).toBe("");
  await search("foo");
  await act(async () => fake.matches([contentMatch("a.ts", 1, "foo", "foo")]));
  await act(async () => fake.end({ matches: 1, files: 1, complete: false, stopped_by: "matches" }));
  const mark = status().querySelector<HTMLElement>('[aria-label="The search is incomplete"]');
  expect(mark).not.toBeNull();
});

it("says a search that ended incomplete with nothing found is incomplete, with its mark, and announces so", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon);
  await search("nothing");
  await act(async () => fake.end({ matches: 0, files: 0, complete: false, stopped_by: "time" }));
  expect(document.body.textContent).toContain("No matches");
  expect(document.body.textContent).toContain("Not every file was searched.");
  expect(status().querySelector('[aria-label="The search is incomplete"]')).not.toBeNull();
  await act(async () => { vi.runOnlyPendingTimers(); });
  expect(announced()).toContain("No matches. Not every file was searched.");
});

it("announces the count once a search ends, saying when it is incomplete", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon);
  await search("foo");
  await act(async () => fake.matches([contentMatch("a.ts", 1, "foo", "foo")]));
  await act(async () => fake.end({ matches: 1, files: 1, complete: false, stopped_by: "matches" }));
  await act(async () => { vi.runOnlyPendingTimers(); });
  expect(announced()).toContain("1 match in 1 file. The search is incomplete.");
});

it("shows the daemon's refusal of the query under the query field, as an alert, and a failed search with Try again", async () => {
  fakeTimers();
  // The regex crate's own explanation travels as `detail`, for diagnosis only.
  const detail = "regex parse error:\n    (\n    ^\nerror: unclosed group";
  let refusal: Error = daemonError("invalid_search_pattern", { cause: "unclosed_group", offset: "0", detail });
  const fake = contentSearchDaemon({ refuse: () => refusal });
  mount(fake.daemon);
  await search("(");
  const message = query().closest("[data-slot=search-field], .search-field")!.querySelector<HTMLElement>("[data-content-search-refusal]")!;
  expect(message).not.toBeNull();
  expect(message.closest("[role=alert]")).not.toBeNull();
  const error = message.closest<HTMLElement>("[id]")!;
  expect(query().getAttribute("aria-describedby")).toContain(error.id);
  // The count under the title stays empty: the refusal is the field's own.
  expect(status().textContent).toBe("");
  // Marked invalid as the file mask is for a refused mask.
  expect(query().getAttribute("aria-invalid")).toBe("true");
  // One sentence in the user's language, worded from why and where, with none of the crate's text.
  expect(message.textContent).toBe("The regular expression is not valid: the parenthesis at character 1 is never closed.");
  refusal = daemonError("unknown_project", { project: "p" });
  await search("((");
  expect(document.body.textContent).toContain("Could not search file contents");
  const retry = [...document.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "Try again");
  expect(retry).toBeDefined();
  await act(async () => retry!.click());
  expect(fake.searches).toHaveLength(3);
});

it("opens the selected result from the query's Enter, and moves into the list with the down arrow", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  const { onOpen } = mount(fake.daemon);
  await search("foo");
  const first = contentMatch("a.ts", 1, "foo", "foo");
  const second = contentMatch("b.ts", 2, "foo", "foo");
  await act(async () => fake.matches([first, second]));
  // The first result is selected until another is chosen.
  expect(rows()[0].getAttribute("aria-selected")).toBe("true");
  key(query(), "ArrowDown");
  expect(document.activeElement?.closest("[data-content-search-results]")).not.toBeNull();
  key(document.activeElement as HTMLElement, "ArrowDown");
  expect(rows()[1].getAttribute("aria-selected")).toBe("true");
  // A batch arriving keeps the choice.
  await act(async () => fake.matches([contentMatch("c.ts", 3, "foo", "foo")]));
  expect(rows()[1].getAttribute("aria-selected")).toBe("true");
  key(document.activeElement as HTMLElement, "Enter");
  expect(onOpen).toHaveBeenLastCalledWith(second);
  act(() => query().focus());
  key(query(), "Enter");
  expect(onOpen).toHaveBeenCalledTimes(2);
  expect(onOpen).toHaveBeenLastCalledWith(second);
});

it("shows the project's name as the scope when the search is not limited to a folder", () => {
  fakeTimers();
  mount(contentSearchDaemon().daemon);
  expect(document.querySelector("[data-content-search-scope]")?.textContent).toBe("In demo");
});

it("keeps the scope and the count in the dialog's header, not with the fields", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon);
  await search("foo");
  await act(async () => fake.matches([contentMatch("src/a.ts", 1, "foo", "foo")]));
  expect(document.querySelectorAll("[data-content-search-status]")).toHaveLength(1);
  expect(status().textContent).toBe("1 match in 1 file");
  // The header is what holds the heading that names the dialog.
  const heading = document.getElementById(document.querySelector("[role=dialog]")!.getAttribute("aria-labelledby")!)!;
  const header = heading.parentElement!;
  expect(header.contains(status())).toBe(true);
  expect(header.contains(document.querySelector("[data-content-search-scope]"))).toBe(true);
  expect(header.contains(query())).toBe(false);
});

it("shows the folder a search is limited to, after the project's name", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon, { directory: "src/lib" });
  expect(document.querySelector("[data-content-search-scope]")?.textContent).toBe("In demo / src/lib");
  await search("foo");
  expect(fake.latest()).toMatchObject({ directory: "src/lib" });
});

const toggle = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;

it("starts with the ignored files left out, and each toggle searches at once with its option", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon);
  expect(toggle("Include ignored files").getAttribute("aria-pressed")).toBe("false");
  expect(["Match case", "Whole word", "Use regular expression"].map((label) => toggle(label).getAttribute("aria-pressed"))).toEqual(["false", "false", "false"]);
  await search("foo");
  expect(fake.latest()).not.toHaveProperty("include_ignored");
  for (const [label, field] of [["Match case", "match_case"], ["Whole word", "whole_word"], ["Use regular expression", "regex"], ["Include ignored files", "include_ignored"]] as const) {
    const before = fake.searches.length;
    act(() => toggle(label).click());
    // No debounce is waited out for a toggle.
    await act(async () => { vi.advanceTimersByTime(0); });
    expect(fake.searches, label).toHaveLength(before + 1);
    expect(fake.latest(), label).toMatchObject({ [field]: true });
  }
  expect(fake.latest()).toMatchObject({ match_case: true, whole_word: true, regex: true, include_ignored: true });
});

it("searches with the file mask once typing settles", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon);
  await search("foo");
  type(document.querySelector<HTMLInputElement>('input[aria-label="File mask"]')!, "*.ts, !test/*");
  await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS - 1); });
  expect(fake.searches).toHaveLength(1);
  await act(async () => { vi.advanceTimersByTime(1); });
  expect(fake.searches).toHaveLength(2);
  expect(fake.latest()).toMatchObject({ file_mask: "*.ts, !test/*" });
});

it("shows the daemon's refusal of the file mask under the mask, tied to it", async () => {
  fakeTimers();
  const fake = contentSearchDaemon({ refuse: () => daemonError("invalid_file_mask", { glob: "src/[a", cause: "unclosed_class", detail: "unclosed class" }) });
  mount(fake.daemon);
  await search("foo");
  const refusals = [...document.querySelectorAll<HTMLElement>("[data-content-search-refusal]")];
  expect(refusals).toHaveLength(1);
  expect(refusals[0]!.textContent).toBe("The file mask src/[a is not valid: the bracket is never closed.");
  expect(refusals[0]!.closest("[role=alert]")).not.toBeNull();
  expect(status().textContent).toBe("");
  // The glob is set apart as a reference, not quoted.
  expect(refusals[0]!.querySelector("code")?.textContent).toBe("src/[a");
  const mask = document.querySelector<HTMLInputElement>('input[aria-label="File mask"]')!;
  expect(mask.getAttribute("aria-invalid")).toBe("true");
  const error = refusals[0]!.closest<HTMLElement>("[id]")!;
  expect(mask.getAttribute("aria-describedby")).toContain(error.id);
  expect(query().getAttribute("aria-describedby") ?? "").not.toContain(error.id);
  expect(query().getAttribute("aria-invalid")).toBeNull();
});

it("opens a result on a double click", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  const { onOpen } = mount(fake.daemon);
  await search("foo");
  const first = contentMatch("a.ts", 1, "foo", "foo");
  const second = contentMatch("b.ts", 2, "foo", "foo");
  await act(async () => fake.matches([first, second]));
  const press = (row: HTMLElement) => act(() => {
    row.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, pointerType: "mouse" }));
    row.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, pointerType: "mouse" }));
    row.click();
  });
  press(rows()[1]);
  expect(rows()[1].getAttribute("aria-selected")).toBe("true");
  expect(onOpen).not.toHaveBeenCalled();
  act(() => {
    rows()[1].dispatchEvent(new MouseEvent("dblclick", { bubbles: true, cancelable: true, detail: 2 }));
  });
  expect(onOpen).toHaveBeenCalledExactlyOnceWith(second);
});

it("counts one match in one file in the singular", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon);
  await search("foo");
  await act(async () => fake.matches([contentMatch("a.ts", 1, "foo", "foo")]));
  expect(status().textContent).toBe("1 match in 1 file");
});

it.each([
  ["time", "took too long"],
  ["matches", "most matches it lists"],
  ["bytes_read", "reading as much"],
  ["result_bytes", "most that can be sent"],
  [undefined, "could not be read"],
] as const)("tells what stopped an incomplete search (%s) in the mark's popover", async (stoppedBy, words) => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon);
  await search("foo");
  await act(async () => fake.matches([contentMatch("a.ts", 1, "foo", "foo")]));
  await act(async () => fake.end({ matches: 1, files: 1, complete: false, ...(stoppedBy && { stopped_by: stoppedBy }) }));
  const mark = status().querySelector<HTMLElement>('[aria-label="The search is incomplete"]')!;
  expect(mark).not.toBeNull();
  await act(async () => mark.click());
  expect(document.body.textContent).toContain(words);
});

it("names the file mask and shows its syntax, with the include-ignored toggle as tall as the field beside it", () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon);
  const mask = document.querySelector<HTMLInputElement>('input[aria-label="File mask"]')!;
  // The example is isolated in the sentence, which lays out in the interface's direction while the field is empty.
  expect(mask.placeholder).toBe("File mask, such as \u2068*.ts, !*.test.ts\u2069");
  expect(mask.getAttribute("dir")).toBeNull();
  type(mask, "*.ts");
  expect(mask.getAttribute("dir")).toBe("ltr");
  const description = document.getElementById(mask.getAttribute("aria-describedby")!.split(" ")[0]!);
  expect(description?.textContent).toBe("File name patterns separated by commas. A pattern starting with ! leaves out the files it matches. Left empty, every file is searched.");
  const toggle = document.querySelector<HTMLElement>('[role="dialog"] button[aria-label="Include ignored files"]')!;
  expect(toggle.classList).toContain("field-control");
  expect(toggle.classList).not.toContain("row-icon-control");
});

it("shows each result's folder after its name where the modal fills the screen and shows no preview", async () => {
  fakeTimers();
  const fake = contentSearchDaemon();
  mount(fake.daemon, { narrow: true });
  await search("foo");
  await act(async () => fake.matches([contentMatch("src/lib/a.ts", 3, "foo", "foo"), contentMatch("top.ts", 1, "foo", "foo")]));
  expect(document.querySelector("[data-content-search-preview]")).toBeNull();
  expect(rows()[0].textContent).toBe("fooa.ts:3src/lib");
  // A file in the project's root has no folder to show.
  expect(rows()[1].textContent).toBe("footop.ts:1");
});
