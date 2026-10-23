import { FileCheck, GitBranch, GitFork, TriangleAlert } from "lucide-react";
import React, { useMemo, useState } from "react";

import { CopyButton } from "../components/CopyButton";
import { Button } from "../components/Button";
import { errorDetails, failureParam, FailureBody } from "../components/DaemonErrorMessage";
import { CenteredEmptyPanel } from "../components/EmptyPanel";
import { FailureMark } from "../components/FailureMark";
import { PaneFailure } from "../components/PaneFailure";
import { useHasPainted } from "../components/useHasPainted";
import { DaemonRequestError } from "../daemon-client";
import { InlineReference } from "../components/InlineReference";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useStableHandler } from "../components/useLatest";
import { useListMotion } from "../components/useListMotion";
import type { ErrorPresentation } from "../errorPresentation";
import { retryCanHelp } from "../errorRecovery";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { joinPhrases } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { displayWirePath, wireRelativeOrAbsolute } from "../wirePath";
import { BranchSelect } from "./BranchSelect";
import { ChangeList } from "./ChangeList";
import type { ChangeViewState } from "./changeView";
import { SECTION_ORDER, type ChangeItem, type ChangeSection } from "./changes";
import { branchGone, changesStatus, partsLoading, type ChangesFailure, type ChangesNotice } from "./changesStatus";
import { GitLoading } from "./gitStates";
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
  const spoken = useSpokenFailures(props, source.state === "loaded" && Boolean(source.source.git));
  return (
    <>
      <StatusAnnouncer text={loading ?? spoken} />
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
  return committed.comparison.state === "loading" ? t("git.changes.comparing", { base: isolate(displayWirePath(committed.base)) }) : t("git.changes.listingCommits");
}

const UNAVAILABLE_TITLE = <Message id="git.unavailable" />;

function GitContent(props: { source: SourceState } & ChangesViewProps): React.ReactElement {
  const t = useT();
  const { source, onRetry } = props;
  const diagnostic = source.state === "loaded" ? source.source.git_error ?? undefined : undefined;
  const failure = useMemo(() => diagnostic === undefined ? undefined : new DaemonRequestError(diagnostic, "git_unavailable", { detail: diagnostic }), [diagnostic]);
  const painted = useHasPainted();
  if (source.state === "loading") return <GitLoading label={t("git.loading")} />;
  if (source.state === "error") return <PaneFailure title={t("git.error.source")} message={source.message} announce={painted} onRetry={onRetry} />;
  const git = source.source.git;
  if (!git) {
    return failure ? (
      <PaneFailure title={UNAVAILABLE_TITLE} message={failure} announce={painted} />
    ) : (
      <CenteredEmptyPanel icon={GitFork} message={<Message id="git.notARepository" />} />
    );
  }
  return <ChangesView {...props} projectRoots={[source.source.root, source.source.resolved_root]} />;
}

const code = (text: string) => (
  <InlineReference monospace wrap>
    {text}
  </InlineReference>
);

/** A line above the list, said in the words of the message it is. */
function Notice({ notice, projectRoots }: { notice: ChangesNotice; projectRoots: readonly string[] }): React.ReactElement {
  const t = useT();
  switch (notice.id) {
    case "unborn":
      return <>{t("git.changes.unborn")}</>;
    case "worktree":
      return (
        <>
          <Message id="git.changes.worktree" params={{ name: code(notice.name) }} />
          {/* Negative vertical margins keep the extra-small button's margin box at the line's 16px height. */}
          <CopyButton
            text={wireRelativeOrAbsolute(notice.root, projectRoots)}
            beforeCopyText={t("git.changes.copyWorktreePath")}
            failedText={t("git.changes.copyWorktreePathFailed")}
            size="xs"
            variant="ghost"
            className="ms-1 -my-1 align-top"
          />
        </>
      );
    case "scopeMissing":
      return <>{t("git.changes.scopeMissing")}</>;
    case "noRemote":
      return <Message id="git.changes.noRemote" params={{ branch: code(notice.branch) }} />;
    case "noBase":
      return <>{t("git.changes.noBase")}</>;
    case "comparing":
      return <Message id="git.changes.comparing" params={{ base: code(notice.base) }} />;
    case "base":
      return <Message id="git.changes.base" params={{ base: code(notice.base), baseCommit: code(notice.baseCommit), tip: code(notice.tip) }} />;
    case "listingCommits":
      return <>{t("git.changes.listingCommits")}</>;
  }
}

