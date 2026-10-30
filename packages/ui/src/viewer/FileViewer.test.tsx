// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS, READ_TIME_MS } from "../components/StatusAnnouncer";
import { errorAlerts } from "../errorAlerts";
import { DaemonRequestError } from "../daemon-client";
import { isolate } from "../i18n/isolate";
import { t } from "../i18n/language";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { ThemeProvider } from "../theme";
import { ChangeBodiesError, type ViewerChangeSide, type ViewerContent, type ViewerSubject } from "./content";
import { diffLayout } from "./diffLayout";
import { FileViewer, type ViewerNavigation } from "./FileViewer";
import { detailsButton } from "../components/detailsTestSupport";
import { enter, leave, openTooltip, press, tap } from "../components/tooltipTestSupport";
import { markNames, ViewerHost } from "./viewerTestSupport";
import { rawFailureText } from "../errorText";
import { viewerErrorText } from "./viewerError";
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
    HighlightedFile: ({ text, wrap, line, onDrawn }: { text: string; wrap: boolean; line?: number; onDrawn?: () => void }) => (
      <div data-line-asked={line}>
        <Stand text={text} wrap={wrap} onDrawn={onDrawn} />
      </div>
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
      loadBodies?: () => Promise<unknown>;
      onExpansion?: (status: unknown) => void;
    }) => {
      if (patch.includes("unrenderable")) throw new Error("The patch cannot be read");
      return (
        <div data-layout={layout} data-expandable={String(loadBodies !== undefined)}>
          <Stand text={patch} wrap={wrap} onDrawn={onDrawn} />
          <button type="button" data-report="changed" onClick={() => onExpansion?.({ kind: "changed" })} />
          <button type="button" data-report="failed" onClick={() => onExpansion?.({ kind: "failed", message: "No luck." })} />
          <button type="button" data-report="read-failed" onClick={() => void loadBodies?.().catch((error: Error) => onExpansion?.({ kind: "failed", message: error.message }))} />
        </div>
      );
    },
    endRendererPool: () => {},
  };
});

// The library's markup is not there to scroll in; what is asked of it is what these tests look at.
const scrolls = vi.hoisted(() => ({ toLine: vi.fn((_frame: HTMLElement, _line: number) => true) }));
vi.mock("./rendererDom", async (importOriginal) => ({ ...(await importOriginal<typeof import("./rendererDom")>()), scrollToLine: scrolls.toLine }));

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

function show(subject: ViewerSubject | undefined, navigation?: ViewerNavigation, host: Omit<React.ComponentProps<typeof ViewerHost>, "children"> = {}, line?: number): void {
  act(() => {
    root.render(
      <ViewerHost {...host}>
        <ThemeProvider>
          <button type="button">Open</button>
          {subject && <FileViewer subject={subject} onClose={() => {}} navigation={navigation} line={line} />}
        </ThemeProvider>
      </ViewerHost>,
    );
  });
}

