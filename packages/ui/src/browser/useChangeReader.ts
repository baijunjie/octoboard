import { createElement, useEffect, useRef, useState } from "react";

import { DaemonRequestError } from "../daemon-client";
import type { Translate } from "../i18n/catalog";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import type {
  ChangeSide,
  ComparisonEndpoint,
  ConflictKind,
  Event,
  FileContent,
  SideRead,
  SideRef,
  SideVersion,
} from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import {
  bodyFromFileContent,
  ChangeBodiesError,
  type ChangeBodies,
  type ViewerChangeSide,
  type ViewerContent,
  type ViewerSubject,
} from "../viewer/content";
import { displayWirePath } from "../wirePath";
import { browseFailure } from "./browseError";
import { CONFLICT_LABELS, SECTION_LABELS, type ChangeItem, type ShownChange } from "./changes";

/** How many times a read is asked again when what it read changed while it was read. */
const CHANGED_RETRIES = 2;

/** The one slot every viewer read goes in, the file reader's included: a window shows one subject
 * at a time, whichever project and mode it is from. */
const READ_SLOT = "viewer";

/** The slot of the request for a change's whole bodies, apart from `READ_SLOT` so that a click on a
 * separator never cancels the read of a patch still on its way (a background listing asks for one
 * while the viewer is open). The daemon's bound of outstanding requests counts it among the
 * viewer's two (see `useDirectoryListings.ts`). */
const BODIES_SLOT = "viewer-bodies";

/** Where a change is read from: a worktree's uncommitted changes (`worktree` none for the one
 * holding the project's directory), or a comparison of two branches at the commits it resolved. */
export type ChangeOrigin =
  | { kind: "worktree"; worktree: string | undefined }
  | { kind: "comparison"; left: ComparisonEndpoint; right: ComparisonEndpoint };

function sameOrigin(a: ChangeOrigin, b: ChangeOrigin): boolean {
  if (a.kind === "worktree") return b.kind === "worktree" && a.worktree === b.worktree;
  const same = (x: ComparisonEndpoint, y: ComparisonEndpoint) => x.branch === y.branch && x.commit === y.commit;
  return b.kind === "comparison" && same(a.left, b.left) && same(a.right, b.right);
}

/** A commit as the UI names it: the first seven characters of its id, as `git` abbreviates one. */
export function shortCommit(commit: string): string {
  return commit.slice(0, 7);
}

