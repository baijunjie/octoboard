import type React from "react";
import { useEffect, useLayoutEffect, useMemo, useRef } from "react";

import type { ViewerSubject } from "../../viewer/content";
import type { ViewerNavigation } from "../../viewer/viewerNavigation";
import type { ProjectBrowserState } from "../browserState";
import type { FileFilterState } from "../FileFilterField";
import { focusFirst } from "../focusRow";
import { scrollRowIntoView } from "../treeRow";
import { type ChangeLayout } from "./changeLayout";
import { changeRowOffset } from "./changeListLayout";
import { filterChanges, type ChangeItem } from "./changes";
import { changesScope } from "./changesScope";
import { branchGone } from "./changesStatus";
import { HEADING_HEIGHT } from "./changeSectionHeading";
import type { GitView } from "./GitView";
import { committedState, uncommittedState } from "./gitViewModel";
import { useBranchList } from "./useBranchList";
import { useChangeList } from "./useChangeList";
import { useChangeReader } from "./useChangeReader";
import { useChangeViewerSync } from "./useChangeViewerSync";
import { useCommittedChanges } from "./useCommittedChanges";
import { useProjectSource } from "./useProjectSource";
import { useSessionBranchFollow } from "./useSessionBranchFollow";

export interface GitReview {
  /** What the Git mode's view shows and does, but for asking again after a failure. */
  view: Omit<React.ComponentProps<typeof GitView>, "onRetry" | "selectedSessionId">;
  /** The worktree the uncommitted changes on screen are read from, by id; none for the one holding
   * the project's directory, or when no worktree has the branch checked out. */
  worktree: string | undefined;
  /** Asks again for everything the Git mode shows: the worktrees, the branches, the uncommitted
   * changes, the comparison and the commits. Settles once those have been answered or have failed.
   * `askedByPerson` for a refresh a person asked for (see `useProjectSource`). */
  refresh: (askedByPerson?: boolean) => Promise<void>;
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
 * working copy has checked out — from the commit the two forked at to the branch's tip. For that
 * very branch it is the comparison with the remote branch the branch list gives it (its upstream,
 * or the one of its name on a remote), so its unpushed commits, or with nothing when there is none,
 * so its whole history. The commits are listed either together or, grouped by commit, under a row
 * per commit since the fork, in one Committed section. `active` is whether the Git mode is on
 * screen; only then is anything asked for.
 *
 * A chosen branch that a complete branch list no longer has gives way to the default, whether the
 * user or a session chose it; any other branch that has gone keeps the comparison it had. The
 * selection is kept in view in the list.
 *
 * `useCommittedChanges` keeps the comparison and the commits current, `useChangeViewerSync` moves
 * the change viewer through the list's rows, and `useSessionBranchFollow` sets the branch from
 * `session`, the selected session when it belongs to this project, none otherwise.
 */
export function useGitReview(project: string, browser: ProjectBrowserState, active: boolean, session: string | undefined, filterState: FileFilterState): GitReview {
  const source = useProjectSource(project, active);
  const git = source.source.state === "loaded" ? source.source.source.git : null;
  const branches = useBranchList(project, active && git !== null);
  const branchList = branches.branches;
  // A chosen branch the repository no longer has, merged and deleted say, has nothing left to
  // review: the view goes back to what the worktree holding the project's directory has checked
  // out, from the render that finds it gone, so the gone branch is never shown as the one chosen.
  // A branch only taken out of its worktree is still there, and stays chosen.
  const chosenGone = branchGone(browser.branch, branchList, git?.worktrees.find((w) => w.branch === browser.branch));
  const { ownBranch, branch, holder, holderId, committed } = changesScope(git, chosenGone ? undefined : browser.branch, branchList);
  const changes = useChangeList(project, holderId, active && holder !== undefined);
  const changeList = changes.list;
  const worktreeGone = changeList.state === "error" && changeList.unavailable;
  const byCommit = browser.byCommit;
  const committedChanges = useCommittedChanges(project, committed, branchList, byCommit, active, git !== null);
  const { comparison, forked, commits } = committedChanges;
  const listRef = useRef<HTMLDivElement>(null);
  const selectorsRef = useRef<HTMLDivElement>(null);
  const { setSelectedChange, setBranch: storeBranch, setByCommit, setCollapsedChangeRows } = browser;
  // A change whose worktree is gone closes as one the list no longer has does.
  const changeReader = useChangeReader(project, () => {
    setSelectedChange(undefined);
    focusFirst(listRef.current, selectorsRef.current);
  });
  const layout = browser.layout;
  const collapsed = browser.collapsedChangeRows;
  const selectedChange = browser.selectedChange;
  const filter = filterState.filter.trim();

  const uncommittedItems = holder !== undefined && changeList.state === "loaded" ? changeList.items : undefined;
  const committedItems = committedChanges.items;
  // The sections' ranks run uncommitted first, then the committed ones, each list already in order.
  const items = useMemo(() => [...(uncommittedItems ?? []), ...(committedItems ?? [])], [uncommittedItems, committedItems]);

  // A worktree found gone may have been replaced by another at its place, or others added: the
  // worktrees are asked for again, and so are the branches, as the branch may have gone with it.
  useEffect(() => {
    if (!worktreeGone) return;
    void source.refresh();
    void branches.refresh();
  }, [changeList]);

  const dismissViewer = () => {
    changeReader.close();
  };

  // Another branch is another list: nothing selected or open from the previous one stands.
  const setBranch = (next: string | undefined) => {
    if ((next ?? ownBranch) !== branch) {
      setSelectedChange(undefined);
      changeReader.close();
    }
    storeBranch(next);
  };
  const settleFollow = useSessionBranchFollow(project, session, active, source, branches, setBranch);
  // A choice the user makes settles what the selected session set: it is not set over it later.
  const changeBranch = (next: string) => {
    settleFollow();
    setBranch(next);
  };

  const refresh = async (askedByPerson = false) => {
    await Promise.all([source.refresh(askedByPerson), branches.refresh(), changes.refresh(), committedChanges.refresh()]);
  };

  // The selection is kept in view, while the viewer moves through the changes. The layout and the
  // filter are part of what was scrolled to: another layout, or the same list narrowed or widened,
  // is a list with its own scroll.
  const scrolledToChange = useRef<string | undefined>(undefined);
  useLayoutEffect(() => {
    const scrolled = selectedChange && `${layout}:${filter}:${selectedChange}`;
    if (scrolled !== scrolledToChange.current && scrollToChange(listRef.current, filterChanges(items, filter), selectedChange, layout, collapsed)) {
      scrolledToChange.current = scrolled;
    }
  }, [selectedChange, items, layout, collapsed, filter]);

  const { openChange, closeChange, navigation } = useChangeViewerSync({
    reader: changeReader,
    selectedChange,
    setSelectedChange,
    listRef,
    selectorsRef,
    holder,
    holderId,
    worktreeGone,
    list: changeList,
    committed: committedChanges,
    items,
    filter,
    layout,
    collapsed,
  });

  // The gone branch's choice is dropped for good, and nothing selected or open from its list stands.
  // A layout effect, so the old change is never painted over the default branch's list, and closing
  // comes before the worktree reread effect could read from the removed worktree.
  useLayoutEffect(() => {
    if (!chosenGone) return;
    setSelectedChange(undefined);
    changeReader.close();
    storeBranch(undefined);
  }, [chosenGone]);

  // The toggle keeps its identity while the comparison has commits and the choice stands, as the
  // list builds its sections again whenever it changes.
  const hasCommits = forked !== undefined;
  const byCommitToggle = hasCommits ? { value: byCommit, onChange: setByCommit } : undefined;
  const changeView = {
    layout,
    byCommit: byCommitToggle,
    collapsed,
    onCollapsedChange: setCollapsedChangeRows,
    ...filterState,
  };

  return {
    view: {
      source: source.source,
      branches: branchList,
      branch,
      onBranchChange: changeBranch,
      uncommitted: uncommittedState(branch, holder, holderId, changeList, worktreeGone),
      committed: committedState(committed, comparison, commits, byCommit, hasCommits),
      items,
      selectedChange,
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
  scrollRowIntoView(list, top, { stickyTop: HEADING_HEIGHT });
  return true;
}
