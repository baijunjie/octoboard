import type React from "react";
import { useEffect, useMemo, useRef } from "react";

import { useStableHandler } from "../../components/hooks/useLatest";
import type { WorktreeInfo } from "../../protocol";
import type { ViewerNavigation } from "../../viewer/viewerNavigation";
import { focusFirst, focusRow } from "../focusRow";
import { neighboursIn, rereadFor, type Evidence } from "../viewedItem";
import type { ChangeLayout } from "./changeLayout";
import { changeEvidence, compareChanges, filterChanges, visibleChanges, type ChangeItem } from "./changes";
import type { ChangeList } from "./useChangeList";
import type { ChangeOrigin, ChangeReader } from "./useChangeReader";
import type { CommittedChanges } from "./useCommittedChanges";

export interface ChangeViewerSyncInput {
  reader: ChangeReader;
  /** The change selected in the list, and its writer. */
  selectedChange: string | undefined;
  setSelectedChange: (key: string | undefined) => void;
  listRef: React.RefObject<HTMLDivElement | null>;
  selectorsRef: React.RefObject<HTMLDivElement | null>;
  /** The worktree the branch's uncommitted changes are read from (by id `holderId`, none for the
   * one holding the project's directory), and whether a read found it gone. */
  holder: WorktreeInfo | undefined;
  holderId: string | undefined;
  worktreeGone: boolean;
  /** The uncommitted list. */
  list: ChangeList;
  committed: Pick<CommittedChanges, "shown" | "listedCommits">;
  /** Every change listed, in list order, and how the list shows them. */
  items: ChangeItem[];
  filter: string;
  layout: ChangeLayout;
  collapsed: ReadonlySet<string>;
}

export interface ChangeViewerSync {
  /** Opens `item` in the viewer and selects it; keeps its identity for the hook's lifetime. */
  openChange: (item: ChangeItem) => void;
  /** Closes the viewer, putting keyboard focus on the selected change's row. */
  closeChange: () => void;
  /** Moving through the list's rows on screen, while a change is open. */
  navigation: ViewerNavigation | undefined;
}

/**
 * Keeps the Git mode's change viewer in step with the list. The viewer moves through the list's
 * rows on screen, from one section into the next, and stops at either end; the changes a file name
 * filter hides have no row, nor do those in a folded section, under a folded commit or, in the tree
 * layout, under a folded directory, so it skips them. While it is open, an uncommitted change is
 * read again when a listing has news of it (`rereadFor`); a committed change is read from the
 * commits it was listed at, and follows the comparison or the commit list when either is made again.
 */