export interface ChangeReader {
  /** What the viewer shows, or nothing while it is closed. */
  subject?: ViewerSubject;
  /** The change shown, as the list named it. */
  item?: ChangeItem;
  /** Where the change shown is read from. */
  origin?: ChangeOrigin;
  /** What is shown of the change, for telling from a listing whether to read it again. */
  shown: ShownChange;
  /** Shows `item`, reading it from `origin`; opening the change already shown keeps what is shown
   * until the answer is in, as `reload` does. */
  open: (item: ChangeItem, origin: ChangeOrigin) => void;
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

/** A patch or a body as text. One that is not valid UTF-8 (a file in another encoding) is read with
 * replacement characters, patch and bodies alike so their lines agree: it is shown, never applied. */
function fileText(content: FileContent): string {
  if (content.kind === "text") return content.text ?? "";
  const bytes = Uint8Array.from(atob(content.data ?? ""), (c) => c.charCodeAt(0));
  return new TextDecoder("utf-8").decode(bytes);
}

/** The version a patch reply reported for a side that is a file, for the request that reads that
 * side's whole body; nothing for a side that is not (one that does not exist, a link, a submodule,
 * one outside the project). */
function fileSideVersion(side: SideRead): SideVersion | undefined {
  return side.state === "present" && side.kind === "file" ? { state: "present", source: side.source } : undefined;
}

/** The budgets whose overrun means the bodies can never be had, as opposed to a read that failed. */
const BODY_LIMITS = new Set(["file_bytes", "patch_bytes", "reply_bytes"]);

type Shown = { key: string; content: ViewerContent; failed?: { changing: boolean } };
type Opened = { item: ChangeItem; origin: ChangeOrigin };

/**
 * The change a project's viewer shows in the Git mode, read on demand from its worktree or from the
 * two commits of a branch comparison: one read out at a time, in the viewer's slot, so moving
 * through changes quickly cancels the reads left behind in the daemon as well; a reply is taken
 * only for the latest read of the change shown, and only for the worktree or the two commits it was
 * asked of. A change in conflict is no two-sided change, and is shown as its file on disk with its
 * conflict markers. A read that fails for another reason than the connection keeps what the list
 * named of the change — its status and paths — beside why; one lost with the connection keeps what
 * was shown until the reconnect, which reads it again from the same place, and says only that the
 * connection is lost when nothing was shown yet.
 *
 * The subject's key is the project, where the change is read from (the worktree, or the two
 * commits) and the change's own key (its group and both sides), so a file staged and changed again
 * is two subjects, and never one of another worktree or another pair of commits.
 */
export function useChangeReader(project: string): ChangeReader {
  const t = useT();
  const language = useCurrentLanguage();
  // The wording a failure is given when it happens, not when the patch it belongs to arrived: a
  // loader lives as long as its patch is shown, and the language can change in between.
  const wording = useRef({ t, language });
  wording.current = { t, language };
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [opened, setOpened] = useState<Opened>();
  const [shown, setShown] = useState<Shown>();
  const live = useRef({ alive: true, sequence: 0, opened: undefined as Opened | undefined });

  const subjectKey = (item: ChangeItem, origin: ChangeOrigin) =>
    origin.kind === "worktree"
      ? `${project}\0git\0${origin.worktree ?? ""}\0${item.key}`
      : `${project}\0compare\0${origin.left.commit}\0${origin.right.commit}\0${item.key}`;

  const send = (item: ChangeItem, origin: ChangeOrigin): Promise<Event> => {
    const { entry } = item;
    const where = origin.kind === "worktree" && origin.worktree !== undefined ? { worktree: origin.worktree } : {};
    if (entry.group === "conflicted") return request({ type: "read_project_file", project, path: entry.path, ...where, slot: READ_SLOT });
    const sides = { old: sideRef(entry.old), new: sideRef(entry.new) };
    if (entry.group !== "committed") {
      return request({ type: "read_project_change", project, ...where, change: { group: entry.group, ...sides }, slot: READ_SLOT });
    }
    if (origin.kind !== "comparison") return Promise.reject(new Error("a comparison's change is read from its two commits"));
    const { left, right } = origin;
    return request({ type: "read_project_comparison_change", project, left, right, change: sides, slot: READ_SLOT });
  };

  /** Whether `reply` answers the read of `item` from `origin`. */
  const answers = (reply: Event, item: ChangeItem, origin: ChangeOrigin): boolean => {
    const { entry } = item;
    if (entry.group === "conflicted") return reply.type === "project_file" && reply.project === project && reply.path === entry.path;
    if (origin.kind === "comparison") {
      return (
        reply.type === "project_comparison_change" &&
        reply.project === project &&
        sameOrigin({ kind: "comparison", left: reply.left, right: reply.right }, origin)
      );
    }
    return reply.type === "project_change" && reply.project === project && reply.group === entry.group;
  };

  /**
   * How the change whose patch `reply` is gets its whole bodies, for expanding the lines the patch
   * collapses: nothing where there is nothing to expand them from (a side that does not exist, or
   * is not a file). The request names the versions `reply` reported, so a body of another revision
   * is refused rather than shown beside this patch. It goes in a slot of its own, so a newer
   * request for the bodies is the only one it cancels.
   */
  const bodiesLoader = (
    item: ChangeItem,
    origin: ChangeOrigin,
    reply: Extract<Event, { type: "project_change" | "project_comparison_change" }>,
  ): (() => Promise<ChangeBodies>) | undefined => {
    const { entry } = item;
    const old = fileSideVersion(reply.old);
    const next = fileSideVersion(reply.new);
    if (entry.group === "conflicted" || !old || !next) return undefined;
    const sides = { old: sideRef(entry.old), new: sideRef(entry.new) };
    const versions = { old, new: next };
    const where = origin.kind === "worktree" && origin.worktree !== undefined ? { worktree: origin.worktree } : {};
    const text = (side: SideRead) => (side.state === "present" && side.file ? fileText(side.file) : undefined);
    return async () => {
      try {
        let bodies: Event;
        if (entry.group !== "committed") {
          bodies = await request({
            type: "read_project_change_bodies",
            project,
            ...where,
            change: { group: entry.group, ...sides },
            ...versions,
            slot: BODIES_SLOT,
          });
        } else if (origin.kind === "comparison") {
          const { left, right } = origin;
          bodies = await request({ type: "read_project_comparison_change_bodies", project, left, right, change: sides, ...versions, slot: BODIES_SLOT });
        } else {
          throw new ChangeBodiesError("unavailable");
        }
        if (bodies.type !== "project_change_bodies" && bodies.type !== "project_comparison_change_bodies") throw new ChangeBodiesError("unavailable");
        const oldText = text(bodies.old);
        const newText = text(bodies.new);
        if (oldText === undefined || newText === undefined) throw new ChangeBodiesError("unavailable");
        return { old: oldText, new: newText };
      } catch (err) {
        if (err instanceof ChangeBodiesError) throw err;
        if (err instanceof DaemonRequestError && ["unsupported_file_type", "invalid_change"].includes(err.code)) throw new ChangeBodiesError("unavailable");
        if (err instanceof DaemonRequestError && err.code === "limit_exceeded" && BODY_LIMITS.has(err.params.limit ?? "")) {
          throw new ChangeBodiesError("unavailable");
        }
        const state = store.getState();
        const root = state.projects.get(project)?.path;
        const { t, language } = wording.current;
        const failure = browseFailure(t, language, err, state, root && abbreviateHome(root, state.homeDir));
        switch (failure.kind) {
          case "superseded":
            throw new ChangeBodiesError("superseded");
          case "changed":
            throw new ChangeBodiesError("changed");
          case "disconnected":
            throw new ChangeBodiesError("failed", t("viewer.expand.disconnected"));
          case "failed":
            throw new ChangeBodiesError("failed", failure.message);
        }
      }
    };
  };

  const read = (item: ChangeItem, origin: ChangeOrigin, attempt = 0) => {
    const s = live.current;
    const sequence = ++s.sequence;
    const key = subjectKey(item, origin);
    const current = () => s.alive && s.sequence === sequence && s.opened?.item.key === item.key && sameOrigin(s.opened.origin, origin);
    const { entry } = item;
    send(item, origin)
      .then((reply) => {
        if (!current() || !answers(reply, item, origin)) return;
        if (entry.group === "conflicted" && reply.type === "project_file") {
          const conflict = conflictText(t, entry.conflict);
          setShown({ key, content: { state: "conflict", conflict, body: bodyFromFileContent(entry.path, reply.file) } });
        } else if (entry.group !== "conflicted" && (reply.type === "project_change" || reply.type === "project_comparison_change")) {
          const change = {
            old: readSide(reply.old, entry.old),
            new: readSide(reply.new, entry.new),
            patch: reply.patch ? fileText(reply.patch) : undefined,
            loadBodies: bodiesLoader(item, origin, reply),
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
        if (failure.kind === "changed" && attempt < CHANGED_RETRIES) return read(item, origin, attempt + 1);
        if (failure.kind === "disconnected") {
          setShown((held) =>
            held?.key === key && !held.failed && held.content.state !== "loading"
              ? held
              : { key, failed: { changing: false }, content: { state: "disconnected", what: "change" } },
          );
          return;
        }
        const message = failure.kind === "failed" ? failure.message : t("git.error.changeChanging");
        setShown(() => {
          const failed = { changing: failure.kind === "changed" };
          if (entry.group === "conflicted") return { key, failed, content: { state: "conflict", conflict: conflictText(t, entry.conflict), message } };
          return { key, failed, content: { state: "change", change: { old: listedSide(entry.old), new: listedSide(entry.new), unavailable: message } } };
        });
      });
  };

  const open = (item: ChangeItem, origin: ChangeOrigin) => {
    live.current.opened = { item, origin };
    setOpened({ item, origin });
    const key = subjectKey(item, origin);
    setShown((held) => (held?.key === key ? held : { key, content: { state: "loading" } }));
    read(item, origin);
  };

  const reload = () => {
    const opened = live.current.opened;
    if (opened) read(opened.item, opened.origin);
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
  const { origin } = opened;
  const key = subjectKey(opened.item, origin);
  const current = shown?.key === key ? shown : undefined;
  const content = current?.content ?? { state: "loading" };
  const isolate = (name: string) => `\u2068${displayWirePath(name)}\u2069`;
  // A worktree's change says where it is from in two ways: the stage of a staged or unstaged one, as
  // a tag of its own; and nothing for the rest, which their status chip already says (untracked,
  // in conflict). Only a comparison has a source to word.
  const { section } = opened.item;
  const stage = section === "staged" || section === "unstaged" ? t(SECTION_LABELS[section]) : undefined;
  // A comparison's change says which branches, at which commits, it is read from.
  const source =
    origin.kind === "comparison"
      ? createElement(Message<"git.compare.source">, {
          id: "git.compare.source",
          params: {
            from: createElement("bdi", { dir: "auto" }, displayWirePath(origin.left.branch)),
            fromCommit: shortCommit(origin.left.commit),
            to: createElement("bdi", { dir: "auto" }, displayWirePath(origin.right.branch)),
            toCommit: shortCommit(origin.right.commit),
          },
        })
      : undefined;
  // The same wording as `source`, as plain text for the description's tooltip, each branch name
  // isolated by U+2068/U+2069 the way the `<bdi>`s isolate it in `source`.
  const sourceText =
    origin.kind === "comparison"
      ? t("git.compare.source", {
          from: isolate(origin.left.branch),
          fromCommit: shortCommit(origin.left.commit),
          to: isolate(origin.right.branch),
          toCommit: shortCommit(origin.right.commit),
        })
      : undefined;
  return {
    subject: { key, path: opened.item.path, source, sourceText, stage, status: opened.item.status, content },
    item: opened.item,
    origin,
    shown: current?.failed ? { state: "error", changing: current.failed.changing } : content.state === "loading" ? { state: "loading" } : { state: "read" },
    open,
    reload,
    close,
  };
}
