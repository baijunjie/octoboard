// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { PREFERENCE_KEYS } from "../preferenceKeys";
import { ThemeProvider } from "../theme";
import type { ViewerChangeSide, ViewerContent, ViewerSubject } from "./content";
import { diffLayout } from "./diffLayout";
import { FileViewer, type ViewerNavigation } from "./FileViewer";
import { wordWrap } from "./wordWrap";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rendering library does not run under jsdom; a stand-in shows the text it is handed, which is
// what these tests look at, and reports it drawn as soon as it is mounted unless `drawing.held`.
// That the library itself escapes repository text is checked in WebKit.
const drawing = vi.hoisted(() => ({ held: false }));
vi.mock("./renderer", async () => {
  const { useEffect } = await import("react");
  const Stand = ({ text, wrap, onDrawn }: { text: string; wrap: boolean; onDrawn?: () => void }) => {
    useEffect(() => {
      if (!drawing.held) onDrawn?.();
    }, [text, onDrawn]);
    return (
      <pre data-renderer="" data-wrap={String(wrap)}>
        {text}
      </pre>
    );
  };
  return {
    HighlightedFile: ({ text, wrap, onDrawn }: { text: string; wrap: boolean; onDrawn?: () => void }) => (
      <Stand text={text} wrap={wrap} onDrawn={onDrawn} />
    ),
    // A patch marked "unrenderable" stands for one the library rejects.
    // The stand-in tells whether it was handed a way to expand, and each button reports the state the
    // real one would.
    RenderedDiff: ({
      patch,
      layout,
      wrap,
      onDrawn,
      loadBodies,
      onExpansion,
    }: {
      patch: string;
      layout: string;
      wrap: boolean;
      onDrawn?: () => void;
      loadBodies?: () => void;
      onExpansion?: (status: unknown) => void;
    }) => {
      if (patch.includes("unrenderable")) throw new Error("The patch cannot be read");
      return (
        <div data-layout={layout} data-expandable={String(loadBodies !== undefined)}>
          <Stand text={patch} wrap={wrap} onDrawn={onDrawn} />
          <button type="button" data-report="changed" onClick={() => onExpansion?.({ kind: "changed" })} />
          <button type="button" data-report="failed" onClick={() => onExpansion?.({ kind: "failed", message: "No luck." })} />
        </div>
      );
    },
    endRendererPool: () => {},
  };
});

// jsdom has neither, and the path line (FadeOverflow) and the theme need them.
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;
window.matchMedia ??= ((query: string) =>
  ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList);

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  // The layout choice outlives a viewer, so a test that makes one must not leave it for the next.
  diffLayout.set("unified");
  wordWrap.set(false);
  localStorage.clear();
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  vi.useRealTimers();
  container.remove();
});

function show(subject: ViewerSubject | undefined, navigation?: ViewerNavigation): void {
  act(() => {
    root.render(
      <ThemeProvider>
        <button type="button">Open</button>
        {subject && <FileViewer subject={subject} onClose={() => {}} navigation={navigation} />}
      </ThemeProvider>,
    );
  });
}

const dialog = () => document.querySelector<HTMLElement>("[role=dialog]");
const button = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;

const failed: ViewerSubject = { key: "a", path: "src/a.ts", content: { state: "error", message: "src/a.ts is gone." } };
const binary: ViewerSubject = { key: "b", path: "bin/b.wasm", content: { state: "file", body: { kind: "binary", size: 3 } } };

// The same dialog shows each subject in turn, and nothing of the previous subject survives the
// change: its error is not carried over to the next file's name.
it("replaces one subject's content with the next in the same dialog", () => {
  show(failed);
  const first = dialog();
  expect(first?.textContent).toContain("src/a.ts is gone.");
  show(binary);
  expect(dialog()).toBe(first);
  expect(dialog()?.textContent).not.toContain("is gone");
  expect(dialog()?.textContent).toContain("This file cannot be displayed.");
});

