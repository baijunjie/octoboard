import { useEffect, useRef, useState } from "react";

import type { Translate } from "../i18n/catalog";
import { useCurrentLanguage, useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import type { ChangeSide, ConflictKind, Event, FileContent, SideRead, SideRef } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { bodyFromFileContent, type ViewerChangeSide, type ViewerContent, type ViewerSubject } from "../viewer/content";
import { browseFailure } from "./browseError";
import { CONFLICT_LABELS, SECTION_LABELS, type ChangeItem, type ShownChange } from "./changes";

/** How many times a read is asked again when what it read changed while it was read. */
const CHANGED_RETRIES = 2;

/** The one slot every viewer read goes in, the file reader's included: a window shows one subject
 * at a time, whichever project and mode it is from. */
const READ_SLOT = "viewer";

export interface ChangeReader {
  /** What the viewer shows, or nothing while it is closed. */
  subject?: ViewerSubject;
  /** The change shown, as the list named it. */
  item?: ChangeItem;
  /** What is shown of the change, for telling from a listing whether to read it again. */
  shown: ShownChange;
  /** Shows `item`, reading it from the worktree `worktree` names (none for the project's own). */
  open: (item: ChangeItem, worktree: string | undefined) => void;
  /** Reads the change shown again, keeping what is shown until the answer is in. */
  reload: () => void;
  close: () => void;
}

/** How a conflict reads, from its kind. */
function conflictText(t: Translate, conflict: ConflictKind): string {
  return t(CONFLICT_LABELS[conflict]);
}

function sideRef(side: ChangeSide): SideRef {
  return side.state === "present" ? { state: "present", path: side.path } : side;
}

/** A side the listing named, with no body: what is shown when the change cannot be read. */
function listedSide(side: ChangeSide): ViewerChangeSide {
  switch (side.state) {
    case "present":
      return { state: "present", path: side.path, kind: side.kind };
    case "absent":
      return side;
    case "out_of_scope":
      return { state: "out_of_scope", repositoryPath: side.repository_path };
  }
}

/** A side as the daemon read it. A side outside the project carries no path in the reply, so its
 * repository path is the listing's. */
function readSide(side: SideRead, listed: ChangeSide): ViewerChangeSide {
  switch (side.state) {
    case "present":
      return { state: "present", path: side.path, kind: side.kind, body: side.file ? bodyFromFileContent(side.path, side.file) : undefined };
    case "absent":
      return side;
    case "out_of_scope":
      return { state: "out_of_scope", repositoryPath: listed.state === "out_of_scope" ? listed.repository_path : "" };
  }
}

/** A patch as text. One that is not valid UTF-8 (a file in another encoding) is read with
 * replacement characters: it is shown, never applied. */
function patchText(patch: FileContent): string {
  if (patch.kind === "text") return patch.text ?? "";
  const bytes = Uint8Array.from(atob(patch.data ?? ""), (c) => c.charCodeAt(0));
  return new TextDecoder("utf-8").decode(bytes);
}

type Shown = { key: string; content: ViewerContent; failed?: { changing: boolean } };

/**
 * The change a project's viewer shows in the Git mode, read from its worktree on demand: one read
 * out at a time, in the viewer's slot, so moving through changes quickly cancels the reads left
 * behind in the daemon as well; a reply is taken only for the latest read of the change shown. A
 * change in conflict is no two-sided change, and is shown as its file on disk with its conflict
 * markers. A failed read keeps what the list named of the change — its status and paths — beside
 * why; one lost with the connection keeps what was shown until the reconnect, which reads it again.
 *
 * The subject's key is the project, the worktree and the change's own key (its group and both
 * sides), so a file staged and changed again is two subjects, and never one of another worktree.
 */
export function useChangeReader(project: string): ChangeReader {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [opened, setOpened] = useState<{ item: ChangeItem; worktree: string | undefined }>();
  const [shown, setShown] = useState<Shown>();
  const live = useRef({ alive: true, sequence: 0, opened: undefined as { item: ChangeItem; worktree: string | undefined } | undefined });

  const subjectKey = (item: ChangeItem, worktree: string | undefined) => `${project}\0git\0${worktree ?? ""}\0${item.key}`;

  const read = (item: ChangeItem, worktree: string | undefined, attempt = 0) => {
    const s = live.current;
    const sequence = ++s.sequence;
    const key = subjectKey(item, worktree);
    const current = () => s.alive && s.sequence === sequence && s.opened?.item.key === item.key && s.opened.worktree === worktree;
    const { entry } = item;
    const where = worktree === undefined ? {} : { worktree };
    const sent: Promise<Event> =
      entry.group === "conflicted"
        ? request({ type: "read_project_file", project, path: entry.path, ...where, slot: READ_SLOT })
        : request({
            type: "read_project_change",
            project,
            ...where,
            change: { group: entry.group, old: sideRef(entry.old), new: sideRef(entry.new) },
            slot: READ_SLOT,
          });
    sent
      .then((reply) => {
        if (!current()) return;
        if (entry.group === "conflicted" && reply.type === "project_file" && reply.project === project && reply.path === entry.path) {
          const conflict = conflictText(t, entry.conflict);
          setShown({ key, content: { state: "conflict", conflict, body: bodyFromFileContent(entry.path, reply.file) } });
        } else if (entry.group !== "conflicted" && reply.type === "project_change" && reply.project === project && reply.group === entry.group) {
          const change = {
            old: readSide(reply.old, entry.old),
            new: readSide(reply.new, entry.new),
            patch: reply.patch ? patchText(reply.patch) : undefined,
          };
          setShown({ key, content: { state: "change", change } });
        }
      })
      .catch((err: unknown) => {
        if (!current()) return;
        const state = store.getState();
        const root = state.projects.get(project)?.path;
        const failure = browseFailure(t, language, err, state, root && abbreviateHome(root, state.homeDir));
        if (failure.kind === "superseded") return;
        if (failure.kind === "changed" && attempt < CHANGED_RETRIES) return read(item, worktree, attempt + 1);
        const message =
          failure.kind === "failed" ? failure.message : failure.kind === "disconnected" ? t("browser.error.disconnected") : t("git.error.changeChanging");
        setShown((held) => {
          if (failure.kind === "disconnected" && held?.key === key && !held.failed && held.content.state !== "loading") return held;
          const failed = { changing: failure.kind === "changed" };
          if (entry.group === "conflicted") return { key, failed, content: { state: "conflict", conflict: conflictText(t, entry.conflict), message } };
          return { key, failed, content: { state: "change", change: { old: listedSide(entry.old), new: listedSide(entry.new), unavailable: message } } };
        });
      });
  };

  const open = (item: ChangeItem, worktree: string | undefined) => {
    live.current.opened = { item, worktree };
    setOpened({ item, worktree });
    const key = subjectKey(item, worktree);
    setShown((held) => (held?.key === key ? held : { key, content: { state: "loading" } }));
    read(item, worktree);
  };

  const reload = () => {
    const opened = live.current.opened;
    if (opened) read(opened.item, opened.worktree);
  };

  const close = () => {
    live.current.opened = undefined;
    live.current.sequence += 1;
    setOpened(undefined);
    setShown(undefined);
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

  if (!opened) return { shown: { state: "loading" }, open, reload, close };
  const key = subjectKey(opened.item, opened.worktree);
  const current = shown?.key === key ? shown : undefined;
  const content = current?.content ?? { state: "loading" };
  return {
    subject: { key, path: opened.item.path, source: t(SECTION_LABELS[opened.item.section]), content },
    item: opened.item,
    shown: current?.failed ? { state: "error", changing: current.failed.changing } : content.state === "loading" ? { state: "loading" } : { state: "read" },
    open,
    reload,
    close,
  };
}
