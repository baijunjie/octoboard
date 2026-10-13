import React, { Suspense, useCallback, useMemo, useState } from "react";

import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useFadeRegion, useScrollFade, type FadeRegion } from "../components/scrollFade";
import type { PlainMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { diffPlan, textPlan } from "./budgets";
import { CODE_THEMES } from "./codeTheme";
import { HighlightedFile, RenderedDiff, RendererBoundary } from "./codeRenderer";
import type { ChangeBodies } from "./content";
import { LayoutToggle, type DiffLayout } from "./diffLayout";
import type { ExpansionStatus, SeparatorLabels } from "./expansion";
import { FrameSurface } from "./FrameSurface";
import { hasHunks, linesAfterLastHunk, withoutNoNewlineMarkers } from "./patch";
import { focusFirstSeparatorControl } from "./rendererDom";
import { isSelectAll, selectCode } from "./selectAll";
import { LoadingOverlay, Notice, Unreadable, type NoticeTone } from "./viewerStates";
import { WrapToggle, wordWrap } from "./wordWrap";

/** What is said beside the diff when its collapsed lines cannot be expanded, by why (`failed` takes
 * the reason, so it is worded where it is shown). */
const EXPANSION_NOTES: Partial<Record<ExpansionStatus["kind"], PlainMessageKey>> = {
  changed: "viewer.expand.changed",
  overBudget: "viewer.expand.overBudget",
  unavailable: "viewer.expand.unavailable",
};

/** Text as it is, laid out by the browser as one preformatted node: what the viewer falls back to
 * past a budget or when the renderer fails, and cheap at any size the daemon sends. It wraps long
 * lines when the user's word wrap choice says so. */
export function PlainText({ text, label, theme }: { text: string; label: string; theme: "light" | "dark" }): React.ReactElement {
  const frame = useCodeFrame(label);
  const wrap = wordWrap.useValue();
  const colours = CODE_THEMES[theme];
  const fade = useScrollFade<HTMLPreElement>({
    region: frame.region,
    style: { backgroundColor: colours.background, color: colours.foreground },
    // `overflow-auto` rather than the fade's own `overflow-y` alone: unwrapped lines scroll
    // sideways too, and the fade is the vertical one either way.
    className: `min-h-0 flex-1 overflow-auto rounded-xl p-3 font-mono text-xs leading-5 ${wrap ? "break-words whitespace-pre-wrap" : "whitespace-pre"}`,
  });
  return (
    <FrameSurface focused={frame.region.focused}>
      <pre dir="ltr" {...fade} onKeyDown={frame.onKeyDown}>
        {text}
      </pre>
    </FrameSurface>
  );
}

/** What both code frames share as focusable regions, there whether or not they scroll: the region
 * itself named `label`, Select All taking the code alone (`selectCode`), left to the browser's own
 * where that cannot, and Tab entering the controls among a diff's collapsed lines. */
function useCodeFrame(label: string): { region: FadeRegion; onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => void } {
  return {
    region: useFadeRegion({ label, always: true }),
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
  const frame = useCodeFrame(label);
  const fade = useScrollFade({
    region: frame.region,
    style: { backgroundColor: CODE_THEMES[theme].background },
    className: "relative min-h-0 flex-1 overflow-auto rounded-xl",
  });
  const [drawnFor, setDrawnFor] = useState<string>();
  const onDrawn = useCallback(() => setDrawnFor(resetKey), [resetKey]);
  const drawn = drawnFor === resetKey;
  return (
    <FrameSurface focused={frame.region.focused}>
      <div dir="ltr" {...fade} onKeyDown={frame.onKeyDown}>
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
  const label = t("viewer.contents", { name: isolate(name) });
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
      <Notice tone="failed" speak={t("viewer.plain.unrenderable")} />
      <PlainText text={text} label={label} theme={theme} />
    </>
  );
  return (
    <>
      <RendererBoundary resetKey={resetKey} fallback={fallback}>
        {plainFor === resetKey && <Notice tone="failed" speak={t("viewer.plain.failed")} />}
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
 * A change as a diff, rendered from its patch within the budget, with the choice of layout (none
 * when `onLayoutChange` is absent, as for a diff with one side only); past the budget, or
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
  lines,
}: {
  resetKey: string;
  name: string;
  patch: string;
  layout: DiffLayout;
  onLayoutChange?: (layout: DiffLayout) => void;
  theme: "light" | "dark";
  /** Reads the whole text of both sides, which offers expanding the lines the patch collapses. */
  loadBodies?: () => Promise<ChangeBodies>;
  /** How many lines each side holds, where the daemon counted them. */
  lines?: { old?: number | null; new?: number | null };
}): React.ReactElement {
  const t = useT();
  const label = t("viewer.contents", { name: isolate(name) });
  const wrap = wordWrap.useValue();
  const marked = useMemo(() => withoutNoNewlineMarkers(patch), [patch]);
  const trailing = useMemo(() => linesAfterLastHunk(marked.patch, { old: lines?.old, new: lines?.new }), [marked.patch, lines?.old, lines?.new]);
  const labels = useMemo<SeparatorLabels>(
    () => ({
      unmodified: (count) => t("viewer.expand.unmodified", { count }),
      unknown: t("viewer.expand.unknown"),
      above: t("viewer.expand.above"),
      below: t("viewer.expand.below"),
      between: t("viewer.expand.between"),
      hiddenAll: (count) => t("viewer.expand.hiddenAll", { count }),
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
  const note = status && EXPANSION_NOTES[status.kind];
  const expansionNote = status?.kind === "failed" ? t("viewer.expand.failed", { reason: isolate(status.message) }) : note ? t(note) : undefined;
  const asText = (reason: string, tone: NoticeTone, speak?: string) => (
    <>
      <WrapToggle />
      <Notice tone={tone} speak={speak}>
        {reason}
      </Notice>
      <PlainText text={patch} label={label} theme={theme} />
    </>
  );
  if (!hasHunks(patch)) return <Unreadable />;
  if (diffPlan(patch) === "patch") return asText(t("viewer.change.patchLarge"), "explained");
  const ending = marked.old && marked.new ? "viewer.change.noNewlineBoth" : marked.old ? "viewer.change.noNewlineOld" : marked.new ? "viewer.change.noNewlineNew" : undefined;
  return (
    <RendererBoundary resetKey={resetKey} fallback={asText(t("viewer.change.patchFailed"), "failed", t("viewer.change.patchFailed"))}>
      {onLayoutChange && <LayoutToggle layout={layout} onLayoutChange={onLayoutChange} />}
      <WrapToggle />
      {ending && <Notice>{t(ending)}</Notice>}
      {expansionNote !== undefined && (
        <Notice tone={status?.kind === "failed" ? "failed" : "explained"} announced>
          {expansionNote}
        </Notice>
      )}
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
            trailing={trailing}
            onExpansion={onExpansion}
          />
        )}
      </CodeFrame>
    </RendererBoundary>
  );
}