it("shows only the side inside the project, naming where the other one is", () => {
  show({
    key: "c",
    path: "src/moved.ts",
    content: {
      state: "change",
      change: {
        old: { state: "out_of_scope", repositoryPath: "lib/moved.ts" },
        new: { state: "present", path: "src/moved.ts", kind: "file", body: { kind: "binary", size: 3 } },
      },
    },
  });
  expect(dialog()?.textContent).toContain("The previous version is outside this project, at lib/moved.ts.");
  expect(dialog()?.textContent).toContain("This file cannot be displayed.");
  expect(dialog()?.textContent).not.toContain("No file on this side.");
});

// Next turning unavailable while it has focus — the last subject reached, or the list changing
// under the same subject — sends focus to Previous rather than out of the dialog.
it.each([
  ["reaching the last subject", binary],
  ["the list changing under the same subject", failed],
])("moves focus to Previous when Next turns unavailable by %s", (_, after) => {
  show(failed, { onPrevious: () => {}, onNext: () => {} });
  act(() => button("Next file").focus());
  show(after, { onPrevious: () => {} });
  expect(button("Next file").disabled).toBe(true);
  expect(document.activeElement).toBe(button("Previous file"));
});

it("gives focus back to what had it when the viewer closes", async () => {
  show(undefined);
  const opener = document.querySelector<HTMLButtonElement>("button")!;
  act(() => opener.focus());
  show(binary);
  expect(dialog()?.contains(document.activeElement)).toBe(true);
  show(undefined);
  await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
  expect(document.activeElement).toBe(opener);
});

// Repository markup is data: an SVG is drawn only through an image element, and one that cannot be
// decoded is shown as its source text, never put in the page as markup.
it("shows an SVG through an image, and as text when it cannot be decoded, never as markup", async () => {
  const svg = '<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"><script>alert(2)</script></svg>';
  show({
    key: "s",
    path: "logo.svg",
    content: { state: "file", body: { kind: "image", mediaType: "image/svg+xml", url: `data:image/svg+xml,${encodeURIComponent(svg)}`, size: svg.length, text: svg } },
  });
  const repositoryMarkup = () => document.querySelectorAll("script, svg[onload]").length;
  expect(repositoryMarkup()).toBe(0);
  const image = document.querySelectorAll<HTMLImageElement>('img[src^="data:image/svg+xml"]');
  expect(image.length).toBe(1);

  act(() => image[0].dispatchEvent(new Event("error")));
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  expect(dialog()?.textContent).toContain("This image cannot be displayed, so its text is shown instead.");
  expect(document.querySelector("img")).toBeNull();
  expect(document.querySelector("[data-renderer]")?.textContent).toBe(svg);
  expect(repositoryMarkup()).toBe(0);
});

it("announces a read failure once, with its title", () => {
  show(failed);
  const alert = document.querySelector("[role=alert]");
  expect(alert?.textContent).toBe("Could not open this file src/a.ts is gone.");
  const title = [...(dialog()?.querySelectorAll("*") ?? [])].find((element) => element.textContent === "Could not open this file");
  expect(title?.getAttribute("aria-hidden")).toBe("true");
});

it("moves to the previous and next subject with Left and Right, as its buttons do", () => {
  const onPrevious = vi.fn();
  const onNext = vi.fn();
  show(binary, { onPrevious, onNext });
  for (const key of ["ArrowLeft", "ArrowRight"]) {
    act(() => {
      dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    });
  }
  expect([onPrevious.mock.calls.length, onNext.mock.calls.length]).toEqual([1, 1]);
});

