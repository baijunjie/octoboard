// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { isolate } from "../i18n/isolate";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { ThemeProvider } from "../theme";
import { ChangeBodiesError, type ViewerChangeSide, type ViewerContent, type ViewerSubject } from "./content";
import { diffLayout } from "./diffLayout";
import { FileViewer, type ViewerNavigation } from "./FileViewer";
import { ViewerHost } from "./viewerTestSupport";
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

// jsdom has no `matchMedia`, which the theme needs.
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
  while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  vi.useRealTimers();
  container.remove();
});

function show(subject: ViewerSubject | undefined, navigation?: ViewerNavigation, host: Omit<React.ComponentProps<typeof ViewerHost>, "children"> = {}): void {
  act(() => {
    root.render(
      <ViewerHost {...host}>
        <ThemeProvider>
          <button type="button">Open</button>
          {subject && <FileViewer subject={subject} onClose={() => {}} navigation={navigation} />}
        </ThemeProvider>
      </ViewerHost>,
    );
  });
}

// The code surface draws through a lazily imported renderer. The import settles several microtask
// hops after the first render that needs it (the module load, then two `.then`s), so a bare `await`
// does not reach it; a timer does. Until it lands the tree shows a fallback, so a test that reads the
// code surface waits here, inside `act`, leaving React no retry to make on its own. It stays out of
// `show`, since tests under fake timers would never see a real timer fire.
const settled = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));

const dialog = () => document.querySelector<HTMLElement>("[role=dialog]");
// The view controls are icon buttons named for what pressing does, so a control is found by the name
// of either of its states.
const controlNamed = (...names: string[]) => [...dialog()!.querySelectorAll("button")].filter((b) => names.includes(b.getAttribute("aria-label") ?? ""));
const layoutToggles = () => controlNamed("Show split diff", "Show unified diff");
const wrapToggles = () => controlNamed("Wrap lines", "Don't wrap lines");
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
  expect(dialog()?.textContent).toContain("This file cannot be displayed");
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
  expect(dialog()?.textContent).toContain("This file cannot be displayed");
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
  await settled();
  expect(dialog()?.textContent).toContain("This image cannot be displayed, so its text is shown instead.");
  expect(document.querySelector("img")).toBeNull();
  expect(document.querySelector("[data-renderer]")?.textContent).toBe(svg);
  expect(repositoryMarkup()).toBe(0);
});

it("requires acknowledgement when a raster image cannot be decoded", () => {
  show({ key: "broken-image", path: "broken.png", content: { state: "file", body: { kind: "image", mediaType: "image/png", url: "data:image/png;base64,broken", size: 10 } } });
  act(() => document.querySelector("img")!.dispatchEvent(new Event("error")));
  expect(errorAlerts.getSnapshot()).toMatchObject([{ message: "This image cannot be displayed", classification: { kind: "actionable", recovery: "image" } }]);
});

it("queues a read failure with its title and keeps recovery content silent", () => {
  show(failed);
  expect(errorAlerts.getSnapshot()).toMatchObject([{ message: "src/a.ts is gone.", title: "Could not open this file" }]);
  expect(dialog()?.textContent).toContain("src/a.ts is gone.");
  expect(document.querySelectorAll("[role=alert]")).toHaveLength(0);
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
    await settled();
    expect(document.querySelector("[data-renderer]")).not.toBeNull();
    expect(loading()).not.toBeNull();
  } finally {
    drawing.held = false;
  }
  show({ ...code, key: "u", path: "src/u.ts" });
  await settled();
  expect(loading()).toBeNull();
});

// The code theme's background is the dialog's own white in light, so the surface around a frame is
// what bounds it there, and only a variant off the dialog's own (`default` equals it) does; every form of
// code has one.
it("sets each frame of code, a diff and plain text in HeroUI's tertiary surface", () => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  const surfaceOf = () =>
    [...(dialog()?.querySelectorAll<HTMLElement>("[role=region]") ?? [])].map((frame) => frame.parentElement?.classList.contains("surface--tertiary"));
  show({ key: "t", path: "src/t.ts", content: { state: "file", body: { kind: "text", text: "const a = 1;\n", size: 13 } } });
  expect(surfaceOf()).toEqual([true]);
  show({ key: "d", path: "src/c.ts", content: { state: "change", change: { old: side, new: side, patch: "@@ -1 +1 @@\n-a\n+b\n" } } });
  expect(surfaceOf()).toEqual([true]);
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  try {
    show({ key: "g", path: "src/c.ts", content: { state: "change", change: { old: side, new: side, patch: "@@ -1,3 +1,3 @@ unrenderable\n a\n+b\n" } } });
    expect(surfaceOf()).toEqual([true]);
  } finally {
    error.mockRestore();
  }
});

