import React, { Suspense, useCallback, useContext } from "react";

import { trackSuspenseFallback } from "../../components/hooks/lazyReadiness";
import { retryableLazy } from "../../components/hooks/retryableLazy";
import { usePlatform } from "../../platform/react";
import { useOctoboardTheme } from "../../preferences/theme";
import { RendererBoundary } from "../../viewer/code/codeRenderer";
import { FencedCode } from "../../viewer/markdown/FencedCode";
import { HeadingLevelContext } from "./headingLevel";

// The Markdown renderer, with its library, loads the first time a page shows a block of it.
const markdownDocument = retryableLazy(() => import("../../viewer/markdown/MarkdownDocument").then((module) => ({ default: module.MarkdownDocument })));
const MarkdownDocument = markdownDocument.Component;

/**
 * A page's Markdown block, drawn by the application's own Markdown renderer in its `untrusted` mode: raw HTML stays
 * the text it was written as (shown, not dropped), an image comes only from a `data:` address (any other stays its
 * description, and nothing is requested for it), a relative link or a link to a heading is plain text and an absolute
 * web or mail link opens in the system browser. Its headings start one level below the heading the block sits under.
 * The page has no say over what the window loads. While the renderer loads, and where it cannot, the text
 * itself is shown.
 */
export function PageMarkdown({ text }: { text: string }): React.ReactElement {
  const { resolved: theme } = useOctoboardTheme();
  const headingLevel = Math.min(useContext(HeadingLevelContext) + 1, 6) as 4 | 5 | 6;
  const links = usePlatform().links;
  const onOpenLink = useCallback((url: string) => links?.open(url), [links]);
  const renderCode = useCallback(
    ({ language, text: code, highlight }: { language?: string; text: string; highlight: boolean }) => (
      <FencedCode language={language} text={code} theme={theme} highlight={highlight} />
    ),
    [theme],
  );
  const plain = <p dir="auto" className="text-sm leading-normal align-match-parent break-words whitespace-pre-wrap">{text}</p>;
  return (
    <RendererBoundary resetKey={text} fallback={plain} retry={markdownDocument.retry}>
      <Suspense fallback={trackSuspenseFallback(plain)}>
        <div className="report-markdown text-sm leading-normal">
          <MarkdownDocument embedded headingLevel={headingLevel} untrusted text={text} path="" theme={theme} renderCode={renderCode} onOpenLink={links && onOpenLink} />
        </div>
      </Suspense>
    </RendererBoundary>
  );
}
