import type React from "react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { FADE_SIZE } from "../components/scrollFade";
import type { ComparisonEndpoint } from "../protocol";
import type { ViewerSubject } from "../viewer/content";
import type { ViewerNavigation } from "../viewer/FileViewer";
import { sessionFollow, type ProjectBrowserState } from "./browserState";
import { changeLayout, type ChangeLayout } from "./changeLayout";
import { changeRowOffset } from "./ChangeList";
import { HEADING_HEIGHT } from "./changeSectionHeading";
import { changeEvidence, changeNeighbours, filterChanges, rereadChange, visibleChanges, type ChangeEvidence, type ChangeItem } from "./changes";
import { focusFirst, focusRow } from "./focusRow";
import type { GitView } from "./GitView";
import { selectionForBranch, sessionBranch } from "./sessionBranch";
import { ROW_HEIGHT } from "./treeRow";
import { useBranchComparison } from "./useBranchComparison";
import { useBranchList } from "./useBranchList";
import { useChangeList } from "./useChangeList";
import { useChangeReader, type ChangeOrigin } from "./useChangeReader";
import { useProjectSource } from "./useProjectSource";

export interface GitReview {
  /** What the Git mode's view shows and does, but for asking again after a failure. */
  view: Omit<React.ComponentProps<typeof GitView>, "onRetry" | "selectedSessionId">;
  /** Asks again for what the Git mode shows: the worktrees, the branches, and the view on screen.
   * Settles once those have been answered or have failed. */
  refresh: () => Promise<void>;
  /** The change the viewer shows, when one is open. */
  viewer?: { subject: ViewerSubject; onClose: () => void; navigation?: ViewerNavigation };
  /** Closes the change viewer without moving focus to its row. */
  dismissViewer: () => void;
}

/**
 * The project pane's Git mode, as one project's browser holds it: the repository's source, the
 * chosen worktree's uncommitted changes, the branches and their comparison, and the change the
 * viewer shows from either. `active` is whether the Git mode is on screen; only the view on screen
 * is asked for, bar the branch list, which both views need.
 *
 * The viewer moves through the change list's rows on screen, from one section into the next, and
 * stops at either end; the changes a file name filter hides have no row, nor do those in a folded
 * section or, in the tree layout, under a folded directory, so it skips them. What it shows is the
 * list's selection, kept in view behind it, and closing it puts keyboard focus on that row. While
 * it is open, a worktree's listings decide when it reads its change again (`rereadChange`); a
 * comparison's change is read from the commits the comparison on screen was made at, and follows
 * that comparison when it is made again.
 *
 * `session` is the selected session when it belongs to this project, none otherwise. Selecting one
 * sets the selectors, once, from the branch it created (`selectionForBranch`), and from one that
 * created none, to their defaults; no session leaves them as they are. Only what was read from
 * the daemon after the selection is weighed, so what a pane held from before it is asked for again.
 */
