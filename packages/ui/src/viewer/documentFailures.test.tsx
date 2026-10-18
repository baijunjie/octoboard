// @vitest-environment jsdom
import katex from "katex";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { DocumentImage } from "./DocumentImage";
import { FormulaView } from "./FormulaView";
import { ViewerHost } from "./viewerTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

async function mount(content: React.ReactNode): Promise<void> {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
    vi.restoreAllMocks();
  });
  await act(async () => {
    root.render(content);
    // Imports and effects settle several microtask hops after the first paint.
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  for (let i = 0; i < 20 && !errorAlerts.getSnapshot().length; i += 1) {
    await act(async () => new Promise((resolve) => setTimeout(resolve, 0)));
  }
}

it.each([false, true])("offers source correction for a KaTeX parse error, inline %s", async (inline) => {
  await mount(<FormulaView source="\\frac{" inline={inline} display={!inline} overBudget={false} />);
  expect(errorAlerts.getSnapshot()).toMatchObject([{
    error: { cause: expect.any(katex.ParseError) },
    classification: { kind: "actionable", recovery: "contentSyntax" },
  }]);
});

it("keeps an arbitrary formula renderer exception unexpected with its cause", async () => {
  const error = new TypeError("Renderer invariant failed");
  vi.spyOn(katex, "renderToString").mockImplementationOnce(() => { throw error; });
  await mount(<FormulaView source="x" inline={false} display overBudget={false} />);
  expect(errorAlerts.getSnapshot()).toMatchObject([{ error, classification: { kind: "unexpected" } }]);
});

it.each([
  ["file_not_found", "path"],
  ["permission_denied", "access"],
  ["unexpected_error", undefined],
])("preserves the project image read error %s for classification", async (code, recovery) => {
  const error = new DaemonRequestError("Image read failed", code!, { path: "image.png" });
  await mount(
    <ViewerHost request={async () => { throw error; }}>
      <DocumentImage src="image.png" alt="Diagram" documentPath="README.md" project="project" overBudget={false} />
    </ViewerHost>,
  );
  expect(errorAlerts.getSnapshot()).toMatchObject([{
    error,
    classification: recovery ? { kind: "actionable", recovery } : { kind: "unexpected" },
  }]);
});

it("offers image recovery when a document names a file that is not an image", async () => {
  await mount(
    <ViewerHost request={async () => ({ type: "project_file", project: "project", worktree: null, source: { kind: "live", root_id: "root", version: "version" }, path: "image.png", file: { kind: "text", text: "not an image", size: 12, media_type: null, data: null } })}>
      <DocumentImage src="image.png" documentPath="README.md" project="project" overBudget={false} />
    </ViewerHost>,
  );
  expect(errorAlerts.getSnapshot()).toMatchObject([{ classification: { kind: "actionable", recovery: "image" } }]);
});

it("offers image recovery when document image data cannot be decoded", async () => {
  await mount(
    <ViewerHost request={async () => ({ type: "project_file", project: "project", worktree: null, source: { kind: "live", root_id: "root", version: "version" }, path: "image.png", file: { kind: "binary", text: null, size: 3, media_type: "image/png", data: "AAEC" } })}>
      <DocumentImage src="image.png" documentPath="README.md" project="project" overBudget={false} />
    </ViewerHost>,
  );
  act(() => document.querySelector("img")!.dispatchEvent(new Event("error")));
  expect(errorAlerts.getSnapshot()).toMatchObject([{ classification: { kind: "actionable", recovery: "image" } }]);
});
