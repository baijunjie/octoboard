import React, { useEffect, useId, useState } from "react";

import { FadeOverflow } from "../../../components/text/FadeOverflow";
import { wireBaseName } from "../../../format/wirePath";
import { useT } from "../../../i18n/react";
import { useOctoboardTheme } from "../../../preferences/theme";
import type { ContentMatch } from "../../../protocol";
import { BodyView } from "../../../viewer/BodyView";
import { CodeSurface } from "../../../viewer/code/CodeSurface";
import { Disconnected, Failure, Loading } from "../../../viewer/viewerStates";
import { useFileReader } from "../useFileReader";
import { matchFolder } from "./matchText";

/** How long the selection must rest on a result in another file before that file is read: moving quickly through the
 * results reads only the file it stops on. A result in the file already shown is shown at once. */
export const PREVIEW_DELAY_MS = 150;

/** The result the preview shows: `selected` once it has rested `PREVIEW_DELAY_MS` on another file, at once in the same
 * file. Until then the result shown before stays, its file, line and marks together, never one file's body under
 * another's line. */
function useShownMatch(selected: ContentMatch | undefined): ContentMatch | undefined {
  const [shown, setShown] = useState(selected);
  useEffect(() => {
    if (selected === shown) return;
    if (selected === undefined || shown === undefined || selected.path === shown.path) {
      setShown(selected);
      return;
    }
    const timer = setTimeout(() => setShown(selected), PREVIEW_DELAY_MS);
    return () => clearTimeout(timer);
  }, [selected, shown]);
  return shown;
}

/**
 * The selected result's file under the result list, read-only: headed by the file's name and folder, shown as code
 * with line numbers, scrolled to the result's line with its matches marked by the daemon's ranges. A Markdown file is
 * shown as its source, since a result's line is a line of the source. The file is read from the project's directory in
 * the window's viewer slot (`useFileReader`), only once the selection has rested on it (`PREVIEW_DELAY_MS`), and kept
 * while the selection moves within it. Nothing is spoken as a file loads: the list's selection is what the reader
 * follows.
 */
export function ContentSearchPreview({ project, selected }: { project: string; selected: ContentMatch | undefined }): React.ReactElement | null {
  const t = useT();
  const { resolved: theme } = useOctoboardTheme();
  const shown = useShownMatch(selected);
  const reader = useFileReader(project);
  const headingId = useId();
  const path = shown?.path;
  useEffect(() => {
    if (path === undefined) reader.close();
    else reader.open(path);
  }, [path]);
  if (!shown) return null;
  // The reader names the new path a render after it is asked to: until then the file is on its way.
  const subject = reader.subject?.path === shown.path ? reader.subject : undefined;
  const content = subject?.content ?? { state: "loading" as const };
  const key = subject?.key ?? shown.path;
  const name = wireBaseName(shown.path);
  const folder = matchFolder(shown.path);
  return (
    <section aria-labelledby={headingId} data-content-search-preview="" className="flex min-h-0 flex-1 flex-col groove-t">
      {/* The heading lays out in the interface's direction, on the start side like the modal's other text; the name and
          the folder each read left to right. */}
      <h3 id={headingId} className="flex min-w-0 shrink-0 items-baseline gap-2 px-6 py-1.5 text-xs font-medium max-tablet:px-3">
        <FadeOverflow as="span" dir="ltr" className="min-w-0 shrink" titleWhenClipped={name}>{name}</FadeOverflow>
        {folder !== "" && <FadeOverflow as="span" dir="ltr" className="min-w-0 shrink-[100] font-normal text-app-muted" titleWhenClipped={folder}>{folder}</FadeOverflow>}
      </h3>
      <div className="flex min-h-0 flex-1 flex-col">
        {content.state === "file" ? (
          content.body.kind === "text" ? (
            <CodeSurface resetKey={key} name={name} text={content.body.text} theme={theme} line={shown.line} marks={shown.ranges} />
          ) : (
            <BodyView resetKey={key} name={name} body={content.body} theme={theme} />
          )
        ) : content.state === "error" ? (
          <Failure title={t("viewer.error")} message={content.message} path={shown.path} />
        ) : content.state === "disconnected" ? (
          <Disconnected what="file" />
        ) : (
          <Loading />
        )}
      </div>
    </section>
  );
}
