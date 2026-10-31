import React, { useState } from "react";

import { PaneFailure } from "../components/alerts/PaneFailure";
import { useT } from "../i18n/react";
import { CodeSurface } from "./CodeSurface";
import { isMarkdownName, type ViewerBody } from "./content";
import { FrameSurface } from "./FrameSurface";
import { MarkdownSurface } from "./MarkdownSurface";
import { FileNotice } from "./notices";
import { Unsupported } from "./viewerStates";

/** One file body, whichever kind it is. A Markdown file's text is offered as a document when
 * `asDocument` says so: a whole file shown alone, not a side of a comparison or a conflict body, whose
 * markers are what it is looked at for. */
export function BodyView({
  resetKey,
  name,
  body,
  theme,
  asDocument = false,
  line,
}: {
  resetKey: string;
  name: string;
  body: ViewerBody;
  theme: "light" | "dark";
  asDocument?: boolean;
  line?: number;
}): React.ReactElement {
  const t = useT();
  switch (body.kind) {
    case "text":
      if (asDocument && isMarkdownName(name)) return <MarkdownSurface resetKey={resetKey} name={name} text={body.text} theme={theme} line={line} />;
      return <CodeSurface resetKey={resetKey} name={name} text={body.text} theme={theme} line={line} />;
    case "image":
      return <ImageView key={resetKey} name={name} body={body} theme={theme} />;
    case "binary":
      return <Unsupported message={t("viewer.binary")} sizes={[body.size]} />;
  }
}

/** An image scaled down to fit, never up. One that cannot be decoded falls back to its own text
 * when it has some (an SVG), and otherwise is an error in place of the image, since the pane's
 * subject failed to load. Keyed by its subject by
 * the caller, so a failure is never carried over to the next image. */
function ImageView({
  name,
  body,
  theme,
}: {
  name: string;
  body: Extract<ViewerBody, { kind: "image" }>;
  theme: "light" | "dark";
}): React.ReactElement {
  const t = useT();
  const [failed, setFailed] = useState(false);
  if (failed) {
    return body.text !== undefined ? (
      <>
        {/* The image fails only once it has been fetched and decoded, long after the dialog was
            drawn, so the fall back to its text, like the mark it brings, is spoken as well as shown. */}
        <FileNotice tone="failed" sentence={t("viewer.image.asText")} speak />
        <CodeSurface resetKey={body.url} name={name} text={body.text} theme={theme} />
      </>
    ) : (
      <PaneFailure announce title={t("viewer.image.failed")} summary={t("viewer.image.failed.summary")} remedy={t("viewer.image.failed.remedy")} />
    );
  }
  return (
    <FrameSurface>
      <div className="viewer-checkerboard flex min-h-0 flex-1 items-center justify-center overflow-hidden p-2">
        {/* The `error` event is the signal: an image's size is no test (an SVG with only a `viewBox`
            has none of its own) and `decode()` has wrongly rejected valid SVGs in older WebKit. */}
        <img src={body.url} alt={name} onError={() => setFailed(true)} className="max-h-full max-w-full object-contain" />
      </div>
    </FrameSurface>
  );
}