it.each(["error", "conflict", "change"] as const)("does not repeat the header path in a %s failure", (state) => {
  const path = "src/gone.ts";
  const error = new DaemonRequestError("src/gone.ts does not exist any more.", "file_not_found", { path });
  const content: ViewerContent = state === "error"
    ? { state, message: error }
    : state === "conflict"
      ? { state, conflict: "Merge conflict", message: error }
      : { state, change: { old: { state: "present", path, kind: "file" }, new: { state: "absent" }, unavailable: error } };
  show({ key: state, path, content });
  const dialog = document.querySelector("[role=dialog]")!;
  expect(dialog.textContent).toContain("This file does not exist any more.");
  if (state === "conflict") {
    // The failure alone: the conflict's notice speaks of markers shown below, and there are none.
    expect(markNames(dialog)).toEqual([]);
    expect(dialog.querySelector('[data-slot="alert-root"]')).toBeNull();
  }
  expect(dialog.textContent?.match(/src\/gone\.ts/g)).toHaveLength(1);
  // A screen state: shown on the screen and nowhere else.
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

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

const NO_RECORDS = { consoles: new Map(), projects: new Map(), sessions: new Map() };
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
  expect(markNames(dialog()!)).toEqual([`The previous version is outside this project, at ${isolate("lib/moved.ts")}. Only the version in this project is shown.`]);
  expect(dialog()?.textContent).toContain("This file cannot be displayed");
  expect(dialog()?.textContent).not.toContain("No file on this side.");
  tap(dialog()!.querySelector("[data-status-mark]")!, "Enter");
  const popover = document.querySelector("[data-status-mark-popover] [role=dialog]");
  expect(popover?.getAttribute("aria-label")).toBe("Warning");
  expect(popover?.querySelector("[data-inline-reference] code")?.textContent).toBe("lib/moved.ts");
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
  expect(markNames(dialog()!)).toEqual(["This image cannot be displayed, so its text is shown instead."]);
  expect(document.querySelector("img")).toBeNull();
  expect(document.querySelector("[data-renderer]")?.textContent).toBe(svg);
  expect(repositoryMarkup()).toBe(0);
});

it("shows an empty-state failure that announces itself when a raster image cannot be decoded", () => {
  show({ key: "broken-image", path: "broken.png", content: { state: "file", body: { kind: "image", mediaType: "image/png", url: "data:image/png;base64,broken", size: 10 } } });
  act(() => document.querySelector("img")!.dispatchEvent(new Event("error")));
  expect(errorAlerts.getSnapshot()).toEqual([]);
  // The image was the whole subject, so its place on screen is the empty-state failure, not an
  // Alert; it fails long after the dialog was drawn, so it speaks, and its heading alone says what failed.
  expect(document.querySelector("img")).toBeNull();
  const panel = document.querySelector('[data-slot="empty-state"]');
  expect(document.querySelector('[data-slot="alert-root"]')).toBeNull();
  // The line under the heading says what is wrong and the remedy follows it, with no raw text and so no Details button.
  const [summary, remedy] = panel!.querySelectorAll("p");
  expect(summary?.textContent).toBe("The image file may be damaged, too large or in a format the viewer does not support.");
  expect(remedy?.textContent).toBe("Repair or replace the file, or choose another image, then open it again.");
  expect(detailsButton(panel!)).toBeUndefined();
  expect(panel?.querySelector("[role=alert]")?.textContent).toContain("This image cannot be displayed");
});

