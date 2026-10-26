import type React from "react";
import { createElement, useEffect, useRef, useState } from "react";

import { InlineReference } from "../components/InlineReference";
import { useLatest } from "../components/useLatest";
import { DaemonRequestError } from "../daemon-client";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
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
import { WORKTREE_UNAVAILABLE } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import {
  bodyFromFileContent,
  ChangeBodiesError,
  type ChangeBodies,
  type ViewerChangeSide,
  type ViewerContent,
  type ViewerSubject,
} from "../viewer/content";
import { changeBodiesFailure } from "../viewer/viewerError";
import { displayWirePath } from "../wirePath";
import { browseFailure } from "./browseError";
import { CONFLICT_LABELS, SECTION_LABELS, type ChangeItem, type CommitRef, type ShownChange } from "./changes";
import { shortCommit } from "./shortCommit";

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
 * holding the project's directory), a comparison of two branches at the commits it resolved — the
 * old side from `base`, their merge base, the new side from `right`'s commit — or one commit, its
 * old side from its first parent (the empty tree for none) and its new side from the commit. */
export type ChangeOrigin =
  | { kind: "worktree"; worktree: string | undefined }
  | { kind: "comparison"; left: ComparisonEndpoint; right: ComparisonEndpoint; base: string }
  | ({ kind: "commit" } & CommitRef);

/** Whether `err` says the worktree an uncommitted change from `origin` is read from is gone. */
function isWorktreeGone(err: unknown, origin: ChangeOrigin): boolean {
  return origin.kind === "worktree" && err instanceof DaemonRequestError && err.code === WORKTREE_UNAVAILABLE;
}

