// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { ThemeProvider } from "../../preferences/theme";
import type { ViewerSubject } from "../content";
import { FileViewer } from "../FileViewer";
import { ViewerHost } from "../viewerTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rendering library does not run under jsdom; a stand-in shows the text it is handed.
vi.mock("../code/renderer", () => ({
  HighlightedFile: ({ text, onDrawn }: { text: string; onDrawn?: () => void }) => {
    onDrawn?.();
    return <pre data-renderer="">{text}</pre>;
  },
  RenderedDiff: () => <div data-renderer="" />,
  endRendererPool: () => {},
}));

// The document renderer's import fails once: its export is read when the lazy component loads. In a file of
// its own, as the lazy component loads for good the first time it succeeds.
const documentImport = vi.hoisted(() => ({ failNext: true }));
vi.mock("./MarkdownDocument", async (importOriginal) => {
  const original = await importOriginal<typeof import("./MarkdownDocument")>();
  return {
    ...original,
    get MarkdownDocument() {
      if (documentImport.failNext) {
        documentImport.failNext = false;
        throw new Error("chunk");
      }
      return original.MarkdownDocument;
    },
  };
});

window.matchMedia ??= (query: string) =>
  ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList;

beforeEach(() => {
  document.documentElement.style.setProperty("--breakpoint-phone", "594px");
});

const markdownFile = (path: string): ViewerSubject => ({
  key: path,
  path,
  content: { state: "file", body: { kind: "text", text: "# Title\n\nSome words.\n", size: 21 } },
});

it("shows the source when the document renderer fails to load, and the document once the file changes", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const show = (path: string) =>
    act(() =>
      root.render(
        <ViewerHost>
          <ThemeProvider>
            <FileViewer subject={markdownFile(path)} onClose={() => {}} />
          </ThemeProvider>
        </ViewerHost>,
      ),
    );
  // The renderer is imported on first use, which takes longer than a few ticks on a loaded machine.
  const settleUntil = async (done: () => boolean) => {
    for (let i = 0; i < 1000 && !done(); i += 1) await act(() => new Promise((resolve) => setTimeout(resolve, 5)));
  };
  const sentence = "Shown as source because the document view could not be drawn.";
  show("docs/a.md");
  await settleUntil(() => document.body.textContent!.includes(sentence));
  expect(document.body.textContent).toContain(sentence);
  expect(document.querySelector("[data-renderer]")?.textContent).toContain("# Title");
  expect(document.querySelector("[role=region] h1")).toBeNull();
  show("docs/b.md");
  await settleUntil(() => document.querySelector("[role=region] h1") !== null);
  expect(document.querySelector("[role=region] h1")?.textContent).toBe("Title");
  expect(document.body.textContent).not.toContain(sentence);
});