export function useGitReview(project: string, browser: ProjectBrowserState, active: boolean, session: string | undefined): GitReview {
  const source = useProjectSource(project, active);
  // Changes are listed only once the source says there is a repository with the chosen worktree in
  // it: a project in none, or a worktree that has gone, has nothing to ask for until that changes.
  const git = source.source.state === "loaded" ? source.source.source.git : null;
  const listable = git !== null && (browser.worktree === undefined || git.worktrees.some((w) => w.id === browser.worktree));
  const gitView = browser.gitView;
  const changes = useChangeList(project, browser.worktree, active && gitView === "worktree" && listable);
  const comparing = active && gitView === "compare" && git !== null;
  // Either view needs the branch list: the Compare view for its selectors, and the session's
  // branch is found in it for both.
  const branches = useBranchList(project, active && git !== null);
  const comparison = useBranchComparison(project, browser.left, browser.right, comparing);
  const changeReader = useChangeReader(project);
  const changeListRef = useRef<HTMLDivElement>(null);
  const comparedListRef = useRef<HTMLDivElement>(null);
  const selectorsRef = useRef<HTMLDivElement>(null);
  const layout = changeLayout.useValue();
  const collapsed = browser.collapsedChangeRows;
  // The file name text narrowing both views' lists, and whether the field it is typed in is on
  // screen at all: kept while this project's browser is, shared by the two views, and not stored
  // (they narrow what is on screen now, not a choice to come back to). Closing the field drops the
  // text with it, so no filter the user cannot see narrows a list.
  const [filter, setFilter] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const filterToggleRef = useRef<HTMLButtonElement>(null);
  const shownFilter = filter.trim();
  const openFilter = (open: boolean) => {
    setFilterOpen(open);
    if (!open) setFilter("");
  };

  // A worktree found gone may have been replaced by another at its place, or others added: what
  // the selector offers is asked for again.
  useEffect(() => {
    if (changes.list.state === "error" && changes.list.unavailable) source.refresh();
  }, [changes.list]);

  // What the list said of the change when it was opened, and the list then: a later listing that
  // says otherwise is news, and the change is read again (`rereadChange`).
  const lastChangeEvidence = useRef<{ key: string; evidence: ChangeEvidence; list: unknown } | undefined>(undefined);
  const openChange = (item: ChangeItem) => {
    browser.setSelectedChange(item.key);
    lastChangeEvidence.current = { key: item.key, evidence: { state: "listed", versions: item.versions }, list: evidenceList };
    changeReader.open(item, { kind: "worktree", worktree: browser.worktree });
  };

  /** Opens a change of the comparison on screen, read from the two commits it was made at. */
  const shownComparison = comparison.comparison.state === "loaded" ? comparison.comparison : undefined;
  const comparedOrigin = (from: { left: ComparisonEndpoint; right: ComparisonEndpoint; base: string }): ChangeOrigin => ({
    kind: "comparison",
    left: from.left,
    right: from.right,
    base: from.base,
  });
  const openComparedChange = (item: ChangeItem) => {
    if (!shownComparison) return;
    browser.setSelectedComparedChange(item.key);
    changeReader.open(item, comparedOrigin(shownComparison));
  };

  const dismissViewer = () => {
    changeReader.close();
  };

  const closeChange = () => {
    const comparedChange = changeReader.origin?.kind === "comparison";
    changeReader.close();
    const [list, key] = comparedChange ? [comparedListRef.current, browser.selectedComparedChange] : [changeListRef.current, browser.selectedChange];
    if (key !== undefined) focusRow(list, key);
  };

  const setWorktree = (worktree: string | undefined) => {
    if (worktree === browser.worktree) return;
    browser.setSelectedChange(undefined);
    browser.setWorktree(worktree);
  };
  // Other branches are another comparison: nothing selected in the previous one stands.
  const setBranches = (left: string | undefined, right: string | undefined) => {
    if (left === browser.left && right === browser.right) return;
    browser.setSelectedComparedChange(undefined);
    browser.setBranches(left, right);
  };
  // A choice the user makes in a selector settles what the selected session set: it is not set
  // over it later.
  const settle = () => {
    if (session !== undefined) sessionFollow(project, session).settled = true;
  };
  const changeWorktree = (worktree: string | undefined) => {
    settle();
    setWorktree(worktree);
  };
  const changeBranches = (left: string | undefined, right: string | undefined) => {
    settle();
    setBranches(left, right);
  };

  // Selecting a session sets the selectors once the repository's worktrees and branches have been
  // read since, whichever view is on screen. A session with no branch yet leaves them at their
  // defaults, and one that shows up in a later branch list is set then, unless the user has chosen
  // in the meantime. A branch no worktree has is asked for once more: the worktree may have been
  // made after the list was read.
  useEffect(() => {
    if (session === undefined || source.source.state !== "loaded" || branches.branches.state === "loading") return;
    const follow = sessionFollow(project, session);
    if (follow.settled) return;
    const { sentAt: sourceSentAt, source: info } = source.source;
    const list = branches.branches;
    const fresh = { source: sourceSentAt > follow.sourceSince, branches: list.state !== "loaded" || list.sentAt > follow.since };
    if (active && !follow.asked) {
      follow.asked = true;
      if (!fresh.source) void source.refresh();
      if (!fresh.branches || list.state === "error") void branches.refresh();
    }
    if (!fresh.source || !fresh.branches || info.git === null) return;
    const branch = sessionBranch(list.state === "loaded" ? list.branches : [], session);
    const next = selectionForBranch(branch, info.git.worktrees, info.git.worktree);
    setBranches(next.left, next.right);
    setWorktree(next.worktree);
    if (branch === undefined) return;
    if (info.git.worktrees.some((w) => w.branch === branch) || follow.retried) {
      follow.settled = true;
    } else {
      follow.retried = true;
      follow.sourceSince = performance.now();
      void source.refresh();
    }
  }, [session, active, source.source, branches.branches]);

  const refresh = async () => {
    await Promise.all([
      source.refresh(),
      branches.refresh(),
      ...(gitView === "worktree" ? [changes.refresh()] : [comparison.refresh()]),
    ]);
  };

  // The change lists' selections are kept in view, while the viewer moves through the changes.
  const scrolledToChange = useRef<string | undefined>(undefined);
  // The layout and the filter are part of what was scrolled to: another layout, or the same list
  // narrowed or widened, is a list with its own scroll.
  useLayoutEffect(() => {
    const selected = browser.selectedChange;
    const scrolled = selected && `${layout}:${shownFilter}:${selected}`;
    if (scrolled !== scrolledToChange.current && changes.list.state === "loaded" && scrollToChange(changeListRef.current, filterChanges(changes.list.items, shownFilter), selected, layout, collapsed)) {
      scrolledToChange.current = scrolled;
    }
  }, [browser.selectedChange, changes.list, layout, collapsed, shownFilter]);
  const scrolledToCompared = useRef<string | undefined>(undefined);
  useLayoutEffect(() => {
    const selected = browser.selectedComparedChange;
    const scrolled = selected && `${layout}:${shownFilter}:${selected}`;
    if (scrolled !== scrolledToCompared.current && shownComparison && scrollToChange(comparedListRef.current, filterChanges(shownComparison.items, shownFilter), selected, layout, collapsed)) {
      scrolledToCompared.current = scrolled;
    }
  }, [browser.selectedComparedChange, shownComparison, layout, collapsed, shownFilter]);

  // The change list's say about the change in the viewer, as `rereadFor` weighs the listings' say
  // about a file; a fresh list counts as news only for a change whose sources kept moving.
  const viewedChange = changeReader.item;
  const viewedOrigin = changeReader.origin;
  // A worktree found gone — by its list, or by the source no longer listing it — has no changes
  // left: the open change counts as gone from it, and is read again to say so.
  const worktreeGone = (changes.list.state === "error" && changes.list.unavailable) || (git !== null && !listable);
  // The React Compiler cannot keep this memo and so leaves the hook uncompiled; the memo stays, as
  // the list's identity is what tells the reread below that a fresh list arrived.
  // eslint-disable-next-line react-hooks/preserve-manual-memoization
  const evidenceList = useMemo(
    () => (worktreeGone ? { items: [], complete: true, refreshing: false } : changes.list.state === "loaded" ? changes.list : undefined),
    [worktreeGone, changes.list],
  );
  useEffect(() => {
    if (viewedChange === undefined || viewedOrigin?.kind !== "worktree" || changeReader.shown.state === "loading") return;
    const evidence = changeEvidence(evidenceList, viewedChange.key);
    const last = lastChangeEvidence.current?.key === viewedChange.key ? lastChangeEvidence.current : undefined;
    if (rereadChange(changeReader.shown, evidence, last?.evidence, evidenceList !== last?.list)) {
      // A change keeps its key through a status change (modified to type changed: the key names a
      // present side by path only), so the reread opens the list's item, not the one held from
      // the first open, whose status the chip would keep showing.
      const listedItem = evidence.state === "listed" ? evidenceList?.items.find((item) => item.key === viewedChange.key) : undefined;
      if (listedItem) changeReader.open(listedItem, viewedOrigin);
      else changeReader.reload();
    }
    if (evidence.state !== "unknown") lastChangeEvidence.current = { key: viewedChange.key, evidence, list: evidenceList };
  }, [evidenceList, viewedChange?.key, changeReader.shown.state]);

  // A comparison's change follows the comparison on screen: made again at other commits, the change
  // is read again from those, so the viewer never shows another pair than the list. A change the
  // new comparison does not have is no change between its commits: the viewer closes, and keyboard
  // focus goes to the list's first row, or to the selectors when it has none.
  useEffect(() => {
    if (viewedChange === undefined || viewedOrigin?.kind !== "comparison" || !shownComparison) return;
    const { base, right } = viewedOrigin;
    const now = shownComparison;
    if (base === now.base && right.commit === now.right.commit) return;
    const item = now.items.find((candidate) => candidate.key === viewedChange.key);
    if (item) return changeReader.open(item, comparedOrigin(now));
    changeReader.close();
    browser.setSelectedComparedChange(undefined);
    focusFirst(comparedListRef.current, selectorsRef.current);
  }, [shownComparison]);

  const navigation = useMemo(() => {
    if (viewedChange === undefined) return undefined;
    const compared = viewedOrigin?.kind === "comparison";
    const items = compared ? (shownComparison?.items ?? []) : changes.list.state === "loaded" ? changes.list.items : [];
    const { previous, next } = changeNeighbours(visibleChanges(filterChanges(items, shownFilter), layout, collapsed), viewedChange);
    const open = compared ? openComparedChange : openChange;
    return {
      onPrevious: previous === undefined ? undefined : () => open(previous),
      onNext: next === undefined ? undefined : () => open(next),
    };
  }, [changes.list, shownComparison, viewedChange, viewedOrigin, layout, collapsed, shownFilter]);

  const changeView = {
    layout,
    collapsed,
    onCollapsedChange: browser.setCollapsedChangeRows,
    filterOpen,
    onFilterOpenChange: openFilter,
    filter,
    onFilterChange: setFilter,
    filterToggleRef,
  };

  return {
    view: {
      source: source.source,
      view: gitView,
      onViewChange: browser.setGitView,
      changeView,
      worktree: {
        list: changes.list,
        worktree: browser.worktree,
        onWorktreeChange: changeWorktree,
        selectedChange: browser.selectedChange,
        onOpen: openChange,
        listRef: changeListRef,
      },
      compare: {
        branches: branches.branches,
        comparison: comparison.comparison,
        left: browser.left,
        right: browser.right,
        onBranchesChange: changeBranches,
        selectedChange: browser.selectedComparedChange,
        onOpen: openComparedChange,
        listRef: comparedListRef,
        selectorsRef,
      },
    },
    refresh,
    viewer: changeReader.subject && { subject: changeReader.subject, onClose: closeChange, navigation },
    dismissViewer,
  };
}

/** Scrolls `list` so the row of change `key` among `items` is in view; false when there is no
 * such row on screen yet. Rows and headings are each one height, so where a row is follows from
 * its place (`changeRowOffset`). */
function scrollToChange(
  list: HTMLElement | null,
  items: readonly ChangeItem[],
  key: string | undefined,
  layout: ChangeLayout,
  collapsed: ReadonlySet<string>,
): boolean {
  if (key === undefined || !list) return false;
  const top = changeRowOffset(items, key, layout, collapsed);
  if (top === undefined) return false;
  // Clear of the sticky heading and fade, as scroll padding keeps the rows react-aria scrolls.
  if (top - HEADING_HEIGHT - FADE_SIZE < list.scrollTop) list.scrollTop = top - HEADING_HEIGHT - FADE_SIZE;
  else if (top + ROW_HEIGHT + FADE_SIZE > list.scrollTop + list.clientHeight) list.scrollTop = top + ROW_HEIGHT + FADE_SIZE - list.clientHeight;
  return true;
}
