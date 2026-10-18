import { FileCheck, GitBranch, GitFork, TriangleAlert } from "lucide-react";
import React from "react";

import { failureParam } from "../components/DaemonErrorMessage";
import { CenteredEmptyPanel } from "../components/EmptyPanel";
import { ErrorAlert } from "../components/ErrorAlert";
import { CodeBlock } from "../components/CodeBlock";
import { InlineChip } from "../components/InlineChip";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useListMotion } from "../components/useListMotion";
import type { Translate } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import { useDaemonStore } from "../store";
import { displayWirePath } from "../wirePath";
import { BranchSelect } from "./BranchSelect";
import { ChangeList } from "./ChangeList";
import type { ChangeViewState } from "./changeView";
import type { ChangeItem } from "./changes";
import { branchGone, changesStatus, partsLoading, type ChangesNotice } from "./changesStatus";
import { GitFailure, GitLoading } from "./gitStates";
import { PaneNote } from "./paneNote";
import type { BranchList } from "./useBranchList";
import type { CommittedState, UncommittedState } from "./useGitReview";
import type { SourceState } from "./useProjectSource";

/** What the Changes view shows and does: one branch's changes, uncommitted and committed, as one
 * list. */
export interface ChangesViewProps {
  branches: BranchList;
  /** The branch shown, as a wire path; none when no branch is chosen and the worktree holding the
   * project's directory has none checked out. */
  branch: string | undefined;
  onBranchChange: (branch: string) => void;
  uncommitted: UncommittedState;
  committed: CommittedState;
  /** Every change listed, uncommitted and committed, in list order. */
  items: ChangeItem[];
  selectedChange: string | undefined;
  onOpen: (item: ChangeItem) => void;
  listRef: React.Ref<HTMLDivElement>;
  /** The branch selector's strip, which takes keyboard focus when what held it below goes. */
  selectorsRef: React.RefObject<HTMLDivElement | null>;
  changeView: ChangeViewState;
  /** The session selected in the sidebar, whose branches the selector's list marks. */
  selectedSessionId: string | undefined;
  /**
   * Asks again for whatever failed. `from` is the control pressed, which goes away with the
   * failure.
   */
  onRetry: (from: Element | null) => void;
}

/**
 * The project pane's Git mode: the changes of one branch, chosen in the selector at the top. What
 * is uncommitted is read from the worktree that has the branch checked out, in the worktree's
 * sections — Conflicted, Staged, Unstaged, Untracked — and what the branch has committed since it
 * forked from the branch the repository's main working copy has checked out follows them, as one
 * Committed section, grouped by commit or not. The lines above the list say what it is made of and
 * what could not be read; a part that failed says why while the rest is shown. Nothing here checks
 * anything out.
 */
export function GitView(props: { source: SourceState } & ChangesViewProps): React.ReactElement {
  const t = useT();
  const { source } = props;
  // One region for every loading the mode shows, outside `GitContent`, which swaps its whole tree
  // as the source loads. The source's loading and the list's say the same words, so the handoff
  // between them leaves the text as it is and it is said once.
  const loading = source.state === "loading" ? t("git.loading") : source.state === "loaded" && source.source.git ? loadingText(t, props) : undefined;
  return (
    <>
      <StatusAnnouncer text={loading} />
      <GitContent {...props} />
    </>
  );
}

/** What the view is reading, as it is announced: the list as a whole while it has nothing to show,
 * and otherwise the part of it still being read. */
function loadingText(t: Translate, props: ChangesViewProps): string | undefined {
  const { committed, items, branch } = props;
  if (branch === undefined) return undefined;
  const parts = partsLoading(props);
  if (items.length === 0) return parts.uncommitted || parts.committed ? t("git.loading") : undefined;
  if (committed.state !== "compare" || !parts.committed) return undefined;
  return committed.comparison.state === "loading" ? t("git.changes.comparing", { base: displayWirePath(committed.base) }) : t("git.changes.listingCommits");
}

function GitContent(props: { source: SourceState } & ChangesViewProps): React.ReactElement {
  const t = useT();
  const { source, onRetry } = props;
  if (source.state === "loading") return <GitLoading label={t("git.loading")} />;
  if (source.state === "error") return <GitFailure title={t("git.error.source")} message={source.message} onRetry={onRetry} />;
  const git = source.source.git;
  if (!git) {
    return source.source.git_error !== null ? (
      <div className="flex min-w-0 flex-1 items-center justify-center p-4">
        <ErrorAlert title={t("git.unavailable")} className="w-full max-w-lg">
          <CodeBlock>{source.source.git_error}</CodeBlock>
        </ErrorAlert>
      </div>
    ) : (
      <CenteredEmptyPanel icon={GitFork} message={t("git.notARepository")} />
    );
  }
  return <ChangesView {...props} />;
}

const code = (text: string) => (
  <InlineChip monospace wrap size="inherit">
    {text}
  </InlineChip>
);

/** A line above the list, said in the words of the message it is. */
function Notice({ notice }: { notice: ChangesNotice }): React.ReactElement {
  const t = useT();
  switch (notice.id) {
    case "notCheckedOut":
      return <Message id="git.changes.notCheckedOut" params={{ branch: code(notice.branch) }} />;
    case "unborn":
      return <>{t("git.changes.unborn")}</>;
    case "worktree":
      return <Message id="git.changes.worktree" params={{ path: code(notice.path) }} />;
    case "scopeMissing":
      return <>{t("git.changes.scopeMissing")}</>;
    case "isBase":
      return <Message id="git.changes.isBase" params={{ branch: code(notice.branch) }} />;
    case "noBase":
      return <>{t("git.changes.noBase")}</>;
    case "comparing":
      return <>{t("git.changes.comparing", { base: notice.base })}</>;
    case "base":
      return <Message id="git.changes.base" params={{ base: code(notice.base), baseCommit: code(notice.baseCommit), tip: code(notice.tip) }} />;
    case "listingCommits":
      return <>{t("git.changes.listingCommits")}</>;
  }
}

