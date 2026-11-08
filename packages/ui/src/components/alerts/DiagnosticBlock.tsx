import { createContext, useContext, useLayoutEffect, useRef, useState, type ReactElement } from "react";

import { useT } from "../../i18n/react";
import { CopyButton } from "../controls/CopyButton";
import { CODE_SURFACE_CLASS, CodeBlock } from "../text/CodeBlock";
import { FadedScroller, useFadeRegion } from "../scroll/scrollFade";
import { Surface } from "../../primitives";

/** The id of the summary of the disclosure a block stands in, which names the block's region unless it is given its own
 * `labelledBy`: a block a caller passes as the content of an "Error details" disclosure is labelled by the disclosure
 * without knowing its id. */
export const DiagnosticLabelContext = createContext<string | undefined>(undefined);

/** Raw diagnostics preserve their original line breaks and indentation in every surface. Their height is capped at
 * 40% of the viewport, with both axes scrollable from the named keyboard region. The copy control stays outside the
 * scroller so it is always reachable, never faded, and copies the original text rather than the visible portion. */
export function DiagnosticBlock({ children, labelledBy }: { children: string; labelledBy?: string }): ReactElement {
  const t = useT();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollbarWidth, setScrollbarWidth] = useState(0);
  useLayoutEffect(() => {
    const scroller = scrollRef.current;
    if (!scroller) return;
    const measure = () => setScrollbarWidth(Math.max(0, scroller.offsetWidth - scroller.clientWidth));
    const resize = new ResizeObserver(measure);
    resize.observe(scroller);
    measure();
    return () => resize.disconnect();
  });
  const disclosure = useContext(DiagnosticLabelContext);
  const labelId = labelledBy ?? disclosure;
  const region = useFadeRegion(labelId ? { labelledBy: labelId } : { label: t("error.diagnostics") });
  return (
    <Surface variant="secondary" className={`${CODE_SURFACE_CLASS} relative shrink-0 ${region.ring}`}>
      <FadedScroller
        dir="ltr"
        ref={scrollRef}
        region={region}
        className="min-w-0 max-w-full max-h-[40vh] overflow-auto rounded-md [&>.app-code-block]:pe-7"
      >
        <CodeBlock dir="ltr" wrap={false} flush>{children}</CodeBlock>
      </FadedScroller>
      <div className="absolute top-[calc(var(--spacing)*3+0.5lh)] z-10 -translate-y-1/2 rounded-md bg-surface-secondary" style={{ right: `calc(${scrollbarWidth}px + var(--spacing))` }}>
        <CopyButton text={children} failedText={t("error.copyDetailsFailed")} size="xs" variant="tertiary" className="diagnostic-copy-button rounded-md" />
      </div>
    </Surface>
  );
}
