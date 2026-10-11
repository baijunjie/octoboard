import React, { Suspense, lazy, useCallback } from "react";

import { useFocusVisibleProps } from "../components/useFocusVisibleProps";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { textPlan } from "./budgets";
import { CODE_THEMES } from "./codeTheme";
import { RendererBoundary } from "./codeRenderer";
import { CodeSurface } from "./CodeSurface";
import { FencedCode } from "./FencedCode";
import { FrameSurface } from "./FrameSurface";
import { markdownView, MarkdownViewToggle } from "./markdownView";
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
  const focus = useFocusVisibleProps();
  const colours = CODE_THEMES[theme];
  // Stable, so the frame's focus ring changing does not draw the document again.
  const renderCode = useCallback(
    ({ language, text: code, highlight }: { language?: string; text: string; highlight: boolean }) => (
      <FencedCode language={language} text={code} theme={theme} highlight={highlight} />
    ),
    [theme],
  );
  const onOpenLink = useCallback(
    (url: string) =>
      void links?.open(url).catch((error: unknown) => {
        console.warn("Viewer: cannot open the link", error);
      }),
    [links],
  );
  const fallback = (
    <>
      <Notice>{t("viewer.markdown.failed")}</Notice>
      <CodeSurface resetKey={resetKey} name={name} text={text} theme={theme} />
    </>
  );
  return (
    <RendererBoundary resetKey={resetKey} fallback={fallback}>
      <FrameSurface>
        <div
          key={resetKey}
          tabIndex={0}
          role="region"
          aria-label={t("viewer.contents", { name })}
          {...focus}
          onKeyDown={(event) => {
            if (isSelectAll(event) && selectDocument(event.currentTarget)) event.preventDefault();
          }}
          onCopy={(event) => void copyDocument(event, event.currentTarget, text)}
          style={
            { backgroundColor: colours.background, color: colours.foreground, "--document-link": LINK_COLOURS[theme] } as React.CSSProperties
          }
          className="relative min-h-0 flex-1 overflow-auto rounded-xl p-4 text-sm leading-6 outline-none data-focus-visible:ring-2 data-focus-visible:ring-focus"
        >
          <Suspense fallback={<LoadingOverlay />}>
            <MarkdownDocument text={text} renderCode={renderCode} onOpenLink={links && onOpenLink} />
          </Suspense>
        </div>
      </FrameSurface>
    </RendererBoundary>
  );
}