it("shows a read failure the viewer opened with in the empty-state shape, without an error alert dialog or an announcement", () => {
  show(failed);
  expect(errorAlerts.getSnapshot()).toEqual([]);
  expect(dialog()?.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(dialog()?.querySelector('[data-slot="empty-state"] h2')?.textContent).toBe("Could not open this file");
  expect(dialog()?.textContent).toContain("src/a.ts is gone.");
  expect(document.querySelectorAll("[role=alert]")).toHaveLength(0);
});

it("announces a read failure that arrives as the viewer opens after the dialog's focus move, title and reason only", () => {
  vi.useFakeTimers();
  const loading: ViewerSubject = { ...failed, content: { state: "loading" } };
  show(loading);
  expect(dialog()?.querySelector('[data-slot="empty-state"]')).toBeNull();
  show(failed);
  expect(dialog()?.querySelector('[data-slot="empty-state"]')).not.toBeNull();
  expect(document.querySelectorAll("[role=alert]")).toHaveLength(0);
  act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  const announcement = document.querySelectorAll("[role=alert]");
  expect(announcement).toHaveLength(1);
  expect(announcement[0].querySelector("h2")?.textContent).toBe("Could not open this file");
  expect(announcement[0].textContent).toContain("src/a.ts is gone.");
  expect(announcement[0].querySelector("button")).toBeNull();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("announces at once a read failure that arrives after moving to another file", () => {
  vi.useFakeTimers();
  show(binary);
  show({ ...failed, content: { state: "loading" } });
  show(failed);
  const announcement = document.querySelectorAll("[role=alert]");
  expect(announcement).toHaveLength(1);
  expect(announcement[0].textContent).toContain("src/a.ts is gone.");
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
  expect(markNames(dialog()!)).toEqual(["Renamed without changes to its content."]);
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
  expect(markNames(dialog()!)).toEqual([`Could not show this file's content. ${isolate("git failed")}`]);
  expect(dialog()?.textContent).toContain("No differences");
});

it.each([
  { case: "an internal failure", failure: () => {
    const error = new DaemonRequestError("database is locked", "internal_error", { detail: "database is locked" });
    // As `useChangeReader` raises it: a failure the application has no words for carries no wording.
    return new ChangeBodiesError("failed", undefined, rawFailureText(error));
  } },
  { case: "a failure that is no read refusal", failure: () => new TypeError("database is locked") },
])("says $case of the unchanged content as the generic sentence, its own text only in the mark's popover", async ({ failure }) => {
  const side = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file" });
  const loadBodies = () => Promise.reject(failure());
  show({ key: "internal", path: "f", content: { state: "change", change: { old: side("f"), new: side("f"), patch: "", loadBodies } } });
  await settled();
  expect(markNames(dialog()!)).toEqual(["Could not show this file's content."]);
  tap(dialog()!.querySelector("[data-status-mark]")!, "Enter");
  const popover = document.querySelector("[data-status-mark-popover] [role=dialog]");
  expect(popover?.getAttribute("aria-label")).toBe("Warning");
  expect(popover?.textContent).toContain("Could not show this file's content.");
  expect(popover?.textContent).not.toContain("Something unexpected");
  expect(popover?.querySelector("pre")?.textContent).toBe("database is locked");
});

it.each([false, true])("does not repeat the path when unchanged content fails to load, renamed %s", async (renamed) => {
  const path = "src/gone.ts";
  const side = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file" });
  const message = viewerErrorText(t, "en", new DaemonRequestError("gone", "file_not_found", { path }), path, NO_RECORDS);
  const loadBodies = () => Promise.reject(new ChangeBodiesError("failed", message));
  show({ key: "missing-content", path, content: { state: "change", change: { old: side(renamed ? "src/old.ts" : path), new: side(path), patch: "", loadBodies } } });
  await settled();
  expect(markNames(dialog()!)).toEqual([`Could not show this file's content. ${isolate(message)}`]);
  expect(dialog()?.textContent?.match(/src\/gone\.ts/g)).toHaveLength(1);
});

it("sets a path in the cause of a failed read of the unchanged content as a chip in the mark's popover", async () => {
  const side: ViewerChangeSide = { state: "present", path: "f", kind: "file" };
  const failure = { code: "file_not_found", params: { path: "src/other.ts" } };
  const message = viewerErrorText(t, "en", new DaemonRequestError("gone", failure.code, failure.params), "f", NO_RECORDS);
  const loadBodies = () => Promise.reject(new ChangeBodiesError("failed", message, undefined, failure));
  show({ key: "chip", path: "f", content: { state: "change", change: { old: side, new: side, patch: "", loadBodies } } });
  await settled();
  expect(markNames(dialog()!)).toEqual([`Could not show this file's content. ${isolate(message)}`]);
  tap(dialog()!.querySelector("[data-status-mark]")!, "Enter");
  const popover = document.querySelector("[data-status-mark-popover] [role=dialog]");
  expect(popover?.querySelector("[data-inline-reference]")?.textContent).toBe("src/other.ts");
});

it("does not repeat the path in an expansion failure", async () => {
  const path = "src/gone.ts";
  const side: ViewerChangeSide = { state: "present", path, kind: "file" };
  const message = viewerErrorText(t, "en", new DaemonRequestError("gone", "file_not_found", { path }), path, NO_RECORDS);
  const loadBodies = () => Promise.reject(new ChangeBodiesError("failed", message));
  show({ key: "missing-lines", path, content: { state: "change", change: { old: side, new: side, patch: "@@ -1 +1 @@\n-a\n+b\n", loadBodies } } });
  await settled();
  await act(async () => dialog()!.querySelector<HTMLButtonElement>("[data-report=read-failed]")!.click());
  expect(markNames(dialog()!)).toEqual([`Could not read the unmodified lines. ${isolate(message)}`]);
  expect(dialog()?.textContent?.match(/src\/gone\.ts/g)).toHaveLength(1);
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
  expect(markNames(dialog()!)).toEqual(["This change has moved on since it was read, so its unmodified lines cannot be shown."]);
  report("failed");
  expect(markNames(dialog()!)).toEqual([`Could not read the unmodified lines. ${isolate("No luck.")}`]);
  expect(dialog()?.querySelector("[data-renderer]")?.textContent).toBe(patch);
  show(change("another", true));
  expect(markNames(dialog()!)).toEqual([]);
  // The same change read again hands a new way to expand under the same key and patch, and the
  // failure of the earlier read is not said of it.
  report("failed");
  expect(markNames(dialog()!)).toHaveLength(1);
  show(change("another", true, async () => ({ old: "a\n", new: "b\n" })));
  expect(markNames(dialog()!)).toEqual([]);
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
    expect(markNames(dialog()!)).toEqual(["Shown as a plain patch because the diff could not be rendered."]);
    expect(dialog()?.querySelector("pre")?.textContent).toContain("unrenderable");
  } finally {
    error.mockRestore();
  }
});

it("says a rendering failure once, as a status, with a mark that holds the same sentence and no error alert dialog", async () => {
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
    expect(markNames(dialog()!)).toEqual([sentence]);
    expect(dialog()!.querySelector('[data-slot="alert-root"]')).toBeNull();
    expect(errorAlerts.getSnapshot()).toEqual([]);
  } finally {
    error.mockRestore();
  }
});

const statuses = () => [...dialog()!.querySelectorAll("[role=status]")].map((status) => status.textContent);

it("announces a conflict's read failure once, as an alert on its pane, with the delay of any read failure", () => {
  vi.useFakeTimers();
  const conflict = (content: Partial<Extract<ViewerContent, { state: "conflict" }>>): ViewerSubject => ({
    key: "k",
    path: "src/gone.ts",
    content: { state: "conflict", conflict: "Merge conflict", ...content },
  });
  const error = new DaemonRequestError("gone", "file_not_found", { path: "src/gone.ts" });
  show(conflict({}));
  show(conflict({ message: error }));
  expect(document.querySelectorAll("[role=alert]")).toHaveLength(0);
  act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  // The pane has the failure's placeholder and the header no mark; the pane speaks it.
  expect(dialog()!.querySelector('[data-slot="empty-state"] h2')?.textContent).toBe("Could not open this file");
  expect(markNames(dialog()!)).toEqual([]);
  expect(statuses()).not.toContain("Could not open this file");
  expect(document.querySelectorAll("[role=alert]")).toHaveLength(1);
});

it("speaks a notice that appears after the first paint once, and empties its region once it has been read", async () => {
  vi.useFakeTimers();
  const svg = "<svg xmlns='http://www.w3.org/2000/svg'/>";
  const image: ViewerSubject = { key: "s", path: "logo.svg", content: { state: "file", body: { kind: "image", mediaType: "image/svg+xml", url: "data:image/svg+xml,x", size: 3, text: svg } } };
  show(image);
  act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  act(() => void document.querySelector("img")!.dispatchEvent(new Event("error")));
  expect(markNames(dialog()!)).toEqual(["This image cannot be displayed, so its text is shown instead."]);
  const sentence = "This image cannot be displayed, so its text is shown instead.";
  act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  expect(statuses().filter((text) => text === sentence)).toHaveLength(1);
  act(() => void vi.advanceTimersByTime(READ_TIME_MS));
  expect(statuses()).not.toContain(sentence);
});

it("empties the unchanged note's region once it has been read", async () => {
  vi.useFakeTimers();
  const side = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file" });
  const loadBodies = async () => ({ old: "kept\n", new: "kept\n" });
  const patch = "diff --git a/f b/g\nsimilarity index 100%\nrename from f\nrename to g\n";
  show({ key: "r", path: "g", content: { state: "change", change: { old: side("f"), new: side("g"), patch, loadBodies } } });
  await act(async () => void (await vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS)));
  const sentence = "Renamed without changes to its content.";
  expect(statuses()).toContain(sentence);
  act(() => void vi.advanceTimersByTime(READ_TIME_MS));
  expect(statuses()).not.toContain(sentence);
});