it.each([
  ["an added file, which has one side", "@@ -0,0 +1 @@\n+a\n", false],
  ["a modified file", "@@ -1 +1 @@\n-a\n+b\n", true],
])("offers the diff layout for %s", async (_, patch, offered) => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  show({
    key: patch,
    path: "src/c.ts",
    content: { state: "change", change: { old: offered ? side : { state: "absent" }, new: side, patch } },
  });
  await settled();
  expect(layoutToggles().length > 0).toBe(offered);
});

// git writes a type change as a removal and an addition; the viewer draws them as one diff, framed
// once and with the layout choice, like any change.
it("draws a type change as one diff with the layout choice", () => {
  const file: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  const link: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "symlink" };
  const patch =
    "diff --git a/src/c.ts b/src/c.ts\ndeleted file mode 100644\n@@ -1 +0,0 @@\n-a\ndiff --git a/src/c.ts b/src/c.ts\nnew file mode 120000\n@@ -0,0 +1 @@\n+b\n";
  show({ key: "tc", path: "src/c.ts", content: { state: "change", change: { old: file, new: link, patch } } });
  expect(dialog()?.querySelectorAll("[data-layout]")).toHaveLength(1);
  expect(dialog()?.querySelectorAll("[role=region]")).toHaveLength(1);
  expect(layoutToggles()).toHaveLength(1);
  expect(dialog()?.querySelector("[data-renderer]")?.textContent).toContain("-a\n+b\n");
  expect(dialog()?.textContent).not.toContain("Before");
});

// A change with no lines to draw still has content worth showing, which is read as the change
// opens, so a rename that changed nothing looks like a rename across the project's boundary.
it("shows the content of a change with no lines to draw", async () => {
  const side = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file" });
  const loadBodies = async () => ({ old: "kept\n", new: "kept\n" });
  const patch = "diff --git a/f b/g\nsimilarity index 100%\nrename from f\nrename to g\n";
  show({ key: "r", path: "g", content: { state: "change", change: { old: side("f"), new: side("g"), patch, loadBodies } } });
  await settled();
  expect(dialog()?.textContent).toContain("Renamed without changes to its content.");
  expect(dialog()?.textContent).toContain("kept");
});

// Two sides that compare equal through a clean filter can still differ on disk, so the content is
// shown only where the two bodies agree; otherwise the viewer says nothing it cannot stand behind.
it("shows no content for a change whose bodies differ from what its patch says", async () => {
  const side = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file" });
  const loadBodies = async () => ({ old: "cleaned\n", new: "raw\n" });
  show({ key: "c", path: "f", content: { state: "change", change: { old: side("f"), new: side("f"), patch: "", loadBodies } } });
  await settled();
  expect(dialog()?.textContent).toContain("No differences");
  expect(dialog()?.textContent).not.toContain("raw");
});

it("falls back to saying nothing changed when that content cannot be read", async () => {
  const side = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file" });
  const loadBodies = () => Promise.reject(new ChangeBodiesError("failed", "git failed"));
  show({ key: "e", path: "f", content: { state: "change", change: { old: side("f"), new: side("f"), patch: "", loadBodies } } });
  await settled();
  expect(dialog()?.textContent).toContain(`Could not read this file's content. ${isolate("git failed")}`);
  expect(dialog()?.textContent).toContain("No differences");
});

// A joined patch holds both sides whole, so nothing in it is collapsed: offering to expand it would
// draw a separator that opens onto nothing.
it("offers no expansion to a joined removal and addition", async () => {
  const file = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file" });
  const patch =
    "diff --git a/f b/f\ndeleted file mode 100644\n@@ -1 +0,0 @@\n-a\ndiff --git a/f/g b/f/g\nnew file mode 100644\n@@ -0,0 +1 @@\n+b\n";
  const loadBodies = async () => ({ old: "a\n", new: "b\n" });
  show({ key: "nested", path: "f/g", content: { state: "change", change: { old: file("f"), new: file("f/g"), patch, loadBodies } } });
  await settled();
  expect(dialog()?.querySelector("[data-expandable]")?.getAttribute("data-expandable")).toBe("false");
});

