import React, { useEffect, useState } from "react";

import { useFadeRegion, useScrollFade } from "../components/scrollFade";
import { ActionableError } from "../errorRecovery";
import { useT } from "../i18n/react";
import { InlineNotice } from "./inlineNotice";
import { Notice } from "./viewerStates";

const SOURCE = "font-mono text-[0.9em] break-words whitespace-pre-wrap";

/**
 * Options for a formula the document did not write. `trust` is false, so a command that would
 * fetch or link is drawn as its own source in the error colour and is not a link or an image;
 * that does not throw, so the formula still stands. `throwOnError` rejects a formula that cannot
 * be parsed, which is shown as its source. `maxExpand` and `maxSize` are finite so one formula
 * cannot grow without bound. The MathML is the spoken reading; the HTML is only the glyphs.
 */
const FORMULA_OPTIONS = {
  throwOnError: true,
  trust: false,
  output: "htmlAndMathml" as const,
  strict: "ignore" as const,
  maxExpand: 100,
  maxSize: 500,
};

/**
 * One formula. The library loads the first time a formula is drawn. An inline formula is a
 * left-to-right isolate in its paragraph. It stays a `span`, since a `div` there would sit inside
 * a `p`, and it becomes a named region only while it is wider than the line, so that overflow can
 * be scrolled from the keyboard. A display formula on its own line is always a named region, and
 * scrolls sideways inside it. The fade is a mask on that scroller, so the focus ring is drawn on
 * the element around it.
 */
export function FormulaView({
  source,
  display,
  inline,
  overBudget,
}: {
  source: string;
  /** Drawn as display mathematics. `$$` in a paragraph is display and still inline in the tree. */
  display: boolean;
  /** Inside a paragraph, so the failure and the budget notice have to be spans. */
  inline: boolean;
  overBudget: boolean;
}): React.ReactElement {
  const t = useT();
  const [drawn, setDrawn] = useState<{ source: string; html?: string; failure?: { error: unknown } }>();
  useEffect(() => {
    if (overBudget) return;
    let live = true;
    void import("katex")
      .then(async (katex) => {
        await import("katex/dist/katex.min.css");
        try {
          return katex.default.renderToString(source, { ...FORMULA_OPTIONS, displayMode: display });
        } catch (error) {
          if (error instanceof katex.default.ParseError) throw new ActionableError(error.message, "contentSyntax", { cause: error });
          throw error;
        }
      })
      .then(
        (html) => {
          if (live) setDrawn({ source, html });
        },
        (error: unknown) => {
          if (live) setDrawn({ source, failure: { error } });
        },
      );
    return () => {
      live = false;
    };
  }, [source, display, overBudget]);

  const current = drawn?.source === source ? drawn : undefined;
  if (overBudget) return <FormulaFallback source={source} inline={inline} notice={t("viewer.markdown.large")} />;
  if (current?.failure) {
    return <FormulaFallback source={source} inline={inline} notice={t("viewer.markdown.formulaFailed")} failure={current.failure} />;
  }
  if (!current?.html) return <FormulaSource source={source} inline={inline} />;
  if (!inline) return <FormulaRegion html={current.html} />;
  return <InlineFormula html={current.html} />;
}

function FormulaSource({ source, inline }: { source: string; inline: boolean }): React.ReactElement {
  if (inline) {
    return (
      <code dir="ltr" className={`rounded bg-current/10 px-1 py-0.5 ${SOURCE}`}>
        {source}
      </code>
    );
  }
  return (
    <pre dir="ltr" className={`p-3 ${SOURCE}`}>
      <code>{source}</code>
    </pre>
  );
}

function FormulaFallback({
  source,
  inline,
  notice,
  failure,
}: {
  source: string;
  inline: boolean;
  notice: string;
  failure?: { error: unknown };
}): React.ReactElement {
  const body = <FormulaSource source={source} inline={inline} />;
  if (inline) {
    return (
      <>
        <InlineNotice tone={failure ? "failed" : "explained"} speak={failure ? notice : undefined} error={failure?.error}>
          {notice}
        </InlineNotice>
        {body}
      </>
    );
  }
  return (
    <div className="my-3">
      {failure ? <Notice appearance="document" tone="failed" speak={notice} error={failure.error} /> : <Notice appearance="document">{notice}</Notice>}
      {body}
    </div>
  );
}

/** A formula wider than its line, still inside the paragraph. A stop only while it overflows. */
function InlineFormula({ html }: { html: string }): React.ReactElement {
  const t = useT();
  const region = useFadeRegion({ label: t("viewer.markdown.formula") });
  const fade = useScrollFade<HTMLSpanElement>({
    orientation: "horizontal",
    region,
    className: "[unicode-bidi:isolate] inline-block max-w-full overflow-auto align-middle",
  });
  return (
    <span className={region.focused ? "ring-2 ring-app-focus" : undefined}>
      <span dir="ltr" {...fade} dangerouslySetInnerHTML={{ __html: html }} />
    </span>
  );
}

/** A display formula on its own line. Named, and reached from the keyboard so it can be scrolled. */
function FormulaRegion({ html }: { html: string }): React.ReactElement {
  const t = useT();
  const region = useFadeRegion({ label: t("viewer.markdown.formula"), always: true });
  const fade = useScrollFade({
    orientation: "horizontal",
    region,
    className: "max-w-full overflow-auto rounded-sm",
  });
  return (
    <div className={`my-3 rounded-sm ${region.focused ? "ring-2 ring-app-focus" : ""}`}>
      <div dir="ltr" {...fade} dangerouslySetInnerHTML={{ __html: html }} />
    </div>
  );
}