it("puts each side's image notice beside that side's heading, and a single image's after the path", async () => {
  const svg = "<svg xmlns='http://www.w3.org/2000/svg'/>";
  const side = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file", body: { kind: "image", mediaType: "image/svg+xml", url: `data:image/svg+xml,${path}`, size: 3, text: svg } });
  show({ key: "i", path: "logo.svg", content: { state: "change", change: { old: side("a"), new: side("b"), patch: "Binary files differ" } } });
  act(() => void document.querySelectorAll("img")[0].dispatchEvent(new Event("error")));
  await settled();
  const sections = dialog()!.querySelectorAll("section");
  expect(markNames(sections[0])).toEqual(["This image cannot be displayed, so its text is shown instead."]);
  expect(markNames(sections[1])).toEqual([]);
  expect(markNames(dialog()!.querySelector("[data-viewer-description]")!)).toEqual([]);
});

it("stands the marks after the path, outside the path's own fade", async () => {
  const side = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file" });
  const loadBodies = () => Promise.reject(new ChangeBodiesError("failed", "git failed"));
  show({ key: "order", path: "f", content: { state: "change", change: { old: side("f"), new: side("f"), patch: "", loadBodies } } });
  await settled();
  const detail = dialog()!.querySelector("[data-viewer-description]")!;
  const mark = detail.querySelector("[data-status-mark]")!;
  const pathText = [...detail.querySelectorAll("[dir=ltr]")].find((element) => element.textContent === "f")!;
  expect(mark.tagName).toBe("BUTTON");
  expect(pathText.contains(mark)).toBe(false);
  expect(pathText.compareDocumentPosition(mark) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("orders the marks failed, then warned, then notes, whichever came first", async () => {
  const side: ViewerChangeSide = { state: "present", path: "f", kind: "file" };
  const loadBodies = async () => ({ old: "a\n", new: "b\n" });
  const patch = "@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n";
  show({ key: "marks", path: "f", content: { state: "change", change: { old: side, new: side, patch, loadBodies } } });
  await settled();
  expect(markNames(dialog()!)).toEqual(["The old version has no newline at its end."]);
  act(() => void dialog()!.querySelector<HTMLButtonElement>("[data-report=failed]")!.click());
  expect(markNames(dialog()!)).toEqual([`Could not read the unmodified lines. ${isolate("No luck.")}`, "The old version has no newline at its end."]);
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

// The stage and the kind of change are tags after the name in the title, the stage a neutral bordered
// badge and the kind coloured; the description line has the path and the layout choice at its end.
it("puts the stage and the status in the title and the layout choice on the description's row", () => {
  show(modified());
  const heading = dialog()!.querySelector("h2")!;
  expect(titleName()).toBe("c.ts: Unstaged, Modified");
  const tags = heading.querySelectorAll("[data-variant]");
  expect([...tags].map((tag) => tag.textContent)).toEqual(["Unstaged", "Modified"]);
  const drawn = heading.querySelector('[aria-hidden="true"]')!;
  expect([...tags].every((tag) => drawn.contains(tag))).toBe(true);
  // The drawn name comes first, then the tags, as the accessible name reads.
  expect(drawn.textContent).toBe("c.tsUnstagedModified");
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
  const tags = dialog()!.querySelectorAll("[data-variant]");
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
  expect(titleName()).toBe("m.ts: Conflicted. Both sides modified this file.");
  expect(row().textContent).not.toMatch(/staged/i);
  show({ ...modified(), stage: undefined, source: "main at 1234567 to feature at fedcba0" });
  expect(titleName()).toBe("c.ts: Modified");
  expect(row().textContent).not.toContain("main at");
  expect(subtitle()!.textContent).toBe("main at 1234567 to feature at fedcba0");
});

it("says a conflict's kind on the status badge's tooltip and after the heading's name, with no mark of its own", () => {
  vi.useFakeTimers();
  const body = { kind: "text", text: "x\n", size: 2 } as const;
  show({ key: "c", path: "src/m.ts", content: { state: "conflict", conflict: "Both sides modified this file.", body } });
  expect(markNames(dialog()!)).toEqual([]);
  expect(titleName()).toBe("m.ts: Conflicted. Both sides modified this file.");
  const badge = dialog()!.querySelector('h2 [data-slot="chip"]')!;
  enter(badge);
  act(() => void vi.runOnlyPendingTimers());
  expect(openTooltip()?.textContent).toBe("Both sides modified this file.");
  leave(badge);
  // A finger has no hover: a tap opens the same sentence.
  press(badge);
  expect(document.querySelectorAll("[role=dialog]")[1]?.textContent).toBe("Both sides modified this file.");
});

// An untracked file has no old side, which reads as added; the list says untracked, and so does the
// badge, with no note repeating that there is nothing to compare it with.
it("marks an untracked file Untracked, with no note that it is new content", () => {
  const body = { kind: "text", text: "x\n", size: 2 } as const;
  const content: ViewerContent = {
    state: "change",
    change: { old: { state: "absent" }, new: { state: "present", path: "src/n.ts", kind: "file", body } },
  };
  show({ key: "u", path: "src/n.ts", status: "untracked", content });
  expect(titleName()).toBe("n.ts: Untracked");
  expect(dialog()!.querySelector("h2 [data-variant]")!.getAttribute("data-color")).toBe("default");
  expect(dialog()!.textContent).not.toMatch(/new content/i);
  show({ key: "a", path: "src/n.ts", content });
  expect(titleName()).toBe("n.ts: Added");
  expect(dialog()!.querySelector("h2 [data-variant]")!.getAttribute("data-color")).toBe("success");
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

it("draws the size as a badge after the path", () => {
  show(binary);
  const items = [...dialog()!.querySelector("[data-viewer-description]")!.children].slice(1);
  expect(items.map((item) => item.textContent)).toEqual(["bin/b.wasm", "3 bytes"]);
  expect(items[1].querySelector("[data-variant]")!.textContent).toBe("3 bytes");
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
  await import("./MarkdownDocument");
  const error = vi.fn();
  const cause = new Error("Browser launch failed");
  const links = { open: vi.fn(async () => { throw cause; }) };
  show({ key: "document-link", path: "README.md", content: { state: "file", body: { kind: "text", text: "[Documentation](https://example.com/docs)", size: 48 } } }, undefined, { links, toasts: { error, notice: vi.fn() } });
  // A cold document renderer can take longer to load than a fixed polling budget allows.
  await act(async () => { await import("./MarkdownDocument"); });
  await act(async () => document.querySelector<HTMLAnchorElement>('a[href="https://example.com/docs"]')!.click());
  expect(links.open).toHaveBeenCalledExactlyOnceWith("https://example.com/docs");
  expect(error).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual(expect.arrayContaining([expect.objectContaining({ message: "The link did not open in a browser. Its address is shown below.", error: expect.objectContaining({ cause }), classification: { kind: "actionable", recovery: "openExternal" } })]));
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
it("keeps Copy path, then the layout choice, then the wrap choice, when the layout choice mounts second", () => {
  const side: ViewerChangeSide = { state: "present", path: "src/c.ts", kind: "file" };
  const change = (key: string, patch: string, old: ViewerChangeSide): ViewerSubject => ({
    key,
    path: "src/c.ts",
    content: { state: "change", change: { old, new: side, patch } },
  });
  const controls = () => [...dialog()!.querySelector("[data-viewer-description]")!.querySelectorAll("button")].map((b) => b.getAttribute("aria-label"));
  show(change("one", "@@ -0,0 +1 @@\n+a\n", { state: "absent" }));
  expect(controls()).toEqual(["Copy path", "Wrap lines"]);
  show(change("two", "@@ -1 +1 @@\n-a\n+b\n", side));
  expect(controls()).toEqual(["Copy path", "Show split diff", "Wrap lines"]);
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

const textFile = (path: string, text: string, host?: ViewerSubject["host"]): ViewerSubject => ({
  key: path,
  path,
  host,
  content: { state: "file", body: { kind: "text", text, size: text.length } },
});

it("offers File manager and Default application last in the controls, on a subject with a place on the host", async () => {
  const request = vi.fn(async () => ({ type: "ack", id: null }) as never);
  show(textFile("src/a.ts", "x", { project: "p", worktree: "w", path: "src/a.ts" }), undefined, { clipboard: { writeText: async () => {} }, request });
  await settled();
  const group = dialog()!.querySelector("[data-viewer-description] .float-end")!;
  const labels = [...group.querySelectorAll("button")].map((b) => b.getAttribute("aria-label"));
  expect(labels.slice(-2)).toEqual(["Open in file manager", "Open in default application"]);
  act(() => button("Open in file manager").click());
  act(() => button("Open in default application").click());
  expect(request.mock.calls).toEqual([
    [{ type: "reveal_project_path", project: "p", worktree: "w", path: "src/a.ts" }],
    [{ type: "open_project_path", project: "p", worktree: "w", path: "src/a.ts" }],
  ]);
});

it("sends no worktree for a file of the project's own directory, and offers the buttons for nothing without a host path", async () => {
  const request = vi.fn(async () => ({ type: "ack", id: null }) as never);
  show(textFile("a.ts", "x", { project: "p", path: "a.ts" }), undefined, { request });
  await settled();
  act(() => button("Open in default application").click());
  expect(request).toHaveBeenCalledWith({ type: "open_project_path", project: "p", path: "a.ts" });
  show(textFile("b.ts", "x"), undefined, { request });
  await settled();
  expect(document.querySelector('button[aria-label="Open in file manager"]')).toBeNull();
  expect(document.querySelector('button[aria-label="Open in default application"]')).toBeNull();
});

it("hands a line to the code it shows, and opens a Markdown file's source at one", async () => {
  show(textFile("a.ts", "one\ntwo"), undefined, {}, 2);
  await settled();
  expect(dialog()!.querySelector("[data-line-asked]")?.getAttribute("data-line-asked")).toBe("2");
  // The document has no lines, so a line asks for the source, whatever the saved choice is.
  localStorage.clear();
  show(textFile("README.md", "# Title\ntext"), undefined, {}, 2);
  await settled();
  expect(dialog()!.querySelector("[data-line-asked]")?.getAttribute("data-line-asked")).toBe("2");
  show(textFile("NOTES.md", "# Title\ntext"));
  await settled();
  expect(dialog()!.querySelector("[data-line-asked]")).toBeNull();
});

it("brings a line into view once for each file and line asked for, however often the code is drawn again", async () => {
  scrolls.toLine.mockClear();
  const subject = textFile("a.ts", "one\ntwo\nthree");
  show(subject, undefined, {}, 2);
  await settled();
  show(subject, undefined, {}, 2);
  act(() => wordWrap.set(true));
  await settled();
  expect(scrolls.toLine.mock.calls.map(([, line]) => line)).toEqual([2]);
  show(subject, undefined, {}, 3);
  await settled();
  expect(scrolls.toLine.mock.calls.map(([, line]) => line)).toEqual([2, 3]);
});
