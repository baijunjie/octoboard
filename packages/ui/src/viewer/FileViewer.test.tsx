// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { ThemeProvider } from "../theme";
import type { ViewerSubject } from "./content";
import { FileViewer, type ViewerNavigation } from "./FileViewer";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rendering library does not run under jsdom; a stand-in shows the text it is handed, which is
// what these tests look at. That the library itself escapes repository text is checked in WebKit.
vi.mock("./renderer", () => ({
  HighlightedFile: ({ text }: { text: string }) => <pre data-renderer="">{text}</pre>,
  RenderedDiff: ({ patch }: { patch: string }) => <pre data-renderer="">{patch}</pre>,
  endRendererPool: () => {},
}));

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
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
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