// A rename below its own path is one section, the diff between its two versions, like any edit.
it("offers expansion to a rename below its own path drawn from one section", async () => {
  const file = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file" });
  const patch = "diff --git a/f b/f/g\nindex 1111111..2222222 100644\n--- a/f\n+++ b/f/g\n@@ -1 +1 @@\n-a\n+b\n";
  const loadBodies = async () => ({ old: "a\n", new: "b\n" });
  show({ key: "nested-one", path: "f/g", content: { state: "change", change: { old: file("f"), new: file("f/g"), patch, loadBodies } } });
  await settled();
  expect(dialog()?.querySelector("[data-expandable]")?.getAttribute("data-expandable")).toBe("true");
  expect(layoutToggles()).toHaveLength(1);
});

// A rename whose two versions are equal is said to be exactly that, with a patch that is empty or
// has only the rename's header.
it.each([
  ["empty", ""],
  ["a rename header alone", "diff --git a/f b/g\nsimilarity index 100%\nrename from f\nrename to g\n"],
])("says a renamed change with unchanged content is just that (%s)", (_, patch) => {
  const file = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file" });
  show({ key: `renamed-${patch.length}`, path: "g", content: { state: "change", change: { old: file("f"), new: file("g"), patch } } });
  expect(dialog()?.textContent).toContain("Renamed without changes");
  expect(dialog()?.textContent).not.toContain("to its content");
  expect(dialog()?.textContent).not.toContain("No diff to show");
  expect(dialog()?.querySelector("[data-renderer]")).toBeNull();
});

// What a rename's patch says besides its header is not "unchanged": a binary line says the content
// differs, a mode change says the mode does, and neither is lost to the rename wording.
it.each([
  ["a binary line", "diff --git a/f b/g\nBinary files a/f and b/g differ\n"],
  ["a mode change", "diff --git a/f b/g\nold mode 100644\nnew mode 100755\n"],
])("does not call a rename with %s unchanged", (_, patch) => {
  const file = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file" });
  show({ key: `renamed-other-${patch.length}`, path: "g", content: { state: "change", change: { old: file("f"), new: file("g"), patch } } });
  expect(dialog()?.textContent).not.toContain("Renamed without changes");
  expect(dialog()?.textContent).toContain("No diff to show");
});

// The layout the viewer remembers is for diffs with two sides; one without is drawn unified, with
// no empty column and no choice to make.
it("draws a one-sided diff unified even when split was chosen for another", async () => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  const change = (patch: string, old: ViewerChangeSide): ViewerSubject => ({
    key: patch,
    path: "src/c.ts",
    content: { state: "change", change: { old, new: side, patch } },
  });
  const layout = () => dialog()?.querySelector("[data-layout]")?.getAttribute("data-layout");
  show(change("@@ -1 +1 @@\n-a\n+b\n", side));
  await settled();
  act(() => {
    controlNamed("Show split diff")[0].click();
  });
  expect(controlNamed("Show split diff")).toHaveLength(0);
  expect(controlNamed("Show unified diff")).toHaveLength(1);
  expect(layout()).toBe("split");
  show(change("@@ -0,0 +1 @@\n+a\n", { state: "absent" }));
  expect(layout()).toBe("unified");
});

