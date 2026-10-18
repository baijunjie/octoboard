import React, { Suspense, lazy, useCallback, useRef } from "react";

import { useFadeRegion, useScrollFade } from "../components/scrollFade";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { textPlan } from "./budgets";
import { CODE_THEMES } from "./codeTheme";
import { RendererBoundary } from "./codeRenderer";
import { CodeSurface } from "./CodeSurface";
import { FencedCode } from "./FencedCode";
import { FrameSurface } from "./FrameSurface";
import { markdownView, MarkdownViewToggle } from "./markdownView";
import { useMarkdownLocation } from "./markdownLocation";
import { copyDocument, isSelectAll, selectDocument } from "./selectAll";
import { LoadingOverlay, Notice } from "./viewerStates";

// The Markdown renderer, with its library, loads the first time a document is shown.
const MarkdownDocument = lazy(() => import("./MarkdownDocument").then((module) => ({ default: module.MarkdownDocument })));

/** Link colours measured at 4.5:1 or more on the code background of each appearance (the accent
 * colours of the same GitHub high-contrast palettes the code is drawn in). Links are underlined too,
 * so colour is not what marks them. */
const LINK_COLOURS = { light: "#0349b4", dark: "#71b7ff" } as const;

/**
 * A Markdown file, as the rendered document or as its source by the user's choice
 * (`markdownView`), which the header's toggle changes. The document is the default. A file past the
 * highlighting budget has no document: it is shown as the plain text it already falls back to, and
 * without the toggle, since a document of that size would hold the window as long as highlighting it
 * would.
 */
export function MarkdownSurface({
  resetKey,
  name,
  text,
  theme,
}: {
  resetKey: string;
  name: string;
  text: string;
  theme: "light" | "dark";
}): React.ReactElement {
  const view = markdownView.useValue();
  if (textPlan(text) === "plain") return <CodeSurface resetKey={resetKey} name={name} text={text} theme={theme} />;
  return (
    <>
      <MarkdownViewToggle view={view} onViewChange={markdownView.set} />
      {view === "source" ? (
        <CodeSurface resetKey={resetKey} name={name} text={text} theme={theme} />
      ) : (
        <DocumentSurface resetKey={resetKey} name={name} text={text} theme={theme} />
      )}
    </>
  );
}

/** The rendered document in a scrolling frame of its own, which takes focus so the document can be
 * scrolled from the keyboard. Where the renderer cannot load, the source is shown with a note. */
function DocumentSurface({
  resetKey,
  name,
  text,
  theme,
}: {
  resetKey: string;
  name: string;
  text: string;
  theme: "light" | "dark";
}): React.ReactElement {
  const t = useT();
  const links = usePlatform().links;
  const location = useMarkdownLocation();
  const colours = CODE_THEMES[theme];
  const region = useFadeRegion({ label: t("viewer.contents", { name: isolate(name) }), always: true });
  const fade = useScrollFade({
    region,
    style: { backgroundColor: colours.background, color: colours.foreground, "--document-link": LINK_COLOURS[theme] } as React.CSSProperties,
    className: "relative min-h-0 flex-1 overflow-auto rounded-xl p-4 text-sm leading-6",
  });
  // Stable across the caller's renders. A new callback would mount every link and diagram again,
  // and take the focus of a link with it, whenever the frame gains or loses its focus ring.
  // Written during render, which leaves this component out of the React Compiler: the document's
  // links call `isDirectory` while they render, and have to get this render's answer.
  const openFileRef = useRef(location?.onOpenFile);
  // eslint-disable-next-line react-hooks/refs
  openFileRef.current = location?.onOpenFile;
  const isDirectoryRef = useRef(location?.isDirectory);
  // eslint-disable-next-line react-hooks/refs
  isDirectoryRef.current = location?.isDirectory;
  const onOpenFile = useCallback((file: string, fragment?: string) => {
    openFileRef.current?.(file, fragment);
  }, []);
  const isDirectory = useCallback((file: string) => isDirectoryRef.current?.(file) ?? false, []);
  // Stable, so the frame's focus ring changing does not draw the document again.
  const renderCode = useCallback(
    ({ language, text: code, highlight }: { language?: string; text: string; highlight: boolean }) => (
      <FencedCode language={language} text={code} theme={theme} highlight={highlight} />
    ),
    [theme],
  );
  const onOpenLink = useCallback((url: string) => links?.open(url), [links]);
  const fallback = (error: unknown) => (
    <>
      <Notice tone="failed" speak={t("viewer.markdown.failed")} error={error} />
      <CodeSurface resetKey={resetKey} name={name} text={text} theme={theme} />
    </>
  );
  return (
    <RendererBoundary resetKey={resetKey} fallback={fallback}>
      <FrameSurface focused={region.focused}>
        <div
          key={resetKey}
          {...fade}
          onKeyDown={(event) => {
            if (isSelectAll(event) && selectDocument(event.currentTarget)) event.preventDefault();
          }}
          onCopy={(event) => void copyDocument(event, event.currentTarget, text)}
        >
          <Suspense fallback={<LoadingOverlay />}>
            <MarkdownDocument
              text={text}
              path={location?.path ?? ""}
              project={location?.project}
              theme={theme}
              renderCode={renderCode}
              onOpenLink={links && onOpenLink}
              onOpenFile={location?.onOpenFile && onOpenFile}
              isDirectory={isDirectory}
              anchor={location?.anchor}
            />
          </Suspense>
        </div>
      </FrameSurface>
    </RendererBoundary>
  );
}