// The library draws nothing until its worker pool has started; the frame says it is loading until
// the renderer reports the code drawn, rather than standing blank.
it("shows it is loading until the renderer has drawn the code", async () => {
  const code: ViewerSubject = {
    key: "t",
    path: "src/t.ts",
    content: { state: "file", body: { kind: "text", text: "const a = 1;\n", size: 13 } },
  };
  const loading = () => [...(dialog()?.querySelectorAll("[role=region] *") ?? [])].find((e) => e.textContent === "Loading…") ?? null;
  drawing.held = true;
  try {
    show(code);
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
    expect(document.querySelector("[data-renderer]")).not.toBeNull();
    expect(loading()).not.toBeNull();
  } finally {
    drawing.held = false;
  }
  show({ ...code, key: "u", path: "src/u.ts" });
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  expect(loading()).toBeNull();
});

it.each([
  ["an added file, which has one side", "@@ -0,0 +1 @@\n+a\n", false],
  ["a modified file", "@@ -1 +1 @@\n-a\n+b\n", true],
])("offers the diff layout for %s", (_, patch, offered) => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  show({
    key: patch,
    path: "src/c.ts",
    content: { state: "change", change: { old: offered ? side : { state: "absent" }, new: side, patch } },
  });
  expect(dialog()?.querySelector('[aria-label="Diff layout"]') !== null).toBe(offered);
});

// The layout the viewer remembers is for diffs with two sides; one without is drawn unified, with
// no empty column and no choice to make.
it("draws a one-sided diff unified even when split was chosen for another", () => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  const change = (patch: string, old: ViewerChangeSide): ViewerSubject => ({
    key: patch,
    path: "src/c.ts",
    content: { state: "change", change: { old, new: side, patch } },
  });
  const layout = () => dialog()?.querySelector("[data-layout]")?.getAttribute("data-layout");
  show(change("@@ -1 +1 @@\n-a\n+b\n", side));
  act(() => {
    [...dialog()!.querySelectorAll("button")].find((b) => b.textContent === "Split")!.click();
  });
  expect(layout()).toBe("split");
  show(change("@@ -0,0 +1 @@\n+a\n", { state: "absent" }));
  expect(layout()).toBe("unified");
});

// The diff is handed a way to expand its collapsed lines only when the change has one, and what
// goes wrong with an expansion is said beside the code, without taking the patch away.
it("offers expansion only to a change that can be expanded, and says why one did not happen", () => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  const patch = "@@ -1 +1 @@\n-a\n+b\n";
  const loadBodies = async () => ({ old: "a\n", new: "b\n" });
  const change = (key: string, expandable: boolean, read = loadBodies): ViewerSubject => ({
    key,
    path: "src/c.ts",
    content: { state: "change", change: { old: side, new: side, patch, ...(expandable ? { loadBodies: read } : {}) } },
  });
  const expandable = () => dialog()?.querySelector("[data-expandable]")?.getAttribute("data-expandable");
  show(change("plain", false));
  expect(expandable()).toBe("false");
  show(change("expandable", true));
  expect(expandable()).toBe("true");
  const report = (which: string) => act(() => void dialog()!.querySelector<HTMLButtonElement>(`[data-report=${which}]`)!.click());
  report("changed");
  expect(dialog()?.textContent).toContain("This change has moved on since it was read, so its unmodified lines cannot be shown.");
  report("failed");
  expect(dialog()?.textContent).toContain("Could not read the unmodified lines. No luck.");
  expect(dialog()?.querySelector("[data-renderer]")?.textContent).toBe(patch);
  show(change("another", true));
  expect(dialog()?.textContent).not.toContain("unmodified lines");
  // The same change read again hands a new way to expand under the same key and patch, and the
  // failure of the earlier read is not said of it.
  report("failed");
  expect(dialog()?.textContent).toContain("Could not read the unmodified lines.");
  show(change("another", true, async () => ({ old: "a\n", new: "b\n" })));
  expect(dialog()?.textContent).not.toContain("unmodified lines");
});

