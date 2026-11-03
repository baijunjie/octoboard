import { useRef, useState } from "react";

import { BROWSE_SLOTS } from "../../browse/budget";
import { useLatestRequest } from "../../components/hooks/useLatestRequest";
import type { Event } from "../../protocol";
import { useDaemon } from "../../store";
import { bodyFromFileContent, type ViewerContent, type ViewerSubject } from "../../viewer/content";
import type { Shown as ShownRead } from "../viewedItem";

export interface FileReader {
  /** What the viewer shows, or nothing while it is closed. */
  subject?: ViewerSubject;
  /** What is shown of the file, for telling from a listing whether to read it again. */
  shown: ShownRead;
  /** Shows `path` (a wire path), reading it from disk. With `read`, the reply of an earlier read of
   * it, the file is shown at once from that and not read again until something asks, as a new
   * connection does. */
  open: (path: string, read?: Extract<Event, { type: "project_file" }>) => void;
  /** Reads the file shown again, keeping what is shown until the answer is in. */
  reload: () => void;
  close: () => void;
}

type Shown = { content: ViewerContent; version?: string; changing?: boolean };

const LOADING: Shown = { content: { state: "loading" } };

/**
 * The file a project's viewer shows, read from the project's directory on demand. One read is out
 * at a time, in the window's viewer slot, so moving through files quickly cancels the reads left
 * behind in the daemon as well; a reply is taken only for the latest read, of this project and of
 * the path shown. Moving to another file shows it loading, never the previous file's body under its
 * name; a failed read shows why, in place of whatever was shown. A read again that finds the same
 * version keeps what is shown as it is, and one lost with the connection keeps the body shown until
 * the reconnect, which reads the file again.
 *
 * The subject's key is the project, the source (the live file on disk, in a worktree when it is one
 * that is read) and the path: what makes it a different subject for the viewer.
 */
export function useFileReader(project: string, worktree?: string): FileReader {
  const { request } = useDaemon();
  const [path, setPath] = useState<string>();
  // The path shown as of the latest open or close, ahead of the render that shows it.
  const opened = useRef<string | undefined>(undefined);
  const latest = useLatestRequest<Shown>({
    scope: path,
    initial: LOADING,
    project,
    onReconnect: () => reload(),
  });

  const read = (target: string) => {
    void latest.send({
      scope: target,
      wanted: () => opened.current === target,
      request: () => request({ type: "read_project_file", project, ...(worktree !== undefined && { worktree }), path: target, slot: BROWSE_SLOTS.viewer }),
      accept: (reply) => {
        if (reply.type !== "project_file" || reply.project !== project || reply.path !== target || (reply.worktree ?? undefined) !== worktree) return undefined;
        const version = reply.source.kind === "live" ? reply.source.version : undefined;
        return (shown) =>
          shown.content.state === "file" && version !== undefined && shown.version === version
            ? shown
            : { content: { state: "file", body: bodyFromFileContent(target, reply.file) }, version };
      },
      disconnected: (shown) => (shown.content.state === "file" ? shown : { content: { state: "disconnected", what: "file" } }),
      failed: (failure, _held, t) => ({
        content: { state: "error", message: failure.kind === "failed" ? failure.message : t("browser.error.fileChanged") },
        changing: failure.kind === "changed",
      }),
    });
  };

  const open = (target: string, earlier?: Extract<Event, { type: "project_file" }>) => {
    opened.current = target;
    setPath(target);
    const from = earlier?.path === target && earlier.project === project && (earlier.worktree ?? undefined) === worktree ? earlier : undefined;
    if (from) {
      const version = from.source.kind === "live" ? from.source.version : undefined;
      latest.hold(() => ({ content: { state: "file", body: bodyFromFileContent(target, from.file) }, version }), target);
    } else {
      latest.hold((shown) => shown, target);
      read(target);
    }
  };

  const reload = () => {
    if (opened.current !== undefined) read(opened.current);
  };

  const close = () => {
    opened.current = undefined;
    setPath(undefined);
    latest.reset();
  };

  const shown = path === undefined ? undefined : latest.value;
  return {
    subject:
      path === undefined
        ? undefined
        : { key: `${project}\0live\0${worktree ?? ""}\0${path}`, path, host: { project, worktree, path }, content: shown?.content ?? { state: "loading" } },
    shown:
      shown?.content.state === "file" && shown.version !== undefined
        ? { state: "read", version: shown.version }
        : shown?.content.state === "error" || shown?.content.state === "disconnected"
          ? { state: "error", changing: shown.changing }
          : { state: "loading" },
    open,
    reload,
    close,
  };
}
