import React, { useEffect, useState } from "react";

import { useT } from "../i18n/react";
import { useDaemon } from "../store";
import { RENDER_BUDGETS } from "./budgets";
import { bodyFromFileContent } from "./content";
import { readDocumentImage } from "./documentImageReads";
import { SpotNotice } from "./notices";
import { resolveProjectAddress } from "./projectAddress";

type Read = { path: string; url?: string; failed?: boolean; tooLarge?: boolean };

/**
 * An image the document names. Only a path that resolves to a project file is read, in the
 * window's lane for document images, never the viewer's slot, so it does not cancel the document's
 * own read; the read is let go of when the image goes, with its document or its path. The bytes
 * are shown as a `data:` URL the way a file opened as an image is, which does not run a script or
 * fetch anything further. Anything else — an address outside the project, an absolute
 * one, a path that resolves to nothing — is the description alone and is never requested.
 *
 * While the read is out, the description is what is shown, so there is no empty gap. A read that
 * fails, or a file that is not an image, says so once it has failed. One past the byte cap, or a
 * document past its count of images, stays the description.
 */
export function DocumentImage({
  src,
  alt,
  documentPath,
  project,
  overBudget,
}: {
  src?: string;
  alt?: string;
  documentPath: string;
  project?: string;
  overBudget: boolean;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const address = resolveProjectAddress(documentPath, src);
  const path = address.kind === "file" ? address.path : undefined;
  const [read, setRead] = useState<Read>();
  useEffect(() => {
    if (!project || !path || overBudget) return;
    let live = true;
    const reading = new AbortController();
    void readDocumentImage(request, project, path, reading.signal)
      .then((reply) => {
        if (!live || reply.type !== "project_file" || reply.path !== path) return;
        if (reply.file.size > RENDER_BUDGETS.documentImageBytes) {
          setRead({ path, tooLarge: true });
          return;
        }
        const body = bodyFromFileContent(path, reply.file);
        setRead(body.kind === "image" ? { path, url: body.url } : { path, failed: true });
      })
      .catch(() => {
        if (live) setRead({ path, failed: true });
      });
    return () => {
      live = false;
      reading.abort();
    };
  }, [project, path, overBudget, request]);

  const current = read?.path === path ? read : undefined;
  if (current?.url) return <img src={current.url} alt={alt ?? ""} onError={() => setRead({ path: current.path, failed: true })} className="max-w-full h-auto" />;

  const description = alt ? <span>{alt}</span> : null;
  // The count is known as the document is drawn, so that notice is part of the content. A read
  // that fails, or a file that turns out larger than the cap, arrives later and is announced.
  if (overBudget) {
    return (
      <>
        {description}
        <SpotNotice>{t("viewer.markdown.imageLarge")}</SpotNotice>
      </>
    );
  }
  if (!current?.failed && !current?.tooLarge) return <>{description}</>;
  const sentence = current.tooLarge ? t("viewer.markdown.imageTooLarge") : t("viewer.markdown.imageFailed");
  return (
    <>
      {description}
      <SpotNotice tone={current.tooLarge ? "explained" : "failed"} speak>
        {sentence}
      </SpotNotice>
    </>
  );
}