it("shows the raw patch when the diff cannot be rendered", () => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  try {
    show({
      key: "g",
      path: "src/c.ts",
      content: { state: "change", change: { old: side, new: side, patch: "@@ -1,3 +1,3 @@ unrenderable\n a\n+b\n" } },
    });
    expect(dialog()?.textContent).toContain("Shown as a plain patch because the diff could not be rendered.");
    expect(dialog()?.querySelector("pre")?.textContent).toContain("unrenderable");
  } finally {
    error.mockRestore();
  }
});

it("says a lost connection in neutral text, not as a failure", () => {
  vi.useFakeTimers();
  show({ key: "d", path: "src/d.ts", content: { state: "disconnected", what: "file" } });
  act(() => void vi.advanceTimersByTime(1600));
  expect(document.querySelector("[role=alert]")).toBeNull();
  expect(dialog()?.textContent).not.toContain("Could not open this file");
  expect(dialog()?.querySelector("[role=status]:not([aria-hidden])")?.textContent).toBe(
    "The connection to the daemon was lost. This file is read again when the connection is back.",
  );
});

// A patch with no hunk (an empty file added, a mode change) gives the renderer no line to draw, so
// it would never report the code drawn and the frame would say it is loading for good.
it("says a change whose patch has no lines has no diff, rather than loading for good", () => {
  const patch = "diff --git a/e b/e\nnew file mode 100644\nindex 0000000..e69de29\n";
  show({
    key: "e",
    path: "e",
    content: { state: "change", change: { old: { state: "absent" }, new: { state: "present", path: "e", kind: "file" }, patch } },
  });
  expect(dialog()?.textContent).toContain("This change has no diff to show.");
  expect(dialog()?.querySelector("[data-renderer], [aria-label='Diff layout']")).toBeNull();
  expect(dialog()?.querySelector("[role=status]:not([aria-hidden])")?.textContent).toBe("");
});

// The same subject's content can be replaced under the viewer — read again, or the connection lost
// before anything was read — and the code region holding focus goes with it; focus stays in the
// dialog, where Escape and Tab work, instead of falling to `<body>`.
it("keeps focus in the dialog when the subject's content is replaced", () => {
  const code: ViewerSubject = { key: "k", path: "src/k.ts", content: { state: "file", body: { kind: "text", text: "a\n", size: 2 } } };
  show(code);
  act(() => dialog()!.querySelector<HTMLElement>("[role=region]")!.focus());
  show({ ...code, content: { state: "disconnected", what: "file" } });
  expect(dialog()?.contains(document.activeElement)).toBe(true);
});

// The wrap choice goes when the subject has no code left, a commit after the content changes.
it("keeps focus in the dialog when the wrap choice it was on goes", () => {
  show(modified());
  act(() => [...dialog()!.querySelectorAll("button")].find((b) => b.textContent === "Wrap lines")!.focus());
  show(binary);
  expect(dialog()?.contains(document.activeElement)).toBe(true);
});

// The title's accessible name: the one screen-reader-only string the drawn tags and name are hidden from.
const titleName = () => dialog()!.querySelector("h2 > .sr-only")!.textContent;

const modified = (path = "src/c.ts"): ViewerSubject => {
  const side: ViewerChangeSide = { state: "present", path, kind: "file" };
  return { key: `m:${path}`, path, stage: "Unstaged", content: { state: "change", change: { old: side, new: side, patch: "@@ -1 +1 @@\n-a\n+b\n" } } };
};

// The kind of change and the stage are tags before the name in the title, the kind coloured and the
// stage neutral; the description line has the path, where the change is from, and the layout choice at its end.
it("puts the status and the stage in the title and the layout choice on the description's row", () => {
  show(modified());
  const heading = dialog()!.querySelector("h2")!;
  expect(titleName()).toBe("Modified, Unstaged: c.ts");
  const tags = heading.querySelectorAll(".chip");
  expect([...tags].map((tag) => tag.textContent)).toEqual(["Modified", "Unstaged"]);
  const drawn = heading.querySelector('[aria-hidden="true"]')!;
  expect([...tags].every((tag) => drawn.contains(tag))).toBe(true);
  expect(drawn.textContent).toContain("c.ts");
  const row = dialog()!.querySelector<HTMLElement>("[aria-label='Diff layout']")!.closest<HTMLElement>("[data-viewer-description]")!;
  expect(row.textContent).toContain("src/c.ts");
  expect(row.textContent).not.toMatch(/Modified|staged/i);
});

