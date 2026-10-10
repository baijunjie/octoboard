import { Button, Label, ListBox, Select } from "@heroui/react";
import { ArrowUpDown, GitBranch, GitCompareArrows, GitFork, TriangleAlert } from "lucide-react";
import type React from "react";
import { ListLayout, Virtualizer } from "react-aria-components";

import { EmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { Message, useT } from "../i18n/react";
import type { BranchInfo } from "../protocol";
import { displayWirePath } from "../wirePath";
import { ChangeList, type ChangeListView } from "./ChangeList";
import type { ChangeItem } from "./changes";
import { comparisonNotices } from "./comparisonNotices";
import { GitFailure, GitLoading } from "./gitStates";
import type { BranchList } from "./useBranchList";
import type { Comparison } from "./useBranchComparison";
import { shortCommit } from "./useChangeReader";

/** Every option in a branch selector is one line of this height, which is what lets a repository's
 * thousands of branches be listed virtualized. */
const BRANCH_ROW_HEIGHT = 32;

/**
 * The Git mode's comparison of two local branches: a selector for each — From, the old side, and
 * To, the new side — and the changes between the two branches' commits that touch the project. A
 * comparison is of the commits the branches were at when it was made, which it names; when the
 * branch list read since says one of them has moved or gone, it says so, and Refresh compares the
 * branches as they are now. Nothing here checks anything out, and the worktree chosen for the
 * uncommitted changes plays no part in it.
 */
export function BranchComparisonView({
  branches,
  comparison,
  left,
  right,
  onBranchesChange,
  selectedChange,
  onOpen,
  onRetry,
  listRef,
  selectorsRef,
  changeView,
}: {
  branches: BranchList;
  comparison: Comparison;
  left: string | undefined;
  right: string | undefined;
  onBranchesChange: (left: string | undefined, right: string | undefined) => void;
  selectedChange: string | undefined;
  onOpen: (item: ChangeItem) => void;
  onRetry: (from: Element | null) => void;
  listRef: React.Ref<HTMLDivElement>;
  /** The selectors' strip, which takes keyboard focus when what held it below goes. */
  selectorsRef: React.RefObject<HTMLDivElement | null>;
  changeView: ChangeListView;
}): React.ReactElement {
  const t = useT();
  // When what holds focus below the selectors goes — the list replaced by a failure or an empty
  // state — focus goes to the first selector rather than falling to `<body>`.
  const handoff = useFocusHandoff(() => selectorsRef.current?.querySelector<HTMLElement>("button")?.focus());
  const listed = branches.state === "loaded" ? branches.branches : [];
  const known = (name: string | undefined) => name === undefined || branches.state !== "loaded" || !branches.complete || listed.some((b) => b.name === name);
  // A list that failed to refresh stays, and says why; one that never came says so in its place.
  const branchesError = branches.state === "error" ? branches.message : branches.state === "loaded" ? branches.error : undefined;
  return (
    <>
      <div ref={selectorsRef} className="flex shrink-0 items-center gap-1 border-b border-separator py-2 ps-3 pe-2">
        {/* One grid for both selectors, each a subgrid of it, so the label column is as wide as
            the widest label in the current language. */}
        <div className="grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)] items-center gap-x-3 gap-y-2">
          <BranchSelect label={t("git.compare.from")} branches={listed} value={left} gone={!known(left)} onChange={(name) => onBranchesChange(name, right)} />
          <BranchSelect label={t("git.compare.to")} branches={listed} value={right} gone={!known(right)} onChange={(name) => onBranchesChange(left, name)} />
        </div>
        <TitledControl title={t("git.compare.swap")}>
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            aria-label={t("git.compare.swap")}
            isDisabled={left === undefined && right === undefined}
            preventFocusOnPress
            onPress={() => onBranchesChange(right, left)}
          >
            <ArrowUpDown aria-hidden="true" className="size-4" />
          </Button>
        </TitledControl>
      </div>
      {branchesError !== undefined && !(comparison.state === "idle" && branches.state === "error") && (
        <p role="alert" className="shrink-0 px-3 py-2 text-xs text-muted">
          {t("git.compare.branchesError", { message: branchesError })}
        </p>
      )}
      <div className="flex min-h-0 flex-1 flex-col" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
        <ComparisonContent
          branches={branches}
          comparison={comparison}
          selectedChange={selectedChange}
          onOpen={onOpen}
          onRetry={onRetry}
          listRef={listRef}
          changeView={changeView}
        />
      </div>
    </>
  );
}

function ComparisonContent({
  branches,
  comparison,
  selectedChange,
  onOpen,
  onRetry,
  listRef,
  changeView,
}: {
  branches: BranchList;
  comparison: Comparison;
  selectedChange: string | undefined;
  onOpen: (item: ChangeItem) => void;
  onRetry: (from: Element | null) => void;
  listRef: React.Ref<HTMLDivElement>;
  changeView: ChangeListView;
}): React.ReactElement {
  const t = useT();
  if (comparison.state === "idle") {
    if (branches.state === "error") return <GitFailure title={t("git.compare.branchesFailed")} message={branches.message} onRetry={onRetry} />;
    if (branches.state === "loaded" && branches.branches.length === 0) return <EmptyPanel icon={GitFork} message={t("git.compare.noBranches")} />;
    return <EmptyPanel icon={GitCompareArrows} message={t("git.compare.prompt")} />;
  }
  if (comparison.state === "loading") return <GitLoading label={t("git.compare.loading")} />;
  if (comparison.state === "error") return <GitFailure title={t("git.compare.error")} message={comparison.message} onRetry={onRetry} />;
  const { left, right, items, complete } = comparison;
  const notices = comparisonNotices(branches, comparison);
  return (
    <>
      <p className="shrink-0 px-3 py-2 text-xs text-muted">
        {t("git.compare.commits", { from: shortCommit(left.commit), to: shortCommit(right.commit) })}
      </p>
      {/* Mounted for as long as the comparison is, so a notice appearing in it is announced. */}
      <div role="status" className="flex shrink-0 flex-col gap-1 px-3 text-xs text-muted not-empty:pb-2">
        {notices.map((notice) => (
          <p key={notice.branch} className="flex items-start gap-1.5">
            <TriangleAlert aria-hidden="true" className="mt-px size-3.5 shrink-0" />
            <span>
              {notice.kind === "moved" ? (
                <Message id="git.compare.moved" params={{ branch: <bdi dir="auto">{displayWirePath(notice.branch)}</bdi> }} />
              ) : (
                <Message
                  id="git.compare.deleted"
                  params={{ branch: <bdi dir="auto">{displayWirePath(notice.branch)}</bdi>, commit: shortCommit(notice.commit) }}
                />
              )}
            </span>
          </p>
        ))}
      </div>
      {left.commit === right.commit ? (
        <EmptyPanel icon={GitCompareArrows} message={t("git.compare.same")} />
      ) : items.length === 0 && complete ? (
        <EmptyPanel icon={GitCompareArrows} message={t("git.compare.empty")} />
      ) : (
        <>
          <ChangeList items={items} selected={selectedChange} label={t("git.compare.list.label")} onOpen={onOpen} listRef={listRef} changeView={changeView} />
          {!complete && <p className="shrink-0 border-t border-separator px-3 py-2 text-xs text-muted">{t("git.partial")}</p>}
        </>
      )}
    </>
  );
}

/** One end of the comparison: its label in the grid's first column, its trigger in the second. A
 * branch the list no longer has stays the value, named as gone, until the user picks another. */
function BranchSelect({
  label,
  branches,
  value,
  gone,
  onChange,
}: {
  label: string;
  branches: BranchInfo[];
  value: string | undefined;
  gone: boolean;
  onChange: (branch: string) => void;
}): React.ReactElement {
  const t = useT();
  const name = value === undefined ? undefined : displayWirePath(value);
  const shown =
    name === undefined ? t("git.compare.choose") : gone ? <Message id="git.compare.branchGone" params={{ branch: <bdi dir="auto">{name}</bdi> }} /> : name;
  const title = name === undefined ? undefined : gone ? t("git.compare.branchGone", { branch: name }) : name;
  return (
    // The subgrid restates the column gap: HeroUI's `.select` sets a gap of its own, which would
    // otherwise replace the parent grid's for this row's gutter.
    <Select
      fullWidth
      value={value ?? null}
      onChange={(key) => key !== null && onChange(String(key))}
      className="col-span-2 grid grid-cols-subgrid items-center gap-x-3"
    >
      <Label className="text-sm text-muted">{label}</Label>
      <Select.Trigger className="min-w-0">
        <Select.Value className="flex min-w-0 items-center gap-2">
          {() => (
            <>
              <GitBranch aria-hidden="true" className="size-4 shrink-0 text-muted" />
              <FadeOverflow as="span" dir="auto" className={`min-w-0 flex-1${name === undefined ? " text-muted" : ""}`} titleWhenClipped={title}>
                {shown}
              </FadeOverflow>
            </>
          )}
        </Select.Value>
        <Select.Indicator />
      </Select.Trigger>
      {/* As wide as the trigger: the virtualized list lays its rows out to the popover's width
          rather than taking one from them, and without a width of its own the popover grows past
          the window. */}
      <Select.Popover className="w-(--trigger-width)">
        <Virtualizer layout={ListLayout} layoutOptions={{ rowHeight: BRANCH_ROW_HEIGHT }}>
          {/* The side inset moves from the scroll container onto the rows: the virtualized layout
              makes each row as wide as the list's inner width and places it inside the list's own
              padding, so a padded list overflows sideways and clips the rows' focus ring. `pe-7` on
              a row restores the room HeroUI reserves for the check mark, which its Select styles
              override with a narrower padding, so the commit would run under the mark. */}
          <ListBox items={branches} className="max-h-80 overflow-auto px-0">
            {(branch) => {
              const branchName = displayWirePath(branch.name);
              return (
                <ListBox.Item id={branch.name} textValue={branchName} className="mx-1.5 h-8 min-h-0 w-auto py-0 pe-7">
                  <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={branchName}>
                    {branchName}
                  </FadeOverflow>
                  <span dir="ltr" className="shrink-0 font-mono text-xs text-muted">
                    {shortCommit(branch.commit)}
                  </span>
                  <ListBox.ItemIndicator />
                </ListBox.Item>
              );
            }}
          </ListBox>
        </Virtualizer>
      </Select.Popover>
    </Select>
  );
}
