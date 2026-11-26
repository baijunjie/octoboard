// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { ClientRequestBody, Event } from "../../../protocol";
import type { LineRange } from "../../../viewer/code/lineMarks";
import { fakeTimers } from "../../../test/clock";
import { daemonError } from "../../../test/daemonError";
import { type } from "../../../test/dom";
import { ThemeProvider } from "../../../preferences/theme";
import { PlatformProvider } from "../../../platform/react";
import { BROWSER_HOST } from "../../../test/hosts";
import { DaemonProvider } from "../../../store";
import { SEARCH_DEBOUNCE_MS } from "../useFileSearch";
import { QuoteTargetContext, type QuoteTarget } from "../../../components/host/quoteTarget";
import { chooseItem, rightClick } from "../../../viewer/viewerTestSupport";
import { ContentSearchDialog } from "./ContentSearchDialog";
import { PREVIEW_DELAY_MS } from "./ContentSearchPreview";
import { contentMatch, contentSearchDaemon, modalPage } from "./contentSearchTestSupport";
import { DEFAULT_CONTENT_SEARCH_OPTIONS, type ContentSearchOptions } from "./useContentSearch";

// The code surface's drawing has its own suite; here it records what it is asked to show, with the real text menu
// over its text, as the real surface has.
const drawn = vi.hoisted(() => [] as { name: string; text: string; line?: number; marks?: readonly LineRange[] }[]);
vi.mock("../../../viewer/code/CodeSurface", async () => {
  const { useRef } = await import("react");
  const { ViewerEditMenu } = await import("../../../viewer/ViewerEditMenu");
  const { FIND_REGION } = await import("../../../viewer/find/findDom");
  return {
    CodeSurface: (props: { name: string; text: string; line?: number; marks?: readonly LineRange[] }) => {
      drawn.push(props);
      const region = useRef<HTMLPreElement>(null);
      return (
        <>
          <pre ref={region} tabIndex={0} {...FIND_REGION} data-code-surface="">{props.text}</pre>
          <ViewerEditMenu region={region} selectAll={() => true} />
        </>
      );
    },
  };
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const file = (path: string, text: string): Event => ({
  type: "project_file",
  id: null,
  project: "p",
  worktree: null,
  path,
  source: { kind: "live", root_id: "r", version: "v" },
  file: { size: text.length, kind: "text", text, data: null, media_type: null },
});

async function mount({ narrow = false, read, quoteTarget, onClose = () => {} }: {
  narrow?: boolean;
  read?: (path: string) => Promise<Event>;
  quoteTarget?: QuoteTarget;
  onClose?: () => void;
} = {}) {
  modalPage({ narrow });
  drawn.length = 0;
  const reads: string[] = [];
  const fake = contentSearchDaemon({
    request: async (body: ClientRequestBody) => {
      if (body.type !== "read_project_file") throw new Error(`unexpected ${body.type}`);
      reads.push(body.path);
      return read ? read(body.path) : file(body.path, `text of ${body.path}`);
    },
  });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  function Host() {
    const [query, setQuery] = useState("");
    const [options, setOptions] = useState<ContentSearchOptions>(DEFAULT_CONTENT_SEARCH_OPTIONS);
    return <ContentSearchDialog project="p" projectName="demo" query={query} options={options} onQueryChange={setQuery} onOptionsChange={setOptions} onOpen={() => {}} onClose={onClose} />;
  }
  act(() => root.render(<ThemeProvider><QuoteTargetContext.Provider value={quoteTarget}><PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}><DaemonProvider value={fake.daemon}><Host /></DaemonProvider></PlatformProvider></QuoteTargetContext.Provider></ThemeProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  type(document.querySelector<HTMLInputElement>('input[aria-label="Search file contents"]')!, "foo");
  await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
  return { fake, reads };
}

const preview = () => document.querySelector<HTMLElement>("[data-content-search-preview]");
const rows = () => [...document.querySelectorAll<HTMLElement>("[data-content-search-result]")];
const select = (row: HTMLElement) => act(async () => {
  row.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, pointerType: "mouse" }));
  row.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, pointerType: "mouse" }));
  row.click();
});
const lastDrawn = () => drawn.at(-1);