// The tags are one group, closer to each other than the group is to the name, and each detail of the
// description line is an item of its own, so one too long for the space beside the view controls can
// move to a row of its own (layout is checked in WebKit); the paths carry the file icon.
it("keeps the tags in one group and each detail as an item of its own, the paths with an icon", () => {
  const side = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file", body: { kind: "binary", size: 3 } });
  show({
    ...modified(),
    source: "main at 1234567 to feature at fedcba0",
    content: { state: "change", change: { old: side("src/old.ts"), new: side("src/c.ts") } },
  });
  const tags = dialog()!.querySelectorAll(".chip");
  expect(tags[0].parentElement).toBe(tags[1].parentElement);
  expect(tags[0].parentElement!.children.length).toBe(2);
  const row = dialog()!.querySelector<HTMLElement>("[data-viewer-description]")!;
  // The view controls come first in the source, floated to the row's end; they are what the details flow around.
  const [controls, ...details] = [...row.children];
  expect(controls.className).toContain("float-end");
  expect(details.map((item) => item.textContent)).toEqual(["src/c.ts", "main at 1234567 to feature at fedcba0", "Renamed from src/old.ts"]);
  expect(details.filter((item) => item.querySelector("svg")).length).toBe(2);
});

// Before the change is read, a staged or unstaged one has only its stage to show, and the title's
// accessible name has no tag separator, since there is no status tag to separate it from.
it("names a title that has only a stage tag without a tag separator", () => {
  show({ ...modified(), content: { state: "disconnected", what: "change" } });
  expect(titleName()).toBe("Unstaged: c.ts");
});

it("shows no stage tag for an untracked, conflicted or compared change, and the branches for the last", () => {
  const row = () => dialog()!.querySelector<HTMLElement>("[data-viewer-description]")!;
  const body = { kind: "text", text: "x\n", size: 2 } as const;
  show({ ...modified(), stage: undefined, status: "untracked" });
  expect(titleName()).toBe("Untracked: c.ts");
  show({ key: "c", path: "src/m.ts", content: { state: "conflict", conflict: "Both sides modified this file.", body } });
  expect(titleName()).toBe("Conflicted: m.ts");
  expect(row().textContent).not.toMatch(/staged/i);
  show({ ...modified(), stage: undefined, source: "main at 1234567 to feature at fedcba0" });
  expect(titleName()).toBe("Modified: c.ts");
  expect(row().textContent).toContain("main at 1234567 to feature at fedcba0");
});

// An untracked file has no old side, which reads as added; the list says untracked, and so does the
// chip, with no note repeating that there is nothing to compare it with.
it("marks an untracked file Untracked, with no note that it is new content", () => {
  const body = { kind: "text", text: "x\n", size: 2 } as const;
  const content: ViewerContent = {
    state: "change",
    change: { old: { state: "absent" }, new: { state: "present", path: "src/n.ts", kind: "file", body } },
  };
  show({ key: "u", path: "src/n.ts", status: "untracked", content });
  expect(titleName()).toBe("Untracked: n.ts");
  expect(dialog()!.querySelector("h2 .chip")!.classList).toContain("chip-untracked");
  expect(dialog()!.textContent).not.toMatch(/new content/i);
  show({ key: "a", path: "src/n.ts", content });
  expect(titleName()).toBe("Added: n.ts");
  expect(dialog()!.querySelector("h2 .chip")!.classList).not.toContain("chip-untracked");
  // The status the list gave holds while the content is not read, and when reading it failed.
  show({ key: "l", path: "src/n.ts", status: "untracked", content: { state: "loading" } });
  expect(titleName()).toBe("Untracked: n.ts");
  const unavailable: ViewerContent = { state: "change", change: { old: { state: "absent" }, new: { state: "absent" }, unavailable: "Could not read it." } };
  show({ key: "f", path: "src/n.ts", status: "untracked", content: unavailable });
  expect(titleName()).toBe("Untracked: n.ts");
  show({ key: "d", path: "src/n.ts", status: "untracked", content: { state: "disconnected", what: "change" } });
  expect(titleName()).toBe("Untracked: n.ts");
});