/** The failure of one part of the list while the rest still shows: a danger glyph named by what
 * failed, whose popover holds the reason, its remedy, the raw text of the failure and Try again,
 * which a failure that is the state of the repository (no common commit) has none of, a retry
 * giving the same answer. `inHeader` is for the mark a section's heading carries, which the list's
 * own walk does not reach (see `FailureMark`). */
function PartFailureMark({ label, line, message, inHeader = false, onRetry }: { label: string; line: ChangesFailure["line"] | "git.branch.branchesError"; message: ErrorPresentation; inHeader?: boolean; onRetry: () => void }): React.ReactElement {
  const t = useT();
  return (
    <FailureMark
      label={label}
      inHeader={inHeader}
      actions={retryCanHelp(message) ? (close) => (
        <Button size="sm" variant="secondary" preventFocusOnPress onPress={() => { close(); onRetry(); }}>
          {t("error.tryAgain")}
        </Button>
      ) : undefined}
    >
      <FailureBody error={message} summary={<Message id={line} params={{ message: failureParam(message) }} />} />
    </FailureMark>
  );
}

/** Where each failed part of the list is shown. With nothing to list and a part failed, the failure
 * takes the list's place (`panelFailures`: the one, or the two together, with Try again); otherwise
 * each is a mark (`lineFailures`, on the heading of the section it could not fill). A branch list
 * that failed to refresh stays and says why in a mark by the selector; one that never came says so
 * in the list's place. */
function failureParts({ branches, branch, uncommitted, committed, items }: Pick<ChangesViewProps, "branches" | "branch" | "uncommitted" | "committed" | "items">) {
  const { failures } = changesStatus({ branches, uncommitted, committed });
  const loading = partsLoading({ uncommitted, committed });
  const panelFailures = items.length === 0 && !loading.uncommitted && !loading.committed ? failures : [];
  const branchesError = branches.state === "error" ? branches.message : branches.state === "loaded" ? branches.error : undefined;
  return {
    panelFailures,
    lineFailures: panelFailures.length > 0 ? [] : failures,
    branchesError,
    branchesMark: branchesError !== undefined && !(branch === undefined && branches.state === "error"),
  };
}

/** The uncommitted changes and the branch's commits both failed and nothing is listed: one heading
 * and one Try again, with each part's own sentence in Details, then its raw text, in place of the two
 * marks. */
function BothFailed({ failures, announce, onRetry }: { failures: ChangesFailure[]; announce: boolean; onRetry: (from: Element | null) => void }): React.ReactElement {
  const t = useT();
  return (
    <PaneFailure
      title={t("git.changes.failed")}
      summary={t("git.changes.failed.summary")}
      details={failures.map((failure) => (
        <div key={failure.key} className="flex flex-col gap-2">
          <p><Message id={failure.line} params={{ message: failureParam(failure.message) }} /></p>
          {errorDetails(failure.message)}
        </div>
      ))}
      announce={announce}
      onRetry={failures.some((failure) => retryCanHelp(failure.message)) ? onRetry : undefined}
    />
  );
}

/** The marks that appear once the view is open, said once through the view's status region: a mark
 * is no alert. What failed before the view first painted is part of what it opened with and stays
 * unspoken. */