export function useChangeViewerSync({
  reader,
  selectedChange,
  setSelectedChange,
  listRef,
  selectorsRef,
  holder,
  holderId,
  worktreeGone,
  list,
  committed,
  items,
  filter,
  layout,
  collapsed,
}: ChangeViewerSyncInput): ChangeViewerSync {
  const shownComparison = committed.shown;
  const listedCommits = committed.listedCommits;
  const viewedChange = reader.item;
  const viewedOrigin = reader.origin;
  const shownState = reader.shown.state;

  // The uncommitted list's say about the change in the viewer, as `rereadFor` weighs the listings'
  // say about a file; a fresh list counts as news only for a change whose sources kept moving. A
  // worktree found gone, or no longer the one the branch is checked out in, has no changes left
  // here: the open change counts as gone from it, and is read again to say what it is now. The
  // list's identity is what tells the reread below that a fresh list arrived.
  const viewedWorktree = viewedOrigin?.kind === "worktree" ? viewedOrigin.worktree : undefined;
  const worktreeLeft = worktreeGone || holder === undefined || viewedWorktree !== holderId;
  const evidenceList = useMemo(
    () => (worktreeLeft ? { items: [], complete: true, refreshing: false } : list.state === "loaded" ? list : undefined),
    [worktreeLeft, list],
  );

  // What the list said of an uncommitted change when it was opened, and the list then: a later
  // listing that says otherwise is news, and the change is read again (`rereadFor`).
  const lastChangeEvidence = useRef<{ key: string; evidence: Evidence; list: unknown } | undefined>(undefined);
  const originOf = (item: ChangeItem): ChangeOrigin | undefined => {
    if (item.commit) return { kind: "commit", ...item.commit };
    if (item.section !== "committed") return { kind: "worktree", worktree: holderId };
    return shownComparison && { kind: "comparison", left: shownComparison.left, right: shownComparison.right, base: shownComparison.base };
  };
  // One function for the hook's life, so that `navigation` and the list's rows change only with
  // what they show.
  const openChange = useStableHandler((item: ChangeItem) => {
    const origin = originOf(item);
    if (!origin) return;
    setSelectedChange(item.key);
    if (origin.kind === "worktree") lastChangeEvidence.current = { key: item.key, evidence: { state: "listed", version: item.versions }, list: evidenceList };
    reader.open(item, origin);
  });

  const closeChange = () => {
    reader.close();
    if (selectedChange !== undefined) focusRow(listRef.current, selectedChange);
  };

  useEffect(() => {
    if (viewedChange === undefined || viewedOrigin?.kind !== "worktree" || shownState === "loading") return;
    const evidence = changeEvidence(evidenceList, viewedChange.key);
    const last = lastChangeEvidence.current?.key === viewedChange.key ? lastChangeEvidence.current : undefined;
    if (rereadFor(reader.shown, evidence, last?.evidence, evidenceList !== last?.list, true)) {
      // A change keeps its key through a status change (modified to type changed: the key names a
      // present side by path only), so the reread opens the list's item, not the one held from
      // the first open, whose status the chip would keep showing.
      const listedItem = evidence.state === "listed" ? evidenceList?.items.find((item) => item.key === viewedChange.key) : undefined;
      if (listedItem) reader.open(listedItem, viewedOrigin);
      else reader.reload();
    }
    if (evidence.state !== "unknown") lastChangeEvidence.current = { key: viewedChange.key, evidence, list: evidenceList };
  }, [evidenceList, viewedChange?.key, shownState]);

  // A committed change follows the list it came from: the comparison made again at other commits
  // reads it again from those, so the viewer never shows another pair than the list; a change the
  // new comparison, or the commits listed again, do not have is no change between their commits:
  // the viewer closes, and keyboard focus goes to the list's first row, or to the branch selector
  // when it has none.
  const closeGone = () => {
    reader.close();
    setSelectedChange(undefined);
    focusFirst(listRef.current, selectorsRef.current);
  };
  useEffect(() => {
    if (viewedChange === undefined || viewedOrigin?.kind !== "comparison" || !shownComparison) return;
    const { base, right } = viewedOrigin;
    const now = shownComparison;
    if (base === now.base && right.commit === now.right.commit) return;
    const item = now.items.find((candidate) => candidate.key === viewedChange.key);
    if (item) return reader.open(item, { kind: "comparison", left: now.left, right: now.right, base: now.base });
    closeGone();
  }, [shownComparison]);
  useEffect(() => {
    if (viewedChange === undefined || viewedOrigin?.kind !== "commit" || !listedCommits) return;
    if (!listedCommits.items.some((candidate) => candidate.key === viewedChange.key)) closeGone();
  }, [listedCommits]);

  const navigation = useMemo(() => {
    if (viewedChange === undefined) return undefined;
    const { previous, next } = neighboursIn(visibleChanges(filterChanges(items, filter), layout, collapsed), viewedChange, (a, b) => a.key === b.key, compareChanges);
    return {
      onPrevious: previous === undefined ? undefined : () => openChange(previous),
      onNext: next === undefined ? undefined : () => openChange(next),
    };
  }, [items, viewedChange, layout, collapsed, filter, openChange]);

  return { openChange, closeChange, navigation };
}
