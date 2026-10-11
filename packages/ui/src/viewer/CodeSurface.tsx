import React, { Suspense, useCallback, useMemo, useState } from "react";

import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useFocusVisibleProps } from "../components/useFocusVisibleProps";
import { useT } from "../i18n/react";
import { diffPlan, textPlan } from "./budgets";
import { CODE_THEMES } from "./codeTheme";
import { HighlightedFile, RenderedDiff, RendererBoundary } from "./codeRenderer";
import { hasHunks, withoutNoNewlineMarkers, type ChangeBodies } from "./content";
import { LayoutToggle, type DiffLayout } from "./diffLayout";
import type { ExpansionStatus, SeparatorLabels } from "./expansion";
import { FrameSurface } from "./FrameSurface";
import { focusFirstSeparatorControl } from "./rendererDom";
import { isSelectAll, selectCode } from "./selectAll";
import { LoadingOverlay, Notice, Unreadable } from "./viewerStates";
import { WrapToggle, wordWrap } from "./wordWrap";

/** Text as it is, laid out by the browser as one preformatted node: what the viewer falls back to
 * past a budget or when the renderer fails, and cheap at any size the daemon sends. It wraps long
 * lines when the user's word wrap choice says so. */
export function PlainText({ text, label, theme }: { text: string; label: string; theme: "light" | "dark" }): React.ReactElement {
  const frame = useCodeFrame();
  const wrap = wordWrap.useValue();
  const colours = CODE_THEMES[theme];
  return (
    <FrameSurface>
      <pre
        dir="ltr"
        tabIndex={0}
        role="region"
        aria-label={label}
        {...frame}
        style={{ backgroundColor: colours.background, color: colours.foreground }}
        className={`min-h-0 flex-1 overflow-auto rounded-xl p-3 font-mono text-xs leading-5 outline-none data-focus-visible:ring-2 data-focus-visible:ring-focus ${wrap ? "break-words whitespace-pre-wrap" : "whitespace-pre"}`}
      >
        {text}
      </pre>
    </FrameSurface>
  );
}

/** What both code frames share as focusable regions: a focus ring (`useFocusVisibleProps`), Select
 * All taking the code alone (`selectCode`), left to the browser's own where that cannot, and Tab
 * entering the controls among a diff's collapsed lines. */
function useCodeFrame() {
  return {
    ...useFocusVisibleProps(),
    onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => {
      // Tab from the frame itself goes to the first control among its collapsed lines, if it has one;
      // the dialog's own Tab handling, which would go past it, does not see the frame's key.
      if (event.key === "Tab" && !event.shiftKey && event.target === event.currentTarget && focusFirstSeparatorControl(event.currentTarget)) {
        event.preventDefault();
        event.stopPropagation();
        return;
      }
      if (!isSelectAll(event)) return;
      if (selectCode(event.currentTarget)) event.preventDefault();
    },
  };
}

/**
 * The scrolling frame around rendered code. It takes focus so the code can be scrolled from the
 * keyboard, and keeps the code in its own reading direction under a right-to-left language.
 *
 * It says it is loading until the renderer reports the subject's code drawn (`onDrawn`), not only
 * while the renderer's module loads: the library draws nothing until its worker pool has started,
 * which the pool does again each time the viewer opens, and a large file would otherwise stand blank
 * for seconds.
 */
function CodeFrame({
  label,
  resetKey,
  theme,
  children,
}: {
  label: string;
  resetKey: string;
  theme: "light" | "dark";
  children: (onDrawn: () => void) => React.ReactNode;
}): React.ReactElement {
  const frame = useCodeFrame();
  const [drawnFor, setDrawnFor] = useState<string>();
  const onDrawn = useCallback(() => setDrawnFor(resetKey), [resetKey]);
  const drawn = drawnFor === resetKey;
  return (
    <FrameSurface>
      <div
        dir="ltr"
        tabIndex={0}
        role="region"
        aria-label={label}
        {...frame}
        style={{ backgroundColor: CODE_THEMES[theme].background }}
        className="relative min-h-0 flex-1 overflow-auto rounded-xl outline-none data-focus-visible:ring-2 data-focus-visible:ring-focus"
      >
        {!drawn && <LoadingOverlay />}
        <Suspense fallback={null}>{children(onDrawn)}</Suspense>
      </div>
    </FrameSurface>
  );
}