it("has no status chip for a file, and none for a loading change whose caller gives no status", () => {
  show(binary);
  expect(titleName()).toBe("b.wasm");
  show({ ...modified(), stage: undefined, content: { state: "loading" } });
  expect(titleName()).toBe("c.ts");
});

// The choice is the user's preference: it holds for the next diff, and is stored for the next run.
it("opens later diffs in the layout last chosen, and stores it", () => {
  const layout = () => dialog()?.querySelector("[data-layout]")?.getAttribute("data-layout");
  show(modified("src/a.ts"));
  expect(layout()).toBe("unified");
  act(() => {
    [...dialog()!.querySelectorAll("button")].find((b) => b.textContent === "Split")!.click();
  });
  expect(localStorage.getItem(PREFERENCE_KEYS.diffLayout)).toBe("split");
  show(modified("src/b.ts"));
  expect(layout()).toBe("split");
});

it("reads the stored layout, and falls back to unified for anything else", async () => {
  const read = async (raw: string) => {
    vi.resetModules();
    localStorage.setItem(PREFERENCE_KEYS.diffLayout, raw);
    return (await import("./diffLayout")).diffLayout.get();
  };
  expect(await read("split")).toBe("split");
  expect(await read("sideways")).toBe("unified");
});

// A live region inserted already holding its text is not announced (WebKit, VoiceOver), so the
// loading and the lost connection are said from a status region that is mounted empty, says its
// first text after the dialog's own focus announcement, and stays while the content changes.
it("says loading and a lost connection from one status region that stays mounted", () => {
  vi.useFakeTimers();
  show({ key: "l", path: "src/l.ts", content: { state: "loading" } });
  const region = dialog()!.querySelector("[role=status]:not([aria-hidden])")!;
  expect(region.textContent).toBe("");
  expect(dialog()!.querySelectorAll("[role=status]:not([aria-hidden])")).toHaveLength(1);
  act(() => void vi.advanceTimersByTime(1400));
  expect(region.textContent).toBe("");
  act(() => void vi.advanceTimersByTime(200));
  expect(region.textContent).toBe("Loading…");

  show({ key: "d", path: "src/l.ts", content: { state: "disconnected", what: "file" } });
  expect(dialog()!.querySelector("[role=status]:not([aria-hidden])")).toBe(region);
  expect(region.textContent).toContain("The connection to the daemon was lost");

  show({ key: "f", path: "src/l.ts", content: { state: "file", body: { kind: "text", text: "a\n", size: 2 } } });
  expect(dialog()!.querySelector("[role=status]:not([aria-hidden])")).toBe(region);
  expect(region.textContent).toBe("");
});

it("says nothing for a loading that ends before the first text is written", () => {
  vi.useFakeTimers();
  show({ key: "q", path: "src/q.ts", content: { state: "loading" } });
  show({ key: "q", path: "src/q.ts", content: { state: "file", body: { kind: "text", text: "a\n", size: 2 } } });
  act(() => void vi.advanceTimersByTime(1600));
  expect(dialog()!.querySelector("[role=status]:not([aria-hidden])")!.textContent).toBe("");
});