it("previews the selected result's file under the list, headed by its name and folder, at its line with its matches marked", async () => {
  fakeTimers();
  const { fake, reads } = await mount();
  const match = contentMatch("src/lib/a.md", 4, "## foo", "foo");
  await act(async () => fake.matches([match]));
  expect(reads).toEqual(["src/lib/a.md"]);
  expect(preview()?.querySelector("h3")?.textContent).toBe("a.mdsrc/lib");
  // Shown as its source, even a Markdown file: the line is a line of the source.
  expect(preview()?.querySelector("[data-code-surface]")?.textContent).toBe("text of src/lib/a.md");
  expect(lastDrawn()).toMatchObject({ name: "a.md", line: 4 });
  expect(lastDrawn()?.marks).toBe(match.ranges);
  // The list comes first, the preview after it.
  const list = document.querySelector("[data-content-search-results]")!;
  expect(list.compareDocumentPosition(preview()!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("reads only the file a quick move through the results stops on, and moves within a file without reading it again", async () => {
  fakeTimers();
  const { fake, reads } = await mount();
  const a1 = contentMatch("a.ts", 1, "foo", "foo");
  const a9 = contentMatch("a.ts", 9, "x foo", "foo");
  const b = contentMatch("b.ts", 2, "foo", "foo");
  const c = contentMatch("c.ts", 3, "foo", "foo");
  await act(async () => fake.matches([a1, a9, b, c]));
  expect(reads).toEqual(["a.ts"]);
  await select(rows()[1]);
  // The same file: shown at the new line at once, with its own marks and no new read.
  expect(lastDrawn()).toMatchObject({ name: "a.ts", line: 9 });
  expect(lastDrawn()?.marks).toBe(a9.ranges);
  expect(reads).toEqual(["a.ts"]);
  await select(rows()[2]);
  await act(async () => { vi.advanceTimersByTime(PREVIEW_DELAY_MS - 1); });
  await select(rows()[3]);
  // Until the selection rests, the file shown before stays, at its own line and with its own marks.
  expect(lastDrawn()).toMatchObject({ name: "a.ts", line: 9 });
  expect(lastDrawn()?.marks).toBe(a9.ranges);
  expect(preview()?.querySelector("h3")?.textContent).toBe("a.ts");
  await act(async () => { vi.advanceTimersByTime(PREVIEW_DELAY_MS); });
  expect(reads).toEqual(["a.ts", "c.ts"]);
  expect(lastDrawn()).toMatchObject({ name: "c.ts", line: 3 });
});

it("shows no preview in a narrow window", async () => {
  fakeTimers();
  const { fake, reads } = await mount({ narrow: true });
  await act(async () => fake.matches([contentMatch("a.ts", 1, "foo", "foo")]));
  expect(rows()).toHaveLength(1);
  expect(preview()).toBeNull();
  expect(reads).toEqual([]);
});

it("heads a file in the project's root by its name alone", async () => {
  fakeTimers();
  const { fake } = await mount();
  await act(async () => fake.matches([contentMatch("top.ts", 1, "foo", "foo")]));
  expect(preview()?.querySelector("h3")?.children).toHaveLength(1);
  expect(preview()?.querySelector("h3")?.textContent).toBe("top.ts");
});

it("says why a file could not be read in the preview, and the list stays usable", async () => {
  fakeTimers();
  const { fake } = await mount({ read: () => Promise.reject(daemonError("file_not_found", { path: "gone.ts" })) });
  await act(async () => fake.matches([contentMatch("gone.ts", 1, "foo", "foo")]));
  expect(preview()?.textContent).toContain("Could not open this file");
  expect(preview()?.querySelector("[data-code-surface]")).toBeNull();
  expect(rows()).toHaveLength(1);
});

it("never draws one file's body at another file's line while a slow read finishes, then shows the new file at its line with its marks", async () => {
  fakeTimers();
  let release: ((reply: Event) => void) | undefined;
  const { fake } = await mount({
    read: (path) => (path === "slow.ts" ? new Promise<Event>((resolve) => { release = resolve; }) : Promise.resolve(file(path, `text of ${path}`))),
  });
  const slow = contentMatch("slow.ts", 7, "foo", "foo");
  await act(async () => fake.matches([contentMatch("a.ts", 1, "foo", "foo"), slow]));
  await select(rows()[1]);
  await act(async () => { vi.advanceTimersByTime(PREVIEW_DELAY_MS); });
  expect(drawn.some((props) => props.name === "a.ts" && props.line === 7)).toBe(false);
  expect(drawn.some((props) => props.name === "slow.ts")).toBe(false);
  await act(async () => release!(file("slow.ts", "text of slow.ts")));
  expect(lastDrawn()).toMatchObject({ name: "slow.ts", line: 7 });
  expect(lastDrawn()?.marks).toBe(slow.ranges);
  expect(preview()?.querySelector("h3")?.textContent).toBe("slow.ts");
});

it("quotes a selection of the preview into the session and hands the dialog's close to the target to run with the uncover", async () => {
  fakeTimers();
  const calls: string[] = [];
  const onClose = vi.fn();
  const quoteTarget: QuoteTarget = {
    blocked: () => undefined,
    quote: (text) => calls.push(`quote ${text}`),
    project: () => undefined,
    contextBlocked: () => false,
    addContext: () => {},
    revealInput: (close) => { calls.push("revealInput"); close(); },
  };
  const { fake } = await mount({ quoteTarget, onClose });
  await act(async () => fake.matches([contentMatch("src/a.ts", 1, "foo", "foo")]));
  const text = preview()!.querySelector<HTMLElement>("[data-code-surface]")!;
  document.getSelection()!.selectAllChildren(text);
  await rightClick(text);
  await chooseItem("Quote");
  expect(calls).toEqual(["quote text of src/a.ts", "revealInput"]);
  expect(onClose).toHaveBeenCalledOnce();
});
