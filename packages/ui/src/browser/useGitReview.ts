import type React from "react";
import { useEffect, useLayoutEffect, useMemo, useRef } from "react";

import { FADE_SIZE } from "../components/scrollFade";
import type { WorktreeInfo } from "../protocol";
import type { ViewerSubject } from "../viewer/content";
import type { ViewerNavigation } from "../viewer/FileViewer";
import { sessionFollow, type ProjectBrowserState } from "./browserState";
import { type ChangeLayout } from "./changeLayout";
import { changeRowOffset } from "./ChangeList";
import { changesScope } from "./changesScope";
import { HEADING_HEIGHT } from "./changeSectionHeading";
import { changeEvidence, changeNeighbours, filterChanges, rereadChange, visibleChanges, type ChangeEvidence, type ChangeItem } from "./changes";
import { comparisonNews } from "./comparisonNews";
import type { FileFilterState } from "./FileFilterField";
import { focusFirst, focusRow } from "./focusRow";
import type { GitView } from "./GitView";
import { gitMoment } from "./gitRequests";
import { sessionBranch } from "./sessionBranch";
import { ROW_HEIGHT } from "./treeRow";
import { useBranchComparison } from "./useBranchComparison";
import { useBranchList } from "./useBranchList";
import { useChangeList } from "./useChangeList";
import { useChangeReader, type ChangeOrigin } from "./useChangeReader";
import { useCommitList } from "./useCommitList";
import { useProjectSource } from "./useProjectSource";

/** What a list with nothing in it is, so that one is no new array on every render. */
const NO_ITEMS: ChangeItem[] = [];

export interface GitReview {
  /** What the Git mode's view shows and does, but for asking again after a failure. */
  view: Omit<React.ComponentProps<typeof GitView>, "onRetry" | "selectedSessionId">;
  /** The worktree the uncommitted changes on screen are read from, by id; none for the one holding
   * the project's directory, or when no worktree has the branch checked out. */
  worktree: string | undefined;
  /** Asks again for everything the Git mode shows: the worktrees, the branches, the uncommitted
   * changes, the comparison and the commits. Settles once those have been answered or have failed. */
  refresh: () => Promise<void>;
  /** The change the viewer shows, when one is open. */
  viewer?: { subject: ViewerSubject; onClose: () => void; navigation?: ViewerNavigation };
  /** Closes the change viewer without moving focus to its row. */
  dismissViewer: () => void;
}

/**
 * The project pane's Git mode, as one project's browser holds it: the changes of one branch, as one
 * list. The branch is the one chosen, or by default the one checked out in the worktree holding the
 * project's directory; its uncommitted changes are read from the worktree that has it checked out,
 * when one has, and what it has committed is its comparison with the branch the repository's main
 * working copy has checked out — from the commit the two forked at to the branch's tip — listed
 * either together or, grouped by commit, under a row per commit since the fork, in one Committed
 * section. `active` is whether the Git mode is on screen; only then is anything asked for.
 *
 * The comparison is of the commits the branches were at when it was made; once a branch list read
 * since shows either branch at another commit, it is made again, so the Committed section follows
 * the branch as it commits what was uncommitted. A branch that has gone keeps the comparison it
 * had.
 *
 * The viewer moves through the list's rows on screen, from one section into the next, and stops at
 * either end; the changes a file name filter hides have no row, nor do those in a folded section,
 * under a folded commit or, in the tree layout, under a folded directory, so it skips them. What it
 * shows is the list's selection, kept in view behind it, and closing it puts keyboard focus on that
 * row. While it is open, an uncommitted change is read again when a listing has news of it
 * (`rereadChange`); a committed change is read from the commits it was listed at, and follows the
 * comparison or the commit list when either is made again.
 *
 * `session` is the selected session when it belongs to this project, none otherwise. Selecting one
 * sets the branch, once, to the branch it most recently became responsible for, and from one
 * responsible for none, back to the default; no session leaves it as it is. Only what was read from
 * the daemon after the selection is weighed, so what a pane held from before it is asked for again.
 */