// One wrap choice serves files and diffs: nothing is wrapped until the user asks, and what they chose
// holds for the next file of either kind and is stored for the next run.
it("wraps nothing by default, then wraps files and diffs alike once chosen, and stores it", () => {
  const wrapped = () => dialog()?.querySelector("[data-renderer]")?.getAttribute("data-wrap");
  const toggle = () => [...dialog()!.querySelectorAll("button")].find((b) => b.textContent === "Wrap lines")!;
  show({ key: "w", path: "src/w.ts", content: { state: "file", body: { kind: "text", text: "a\n", size: 2 } } });
  expect(wrapped()).toBe("false");
  expect(toggle().getAttribute("aria-pressed")).toBe("false");
  expect(toggle().closest("[data-viewer-description]")).not.toBeNull();
  act(() => toggle().click());
  expect(wrapped()).toBe("true");
  expect(toggle().getAttribute("aria-pressed")).toBe("true");
  expect(localStorage.getItem(PREFERENCE_KEYS.wordWrap)).toBe("true");
  show(modified("src/b.ts"));
  expect(wrapped()).toBe("true");
  expect(toggle().getAttribute("aria-pressed")).toBe("true");
});

// The two controls portal into places of their own, so the Tab order stays the order on screen
// whichever mounts first: stepping from a one-sided diff (the wrap choice only) to a two-sided one
// brings the layout choice in after it.
it("keeps the layout choice before the wrap choice when the layout choice mounts second", () => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  const change = (key: string, patch: string, old: ViewerChangeSide): ViewerSubject => ({
    key,
    path: "src/c.ts",
    content: { state: "change", change: { old, new: side, patch } },
  });
  const controls = () => [...dialog()!.querySelector("[data-viewer-description]")!.querySelectorAll("button")].map((b) => b.textContent);
  show(change("one", "@@ -0,0 +1 @@\n+a\n", { state: "absent" }));
  expect(controls()).toEqual(["Wrap lines"]);
  show(change("two", "@@ -1 +1 @@\n-a\n+b\n", side));
  expect(controls()).toEqual(["Unified", "Split", "Wrap lines"]);
});

// An image that cannot be decoded falls back to its text on whichever side it happens to, and the
// wrap choice must be there for the text that shows, once.
it.each([
  ["the old side only", [true, false], 1],
  ["the new side only", [false, true], 1],
  ["both sides", [true, true], 1],
  ["neither side", [false, false], 0],
])("offers one wrap choice for an image change whose text shows on %s", async (_, [oldFails, newFails], expected) => {
  const svg = (text: string) => ({
    kind: "image" as const,
    mediaType: "image/svg+xml",
    url: `data:image/svg+xml,${encodeURIComponent(text)}`,
    size: text.length,
    text,
  });
  const side = (text: string): ViewerChangeSide => ({ state: "present", path: "logo.svg", kind: "file", body: svg(text) });
  show({ key: "img", path: "logo.svg", content: { state: "change", change: { old: side("<svg id='old'/>"), new: side("<svg id='new'/>") } } });
  const images = [...document.querySelectorAll<HTMLImageElement>("img")];
  act(() => {
    if (oldFails) images[0].dispatchEvent(new Event("error"));
    if (newFails) images[1].dispatchEvent(new Event("error"));
  });
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  const toggles = [...dialog()!.querySelectorAll("button")].filter((b) => b.textContent === "Wrap lines");
  expect(toggles).toHaveLength(expected);
});

it("offers no wrap choice where there is no code", () => {
  const toggles = () => [...dialog()!.querySelectorAll("button")].filter((b) => b.textContent === "Wrap lines");
  show(binary);
  expect(toggles()).toHaveLength(0);
  show(failed);
  expect(toggles()).toHaveLength(0);
});

it("reads the stored wrap choice, and falls back to unwrapped for anything else", async () => {
  const read = async (raw: string) => {
    vi.resetModules();
    localStorage.setItem(PREFERENCE_KEYS.wordWrap, raw);
    return (await import("./wordWrap")).wordWrap.get();
  };
  expect(await read("true")).toBe(true);
  expect(await read("sideways")).toBe(false);
});
