import React, { useEffect, useId, useState } from "react";

import { useFadeRegion, useScrollFade } from "../components/scrollFade";
import { diagramReading, renderDiagram } from "./diagramConfig";
import { useT } from "../i18n/react";
import { Notice } from "./viewerStates";

const SOURCE = "p-3 font-mono text-xs leading-5 break-words whitespace-pre-wrap";

/**
 * One `mermaid` fence. The library loads the first time a diagram is drawn. While it is drawing,
 * and when it cannot, the source is shown; a failure is announced because it arrives after the
 * document. A diagram past the budget is source from the first paint, so that notice is not
 * announced on its own.
 *
 * The drawing follows the window's appearance. It is left to right, as code is. Links inside it
 * are not followed: the library is configured not to emit HTML (`diagramConfig`), and a link that
 * is in the drawing anyway is not activated.
 */
export function MermaidBlock({
  source,
  overBudget,
  theme,
}: {
  source: string;
  overBudget: boolean;
  theme: "light" | "dark";
}): React.ReactElement {
  const t = useT();
  const id = `diagram-${useId().replace(/:/g, "")}`;
  const [drawn, setDrawn] = useState<{ source: string; theme: string; svg?: string; failed?: boolean }>();
  useEffect(() => {
    if (overBudget) return;
    let live = true;
    void renderDiagram(id, source, theme).then(
      (svg) => {
        if (live) setDrawn({ source, theme, svg });
      },
      () => {
        if (live) setDrawn({ source, theme, failed: true });
      },
    );
    return () => {
      live = false;
    };
  }, [id, source, theme, overBudget]);

  const current = drawn && drawn.source === source && drawn.theme === theme ? drawn : undefined;
  const plain = (
    <pre dir="ltr" className={SOURCE}>
      <code>{source}</code>
    </pre>
  );
  if (overBudget) {
    return (
      <div className="my-4">
        <Notice appearance="document">{t("viewer.markdown.large")}</Notice>
        {plain}
      </div>
    );
  }
  if (current?.failed) {
    return (
      <div className="my-4">
        <Notice appearance="document" tone="failed" speak={t("viewer.markdown.diagramFailed")} />
        {plain}
      </div>
    );
  }
  if (!current?.svg) return <div className="my-4">{plain}</div>;

  const reading = diagramReading(source);
  const name = reading.title ?? t("viewer.markdown.diagram");
  const alternative = reading.description ?? source;
  return <DrawnDiagram svg={current.svg} name={name} alternative={alternative} />;
}

/**
 * A drawn diagram. The sideways fade is a mask on the scroller, so the focus ring is drawn on the
 * element around it. The region is always a tab stop: reaching the diagram is not only for scrolling.
 */
function DrawnDiagram({ svg, name, alternative }: { svg: string; name: string; alternative: string }): React.ReactElement {
  const region = useFadeRegion({ label: name, always: true });
  const fade = useScrollFade({
    orientation: "horizontal",
    region,
    className: "max-w-full overflow-auto rounded-sm",
  });
  return (
    <div className={`my-4 rounded-sm ${region.focused ? "ring-2 ring-focus" : ""}`}>
      <div dir="ltr" {...fade} onClick={refuseDiagramLink} onAuxClick={refuseDiagramLink} onKeyDown={refuseDiagramLink}>
        <div role="img" aria-label={name} dangerouslySetInnerHTML={{ __html: svg }} />
        <p className="sr-only">{alternative}</p>
      </div>
    </div>
  );
}

/** A link drawn inside a diagram is not followed. Strict mode is what keeps the drawing safe. */
function refuseDiagramLink(event: React.SyntheticEvent): void {
  const target = event.target;
  if (target instanceof Element && target.closest("a")) {
    event.preventDefault();
    event.stopPropagation();
  }
}