// The diff is handed a way to expand its collapsed lines only when the change has one, and what
// goes wrong with an expansion is said beside the code, without taking the patch away.
it("offers expansion only to a change that can be expanded, and says why one did not happen", async () => {
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
  await settled();
  expect(expandable()).toBe("false");
  show(change("expandable", true));
  expect(expandable()).toBe("true");
  const report = (which: string) => act(() => void dialog()!.querySelector<HTMLButtonElement>(`[data-report=${which}]`)!.click());
  report("changed");
  expect(dialog()?.textContent).toContain("This change has moved on since it was read, so its unmodified lines cannot be shown.");
  report("failed");
  expect(dialog()?.textContent).toContain(`Could not read the unmodified lines. ${isolate("No luck.")}`);
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

it("shows the raw patch when the diff cannot be rendered", async () => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  try {
    show({
      key: "g",
      path: "src/c.ts",
      content: { state: "change", change: { old: side, new: side, patch: "@@ -1,3 +1,3 @@ unrenderable\n a\n+b\n" } },
    });
    await settled();
    expect(dialog()?.textContent).toContain("Shown as a plain patch because the diff could not be rendered.");
    expect(dialog()?.querySelector("pre")?.textContent).toContain("unrenderable");
  } finally {
    error.mockRestore();
  }
});

it("queues a rendering failure for acknowledgement without a duplicate status announcement", async () => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  try {
    show({
      key: "g",
      path: "src/c.ts",
      content: { state: "change", change: { old: side, new: side, patch: "@@ -1,3 +1,3 @@ unrenderable\n a\n+b\n" } },
    });
    await settled();
    const sentence = "Shown as a plain patch because the diff could not be rendered.";
    const block = Array.from(dialog()!.querySelectorAll("[aria-hidden=true]")).find((e) => e.textContent === sentence);
    expect(block).toBeTruthy();
    expect(Array.from(dialog()!.querySelectorAll("[role=status]")).map((e) => e.textContent)).not.toContain(sentence);
    expect(errorAlerts.getSnapshot()).toMatchObject([{ message: sentence, error: expect.objectContaining({ message: "The patch cannot be read" }), classification: { kind: "unexpected" } }]);
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
  // The stand-in is hidden from screen readers, so the status region says the whole state, not
  // only the heading the stand-in shows.
  expect(dialog()?.querySelector("div[role=status]:not([aria-hidden])")?.textContent).toBe(
    "The connection to the daemon was lost. This file is read again when the connection is back.",
  );
  expect(dialog()?.textContent).toContain("Daemon connection lost");
  // A stand-in left visible would be read a second time beside the region.
  expect(dialog()?.querySelector('[aria-hidden="true"] h2')?.textContent).toBe("Daemon connection lost");
  expect(dialog()?.textContent).toContain("This file is read again when the connection is back.");
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
  expect(dialog()?.textContent).toContain("No diff to show");
  expect(dialog()?.querySelector("[data-renderer]")).toBeNull();
  expect(layoutToggles()).toHaveLength(0);
  expect(dialog()?.querySelector("div[role=status]:not([aria-hidden])")?.textContent).toBe("");
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
  act(() => wrapToggles()[0].focus());
  show(binary);
  expect(dialog()?.contains(document.activeElement)).toBe(true);
});

// The title's accessible name: the one screen-reader-only string the drawn tags and name are hidden from.
// The subtitle is the row under the title, present only for a subject with a source.
const subtitle = () => dialog()!.querySelector<HTMLElement>("[data-viewer-subtitle]");
const titleName = () => dialog()!.querySelector("h2 > .sr-only")!.textContent;

const modified = (path = "src/c.ts"): ViewerSubject => {
  const side: ViewerChangeSide = { state: "present", path, kind: "file" };
  return { key: `m:${path}`, path, stage: "Unstaged", content: { state: "change", change: { old: side, new: side, patch: "@@ -1 +1 @@\n-a\n+b\n" } } };
};

// The kind of change and the stage are tags after the name in the title, the kind coloured and the
// stage neutral; the description line has the path and the layout choice at its end.
it("puts the status and the stage in the title and the layout choice on the description's row", () => {
  show(modified());
  const heading = dialog()!.querySelector("h2")!;
  expect(titleName()).toBe("c.ts: Modified, Unstaged");
  const tags = heading.querySelectorAll(".chip");
  expect([...tags].map((tag) => tag.textContent)).toEqual(["Modified", "Unstaged"]);
  const drawn = heading.querySelector('[aria-hidden="true"]')!;
  expect([...tags].every((tag) => drawn.contains(tag))).toBe(true);
  // The drawn name comes first, then the tags, as the accessible name reads.
  expect(drawn.textContent).toBe("c.tsModifiedUnstaged");
  const row = layoutToggles()[0].closest<HTMLElement>("[data-viewer-description]")!;
  expect(row.textContent).toContain("src/c.ts");
  expect(row.textContent).not.toMatch(/Modified|staged/i);
});

// The tags are one group, closer to each other than the group is to the name, and each detail of the
// description line is an item of its own, so one too long for the space beside the view controls can
// move to a row of its own (layout is checked in WebKit); the paths carry the file icon. Where a
// compared change is from is no detail but the subtitle under the title.
it("keeps the tags in one group and each detail as an item of its own, the paths with an icon", () => {
  const side = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file", body: { kind: "binary", size: 3 } });
  show({
    ...modified(),
    source: "main at 1234567 to feature at fedcba0",
    content: { state: "change", change: { old: side("src/old.ts"), new: side("src/c.ts") } },
  }, undefined, { clipboard: { writeText: async () => {} } });
  const tags = dialog()!.querySelectorAll(".chip");
  expect(tags[0].parentElement).toBe(tags[1].parentElement);
  expect(tags[0].parentElement!.children.length).toBe(2);
  const row = dialog()!.querySelector<HTMLElement>("[data-viewer-description]")!;
  // The view controls come first in the source, floated to the row's end; they are what the details flow around.
  const [controls, ...details] = [...row.children];
  expect(controls.className).toContain("float-end");
  expect(details.map((item) => item.textContent)).toEqual(["src/c.ts", "Renamed from src/old.ts"]);
  // The path and the rename origin carry the file icon.
  expect(details.filter((item) => item.querySelector("svg")).length).toBe(2);
});

// Before the change is read, a staged or unstaged one has only its stage to show, and the title's
// accessible name has no tag separator, since there is no status tag to separate it from.
it("names a title that has only a stage tag without a tag separator", () => {
  show({ ...modified(), content: { state: "disconnected", what: "change" } });
  expect(titleName()).toBe("c.ts: Unstaged");
});

it("shows no stage tag for an untracked, conflicted or compared change, and the branches for the last", () => {
  const row = () => dialog()!.querySelector<HTMLElement>("[data-viewer-description]")!;
  const body = { kind: "text", text: "x\n", size: 2 } as const;
  show({ ...modified(), stage: undefined, status: "untracked" });
  expect(titleName()).toBe("c.ts: Untracked");
  show({ key: "c", path: "src/m.ts", content: { state: "conflict", conflict: "Both sides modified this file.", body } });
  expect(titleName()).toBe("m.ts: Conflicted");
  expect(row().textContent).not.toMatch(/staged/i);
  show({ ...modified(), stage: undefined, source: "main at 1234567 to feature at fedcba0" });
  expect(titleName()).toBe("c.ts: Modified");
  expect(row().textContent).not.toContain("main at");
  expect(subtitle()!.textContent).toBe("main at 1234567 to feature at fedcba0");
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
  expect(titleName()).toBe("n.ts: Untracked");
  expect(dialog()!.querySelector("h2 .chip")!.classList).toContain("chip-untracked");
  expect(dialog()!.textContent).not.toMatch(/new content/i);
  show({ key: "a", path: "src/n.ts", content });
  expect(titleName()).toBe("n.ts: Added");
  expect(dialog()!.querySelector("h2 .chip")!.classList).not.toContain("chip-untracked");
  // The status the list gave holds while the content is not read, and when reading it failed.
  show({ key: "l", path: "src/n.ts", status: "untracked", content: { state: "loading" } });
  expect(titleName()).toBe("n.ts: Untracked");
  const unavailable: ViewerContent = { state: "change", change: { old: { state: "absent" }, new: { state: "absent" }, unavailable: "Could not read it." } };
  show({ key: "f", path: "src/n.ts", status: "untracked", content: unavailable });
  expect(titleName()).toBe("n.ts: Untracked");
  show({ key: "d", path: "src/n.ts", status: "untracked", content: { state: "disconnected", what: "change" } });
  expect(titleName()).toBe("n.ts: Untracked");
});

// Where a compared change is from is a subtitle directly under the title and above the description
// line, isolated per branch and commit, with the plain text as the tooltip; a subject without a
// source has no such row at all.
it("shows where a compared change is from as a subtitle in the header, under the title and before the description line", () => {
  const source = (
    <>
      <bdi>main</bdi> at <bdi>1234567</bdi> to <bdi>feature</bdi> at <bdi>fedcba0</bdi>
    </>
  );
  show({ ...modified(), stage: undefined, source, sourceText: "main at 1234567 to feature at fedcba0" });
  const row = subtitle()!;
  expect(row.textContent).toBe("main at 1234567 to feature at fedcba0");
  expect(row.querySelectorAll("bdi").length).toBe(4);
  const description = dialog()!.querySelector("[data-viewer-description]")!;
  const heading = dialog()!.querySelector("h2")!;
  expect(heading.nextElementSibling).toBe(row);
  expect(row.parentElement).toBe(heading.parentElement);
  expect(row.compareDocumentPosition(description) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  show(modified());
  expect(subtitle()).toBeNull();
});

it("draws the size as a chip after the path", () => {
  show(binary);
  const items = [...dialog()!.querySelector("[data-viewer-description]")!.children].slice(1);
  expect(items.map((item) => item.textContent)).toEqual(["bin/b.wasm", "3 bytes"]);
  expect(items[1].querySelector(".chip")!.textContent).toBe("3 bytes");
});

// The header offers Copy path, as a row does, and copies the path the path item shows.
// Its button is the first control of the group at the row's end.
it("copies the project-relative path from the view-control group's first button, and says so on the button", async () => {
  const writeText = vi.fn(async () => {});
  show(modified("src/dir/c.ts"), undefined, { clipboard: { writeText } });
  const copy = button("Copy path");
  const [group, pathItem] = [...dialog()!.querySelector("[data-viewer-description]")!.children];
  expect(group.className).toContain("float-end");
  expect(group.querySelector("button")).toBe(copy);
  expect(pathItem.querySelector("button")).toBeNull();
  expect(controlNamed("Copy path").length).toBe(1);
  await act(async () => copy.click());
  expect(writeText).toHaveBeenCalledWith("src/dir/c.ts");
  expect(copy.getAttribute("aria-label")).toBe("Path copied");
});

it("says when the path could not be copied", async () => {
  const toasts = { notice: vi.fn(), error: vi.fn() };
  show(modified(), undefined, { clipboard: { writeText: async () => Promise.reject(new Error("no")) }, toasts });
  await act(async () => button("Copy path").click());
  expect(toasts.error).toHaveBeenCalledWith("Could not copy the path");
  expect(toasts.notice).not.toHaveBeenCalled();
});

it("keeps the copy button where the platform has no clipboard, copying through the selection", async () => {
  const copied: string[] = [];
  const execCommand = vi.fn(() => copied.push(document.getSelection()!.toString()) > 0);
  Object.assign(document, { execCommand });
  onTestFinished(() => { Reflect.deleteProperty(document, "execCommand"); });
  show(modified());
  await act(async () => button("Copy path").click());
  expect(execCommand).toHaveBeenCalledWith("copy");
  expect(copied).toEqual(["src/c.ts"]);
  expect(button("Path copied")).not.toBeNull();
});

it("has no status chip for a file, and none for a loading change whose caller gives no status", () => {
  show(binary);
  expect(titleName()).toBe("b.wasm");
  show({ ...modified(), stage: undefined, content: { state: "loading" } });
  expect(titleName()).toBe("c.ts");
});

// The choice is the user's preference: it holds for the next diff, and is stored for the next run.
it("opens later diffs in the layout last chosen, and stores it", async () => {
  const layout = () => dialog()?.querySelector("[data-layout]")?.getAttribute("data-layout");
  show(modified("src/a.ts"));
  await settled();
  expect(layout()).toBe("unified");
  act(() => {
    controlNamed("Show split diff")[0].click();
  });
  expect(controlNamed("Show split diff")).toHaveLength(0);
  expect(controlNamed("Show unified diff")).toHaveLength(1);
  expect(localStorage.getItem(PREFERENCE_KEYS.diffLayout)).toBe("split");
  show(modified("src/b.ts"));
  expect(layout()).toBe("split");
});

it("reports a user-triggered external Markdown link failure", async () => {
  const error = vi.fn();
  const cause = new Error("Browser launch failed");
  const links = { open: vi.fn(async () => { throw cause; }) };
  show({ key: "document-link", path: "README.md", content: { state: "file", body: { kind: "text", text: "[Documentation](https://example.com/docs)", size: 48 } } }, undefined, { links, toasts: { error, notice: vi.fn() } });
  for (let i = 0; i < 100 && !document.querySelector('a[href="https://example.com/docs"]'); i += 1) {
    await act(async () => new Promise((resolve) => setTimeout(resolve, 10)));
  }
  await act(async () => document.querySelector<HTMLAnchorElement>('a[href="https://example.com/docs"]')!.click());
  expect(links.open).toHaveBeenCalledExactlyOnceWith("https://example.com/docs");
  expect(error).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual(expect.arrayContaining([expect.objectContaining({ message: "Could not open the external link.", error: expect.objectContaining({ cause }), classification: { kind: "actionable", recovery: "openExternal" } })]));
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
  const region = dialog()!.querySelector("div[role=status]:not([aria-hidden])")!;
  expect(region.textContent).toBe("");
  expect(dialog()!.querySelectorAll("div[role=status]:not([aria-hidden])")).toHaveLength(1);
  act(() => void vi.advanceTimersByTime(1400));
  expect(region.textContent).toBe("");
  act(() => void vi.advanceTimersByTime(200));
  expect(region.textContent).toBe("Loading…");

  show({ key: "d", path: "src/l.ts", content: { state: "disconnected", what: "file" } });
  expect(dialog()!.querySelector("div[role=status]:not([aria-hidden])")).toBe(region);
  expect(region.textContent).toContain("The connection to the daemon was lost.");

  show({ key: "f", path: "src/l.ts", content: { state: "file", body: { kind: "text", text: "a\n", size: 2 } } });
  expect(dialog()!.querySelector("div[role=status]:not([aria-hidden])")).toBe(region);
  expect(region.textContent).toBe("");
});

it("says nothing for a loading that ends before the first text is written", () => {
  vi.useFakeTimers();
  show({ key: "q", path: "src/q.ts", content: { state: "loading" } });
  show({ key: "q", path: "src/q.ts", content: { state: "file", body: { kind: "text", text: "a\n", size: 2 } } });
  act(() => void vi.advanceTimersByTime(1600));
  expect(dialog()!.querySelector("div[role=status]:not([aria-hidden])")!.textContent).toBe("");
});

// One wrap choice serves files and diffs: nothing is wrapped until the user asks, and what they chose
// holds for the next file of either kind and is stored for the next run.
it("wraps nothing by default, then wraps files and diffs alike once chosen, and stores it", async () => {
  const wrapped = () => dialog()?.querySelector("[data-renderer]")?.getAttribute("data-wrap");
  const toggle = () => wrapToggles()[0];
  show({ key: "w", path: "src/w.ts", content: { state: "file", body: { kind: "text", text: "a\n", size: 2 } } });
  await settled();
  expect(wrapped()).toBe("false");
  expect(toggle().getAttribute("aria-pressed")).toBe("false");
  expect(toggle().closest("[data-viewer-description]")).not.toBeNull();
  act(() => toggle().click());
  expect(wrapped()).toBe("true");
  expect(toggle().getAttribute("aria-pressed")).toBe("true");
  expect(toggle().getAttribute("aria-label")).toBe("Don't wrap lines");
  expect(localStorage.getItem(PREFERENCE_KEYS.wordWrap)).toBe("true");
  show(modified("src/b.ts"));
  await settled();
  expect(wrapped()).toBe("true");
  expect(toggle().getAttribute("aria-pressed")).toBe("true");
});

// The two controls portal into places of their own, so the Tab order stays the order on screen
// whichever mounts first: stepping from a one-sided diff (the wrap choice only) to a two-sided one
// brings the layout choice in after it.
it.each([
  ["with a clipboard", { clipboard: { writeText: async () => {} } }, ["Copy path"]],
  ["without a clipboard", undefined, ["Copy path"]],
])("keeps Copy path, then the layout choice, then the wrap choice, when the layout choice mounts second, %s", (_, host, leading) => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  const change = (key: string, patch: string, old: ViewerChangeSide): ViewerSubject => ({
    key,
    path: "src/c.ts",
    content: { state: "change", change: { old, new: side, patch } },
  });
  const controls = () => [...dialog()!.querySelector("[data-viewer-description]")!.querySelectorAll("button")].map((b) => b.getAttribute("aria-label"));
  show(change("one", "@@ -0,0 +1 @@\n+a\n", { state: "absent" }), undefined, host);
  expect(controls()).toEqual([...leading, "Wrap lines"]);
  show(change("two", "@@ -1 +1 @@\n-a\n+b\n", side), undefined, host);
  expect(controls()).toEqual([...leading, "Show split diff", "Wrap lines"]);
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
  await settled();
  expect(wrapToggles()).toHaveLength(expected);
});

it("offers no wrap choice where there is no code", () => {
  show(binary);
  expect(wrapToggles()).toHaveLength(0);
  show(failed);
  expect(wrapToggles()).toHaveLength(0);
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