/** A text file as code: highlighted within the budget, plain past it or when highlighting fails. */
export function CodeSurface({
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
  const [plainFor, setPlainFor] = useState<string>();
  const onPlainChange = useCallback((plain: boolean) => setPlainFor(plain ? resetKey : undefined), [resetKey]);
  const label = t("viewer.contents", { name });
  const wrap = wordWrap.useValue();
  if (textPlan(text) === "plain") {
    return (
      <>
        <WrapToggle />
        <Notice>{t("viewer.plain.large")}</Notice>
        <PlainText text={text} label={label} theme={theme} />
      </>
    );
  }
  const fallback = (
    <>
      <WrapToggle />
      <Notice>{t("viewer.plain.failed")}</Notice>
      <PlainText text={text} label={label} theme={theme} />
    </>
  );
  return (
    <>
      {plainFor === resetKey && <Notice>{t("viewer.plain.failed")}</Notice>}
      <RendererBoundary resetKey={resetKey} fallback={fallback}>
        <WrapToggle />
        <CodeFrame label={label} resetKey={resetKey} theme={theme}>
          {(onDrawn) => (
            <HighlightedFile name={name} text={text} theme={theme} wrap={wrap} onPlainChange={onPlainChange} onDrawn={onDrawn} />
          )}
        </CodeFrame>
      </RendererBoundary>
    </>
  );
}

/**
 * A change as a diff, rendered from its patch within the budget, with the choice of layout (left to
 * the caller when `onLayoutChange` is absent, for several diffs sharing one); past the budget, or
 * when rendering fails, the patch as plain text, without the layout choice it no longer has. The
 * word wrap choice is offered in every one of these forms.
 *
 * Git's "\ No newline at end of file" marker lines are taken out before rendering and said in a
 * notice of their own: the library draws the marker as a line of the change, in English.
 */
export function DiffSurface({
  resetKey,
  name,
  patch,
  layout,
  onLayoutChange,
  theme,
  loadBodies,
}: {
  resetKey: string;
  name: string;
  patch: string;
  layout: DiffLayout;
  onLayoutChange?: (layout: DiffLayout) => void;
  theme: "light" | "dark";
  /** Reads the whole text of both sides, which offers expanding the lines the patch collapses. */
  loadBodies?: () => Promise<ChangeBodies>;
}): React.ReactElement {
  const t = useT();
  const label = t("viewer.contents", { name });
  const wrap = wordWrap.useValue();
  const marked = useMemo(() => withoutNoNewlineMarkers(patch), [patch]);
  const labels = useMemo<SeparatorLabels>(
    () => ({
      unmodified: (count) => t("viewer.expand.unmodified", { count }),
      unknown: t("viewer.expand.unknown"),
      above: t("viewer.expand.above"),
      below: t("viewer.expand.below"),
      between: t("viewer.expand.between"),
      all: t("viewer.expand.all"),
      loading: t("viewer.expand.loading"),
      failedHere: t("viewer.expand.failedHere"),
    }),
    [t],
  );
  // How the expansion of this patch's collapsed lines stands; a new subject, or the same one read
  // again (which hands a new `loadBodies`, whether or not its patch differs), starts afresh.
  const [reported, setReported] = useState<{ key: string; read?: () => Promise<ChangeBodies>; status: ExpansionStatus }>();
  const status = reported?.key === resetKey && reported.read === loadBodies ? reported.status : undefined;
  const onExpansion = useCallback((next: ExpansionStatus) => setReported({ key: resetKey, read: loadBodies, status: next }), [resetKey, loadBodies]);
  const expansionNote =
    status?.kind === "changed"
      ? t("viewer.expand.changed")
      : status?.kind === "failed"
        ? t("viewer.expand.failed", { reason: status.message })
        : undefined;
  const asText = (reason: string) => (
    <>
      <WrapToggle />
      <Notice>{reason}</Notice>
      <PlainText text={patch} label={label} theme={theme} />
    </>
  );
  if (!hasHunks(patch)) return <Unreadable />;
  if (diffPlan(patch) === "patch") return asText(t("viewer.change.patchLarge"));
  const ending = marked.old && marked.new ? "viewer.change.noNewlineBoth" : marked.old ? "viewer.change.noNewlineOld" : marked.new ? "viewer.change.noNewlineNew" : undefined;
  return (
    <RendererBoundary resetKey={resetKey} fallback={asText(t("viewer.change.patchFailed"))}>
      {onLayoutChange && <LayoutToggle layout={layout} onLayoutChange={onLayoutChange} />}
      <WrapToggle />
      {ending && <Notice>{t(ending)}</Notice>}
      {expansionNote !== undefined && <Notice>{expansionNote}</Notice>}
      <StatusAnnouncer text={status?.kind === "loading" ? t("viewer.expand.loading") : expansionNote} />
      <CodeFrame label={label} resetKey={resetKey} theme={theme}>
        {(onDrawn) => (
          <RenderedDiff
            name={name}
            patch={marked.patch}
            layout={layout}
            theme={theme}
            wrap={wrap}
            onDrawn={onDrawn}
            loadBodies={loadBodies}
            labels={labels}
            onExpansion={onExpansion}
          />
        )}
      </CodeFrame>
    </RendererBoundary>
  );
}