function ChangesView({
  branches,
  branch,
  onBranchChange,
  uncommitted,
  committed,
  items,
  selectedChange,
  onOpen,
  onRetry,
  listRef,
  selectorsRef,
  changeView,
  selectedSessionId,
}: ChangesViewProps): React.ReactElement {
  const t = useT();
  const homeDir = useDaemonStore((s) => s.homeDir);
  const noticesRef = useListMotion<HTMLDivElement>();
  // When what holds focus below the selector goes — the list replaced by a failure or an empty
  // state — focus goes to the selector rather than falling to `<body>`; with nothing chosen in a
  // repository that has no branches the selector is disabled, and focus leaves for the top bar as
  // it does from any pane with nothing to hold it.
  const handoff = useFocusHandoff(() => selectorsRef.current?.querySelector<HTMLElement>("button:not([disabled])")?.focus());
  const listed = branches.state === "loaded" ? branches.branches : [];
  // Nothing chosen in a repository that has no branches: the selector has nothing to offer, and the
  // panel below says so.
  const noBranches = branch === undefined && branches.state === "loaded" && listed.length === 0;
  const gone = branchGone(branch, branches, uncommitted);
  // A list that failed to refresh stays, and says why; one that never came says so in its place.
  const branchesError = branches.state === "error" ? branches.message : branches.state === "loaded" ? branches.error : undefined;

  const { notices, failures, goneBranches, partial, commitsCut } = changesStatus({ branches, uncommitted, committed, homeDir });
  const loading = partsLoading({ uncommitted, committed });
  const nothingYet = items.length === 0;
  // With nothing to list, the first failure takes the list's place, with Try again; every other one
  // is a line above it.
  const panelFailure = nothingYet && !loading.uncommitted && !loading.committed ? failures[0] : undefined;
  const lineFailures = failures.filter((failure) => failure !== panelFailure);
  const shownComparison = committed.state === "compare" && committed.comparison.state === "loaded";

  return (
    <>
      <div ref={selectorsRef} className="shrink-0 groove-b py-2 px-(--pane-inset)">
        <div className="grid min-w-0 grid-cols-[auto_minmax(0,1fr)] items-center gap-x-3">
          <BranchSelect
            label={t("git.changes.branch")}
            branches={listed}
            value={branch}
            gone={gone}
            isDisabled={noBranches}
            selectedSessionId={selectedSessionId}
            onChange={onBranchChange}
          />
        </div>
      </div>
      {branchesError !== undefined && !(branch === undefined && branches.state === "error") && (
        <PaneNote role="alert">
          <Message id="git.branch.branchesError" params={{ message: failureParam(branchesError) }} />
        </PaneNote>
      )}
      <div className="flex min-h-0 flex-1 flex-col" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
        {branch === undefined ? (
          branches.state === "error" ? (
            <GitFailure title={t("git.branch.branchesFailed")} message={branches.message} onRetry={onRetry} />
          ) : noBranches ? (
            <CenteredEmptyPanel icon={GitFork} message={t("git.branch.noBranches")} />
          ) : (
            <CenteredEmptyPanel icon={GitBranch} message={t("git.changes.prompt")} description={t("git.changes.prompt.description")} />
          )
        ) : (
          <>
            <div className="flex shrink-0 flex-col gap-1 px-(--pane-inset) py-2 text-xs text-app-muted empty:hidden">
              {notices.map((notice) => (
                <p key={notice.id}>
                  <Notice notice={notice} />
                </p>
              ))}
              {lineFailures.map((failure) => (
                <p key={failure.key} role="alert">
                  <Message id={failure.line} params={{ message: failureParam(failure.message) }} />
                </p>
              ))}
            </div>
            {/* Mounted for as long as the comparison is, so a notice appearing in it is announced. */}
            {shownComparison && (
              <div ref={noticesRef} role="status" className="relative flex shrink-0 flex-col gap-1 px-(--pane-inset) text-xs text-app-muted not-empty:pb-2">
                {goneBranches.map((name) => (
                  <p key={name} data-list-motion={name} className="flex items-start gap-1.5">
                    <TriangleAlert aria-hidden="true" className="mt-px size-3.5 shrink-0" />
                    <span>
                      <Message id="git.branch.deleted" params={{ branch: code(name) }} />
                    </span>
                  </p>
                ))}
              </div>
            )}
            {nothingYet && (loading.uncommitted || loading.committed) ? (
              <GitLoading label={t("git.loading")} />
            ) : panelFailure ? (
              <GitFailure title={t(panelFailure.title)} message={panelFailure.message} onRetry={onRetry} />
            ) : nothingYet && !partial ? (
              <CenteredEmptyPanel icon={FileCheck} message={t("git.changes.empty")} />
            ) : (
              <>
                <ChangeList items={items} selected={selectedChange} label={t("git.changes.list.label")} onOpen={onOpen} listRef={listRef} changeView={changeView} />
                {partial && <PaneNote divided>{t("git.partial")}</PaneNote>}
                {commitsCut !== undefined && <PaneNote divided>{t("git.changes.commitsPartial", { count: commitsCut })}</PaneNote>}
              </>
            )}
          </>
        )}
      </div>
    </>
  );
}
