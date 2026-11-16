import { useEffect, useRef, useState } from "react";

import { useT } from "../../i18n/react";
import type { RangeFile } from "../../protocol";
import { usePlatform } from "../../platform/react";
import { useDaemon } from "../../store";
import { downloadFile, DownloadCancelled } from "./downloadFile";
import { raiseDownloadFailure } from "./downloadFailure";

/** How far a copy has come: `total` is what the daemon said the file's size is, `null` until the first part is in. */
export type DownloadProgress = { received: number; total: number | null };

export interface FileDownload {
  /** The copy in progress, or nothing while there is none. */
  progress?: DownloadProgress;
  start: () => void;
  /** Ends the copy and removes what had arrived. */
  cancel: () => void;
}

/**
 * Download and open for the viewer's subject: copies the live file to this computer and opens the copy
 * (`downloadFile`). Defined only in a remote window of the desktop shell, for a live file the system could not launch:
 * one this computer would run is neither downloaded nor offered. The copy belongs to the subject it began with, so
 * moving the viewer to another file, closing it or pressing `cancel` ends it. The header's button and the stand-in's
 * share one, so there is one copy at a time. A failure is raised as an error alert; a cancelled copy says nothing.
 */
export function useFileDownload({ key, file, name, launchable }: {
  /** The subject's identity; a new one is another file. */
  key: string;
  /** The live file, or none for a subject that is a change or a committed file. */
  file: RangeFile | undefined;
  name: string;
  /** Whether the system could launch the file, by the daemon's word or by its name. */
  launchable: boolean;
}): FileDownload | undefined {
  const t = useT();
  const { request, isLocal } = useDaemon();
  const { remoteFiles: files, host } = usePlatform();
  const running = useRef<AbortController | null>(null);
  const [held, setHeld] = useState<DownloadProgress & { key: string }>();
  // Ended with its subject, which is another file or none.
  useEffect(
    () => () => {
      running.current?.abort();
      running.current = null;
    },
    [key],
  );
  if (isLocal !== false || !files || !file || launchable) return undefined;
  const start = () => {
    if (running.current) return;
    const abort = new AbortController();
    running.current = abort;
    setHeld({ key, received: 0, total: null });
    void downloadFile({
      file,
      name,
      request,
      files,
      onProgress: (received, total) => {
        if (running.current === abort) setHeld({ key, received, total });
      },
      signal: abort.signal,
    })
      .catch((failure: unknown) => {
        if (!(failure instanceof DownloadCancelled)) raiseDownloadFailure(t, name, failure, host.os);
      })
      .finally(() => {
        if (running.current !== abort) return;
        running.current = null;
        setHeld(undefined);
      });
  };
  // Idle at once; the part still on its way is waited out in the background, and the copy removed.
  const cancel = () => {
    running.current?.abort();
    running.current = null;
    setHeld(undefined);
  };
  return { progress: held?.key === key ? held : undefined, start, cancel };
}
