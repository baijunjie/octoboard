import React, { Suspense, useCallback, useState } from "react";

import { trackSuspenseFallback } from "../../components/hooks/lazyReadiness";
import { useRenderLatest } from "../../components/hooks/useLatest";
import { retryableLazy } from "../../components/hooks/retryableLazy";
import { useFadeRegion, useScrollFade } from "../../components/scroll/scrollFade";
import { isolate } from "../../i18n/isolate";
import { useT } from "../../i18n/react";
import { usePlatform } from "../../platform/react";
import { textPlan } from "../code/budgets";
import { CODE_THEMES } from "../code/codeTheme";
import { RendererBoundary } from "../code/codeRenderer";
import { CodeSurface } from "../code/CodeSurface";
import { FIND_REGION } from "../find/findDom";
import { FencedCode } from "./FencedCode";
import { FrameSurface } from "../FrameSurface";
import { markdownView, MarkdownViewToggle, type MarkdownView } from "./markdownView";
import { useMarkdownLocation } from "./markdownLocation";
import { copyDocument, isSelectAll, selectDocument } from "../selectAll";
import { FileNotice } from "../notices";
import { LoadingOverlay } from "../viewerStates";

// The Markdown renderer, with its library, loads the first time a document is shown.
const markdownDocument = retryableLazy(() => import("./MarkdownDocument").then((module) => ({ default: module.MarkdownDocument })));
const MarkdownDocument = markdownDocument.Component;

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
  line,
}: {
  resetKey: string;
  name: string;
  text: string;
  theme: "light" | "dark";
  /** A line to show: the document has none, so the source opens at it, whatever the user's choice
   * of view, until they choose one for this file. */
  line?: number;
}): React.ReactElement {
  const preferred = markdownView.useValue();
  const askedFor = `${resetKey}\0${line}`;
  const [chosen, setChosen] = useState<{ askedFor: string; view: MarkdownView }>();
  const view = chosen?.askedFor === askedFor ? chosen.view : line !== undefined ? "source" : preferred;
  if (textPlan(text) === "plain") return <CodeSurface resetKey={resetKey} name={name} text={text} theme={theme} line={line} />;
  return (
    <>
      <MarkdownViewToggle
        view={view}
        onViewChange={(next) => {
          setChosen({ askedFor, view: next });
          markdownView.set(next);
        }}
      />
      {view === "source" ? (
        <CodeSurface resetKey={resetKey} name={name} text={text} theme={theme} line={line} />
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
  const { links, host } = usePlatform();
  const location = useMarkdownLocation();
  const colours = CODE_THEMES[theme];
  const region = useFadeRegion({ label: t("viewer.contents", { name: isolate(name) }), always: true });
  const fade = useScrollFade({
    region,
    style: { backgroundColor: colours.background, color: colours.foreground },
    className: "relative min-h-0 flex-1 overflow-auto p-4 text-sm leading-6",
  });
  // Stable across the caller's renders. A new callback would mount every link and diagram again,
  // and take the focus of a link with it, whenever the frame gains or loses its focus ring.
  // The document's links call `isDirectory` while they render, and have to get this render's answer.
  const openFile = useRenderLatest(location?.onOpenFile);
  const isDirectoryOf = useRenderLatest(location?.isDirectory);
  const onOpenFile = useCallback((file: string, fragment?: string) => {
    openFile()?.(file, fragment);
  }, [openFile]);
  const isDirectory = useCallback((file: string) => isDirectoryOf()?.(file) ?? false, [isDirectoryOf]);
  // Stable, so the frame's focus ring changing does not draw the document again.
  const renderCode = useCallback(
    ({ language, text: code, highlight }: { language?: string; text: string; highlight: boolean }) => (
      <FencedCode language={language} text={code} theme={theme} highlight={highlight} />
    ),
    [theme],
  );
  const onOpenLink = useCallback((url: string) => links?.open(url), [links]);
  const fallback = () => (
    <>
      <FileNotice tone="failed" sentence={t("viewer.markdown.failed")} speak />
      <CodeSurface resetKey={resetKey} name={name} text={text} theme={theme} />
    </>
  );
  return (
    <RendererBoundary resetKey={resetKey} fallback={fallback} retry={markdownDocument.retry}>
      <FrameSurface focused={region.focused}>
        <div
          key={resetKey}
          {...fade}
          {...FIND_REGION}
          onKeyDown={(event) => {
            if (isSelectAll(event, host) && selectDocument(event.currentTarget)) event.preventDefault();
          }}
          onCopy={(event) => void copyDocument(event, event.currentTarget, text)}
        >
          <Suspense fallback={trackSuspenseFallback(<LoadingOverlay />)}>
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