export function useGitReview(project: string, browser: ProjectBrowserState, active: boolean, session: string | undefined, filterState: FileFilterState): GitReview {
  const source = useProjectSource(project, active);
  const git = source.source.state === "loaded" ? source.source.source.git : null;
  const branches = useBranchList(project, active && git !== null);
  const { ownBranch, branch, holder, holderId, committed } = changesScope(git, browser.branch);
  const comparing = committed.state === "compare";
  const changes = useChangeList(project, holderId, active && holder !== undefined);
  const comparisonHook = useBranchComparison(project, comparing ? committed.base : undefined, comparing ? committed.branch : undefined, active && git !== null);
  const comparison = comparisonHook.comparison;
  const shownComparison = comparison.state === "loaded" ? comparison : undefined;
  const byCommit = browser.byCommit;
  const forked = shownComparison && shownComparison.base !== shownComparison.right.commit ? { branch: shownComparison.right.branch, baseBranch: shownComparison.left.branch, base: shownComparison.base, tip: shownComparison.right.commit } : undefined;
  const commitsHook = useCommitList(project, forked, active && byCommit);
  const commits = commitsHook.commits;
  const changeReader = useChangeReader(project);
  const listRef = useRef<HTMLDivElement>(null);
  const selectorsRef = useRef<HTMLDivElement>(null);
  const layout = browser.layout;
  const collapsed = browser.collapsedChangeRows;
  const filter = filterState.filter.trim();

  const uncommittedItems = holder !== undefined && changes.list.state === "loaded" ? changes.list.items : undefined;
  const committedItems = !shownComparison ? undefined : !byCommit ? shownComparison.items : commits.state === "loaded" ? commits.items : forked ? undefined : NO_ITEMS;
  // The sections' ranks run uncommitted first, then the committed ones, each list already in order.
  const items = useMemo(() => [...(uncommittedItems ?? []), ...(committedItems ?? [])], [uncommittedItems, committedItems]);

  // A worktree found gone may have been replaced by another at its place, or others added: the
  // worktrees are asked for again.
  useEffect(() => {
    if (changes.list.state === "error" && changes.list.unavailable) source.refresh();
  }, [changes.list]);

  // A branch list read since the comparison that shows either branch at another commit makes it
  // again: what the branch has committed since moves from its uncommitted sections to its committed
  // ones. It is asked once for the commits the branches are at, so a comparison that takes a while
  // is not restarted by every list read meanwhile; branches at other commits are asked for again.
  // One that failed has nothing on screen to read the news from, so it is asked for again only
  // when a later list has the branches at commits other than the last ones seen or asked for in
  // the background.
  const askedFor = useRef<{ pair: string; tips: string } | undefined>(undefined);
  const branchList = branches.branches;
  useEffect(() => {
    if (!active) return;
    let pair: string;
    let tips: string[];
    if (shownComparison) {
      const news = comparisonNews(branchList, shownComparison);
      pair = JSON.stringify([shownComparison.left.branch, shownComparison.right.branch]);
      if (!news.moved) {
        if (news.tips.length > 0) askedFor.current = { pair, tips: JSON.stringify(news.tips) };
        return;
      }
      tips = news.tips;
    } else if (comparing && comparison.state === "error" && branchList.state === "loaded") {
      pair = JSON.stringify([committed.base, committed.branch]);
      if (askedFor.current?.pair !== pair) return;
      tips = [committed.base, committed.branch].map((name) => branchList.branches.find((b) => b.name === name)?.commit ?? "");
      if (tips.includes("")) return;
    } else {
      return;
    }
    const asked = { pair, tips: JSON.stringify(tips) };
    if (askedFor.current?.pair === pair && askedFor.current.tips === asked.tips) return;
    askedFor.current = asked;
    void comparisonHook.update();
  }, [active, branchList, shownComparison, comparison]);

  // What the list said of an uncommitted change when it was opened, and the list then: a later
  // listing that says otherwise is news, and the change is read again (`rereadChange`).
  const lastChangeEvidence = useRef<{ key: string; evidence: ChangeEvidence; list: unknown } | undefined>(undefined);
  const originOf = (item: ChangeItem): ChangeOrigin | undefined => {
    if (item.commit) return { kind: "commit", ...item.commit };
    if (item.section !== "committed") return { kind: "worktree", worktree: holderId };
    return shownComparison && { kind: "comparison", left: shownComparison.left, right: shownComparison.right, base: shownComparison.base };
  };
  const openChange = (item: ChangeItem) => {
    const origin = originOf(item);
    if (!origin) return;
    browser.setSelectedChange(item.key);
    if (origin.kind === "worktree") lastChangeEvidence.current = { key: item.key, evidence: { state: "listed", versions: item.versions }, list: evidenceList };
    changeReader.open(item, origin);
  };

  const dismissViewer = () => {
    changeReader.close();
  };

  const closeChange = () => {
    changeReader.close();
    if (browser.selectedChange !== undefined) focusRow(listRef.current, browser.selectedChange);
  };

  // Another branch is another list: nothing selected or open from the previous one stands.
  const setBranch = (next: string | undefined) => {
    if ((next ?? ownBranch) !== branch) {
      browser.setSelectedChange(undefined);
      changeReader.close();
    }
    browser.setBranch(next);
  };
  // A choice the user makes settles what the selected session set: it is not set over it later.
  const changeBranch = (next: string) => {
    if (session !== undefined) sessionFollow(project, session).settled = true;
    setBranch(next);
  };

  // Selecting a session sets the branch once the repository's worktrees and branches have been read
  // since, whichever state the view is in. A session with no branch yet leaves it at the default,
  // and one that shows up in a later branch list is set then, unless the user has chosen in the
  // meantime. A branch no worktree has checked out has the worktrees asked for once more: the
  // worktree may have been made after the list was read.
  useEffect(() => {
    if (session === undefined || source.source.state !== "loaded" || branchList.state === "loading") return;
    const follow = sessionFollow(project, session);
    if (follow.settled) return;
    const { sentAt: sourceSentAt, source: info } = source.source;
    const fresh = { source: sourceSentAt > follow.sourceSince, branches: branchList.state !== "loaded" || branchList.sentAt > follow.since };
    if (active && !follow.asked) {
      follow.asked = true;
      if (!fresh.source) void source.refresh();
      if (!fresh.branches || branchList.state === "error") void branches.refresh();
    }
    if (!fresh.source || !fresh.branches || info.git === null) return;
    const followed = sessionBranch(branchList.state === "loaded" ? branchList.branches : [], session);
    setBranch(followed);
    if (followed === undefined) return;
    if (info.git.worktrees.some((w) => w.branch === followed) || follow.retried) {
      follow.settled = true;
    } else {
      follow.retried = true;
      follow.sourceSince = gitMoment();
      void source.refresh();
    }
  }, [session, active, source.source, branchList]);

  const refresh = async () => {
    await Promise.all([source.refresh(), branches.refresh(), changes.refresh(), comparisonHook.refresh(), commitsHook.refresh()]);
  };

  // The selection is kept in view, while the viewer moves through the changes. The layout and the
  // filter are part of what was scrolled to: another layout, or the same list narrowed or widened,
  // is a list with its own scroll.
  const scrolledToChange = useRef<string | undefined>(undefined);
  useLayoutEffect(() => {
    const selected = browser.selectedChange;
    const scrolled = selected && `${layout}:${filter}:${selected}`;
    if (scrolled !== scrolledToChange.current && scrollToChange(listRef.current, filterChanges(items, filter), selected, layout, collapsed)) {
      scrolledToChange.current = scrolled;
    }
  }, [browser.selectedChange, items, layout, collapsed, filter]);

  // The uncommitted list's say about the change in the viewer, as `rereadFor` weighs the listings'
  // say about a file; a fresh list counts as news only for a change whose sources kept moving.
  const viewedChange = changeReader.item;
  const viewedOrigin = changeReader.origin;
  // A worktree found gone, or no longer the one the branch is checked out in, has no changes left
  // here: the open change counts as gone from it, and is read again to say what it is now.
  const viewedWorktree = viewedOrigin?.kind === "worktree" ? viewedOrigin.worktree : undefined;
  const worktreeLeft =
    (changes.list.state === "error" && changes.list.unavailable) || holder === undefined || viewedWorktree !== holderId;
  // The React Compiler cannot keep this memo and so leaves the hook uncompiled; the memo stays, as
  // the list's identity is what tells the reread below that a fresh list arrived.
  // eslint-disable-next-line react-hooks/preserve-manual-memoization
  const evidenceList = useMemo(
    () => (worktreeLeft ? { items: [], complete: true, refreshing: false } : changes.list.state === "loaded" ? changes.list : undefined),
    [worktreeLeft, changes.list],
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

  // A committed change follows the list it came from: the comparison made again at other commits
  // reads it again from those, so the viewer never shows another pair than the list; a change the
  // new comparison, or the commits listed again, do not have is no change between their commits:
  // the viewer closes, and keyboard focus goes to the list's first row, or to the branch selector
  // when it has none.
  const closeGone = () => {
    changeReader.close();
    browser.setSelectedChange(undefined);
    focusFirst(listRef.current, selectorsRef.current);
  };
  useEffect(() => {
    if (viewedChange === undefined || viewedOrigin?.kind !== "comparison" || !shownComparison) return;
    const { base, right } = viewedOrigin;
    const now = shownComparison;
    if (base === now.base && right.commit === now.right.commit) return;
    const item = now.items.find((candidate) => candidate.key === viewedChange.key);
    if (item) return changeReader.open(item, { kind: "comparison", left: now.left, right: now.right, base: now.base });
    closeGone();
  }, [shownComparison]);
  const listedCommits = commits.state === "loaded" && forked && commits.base === forked.base && commits.tip === forked.tip ? commits : undefined;
  useEffect(() => {
    if (viewedChange === undefined || viewedOrigin?.kind !== "commit" || !listedCommits) return;
    if (!listedCommits.items.some((candidate) => candidate.key === viewedChange.key)) closeGone();
  }, [listedCommits]);

  const navigation = useMemo(() => {
    if (viewedChange === undefined) return undefined;
    const { previous, next } = changeNeighbours(visibleChanges(filterChanges(items, filter), layout, collapsed), viewedChange);
    return {
      onPrevious: previous === undefined ? undefined : () => openChange(previous),
      onNext: next === undefined ? undefined : () => openChange(next),
    };
  }, [items, viewedChange, layout, collapsed, filter]);

  const changeView = {
    layout,
    collapsed,
    onCollapsedChange: browser.setCollapsedChangeRows,
    ...filterState,
  };

  const uncommitted: UncommittedState =
    branch === undefined
      ? { state: "none" }
      : holder === undefined
        ? { state: "notCheckedOut", branch }
        : { state: "listed", list: changes.list, worktree: holder, own: holderId === undefined };
  const committedState: CommittedState =
    committed.state === "compare"
      ? { state: "compare", base: committed.base, comparison, commits: byCommit && forked ? commits : undefined }
      : committed;

  return {
    view: {
      source: source.source,
      branches: branchList,
      branch,
      onBranchChange: changeBranch,
      uncommitted,
      committed: committedState,
      items,
      selectedChange: browser.selectedChange,
      onOpen: openChange,
      listRef,
      selectorsRef,
      changeView,
    },
    worktree: holderId,
    refresh,
    viewer: changeReader.subject && { subject: changeReader.subject, onClose: closeChange, navigation },
    dismissViewer,
  };
}

/** Where the uncommitted part of the list stands: no branch to read it for, a branch no worktree
 * has checked out, or the list read from the worktree that has (`own` when that is the worktree
 * holding the project's directory). */
export type UncommittedState =
  | { state: "none" }
  | { state: "notCheckedOut"; branch: string }
  | { state: "listed"; list: ReturnType<typeof useChangeList>["list"]; worktree: WorktreeInfo; own: boolean };

/** Where the committed part of the list stands: no branch, or one of the two with no commit yet, a main working copy on no branch to
 * compare with (`noBase`), the branch being the main working copy's own (`isBase`), or the
 * comparison with `base`, the main working copy's branch, and — grouped by commit — its commits. */
export type CommittedState =
  | { state: "none" | "noBase" }
  | { state: "isBase"; branch: string }
  | {
    state: "compare";
    base: string;
    comparison: ReturnType<typeof useBranchComparison>["comparison"];
    commits: ReturnType<typeof useCommitList>["commits"] | undefined;
  };

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
