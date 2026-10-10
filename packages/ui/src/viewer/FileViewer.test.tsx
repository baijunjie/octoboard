// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { PREFERENCE_KEYS } from "../preferenceKeys";
import { ThemeProvider } from "../theme";
import type { ViewerChangeSide, ViewerContent, ViewerSubject } from "./content";
import { diffLayout } from "./diffLayout";
import { FileViewer, type ViewerNavigation } from "./FileViewer";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rendering library does not run under jsdom; a stand-in shows the text it is handed, which is
// what these tests look at, and reports it drawn as soon as it is mounted unless `drawing.held`.
// That the library itself escapes repository text is checked in WebKit.
const drawing = vi.hoisted(() => ({ held: false }));
vi.mock("./renderer", async () => {
  const { useEffect } = await import("react");
  const Stand = ({ text, onDrawn }: { text: string; onDrawn?: () => void }) => {
    useEffect(() => {
      if (!drawing.held) onDrawn?.();
    }, [text, onDrawn]);
    return <pre data-renderer="">{text}</pre>;
  };
  return {
    HighlightedFile: ({ text, onDrawn }: { text: string; onDrawn?: () => void }) => <Stand text={text} onDrawn={onDrawn} />,
    // A patch marked "unrenderable" stands for one the library rejects.
    RenderedDiff: ({ patch, layout, onDrawn }: { patch: string; layout: string; onDrawn?: () => void }) => {
      if (patch.includes("unrenderable")) throw new Error("The patch cannot be read");
      return (
        <div data-layout={layout}>
          <Stand text={patch} onDrawn={onDrawn} />
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