function useSpokenFailures(props: ChangesViewProps, enabled: boolean): string | undefined {
  const t = useT();
  const language = useCurrentLanguage();
  const { lineFailures, branchesMark } = failureParts(props);
  const all = [...lineFailures.map((failure) => ({ key: failure.key, text: t(failure.title) })), ...(branchesMark ? [{ key: "branches", text: t("git.branch.branchesFailed") }] : [])];
  const [openedWith] = useState(() => new Set(all.map((failure) => failure.key)));
  if (!enabled) return undefined;
  const spoken = all.filter((failure) => !openedWith.has(failure.key)).map((failure) => failure.text);
  return spoken.length > 0 ? joinPhrases(language, spoken) : undefined;
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
  projectRoots,
}: ChangesViewProps & {
  /** The project's directory as stored and as it resolves, either of which a worktree's root may be
   * relative to. */
  projectRoots: readonly string[];
}): React.ReactElement {
  const t = useT();
  const noticesRef = useListMotion<HTMLDivElement>();
  // When a focused worktree control or list row goes away, focus goes to the selector rather
  // than falling to `<body>`; with nothing chosen in a
  // repository that has no branches the selector is disabled, and focus leaves for the top bar as
  // it does from any pane with nothing to hold it.
  const handoff = useFocusHandoff(() => selectorsRef.current?.querySelector<HTMLElement>("button:not([disabled])")?.focus());
  const listed = branches.state === "loaded" ? branches.branches : [];
  // Nothing chosen in a repository that has no branches: the selector has nothing to offer, and the
  // panel below says so.
  const noBranches = branch === undefined && branches.state === "loaded" && listed.length === 0;
  const gone = branchGone(branch, branches, uncommitted.state === "listed" ? uncommitted.worktree : undefined);
  const painted = useHasPainted();
  const { notices, goneBranches, partial, commitsCut } = changesStatus({ branches, uncommitted, committed });
  const loading = partsLoading({ uncommitted, committed });
  const nothingYet = items.length === 0;
  const { panelFailures, lineFailures, branchesError, branchesMark } = failureParts({ branches, branch, uncommitted, committed, items });
  const retryAll = useStableHandler(() => onRetry(null));
  // Where the uncommitted part's mark goes: the first worktree section that holds a change.
  const uncommittedSection = SECTION_ORDER.find((section) => section !== "committed" && items.some((item) => item.section === section)) ?? "unstaged";
  // Kept while what it is made of is, since the list rebuilds its sections whenever this changes.
  // `lineFailures` is rebuilt on every render, so what the marks are made of is taken from it: at most
  // one failure of the uncommitted part, and one of the committed part (the comparison's or the commits').
  const uncommittedFailure = lineFailures.find((failure) => failure.key === "uncommitted");
  const committedFailure = lineFailures.find((failure) => failure.key !== "uncommitted");
  const failureMarks = useMemo(() => {
    const mark = (failure: ChangesFailure) => <PartFailureMark key={failure.key} inHeader label={t(failure.title)} line={failure.line} message={failure.message} onRetry={retryAll} />;
    return new Map<ChangeSection, React.ReactNode>([
      ...(uncommittedFailure ? [[uncommittedSection, mark(uncommittedFailure)] as const] : []),
      ...(committedFailure ? [["committed", mark(committedFailure)] as const] : []),
    ]);
  }, [uncommittedFailure?.message, committedFailure?.key, committedFailure?.message, uncommittedSection, t, retryAll]);
  const shownComparison = committed.state === "compare" && committed.comparison.state === "loaded";

  return (
    <>
      <div ref={selectorsRef} className="shrink-0 groove-b py-2 px-(--pane-inset)" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
        <div className={`grid min-w-0 ${branchesMark ? "grid-cols-[auto_minmax(0,1fr)_auto]" : "grid-cols-[auto_minmax(0,1fr)]"} items-center gap-x-3`}>
          <BranchSelect
            label={t("git.changes.branch")}
            branches={listed}
            value={branch}
            gone={gone}
            isDisabled={noBranches}
            selectedSessionId={selectedSessionId}
            onChange={onBranchChange}
          />
          {branchesMark && branchesError !== undefined && (
            <PartFailureMark label={t("git.branch.branchesFailed")} line="git.branch.branchesError" message={branchesError} onRetry={() => onRetry(null)} />
          )}
        </div>
        {branch !== undefined && (
          <>
            <div className="mt-2 flex flex-col gap-1 text-xs text-app-muted empty:hidden">
              {notices.map((notice) => (
                <p key={notice.id}>
                  <Notice notice={notice} projectRoots={projectRoots} />
                </p>
              ))}
            </div>
          </>
        )}
      </div>
      <div className="flex min-h-0 flex-1 flex-col" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
        {branch === undefined ? (
          branches.state === "error" ? (
            <PaneFailure title={t("git.branch.branchesFailed")} message={branches.message} announce={painted} onRetry={onRetry} />
          ) : noBranches ? (
            <CenteredEmptyPanel icon={GitFork} message={t("git.branch.noBranches")} />
          ) : (
            <CenteredEmptyPanel icon={GitBranch} message={t("git.changes.prompt")} description={t("git.changes.prompt.description")} />
          )
        ) : (
          <>
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
            ) : panelFailures.length === 1 ? (
              <PaneFailure title={t(panelFailures[0].title)} message={panelFailures[0].message} announce={painted} onRetry={onRetry} />
            ) : panelFailures.length > 1 ? (
              <BothFailed failures={panelFailures} announce={painted} onRetry={onRetry} />
            ) : nothingYet && !partial && lineFailures.length === 0 ? (
              <CenteredEmptyPanel icon={FileCheck} message={t("git.changes.empty")} />
            ) : (
              <>
                <ChangeList failures={failureMarks} items={items} selected={selectedChange} label={t("git.changes.list.label")} onOpen={onOpen} listRef={listRef} changeView={changeView} />
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
