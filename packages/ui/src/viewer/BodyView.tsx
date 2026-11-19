import React, { useContext, useState } from "react";

import { PaneFailure } from "../components/alerts/PaneFailure";
import { useT } from "../i18n/react";
import { CodeSurface } from "./code/CodeSurface";
import type { LineRange } from "./code/lineMarks";
import { isMarkdownName, type ViewerBody } from "./content";
import { ZoomableImage } from "./image/ZoomableImage";
import { MarkdownSurface } from "./markdown/MarkdownSurface";
import { FileNotice } from "./notices";
import { StandInActionContext } from "./standInAction";
import { Unsupported } from "./viewerStates";

/** One file body, whichever kind it is. A Markdown file's text is offered as a document when
 * `asDocument` says so: a whole file shown alone, not a side of a comparison or a conflict body, whose
 * markers are what it is looked at for. `liveFile` says the body is a file on the daemon's disk the viewer shows as a
 * subject of its own: where it cannot be displayed, or is too large, its stand-in offers what the viewer provides to
 * open it elsewhere (`StandInActionContext`). */
export function BodyView({
  resetKey,
  name,
  body,
  theme,
  asDocument = false,
  liveFile = false,
  line,
  marks,
}: {
  resetKey: string;
  name: string;
  body: ViewerBody;
  theme: "light" | "dark";
  asDocument?: boolean;
  liveFile?: boolean;
  /** A line of a text file to scroll to and mark, and ranges of its text to mark. */
  line?: number;
  marks?: readonly LineRange[];
}): React.ReactElement {
  const t = useT();
  const offered = useContext(StandInActionContext);
  const action = liveFile ? offered : undefined;
  switch (body.kind) {
    case "text":
      if (asDocument && isMarkdownName(name)) return <MarkdownSurface resetKey={resetKey} name={name} text={body.text} theme={theme} line={line} marks={marks} />;
      return <CodeSurface resetKey={resetKey} name={name} text={body.text} theme={theme} line={line} marks={marks} />;
    case "image":
      return <ImageView key={resetKey} name={name} body={body} theme={theme} />;
    case "binary":
      return <Unsupported message={t("viewer.binary")} sizes={[body.size]} action={action} name={name} />;
    case "tooLarge":
      return <Unsupported message={t("viewer.tooLarge")} sizes={[body.size]} action={action} name={name} />;
  }
}

/** An image the reader can zoom, opened scaled down to fit, never up. One that cannot be decoded
 * falls back to its own text when it has some (an SVG), and otherwise is an error in place of the
 * image, since the pane's subject failed to load. Keyed by its subject by the caller, so a failure
 * is never carried over to the next image, nor a zoom to the next file. */
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
  return <ZoomableImage url={body.url} name={name} onError={() => setFailed(true)} />;
}