function sameOrigin(a: ChangeOrigin, b: ChangeOrigin): boolean {
  switch (a.kind) {
    case "worktree":
      return b.kind === "worktree" && a.worktree === b.worktree;
    case "commit":
      return b.kind === "commit" && a.commit === b.commit && a.parent === b.parent;
    case "comparison": {
      const same = (x: ComparisonEndpoint, y: ComparisonEndpoint) => x.branch === y.branch && x.commit === y.commit;
      return b.kind === "comparison" && same(a.left, b.left) && same(a.right, b.right) && a.base === b.base;
    }
  }
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
      return { state: "present", path: side.path, kind: side.kind, body: side.file ? bodyFromFileContent(side.path, side.file) : undefined, lines: side.lines };
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

/** The refusals that mean the same: this change has no whole sides to expand, however often it is asked for. */
const BODIES_NEVER_DELIVERED = new Set(["unsupported_file_type", "invalid_change", "clean_filtered"]);

type Shown = { key: string; content: ViewerContent; failed?: { changing: boolean } };
type Opened = { item: ChangeItem; origin: ChangeOrigin };

/**
 * The change a project's viewer shows in the Git mode, read on demand from its worktree, from the
 * two commits of a branch comparison or from one commit and its parent: one read out at a time, in the viewer's slot, so moving
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
 *
 * An uncommitted change whose worktree a read finds gone is no longer anywhere to be shown: no
 * other worktree's change is shown in its place, so the viewer closes, saying nothing, and
 * `onWorktreeGone` is told.
 */
export function useChangeReader(project: string, onWorktreeGone?: () => void): ChangeReader {
  const t = useT();
  const language = useCurrentLanguage();
  // The wording a failure is given when it happens, not when the patch it belongs to arrived: a
  // loader lives as long as its patch is shown, and the language can change in between.
  const wording = useLatest({ t, language });
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [opened, setOpened] = useState<Opened>();
  const [shown, setShown] = useState<Shown>();
  const live = useRef({ alive: true, sequence: 0, opened: undefined as Opened | undefined });
  const worktreeGone = useLatest(onWorktreeGone);

  const subjectKey = (item: ChangeItem, origin: ChangeOrigin) => {
    switch (origin.kind) {
      case "worktree":
        return `${project}\0git\0${origin.worktree ?? ""}\0${item.key}`;
      case "comparison":
        return `${project}\0compare\0${origin.base}\0${origin.right.commit}\0${item.key}`;
      case "commit":
        return `${project}\0commit\0${origin.parent ?? ""}\0${origin.commit}\0${item.key}`;
    }
  };

  const send = (item: ChangeItem, origin: ChangeOrigin): Promise<Event> => {
    const { entry } = item;
    const where = origin.kind === "worktree" && origin.worktree !== undefined ? { worktree: origin.worktree } : {};
    if (entry.group === "conflicted") return request({ type: "read_project_file", project, path: entry.path, ...where, slot: READ_SLOT });
    const sides = { old: sideRef(entry.old), new: sideRef(entry.new) };
    if (entry.group !== "committed") {
      return request({ type: "read_project_change", project, ...where, change: { group: entry.group, ...sides }, slot: READ_SLOT });
    }
    if (origin.kind === "commit") {
      const { commit, parent } = origin;
      return request({ type: "read_project_commit_change", project, commit, parent, change: sides, slot: READ_SLOT });
    }
    if (origin.kind !== "comparison") return Promise.reject(new Error("a committed change is read from its commits"));
    const { left, right, base } = origin;
    return request({ type: "read_project_comparison_change", project, left, right, base, change: sides, slot: READ_SLOT });
  };

  /** Whether `reply` answers the read of `item` from `origin`. */
  const answers = (reply: Event, item: ChangeItem, origin: ChangeOrigin): boolean => {
    const { entry } = item;
    if (entry.group === "conflicted") return reply.type === "project_file" && reply.project === project && reply.path === entry.path;
    if (origin.kind === "comparison") {
      return (
        reply.type === "project_comparison_change" &&
        reply.project === project &&
        sameOrigin({ kind: "comparison", left: reply.left, right: reply.right, base: reply.base }, origin)
      );
    }
    if (origin.kind === "commit") {
      return reply.type === "project_commit_change" && reply.project === project && reply.commit === origin.commit && reply.parent === origin.parent;
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
    reply: Extract<Event, { type: "project_change" | "project_comparison_change" | "project_commit_change" }>,
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
          const { left, right, base } = origin;
          bodies = await request({ type: "read_project_comparison_change_bodies", project, left, right, base, change: sides, ...versions, slot: BODIES_SLOT });
        } else if (origin.kind === "commit") {
          const { commit, parent } = origin;
          bodies = await request({ type: "read_project_commit_change_bodies", project, commit, parent, change: sides, ...versions, slot: BODIES_SLOT });
        } else {
          throw new ChangeBodiesError("unavailable");
        }
        if (bodies.type !== "project_change_bodies" && bodies.type !== "project_comparison_change_bodies" && bodies.type !== "project_commit_change_bodies") {
          throw new ChangeBodiesError("unavailable");
        }
        const oldText = text(bodies.old);
        const newText = text(bodies.new);
        if (oldText === undefined || newText === undefined) throw new ChangeBodiesError("unavailable");
        return { old: oldText, new: newText };
      } catch (err) {
        if (err instanceof ChangeBodiesError) throw err;
        if (isWorktreeGone(err, origin)) {
          closeGone(item, origin);
          throw new ChangeBodiesError("superseded");
        }
        if (err instanceof DaemonRequestError && BODIES_NEVER_DELIVERED.has(err.code)) throw new ChangeBodiesError("unavailable");
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
            throw changeBodiesFailure(t, language, failure.message, item.path, state);
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
        } else if (entry.group !== "conflicted" && (reply.type === "project_change" || reply.type === "project_comparison_change" || reply.type === "project_commit_change")) {
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
        if (isWorktreeGone(err, origin)) return closeGone(item, origin);
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

  /** Closes the viewer on `item` from `origin`, whose worktree a read found gone, while it is still
   * the change shown. */
  function closeGone(item: ChangeItem, origin: ChangeOrigin) {
    const opened = live.current.opened;
    if (!live.current.alive || opened?.item.key !== item.key || !sameOrigin(opened.origin, origin)) return;
    close();
    worktreeGone.current?.();
  }

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
  const isolatedWirePath = (name: string) => isolate(displayWirePath(name));
  // A worktree's change says where it is from in two ways: the stage of a staged or unstaged one, as
  // a tag of its own; and nothing for the rest, which their status chip already says (untracked,
  // in conflict). Only a committed change has a source to word.
  const { section } = opened.item;
  const stage = section === "staged" || section === "unstaged" ? t(SECTION_LABELS[section]) : undefined;
  const commitChip = (commit: string) => createElement(InlineReference, { monospace: true, wrap: true, color: "default", children: shortCommit(commit) });
  const branchChip = (branch: string) => createElement(InlineReference, { monospace: true, wrap: true, color: "accent", children: displayWirePath(branch) });
  // A comparison's change says which branch it is of, at which commit, and the branch it forked
  // from, at the commit of the fork, or that it is of the whole history when it has no base; one
  // commit's change says which commit it is of, and the parent it is compared with. The plain-text
  // form is the line's tooltip, each branch name and commit isolated by U+2068/U+2069 where the
  // line sets them as code, which reads left to right.
  let source: React.ReactNode;
  let sourceText: string | undefined;
  if (origin.kind === "comparison" && origin.left.empty) {
    source = createElement(Message<"git.compare.sourceRoot">, { id: "git.compare.sourceRoot", params: { to: branchChip(origin.right.branch), toCommit: commitChip(origin.right.commit) } });
    sourceText = t("git.compare.sourceRoot", { to: isolatedWirePath(origin.right.branch), toCommit: isolate(shortCommit(origin.right.commit)) });
  } else if (origin.kind === "comparison") {
    source = createElement(Message<"git.compare.source">, {
      id: "git.compare.source",
      params: { from: branchChip(origin.left.branch), fromCommit: commitChip(origin.base), to: branchChip(origin.right.branch), toCommit: commitChip(origin.right.commit) },
    });
    sourceText = t("git.compare.source", {
      from: isolatedWirePath(origin.left.branch),
      fromCommit: isolate(shortCommit(origin.base)),
      to: isolatedWirePath(origin.right.branch),
      toCommit: isolate(shortCommit(origin.right.commit)),
    });
  } else if (origin.kind === "commit") {
    const { commit, parent } = origin;
    source =
      parent === null
        ? createElement(Message<"git.commit.sourceRoot">, { id: "git.commit.sourceRoot", params: { commit: commitChip(commit) } })
        : createElement(Message<"git.commit.source">, { id: "git.commit.source", params: { commit: commitChip(commit), parent: commitChip(parent) } });
    sourceText =
      parent === null
        ? t("git.commit.sourceRoot", { commit: isolate(shortCommit(commit)) })
        : t("git.commit.source", { commit: isolate(shortCommit(commit)), parent: isolate(shortCommit(parent)) });
  }
  return {
    subject: { key, path: opened.item.path, host: origin.kind === "worktree" && opened.item.status !== "deleted" ? { project, worktree: origin.worktree, path: opened.item.path } : undefined, source, sourceText, stage, status: opened.item.status, content },
    item: opened.item,
    origin,
    shown: current?.failed ? { state: "error", changing: current.failed.changing } : content.state === "loading" ? { state: "loading" } : { state: "read" },
    open,
    reload,
    close,
  };
}
