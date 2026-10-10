import React, { Suspense, useCallback, useState } from "react";

import { CODE_THEMES } from "./codeTheme";
import { HighlightedFile, RendererBoundary } from "./codeRenderer";

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
}: {
  language?: string;
  text: string;
  theme: "light" | "dark";
  highlight: boolean;
}): React.ReactElement {
  const [drawn, setDrawn] = useState(false);
  const onDrawn = useCallback(() => setDrawn(true), []);
  const colours = CODE_THEMES[theme];
  const plain = (
    <pre className="p-3 font-mono text-xs leading-5 break-words whitespace-pre-wrap">
      <code>{text}</code>
    </pre>
  );
  const drawable = highlight && text.trim() !== "";
  return (
    <div
      dir="ltr"
      style={{ backgroundColor: colours.background, color: colours.foreground }}
      className="overflow-hidden rounded-lg border border-current/25"
    >
      {(!drawable || !drawn) && plain}
      {drawable && (
        <RendererBoundary resetKey={text} fallback={drawn ? plain : null}>
          <Suspense fallback={null}>
            <HighlightedFile name="code" language={language} lineNumbers={false} text={text} theme={theme} wrap onDrawn={onDrawn} />
          </Suspense>
        </RendererBoundary>
      )}
    </div>
  );
}
