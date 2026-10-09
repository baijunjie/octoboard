import { Spinner, ToggleButton, ToggleButtonGroup } from "@heroui/react";
import React, { Suspense, lazy, useCallback, useEffect, useMemo, useState } from "react";
import { useFocusRing } from "react-aria";

import { useT } from "../i18n/react";
import { diffPlan, textPlan } from "./budgets";
import { withoutNoNewlineMarkers } from "./content";
import { isSelectAll, selectCode } from "./selectAll";

// The renderer module, with the library and its grammars, loads the first time code is shown.
let rendererLoaded = false;
const renderer = () => {
  rendererLoaded = true;
  return import("./renderer");
};
const HighlightedFile = lazy(() => renderer().then((module) => ({ default: module.HighlightedFile })));
const RenderedDiff = lazy(() => renderer().then((module) => ({ default: module.RenderedDiff })));

/** Shows its fallback in place of a renderer that threw — while rendering, in an effect, or by
 * failing to load its module — instead of letting the failure take the viewer down. A new `resetKey`
 * gives the renderer another try. */
class RendererBoundary extends React.Component<
  { resetKey: string; fallback: React.ReactNode; children: React.ReactNode },
  { failed: boolean; key: string }
> {
  state = { failed: false, key: this.props.resetKey };

  static getDerivedStateFromProps(props: { resetKey: string }, state: { key: string }): { failed: boolean; key: string } | null {
    return props.resetKey === state.key ? null : { failed: false, key: props.resetKey };
  }

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  componentDidCatch(error: unknown): void {
    console.warn("Viewer: the renderer failed", error);
  }

  render(): React.ReactNode {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

/** Keeps the renderer's worker pool for as long as a viewer that calls this is mounted, and ends
 * it, if code was shown at all, once the last one unmounts. The count is checked again once the
 * renderer module is at hand, so a viewer opened in the meantime keeps the pool. */
let mountedViewers = 0;
export function useRendererScope(): void {
  useEffect(() => {
    mountedViewers += 1;
    return () => {
      mountedViewers -= 1;
      if (mountedViewers === 0 && rendererLoaded) {
        void renderer().then((module) => mountedViewers === 0 && module.endRendererPool());
      }
    };
  }, []);
}

/** A line saying why the content below is shown the way it is. */
function Notice({ children }: { children: React.ReactNode }): React.ReactElement {
  return <p className="shrink-0 px-1 pb-2 text-xs text-muted">{children}</p>;
}

/** Text as it is, laid out by the browser as one preformatted node: what the viewer falls back to
 * past a budget or when the renderer fails, and cheap at any size the daemon sends. */
export function PlainText({ text, label }: { text: string; label: string }): React.ReactElement {
  const frame = useCodeFrame();
  return (
    <pre
      dir="ltr"
      tabIndex={0}
      role="region"
      aria-label={label}
      {...frame}
      className="min-h-0 flex-1 overflow-auto rounded-xl bg-surface-secondary p-3 font-mono text-xs leading-5 break-words whitespace-pre-wrap text-foreground outline-none data-focus-visible:ring-2 data-focus-visible:ring-focus"
    >
      {text}
    </pre>
  );
}

/**
 * What both code frames share as focusable regions: a focus ring, and Select All taking the code
 * alone (`selectCode`), left to the browser's own where that cannot.
 *
 * The ring follows react-aria's focus-visible state rather than CSS `:focus-visible`: in the app's
 * WKWebView a frame reached with Tab inside the viewer did not match `:focus-visible` and showed no
 * ring, while the viewer's buttons beside it did.
 */
function useCodeFrame(): React.HTMLAttributes<HTMLElement> & { "data-focus-visible"?: true } {
  const { focusProps, isFocusVisible } = useFocusRing();
  return {
    ...focusProps,
    "data-focus-visible": isFocusVisible || undefined,
    onKeyDown: (event) => {
      if (!isSelectAll(event)) return;
      if (selectCode(event.currentTarget)) event.preventDefault();
    },
  };
}

/** The scrolling frame around rendered code. It takes focus so the code can be scrolled from the
 * keyboard, and keeps the code in its own reading direction under a right-to-left language. */
function CodeFrame({ label, children }: { label: string; children: React.ReactNode }): React.ReactElement {
  const frame = useCodeFrame();
  return (
    <div
      dir="ltr"
      tabIndex={0}
      role="region"
      aria-label={label}
      {...frame}
      className="min-h-0 flex-1 overflow-auto rounded-xl outline-none data-focus-visible:ring-2 data-focus-visible:ring-focus"
    >
      <Suspense fallback={<Loading />}>{children}</Suspense>
    </div>
  );
}

export function Loading(): React.ReactElement {
  const t = useT();
  return (
    <div role="status" className="flex min-h-0 flex-1 items-center justify-center gap-2 text-sm text-muted">
      <Spinner size="sm" aria-hidden="true" />
      {t("viewer.loading")}
    </div>
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
  if (textPlan(text) === "plain") {
    return (
      <>
        <Notice>{t("viewer.plain.large")}</Notice>
        <PlainText text={text} label={label} />
      </>
    );
  }
  const fallback = (
    <>
      <Notice>{t("viewer.plain.failed")}</Notice>
      <PlainText text={text} label={label} />
    </>
  );
  return (
    <>
      {plainFor === resetKey && <Notice>{t("viewer.plain.failed")}</Notice>}
      <RendererBoundary resetKey={resetKey} fallback={fallback}>
        <CodeFrame label={label}>
          <HighlightedFile name={name} text={text} theme={theme} onPlainChange={onPlainChange} />
        </CodeFrame>
      </RendererBoundary>
    </>
  );
}

/** The choice between a unified and a split diff. */
export function LayoutToggle({
  layout,
  onLayoutChange,
}: {
  layout: "unified" | "split";
  onLayoutChange: (layout: "unified" | "split") => void;
}): React.ReactElement {
  const t = useT();
  return (
    <div className="flex shrink-0 justify-end">
      <ToggleButtonGroup
        aria-label={t("viewer.layout")}
        size="sm"
        selectionMode="single"
        disallowEmptySelection
        selectedKeys={[layout]}
        onSelectionChange={(keys) => {
          const [picked] = [...keys];
          if (picked === "unified" || picked === "split") onLayoutChange(picked);
        }}
      >
        <ToggleButton id="unified">{t("viewer.layout.unified")}</ToggleButton>
        <ToggleButton id="split">{t("viewer.layout.split")}</ToggleButton>
      </ToggleButtonGroup>
    </div>
  );
}

/**
 * A change as a diff, rendered from its patch within the budget, with the choice of layout (left to
 * the caller when `onLayoutChange` is absent, for several diffs sharing one); past the budget, or
 * when rendering fails, the patch as plain text, without the layout choice it no longer has.
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
}: {
  resetKey: string;
  name: string;
  patch: string;
  layout: "unified" | "split";
  onLayoutChange?: (layout: "unified" | "split") => void;
  theme: "light" | "dark";
}): React.ReactElement {
  const t = useT();
  const label = t("viewer.contents", { name });
  const marked = useMemo(() => withoutNoNewlineMarkers(patch), [patch]);
  const asText = (reason: string) => (
    <>
      <Notice>{reason}</Notice>
      <PlainText text={patch} label={label} />
    </>
  );
  if (diffPlan(patch) === "patch") return asText(t("viewer.change.patchLarge"));
  const ending = marked.old && marked.new ? "viewer.change.noNewlineBoth" : marked.old ? "viewer.change.noNewlineOld" : marked.new ? "viewer.change.noNewlineNew" : undefined;
  return (
    <RendererBoundary resetKey={resetKey} fallback={asText(t("viewer.change.patchFailed"))}>
      {onLayoutChange && <LayoutToggle layout={layout} onLayoutChange={onLayoutChange} />}
      {ending && <Notice>{t(ending)}</Notice>}
      <CodeFrame label={label}>
        <RenderedDiff name={name} patch={marked.patch} layout={layout} theme={theme} />
      </CodeFrame>
    </RendererBoundary>
  );
}
