// The one module that touches the rendering library (`@pierre/diffs`, over Shiki). It is loaded
// lazily by `codeRenderer.tsx`, so the library and its grammars stay out of the startup bundle, and
// nothing outside it sees a library type: callers hand it plain text and patches. The rendered DOM
// is read elsewhere only by `rendererDom.ts`, which cannot live here, and `expansion.ts`, which
// is the part of it that drives the separators of collapsed lines.
import {
  getFiletypeFromFileName,
  preloadHighlighter,
  processFile,
  type FileContents,
  type FileDiffMetadata,
  type PostRenderPhase,
} from "@pierre/diffs";
import { File, FileDiff, WorkerPoolContext } from "@pierre/diffs/react";
import { getOrCreateWorkerPoolSingleton, terminateWorkerPoolSingleton } from "@pierre/diffs/worker";
import DiffsWorker from "@pierre/diffs/worker/worker.js?worker";
import React, { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { RENDER_BUDGETS } from "./budgets";
import { CODE_THEMES } from "./codeTheme";
import type { ChangeBodies } from "./content";
import type { DiffLayout } from "./diffLayout";
import { EXPANSION_STEP, Expansion, type ExpansionStatus, type HunkExpander, type SeparatorLabels } from "./expansion";

// GitHub's high-contrast palettes: of the bundled themes measured, the ones whose every token
// reaches WCAG AA's 4.5:1 on the code background in both appearances.
const THEMES = { light: CODE_THEMES.light.name, dark: CODE_THEMES.dark.name } as const;

/**
 * Diff colours that keep every token at 4.5:1 and mark a change by more than colour. The library
 * marks a changed word with a tint over the changed line's tint, and in the light appearance the
 * palette's grey comments fall short under either tint; so a changed word is underlined instead of
 * tinted in both appearances, and in the light one the line tints are the lightest that still read
 * as a tint. The dark palette meets the ratio on the library's own line tints. The `+` / `-`
 * indicators mark changed lines. The same sheet draws what the viewer adds to the separators of
 * collapsed lines (`expansion.ts`): the whole-file control, and a focus ring.
 */
function diffCss(theme: "light" | "dark"): string {
  const [added, removed] = theme === "light" ? ["#1a7f37", "#cf222e"] : ["#3fb950", "#f85149"];
  const lineTints =
    theme === "light"
      ? `[data-line][data-line-type="change-addition"] { background-color: #ebf7ed !important; }
         [data-line][data-line-type="change-deletion"] { background-color: #fdf0f0 !important; }`
      : "";
  return `${lineTints}
    /* In the code column (\`[data-content]\`), whose parent \`[data-code]\` is the scroll container when
       lines are not wrapped. Sticky, like the gutter the label and chevrons sit in, so it stays at the visible
       end of the bar and not at the end of the longest line. Because it is in normal flow, not
       absolute, it needs a max-content width, an auto start margin to reach the end and the bar's
       full height; and it overlaps the separator's own wrapper only because the library hides that
       wrapper inside \`[data-content]\`, so a library upgrade is checked against that rule.
       The label sits in the gutter, which paints above the code column (\`z-index\`), and the library
       (\`line-info-basic\`) gives the wrapper's first grid column all of the gutter's width and lets the
       label run on in the next one, past the gutter: a label wider than what is left of the bar would
       paint over the control. So the control is held to \`--whole-file-width\`, that next column ends
       that far short of the end of the visible bar, and the label ends in an ellipsis.
       \`[data-code]\` is made a size container so \`cqi\` is that visible width (it has a definite
       width in both layouts), and the track is \`max(0px, calc(…))\` rather than the natural
       \`minmax(0, calc(…))\`, which WebKit — the app's own engine — resolves to 0. The column is
       measured from the gutter's physical left, as the library pins the gutter there; the code frame
       is \`dir="ltr"\`, so the control is at the right.
       The additions side of a split diff shows no label and is left alone. */
    [data-separator] { --whole-file-width: 9em; }
    [data-code] { container-type: inline-size; }
    [data-code]:not([data-additions]) [data-gutter] [data-separator-wrapper] {
      grid-template-columns: 100% max(0px, calc(100cqi - 100% - var(--whole-file-width)));
    }
    [data-gutter] [data-separator-content] { min-width: 0; }
    [data-gutter] [data-unmodified-lines] {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    [data-separator] [data-whole-file] {
      position: sticky;
      inset-inline-end: 0;
      width: max-content;
      max-width: var(--whole-file-width);
      height: 100%;
      margin-inline-start: auto;
      display: flex;
      align-items: center;
      padding-inline: 1ch;
      color: var(--diffs-fg-number);
      cursor: pointer;
      user-select: none;
      white-space: nowrap;
      overflow: hidden;
    }
    [data-separator] [data-whole-file]:hover { text-decoration: underline; }
    [data-expand-button][data-focus-visible] { outline: 2px solid var(--focus); outline-offset: -2px; }
    [data-line-type="change-addition"] [data-diff-span],
    [data-line-type="change-deletion"] [data-diff-span] {
      background-color: transparent;
      text-decoration: underline 2px;
      text-underline-offset: 3px;
    }
    [data-line-type="change-addition"] [data-diff-span] { text-decoration-color: ${added}; }
    [data-line-type="change-deletion"] [data-diff-span] { text-decoration-color: ${removed}; }`;
}

/**
 * Options shared by every rendering. The library's own header is off (the viewer's title names the
 * file), and so is everything interactive it can add inside the code — line selection, hover
 * utilities — so the rendered code holds no control that keyboard access would have to reach, apart
 * from the separators of collapsed lines (`expansion.ts`). Whether long lines wrap is the caller's
 * choice (`overflowOption`), added to these. `disableErrorHandling` makes a failure throw to the
 * caller's error boundary instead of printing the library's English stack trace into the page.
 */
const BASE_OPTIONS = {
  theme: THEMES,
  disableFileHeader: true,
  disableErrorHandling: true,
  tokenizeMaxLineLength: RENDER_BUDGETS.tokenizeLineLength,
} as const;

/** The library's name for the wrap choice. Unwrapped, it scrolls long lines sideways in its own
 * elements inside its shadow root, which cannot take keyboard focus; wrapped, the one scrolling
 * element is the caller's focusable frame. */
const overflowOption = (wrap: boolean) => (wrap ? "wrap" : "scroll");

/**
 * The worker pool. Highlighting runs in workers so a large file cannot stall the window; when they
 * cannot start, the library highlights on the main thread instead, within the same budgets. The
 * library's own provider ends its pool whenever the last renderer using it unmounts, which would
 * restart the workers and reload every theme and grammar each time the viewer moved from code to
 * an image and back; so the pool is created once here and ended only by `endRendererPool`, when
 * the last viewer closes.
 */
function rendererPool(): ReturnType<typeof getOrCreateWorkerPoolSingleton> {
  return getOrCreateWorkerPoolSingleton({
    poolOptions: { workerFactory: () => new DiffsWorker(), poolSize: 2 },
    highlighterOptions: { theme: THEMES, langs: [] },
  });
}

export function endRendererPool(): void {
  terminateWorkerPoolSingleton();
}

/** The grammar a Markdown fence's info word names, which is a file extension (`ts`, `sh`) as often
 * as a grammar's own name (`typescript`); without one, or one the library does not know, plain
 * text (a name it knows nothing of is told apart only by the grammar failing to load). */
function languageOfHint(hint: string): string {
  const word = hint.trim().toLowerCase();
  if (word === "") return "text";
  const byExtension = getFiletypeFromFileName(`code.${word}`);
  return byExtension === "text" ? word : byExtension;
}

/** Resolves the grammar for `name`, and the themes, before anything is rendered with them. One
 * that fails to load (a chunk that cannot be fetched, a grammar that does not compile under the
 * JavaScript regex engine) would otherwise leave the library waiting on a rejected promise with
 * nothing on screen; here it turns into plain text. */
function useLanguage(name: string, hint?: string): { lang: FileContents["lang"]; failed: boolean } | undefined {
  const detected = hint === undefined ? getFiletypeFromFileName(name) : languageOfHint(hint);
  const [result, setResult] = useState<{ for: string; lang: FileContents["lang"]; failed: boolean }>();
  useEffect(() => {
    let current = true;
    preloadHighlighter({ themes: [THEMES.light, THEMES.dark], langs: [detected] }).then(
      () => current && setResult({ for: detected, lang: detected, failed: false }),
      (error: unknown) => {
        console.warn(`Viewer: cannot highlight ${detected}, showing plain text`, error);
        if (current) setResult({ for: detected, lang: "text", failed: true });
      },
    );
    return () => {
      current = false;
    };
  }, [detected]);
  return result?.for === detected ? result : undefined;
}

/** The library's post-render callback, telling `onDrawn` each time code is in its DOM: the library
 * also reports a render of an empty frame while its worker pool starts, which does not count. Stable
 * for the life of the component, so it never changes the options object the library compares. */
function useDrawnCallback<Instance = unknown>(
  onDrawn: (() => void) | undefined,
  /** Also told of every drawing, with the library's instance. */
  onRender?: (node: HTMLElement, instance: Instance, phase: PostRenderPhase) => void,
): (node: HTMLElement, instance: Instance, phase: PostRenderPhase) => void {
  const latest = useRef({ onDrawn, onRender });
  latest.current = { onDrawn, onRender };
  return useCallback((node, instance, phase) => {
    latest.current.onRender?.(node, instance, phase);
    if (phase !== "unmount" && (node.shadowRoot ?? node).querySelector("[data-code]")) latest.current.onDrawn?.();
  }, []);
}

export interface FileRenderProps {
  /** The file's name, which picks the grammar. */
  name: string;
  text: string;
  theme: "light" | "dark";
  /** Whether long lines wrap. */
  wrap: boolean;
  /** The grammar by the info word of a Markdown fence, in place of the one `name` picks. */
  language?: string;
  /** Whether lines are numbered; they are unless this is false. */
  lineNumbers?: boolean;
  /** Told whether the file is shown without highlighting because its grammar or a theme failed to
   * load; the caller passes a new callback for each file, so a second file in the same failing
   * language is reported too. */
  onPlainChange?: (plain: boolean) => void;
  /** Told once the library has put the file's code in its DOM, which can be seconds after it is
   * handed the text while the worker pool starts. */
  onDrawn?: () => void;
}

// Memoised, with every object handed to the library memoised too: the library compares its inputs
// by reference, and a new one re-parses and redraws the whole file, which at the budget's size
// holds the window for most of a second on any unrelated re-render above.
export const HighlightedFile = memo(function HighlightedFile({
  name,
  text,
  theme,
  wrap,
  language: hint,
  lineNumbers = true,
  onPlainChange,
  onDrawn,
}: FileRenderProps): React.ReactElement | null {
  const language = useLanguage(name, hint);
  const failed = language?.failed;
  useEffect(() => {
    if (failed !== undefined) onPlainChange?.(failed);
  }, [failed, onPlainChange]);
  // The library numbers the empty line after a final newline as a line of its own.
  const file = useMemo<FileContents | undefined>(
    () => (language ? { name, contents: text.endsWith("\n") ? text.slice(0, -1) : text, lang: language.lang } : undefined),
    [name, text, language?.lang],
  );
  const onPostRender = useDrawnCallback(onDrawn);
  const options = useMemo(
    () => ({ ...BASE_OPTIONS, themeType: theme, overflow: overflowOption(wrap), disableLineNumbers: !lineNumbers, onPostRender }) as const,
    [theme, wrap, lineNumbers, onPostRender],
  );
  return (
    <WorkerPoolContext.Provider value={rendererPool()}>
      {file && <File file={file} options={options} disableWorkerPool={file.lang === "text"} />}
    </WorkerPoolContext.Provider>
  );
});

export interface DiffRenderProps {
  /** The file's name, which picks the grammar. */
  name: string;
  /** One change's unified patch, as `git` writes it. */
  patch: string;
  layout: DiffLayout;
  theme: "light" | "dark";
  /** Whether long lines wrap. */
  wrap: boolean;
  /** As `FileRenderProps.onDrawn`. */
  onDrawn?: () => void;
  /** Reads both sides' whole text, for expanding the lines the patch collapses; without it the
   * separators only say how many lines they hide. */
  loadBodies?: () => Promise<ChangeBodies>;
  /** What the separators say, in the app's language. */
  labels: SeparatorLabels;
  /** Told how the expansion stands: reading the text, failed, or the change having moved on. */
  onExpansion?: (status: ExpansionStatus) => void;
}

/** A change rendered from its patch. Memoised for the same reason as `HighlightedFile`. */
export const RenderedDiff = memo(function RenderedDiff({
  name,
  patch,
  layout,
  theme,
  wrap,
  onDrawn,
  loadBodies,
  labels,
  onExpansion,
}: DiffRenderProps): React.ReactElement | null {
  const language = useLanguage(name);
  const lang = language?.lang;
  const fileDiff = useMemo<FileDiffMetadata | undefined>(() => {
    if (!lang) return undefined;
    // Throws on a patch it cannot read, which the caller's boundary turns into the raw patch.
    const metadata = processFile(patch, { throwOnError: true });
    if (!metadata) throw new Error("The patch holds no file change");
    return { ...metadata, lang };
  }, [patch, lang]);
  // What the expansion has come to for this diff: the whole file shown, or no more offered.
  const [ended, setEnded] = useState<{ diff: FileDiffMetadata; how: "all" | "stopped" }>();
  const how = ended && ended.diff === fileDiff ? ended.how : undefined;
  // Called back with the latest props, so a new callback each render does not reset the expansion.
  const latest = useRef({ loadBodies, labels, onExpansion });
  latest.current = { loadBodies, labels, onExpansion };
  // Also for a diff that cannot be expanded: its separators are worded in the app's language all the same.
  const expansion = useMemo(
    () =>
      fileDiff
        ? new Expansion({
            load: () => latest.current.loadBodies!(),
            labels: () => latest.current.labels,
            report: (status) => latest.current.onExpansion?.(status),
            showAll: () => setEnded({ diff: fileDiff, how: "all" }),
            stop: () => setEnded({ diff: fileDiff, how: "stopped" }),
          })
        : undefined,
    [fileDiff],
  );
  const expandable = loadBodies !== undefined;
  useEffect(() => () => expansion?.detach(), [expansion]);
  useEffect(() => expansion?.decorate(), [expansion, labels]);
  // A read of the change again that yields the same patch keeps this expansion, so a failure of the
  // earlier read is dropped by hand.
  useEffect(() => expansion?.forgetFailure(), [expansion, loadBodies]);
  const onPostRender = useDrawnCallback<HunkExpander>(onDrawn, (node, instance, phase) => {
    if (phase === "unmount") expansion?.detach();
    else expansion?.attach(node, instance);
  });
  const options = useMemo(
    () =>
      ({
        ...BASE_OPTIONS,
        themeType: theme,
        overflow: overflowOption(wrap),
        diffStyle: layout,
        diffIndicators: "classic",
        hunkSeparators: "line-info-basic",
        lineDiffType: "word",
        unsafeCSS: diffCss(theme),
        expansionLineCount: EXPANSION_STEP,
        expandUnchanged: how === "all",
        loadDiffFiles: expansion && expandable && how !== "stopped" ? expansion.files : undefined,
        onPostRender,
      }) as const,
    [theme, layout, wrap, onPostRender, expansion, expandable, how],
  );
  return (
    <WorkerPoolContext.Provider value={rendererPool()}>
      {fileDiff && <FileDiff fileDiff={fileDiff} options={options} disableWorkerPool={lang === "text"} />}
    </WorkerPoolContext.Provider>
  );
});
