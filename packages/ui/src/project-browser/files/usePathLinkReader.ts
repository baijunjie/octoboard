import { useEffect, useRef, useState } from "react";

import { BROWSE_SLOTS } from "../../browse/budget";
import { useLatestRequest } from "../../components/hooks/useLatestRequest";
import { useDaemon } from "../../store";
import { bodyFromFileContent, tooLargeBody, type ViewerContent, type ViewerSubject } from "../../viewer/content";

export interface PathLinkReader {
  /** What the viewer shows, or nothing before the first `open`. */
  subject?: ViewerSubject;
  /** Shows `path`, as a terminal wrote it, reading it from the daemon's host. */
  open: (path: string) => void;
}

type Shown = { content: ViewerContent; version?: string };

const LOADING: Shown = { content: { state: "loading" } };

/**
 * The file a terminal's path link names, read through the daemon (`read_path_link`) when it is not in a project's
 * directory (anywhere else on the daemon's host). It reads as `useFileReader` does: one read at a time, in the window's viewer slot, a
 * reply taken only for the latest read and the path shown, a read again that finds the same version keeps what is
 * shown, and one lost with the connection is read again at the reconnect. `project` is the session's, if it has one;
 * the daemon tells a project kept under a temporary directory from scratch space with it.
 *
 * The subject's path is the path as the terminal wrote it, and `linkHost` says it is one of the daemon's host.
 */
export function usePathLinkReader(project: string | undefined): PathLinkReader {
  const { request } = useDaemon();
  const [path, setPath] = useState<string>();
  // The path shown as of the latest open, ahead of the render that shows it.
  const opened = useRef<string | undefined>(undefined);
  const latest = useLatestRequest<Shown>({ scope: path, initial: LOADING, project, onReconnect: () => read() });

  const read = (target = opened.current) => {
    if (target === undefined) return;
    void latest.send({
      scope: target,
      wanted: () => opened.current === target,
      request: () => request({ type: "read_path_link", ...(project !== undefined && { project }), path: target, slot: BROWSE_SLOTS.viewer }),
      accept: (reply) => {
        if (reply.path !== target) return undefined;
        return (shown) =>
          shown.content.state === "file" && shown.version === reply.version
            ? shown
            : { content: { state: "file", body: bodyFromFileContent(target, reply.file) }, version: reply.version };
      },
      disconnected: (shown) => (shown.content.state === "file" ? shown : { content: { state: "disconnected", what: "file" } }),
      failed: (failure, _held, t) =>
        failure.kind === "failed" && failure.tooLarge
          ? { content: { state: "file", body: tooLargeBody(failure.tooLarge.size) } }
          : { content: { state: "error", message: failure.kind === "failed" ? failure.message : t("browser.error.fileChanged") } },
    });
  };

  // The read is sent once the render that names its path is committed: `latest` takes a reply only for the scope it
  // has rendered, which a reply already in hand would otherwise beat.
  const [asks, setAsks] = useState(0);
  useEffect(() => {
    if (asks > 0) read();
  }, [asks]);

  const open = (target: string) => {
    opened.current = target;
    setPath(target);
    latest.hold((shown) => shown, target);
    setAsks((n) => n + 1);
  };

  return {
    subject:
      path === undefined
        ? undefined
        : { key: `${project ?? ""}\0link\0${path}`, path, linkHost: { project, path }, content: latest.value.content },
    open,
  };
}
