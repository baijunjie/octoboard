import React, { Suspense, useCallback, useState } from "react";

import { trackSuspenseFallback } from "../../components/hooks/lazyReadiness";
import { FadedScroller, useFadeRegion } from "../../components/scroll/scrollFade";
import { CODE_THEMES } from "../code/codeTheme";
import { HighlightedFile, RendererBoundary } from "../code/codeRenderer";

/** The end padding of a scrolling block's content: room at its end for something laid over the block's end edge, which
 * the longest line can be scrolled clear of. */
const END_INSET = "pe-9";

/**
 * A fenced block of code inside a rendered document, highlighted as the source view highlights code
 * and always wrapped: the document's frame is the one thing that scrolls, so there is no scrolling
 * region of its own for the keyboard to reach. It is its plain text until the highlighting is drawn,
 * and stays so where it cannot be: when the renderer fails, when the document is past its budget of
 * highlighted blocks (`highlight` false), or when there is nothing to draw (the renderer reports no
 * drawing for an empty block, so the plain text would never be taken away).
 *
 * The plain text is in the block's flow rather than laid over it as the code frame's loading state
 * is: it has a height of its own for the block to keep while the renderer starts, and there is no
 * telling a block still being drawn from one that failed.
 */
export function FencedCode({
  language,
  text,
  theme,
  highlight,
  scroll,
  label,
}: {
  language?: string;
  text: string;
  theme: "light" | "dark";
  highlight: boolean;
  /** Whether long lines scroll the block sideways instead of wrapping. The block's end then has room for something
   * laid over it (`END_INSET`), which the longest line can be scrolled clear of. */
  scroll?: boolean;
  /** The name of the scrolling region, with `scroll`. */
  label?: string;
}): React.ReactElement {
  const [drawn, setDrawn] = useState(false);
  const onDrawn = useCallback(() => setDrawn(true), []);
  const colours = CODE_THEMES[theme];
  const plain = (
    <pre className={scroll ? `w-max min-w-full p-3 ${END_INSET} font-mono text-xs leading-5` : "p-3 font-mono text-xs leading-5 break-words whitespace-pre-wrap"}>
      <code>{text}</code>
    </pre>
  );
  const drawable = highlight && text.trim() !== "";
  const region = useFadeRegion({ label: label ?? "" });
  const content = (
    <>
      {(!drawable || !drawn) && plain}
      {drawable && (
        <RendererBoundary resetKey={text} fallback={drawn ? plain : null}>
          <Suspense fallback={trackSuspenseFallback(null)}>
            <HighlightedFile name="code" language={language} lineNumbers={false} text={text} theme={theme} wrap={!scroll} onDrawn={onDrawn} />
          </Suspense>
        </RendererBoundary>
      )}
    </>
  );
  const surface = { backgroundColor: colours.background, color: colours.foreground };
  if (scroll) {
    // The frame is around the scroller, whose fade would otherwise take the border and fill with it. The library's own
    // scroller is told not to scroll (`--diffs-overflow-override`) and its element is as wide as its longest line, so the
    // block's scroller is the one that scrolls.
    return (
      <div
        dir="ltr"
        style={{ ...surface, "--diffs-overflow-override": "visible" } as React.CSSProperties}
        className={`overflow-hidden rounded-lg border border-current/25 ${region.ring} [&_diffs-container]:box-border [&_diffs-container]:block [&_diffs-container]:w-max [&_diffs-container]:min-w-full [&_diffs-container]:pe-9`}
      >
        <FadedScroller orientation="horizontal" region={region}>{content}</FadedScroller>
      </div>
    );
  }
  return (
    <div dir="ltr" style={surface} className="overflow-hidden rounded-lg border border-current/25">
      {content}
    </div>
  );
}
