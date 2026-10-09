import { useEffect, useRef, useState } from "react";

import { useCurrentLanguage, useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import { useDaemon, useDaemonStore } from "../store";
import { bodyFromFileContent, type ViewerContent, type ViewerSubject } from "../viewer/content";
import { browseFailure } from "./browseError";
import type { ShownFile } from "./tree";

/** How many times a read is asked again when the file changed while it was read. */
const CHANGED_RETRIES = 2;

/** The one slot every viewer read goes in: a window shows one file at a time, whichever project it
 * is in, so a read left behind by a viewer of another project is cancelled too. */
const READ_SLOT = "viewer";

export interface FileReader {
  /** What the viewer shows, or nothing while it is closed. */
  subject?: ViewerSubject;
  /** What is shown of the file, for telling from a listing whether to read it again. */
  shown: ShownFile;
  /** Shows `path` (a wire path), reading it from disk. */
  open: (path: string) => void;
  /** Reads the file shown again, keeping what is shown until the answer is in. */
  reload: () => void;
  close: () => void;
}

type Shown = { path: string; content: ViewerContent; version?: string; changing?: boolean };

/**
 * The file a project's viewer shows, read from the project's directory on demand. One read is out
 * at a time, in one slot, so moving through files quickly cancels the reads left behind in the
 * daemon as well; a reply is taken only for the latest read, of this project and of the path shown.
 * Moving to another file shows it loading, never the previous file's body under its name; a failed
 * read shows why, in place of whatever was shown. A read again that finds the same version keeps
 * what is shown as it is, and one lost with the connection keeps the body shown until the
 * reconnect, which reads the file again.
 *
 * The subject's key is the project, the source (the live file on disk) and the path: what makes it a
 * different subject for the viewer.
 */
export function useFileReader(project: string): FileReader {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [path, setPath] = useState<string>();
  const [content, setContent] = useState<Shown>();
  const live = useRef({ alive: true, sequence: 0, path: undefined as string | undefined });

  const read = (target: string, attempt = 0) => {
    const s = live.current;
    const sequence = ++s.sequence;
    const current = () => s.alive && s.sequence === sequence && s.path === target;
    request({ type: "read_project_file", project, path: target, slot: READ_SLOT })
      .then((reply) => {
        if (!current() || reply.type !== "project_file" || reply.project !== project || reply.path !== target) return;
        const version = reply.source.kind === "live" ? reply.source.version : undefined;
        setContent((shown) =>
          shown?.path === target && shown.content.state === "file" && version !== undefined && shown.version === version
            ? shown
            : { path: target, content: { state: "file", body: bodyFromFileContent(target, reply.file) }, version },
        );
      })
      .catch((err: unknown) => {
        if (!current()) return;
        const state = store.getState();
        const root = state.projects.get(project)?.path;
        const failure = browseFailure(t, language, err, state, root && abbreviateHome(root, state.homeDir));
        if (failure.kind === "superseded") return;
        if (failure.kind === "changed" && attempt < CHANGED_RETRIES) return read(target, attempt + 1);
        const message =
          failure.kind === "failed" ? failure.message : failure.kind === "disconnected" ? t("browser.error.disconnected") : t("browser.error.fileChanged");
        setContent((shown) =>
          failure.kind === "disconnected" && shown?.path === target && shown.content.state === "file"
            ? shown
            : { path: target, content: { state: "error", message }, changing: failure.kind === "changed" },
        );
      });
  };

  const open = (target: string) => {
    live.current.path = target;
    setPath(target);
    setContent((shown) => (shown?.path === target ? shown : { path: target, content: { state: "loading" } }));
    read(target);
  };

  const reload = () => {
    if (live.current.path !== undefined) read(live.current.path);
  };

  const close = () => {
    live.current.path = undefined;
    live.current.sequence += 1;
    setPath(undefined);
    setContent(undefined);
  };

  const firstEpoch = useRef(snapshotEpoch);
  useEffect(() => {
    if (snapshotEpoch !== firstEpoch.current) reload();
  }, [snapshotEpoch]);

  useEffect(() => {
    const s = live.current;
    s.alive = true;
    return () => {
      s.alive = false;
    };
  }, []);

  const shown = path !== undefined && content?.path === path ? content : undefined;
  return {
    subject:
      path === undefined
        ? undefined
        : { key: `${project}\0live\0${path}`, path, content: shown?.content ?? { state: "loading" } },
    shown:
      shown?.content.state === "file" && shown.version !== undefined
        ? { state: "file", version: shown.version }
        : shown?.content.state === "error"
          ? { state: "error", changing: shown.changing }
          : { state: "loading" },
    open,
    reload,
    close,
  };
}
